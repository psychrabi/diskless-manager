use crate::{
    core::reconciliation::storage_spec_for_client, domain::ClientId, persistence::ClientRepository,
    state::AppState,
};
use anyhow::{Context, Result};
use std::path::PathBuf;

fn boot_server_ip(settings: &crate::core::config::Settings) -> String {
    let next = settings.dhcp.next_server_ip.trim();
    if next.is_empty() {
        settings.server.ip_address.trim().to_string()
    } else {
        next.to_string()
    }
}

fn is_legacy_super(client: &crate::domain::Client, has_incomplete_session: bool) -> bool {
    client.is_super_mode() && !has_incomplete_session
}

fn client_menu_path(
    client: &crate::domain::Client,
    settings: &crate::core::config::Settings,
) -> Result<PathBuf> {
    let root = PathBuf::from(&settings.http.root_dir);
    let relative = crate::infrastructure::pxe::client_mac_script_path(client.mac.as_str());
    anyhow::ensure!(
        crate::infrastructure::pxe::is_managed_script_path(&root, &relative),
        "refusing unsafe iPXE menu path: {relative}"
    );
    Ok(root.join(relative))
}

pub async fn read_custom_client_menu(
    client: &crate::domain::Client,
    settings: &crate::core::config::Settings,
) -> Result<Option<String>> {
    let path = client_menu_path(client, settings)?;
    let contents = match tokio::fs::read_to_string(path).await {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            crate::infrastructure::command::read_file_with_sudo(&client_menu_path(
                client, settings,
            )?)
            .map_err(|error| anyhow::anyhow!("cannot read client iPXE menu: {error}"))?
            .context("client iPXE menu does not exist")?
        }
        Err(error) => return Err(error.into()),
    };
    anyhow::ensure!(
        contents.starts_with("#!ipxe"),
        "existing client menu is invalid"
    );
    Ok((!crate::infrastructure::pxe::is_manager_generated_menu(&contents)).then_some(contents))
}

pub async fn restore_client_menu(
    client: &crate::domain::Client,
    settings: &crate::core::config::Settings,
    contents: &str,
) -> Result<()> {
    anyhow::ensure!(
        contents.starts_with("#!ipxe"),
        "saved client menu is invalid"
    );
    crate::infrastructure::dhcp::restore_client_ipxe_menu(settings, client.mac.as_str(), contents)
        .await
}

pub async fn backup_custom_client_menu(
    clients: &ClientRepository,
    client: &crate::domain::Client,
    settings: &crate::core::config::Settings,
) -> Result<()> {
    if let Some(contents) = read_custom_client_menu(client, settings).await? {
        clients
            .save_boot_menu_backup(client.id.as_str(), &contents)
            .await?;
    }
    Ok(())
}

pub async fn render_client_menu(
    clients: &ClientRepository,
    client: &crate::domain::Client,
    settings: &crate::core::config::Settings,
    super_boot: Option<crate::infrastructure::dhcp::SuperBootReservation>,
    assigned_windows: bool,
) -> Result<String> {
    if super_boot.is_none() {
        if let Some(contents) = clients.boot_menu_backup(client.id.as_str()).await? {
            restore_client_menu(client, settings, &contents).await?;
            clients.delete_boot_menu_backup(client.id.as_str()).await?;
            return Ok(contents);
        }
        if let Some(contents) = read_custom_client_menu(client, settings).await? {
            return Ok(contents);
        }
    }

    let target_iqn = client
        .target_iqn
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let target_iqn = match (target_iqn, super_boot.as_ref()) {
        (Some(iqn), _) => iqn,
        (None, Some(crate::infrastructure::dhcp::SuperBootReservation::Deny)) => "",
        _ => anyhow::bail!("client has no target IQN for iPXE menu"),
    };
    let chap = match (
        client.chap_enabled,
        client.chap_user.as_deref(),
        client.chap_secret.as_deref(),
    ) {
        (true, Some(username), Some(password))
            if !username.trim().is_empty() && !password.is_empty() =>
        {
            Some(crate::infrastructure::iscsi::ChapCredentials {
                username: username.to_string(),
                password: password.to_string(),
            })
        }
        _ => None,
    };
    let reservation = crate::infrastructure::dhcp::BootReservation {
        client_name: client.name.clone(),
        mac: client.mac.to_string(),
        ip: client.ip.to_string(),
        target_iqn: target_iqn.to_string(),
        server_ip: boot_server_ip(settings),
        chap,
        super_boot,
        assigned_windows,
    };
    Ok(crate::infrastructure::dhcp::render_client_ipxe(
        &reservation,
    ))
}

/// Restore a menu saved by an earlier Super-session implementation.
pub async fn restore_legacy_client_menu_backup(
    clients: &ClientRepository,
    client: &crate::domain::Client,
    settings: &crate::core::config::Settings,
) -> Result<()> {
    if let Some(contents) = clients.boot_menu_backup(client.id.as_str()).await? {
        restore_client_menu(client, settings, &contents).await?;
        clients.delete_boot_menu_backup(client.id.as_str()).await?;
    }
    Ok(())
}

/// Complete boot storage readiness and render the client's final iPXE script.
pub async fn ensure_client_boot_ready(
    state: &AppState,
    id: &str,
    requesting_mac: &str,
) -> Result<String> {
    let id = ClientId::from_string(id.to_owned()).map_err(|error| anyhow::anyhow!(error))?;
    let readiness_started = std::time::Instant::now();
    let clients = ClientRepository::new(state.db_pool.clone());
    let client = clients
        .find_by_id(&id)
        .await?
        .context("client disappeared while waiting for boot readiness")?;
    let requesting_mac = crate::domain::MacAddress::parse(requesting_mac)
        .map_err(|error| anyhow::anyhow!("invalid requesting MAC: {error}"))?;
    let current_mac_owner = clients
        .find_by_mac(&requesting_mac)
        .await?
        .context("requesting MAC was reassigned while waiting for boot readiness")?;
    anyhow::ensure!(
        current_mac_owner.id == client.id && client.mac == requesting_mac,
        "requesting MAC no longer belongs to this client"
    );
    anyhow::ensure!(client.enabled, "client was disabled before boot readiness");
    let pending_legacy_exit: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM legacy_super_exits WHERE client_id = ?")
            .bind(id.as_str())
            .fetch_one(&state.db_pool)
            .await?;
    anyhow::ensure!(
        pending_legacy_exit == 0,
        "Legacy Super exit recovery is pending; an administrator must retry the offline exit before this client can boot"
    );
    let session = state
        .application
        .super_sessions
        .get_for_client(id.as_str())
        .await?;
    let has_incomplete_session = session
        .as_ref()
        .is_some_and(|session| session.state != crate::persistence::SuperSessionState::Completed);
    if is_legacy_super(&client, has_incomplete_session) {
        anyhow::bail!("Legacy Super client requires an administrator to exit Super mode offline and select a managed image snapshot");
    }
    anyhow::ensure!(
        !client.master.trim().is_empty() && client.master != "pending",
        "client has no assigned boot image"
    );
    if client
        .target_iqn
        .as_deref()
        .map_or(true, |value| value.trim().is_empty())
    {
        anyhow::bail!("client has no persisted iSCSI target IQN");
    }
    anyhow::ensure!(
        client
            .block_store
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty()),
        "client has no persisted iSCSI backstore"
    );
    let dataset = if client
        .snapshot
        .as_deref()
        .is_some_and(|snapshot| !snapshot.trim().is_empty())
    {
        client
            .writeback
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .context("snapshot-backed client has no persisted writeback dataset")?
    } else {
        client.master.as_str()
    };
    let expected_device = format!("/dev/zvol/{dataset}");
    anyhow::ensure!(
        client.block_device.as_deref() == Some(expected_device.as_str()),
        "client block device does not match its persisted boot dataset"
    );
    crate::application::client_lifecycle::validate_boot_storage_owner(state, &client).await?;
    crate::application::client_lifecycle::retry_pending_recovery(state, id.as_str()).await?;
    let session = state
        .application
        .super_sessions
        .get_for_client(id.as_str())
        .await?;
    let mut spec = storage_spec_for_client(
        &state.config,
        &state.application.storage,
        &state.db_pool,
        &client,
    )
    .await?
    .context("client has no storage configuration")?;
    let installer_media = if let Some(media_id) = client.installer_media_id.as_deref() {
        let media = state
            .application
            .installer_media
            .get(media_id)
            .await?
            .context("client installer media is no longer cataloged")?;
        anyhow::ensure!(
            media.validation_state == crate::persistence::StructuralValidationState::Valid,
            "client installer media has not passed structural validation"
        );
        spec.installer_iso = Some(media.managed_path.clone());
        Some(media)
    } else {
        None
    };
    let super_boot = match session.as_ref() {
        Some(session) if session.state == crate::persistence::SuperSessionState::Active => {
            let result = async {
                anyhow::ensure!(
                    client.is_super_mode(),
                    "active Super session and client boot mode are inconsistent"
                );
                let media = match session.installer_media_id.as_deref() {
                    Some(media_id) => Some(
                        state
                            .installer_media_service()
                            .await
                            .verify_integrity(media_id)
                            .await?,
                    ),
                    None => None,
                };
                spec.dataset = session.working_clone.clone();
                spec.source = crate::domain::storage::StorageSource::ExistingVolume(
                    session.working_clone.clone(),
                );
                spec.backstore =
                    crate::application::StorageService::super_session_working_backstore(
                        &session.working_clone,
                    );
                spec.use_game_disk = false;
                spec.game_disks.clear();
                let installer_iso = media.as_ref().map(|media| media.managed_path.clone());
                let target = crate::application::StorageService::build_super_session_iscsi_spec(
                    &spec,
                    installer_iso,
                )?;
                state
                    .application
                    .storage
                    .verify_super_session_target(&target)?;
                Ok::<_, anyhow::Error>(match media {
                    Some(media) => crate::infrastructure::dhcp::SuperBootReservation::Installer {
                        media_type: media.media_type,
                        firmware: client.pxe_mode.clone(),
                    },
                    None => crate::infrastructure::dhcp::SuperBootReservation::WorkingClone,
                })
            }
            .await;
            match result {
                Ok(boot) => Some(boot),
                Err(error) => {
                    return Err(error).context("active Super-session boot validation failed")
                }
            }
        }
        Some(session) if session.state != crate::persistence::SuperSessionState::Completed => {
            anyhow::bail!(
                "Super session is {:?} and cannot boot until recovery completes",
                session.state
            )
        }
        _ => {
            let storage = state.application.storage.clone();
            let desired = spec.clone();
            let storage_started = std::time::Instant::now();
            tokio::task::spawn_blocking(move || storage.ensure_client_storage_for_boot(&desired))
                .await??;
            tracing::info!(
                client_id = id.as_str(),
                elapsed_ms = storage_started.elapsed().as_millis(),
                "PXE storage readiness ensured"
            );
            installer_media.map(|media| {
                crate::infrastructure::dhcp::SuperBootReservation::Installer {
                    media_type: media.media_type,
                    firmware: client.pxe_mode.clone(),
                }
            })
        }
    };

    // CHAP may have been generated while building the spec; render from the
    // canonical persisted metadata consumed by iPXE.
    let client = clients
        .find_by_id(&id)
        .await?
        .context("client disappeared after boot storage readiness")?;
    let settings = state.settings.read().await.clone();
    let assigned_windows = state
        .application
        .images
        .get(&client.master)
        .await
        .map(|image| image.os_type == crate::core::image::OsType::Windows)
        .unwrap_or(false);
    // A durable installation session owns the boot payload: scoped credentials, the
    // recipe's boot commands and its disk-identity gate. A session that fails
    // validation denies boot instead of silently falling back to the generic
    // installer menu, which is bound to neither the client's disk size nor session.
    let script =
        match crate::application::installation_service::InstallationService::new(state.clone())
            .boot_for_client(id.as_str())
            .await?
        {
            Some(script) => script,
            None => {
                render_client_menu(&clients, &client, &settings, super_boot, assigned_windows)
                    .await?
            }
        };
    let fingerprint =
        crate::application::client_lifecycle::lifecycle_fingerprint(state, &client).await?;
    sqlx::query("INSERT INTO client_offline_resets(client_id, fingerprint, offline_since, completed, failures, retry_after, last_error) VALUES (?, ?, NULL, 0, 0, 0, NULL) ON CONFLICT(client_id) DO UPDATE SET fingerprint = excluded.fingerprint, offline_since = NULL, completed = 0, failures = 0, retry_after = 0, last_error = NULL")
        .bind(id.as_str())
        .bind(fingerprint)
        .execute(&state.db_pool)
        .await?;
    sqlx::query("DELETE FROM client_boot_failures WHERE client_id = ?")
        .bind(id.as_str())
        .execute(&state.db_pool)
        .await?;
    tracing::info!(
        client_id = id.as_str(),
        elapsed_ms = readiness_started.elapsed().as_millis(),
        "PXE boot readiness completed"
    );
    Ok(script)
}

pub async fn record_boot_failure(state: &AppState, id: &str, error: &str) -> Result<()> {
    ClientRepository::new(state.db_pool.clone())
        .find_by_id(&ClientId::from_string(id.to_owned()).map_err(|error| anyhow::anyhow!(error))?)
        .await?
        .context("client disappeared while recording boot failure")?;
    let now = chrono::Utc::now().timestamp();
    sqlx::query("INSERT INTO client_boot_failures(client_id, failures, last_failed_at, last_error) VALUES (?, 1, ?, ?) ON CONFLICT(client_id) DO UPDATE SET failures = client_boot_failures.failures + 1, last_failed_at = excluded.last_failed_at, last_error = excluded.last_error")
        .bind(id)
        .bind(now)
        .bind(error)
        .execute(&state.db_pool)
        .await
        .context("failed to persist boot provisioning diagnostics")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::is_legacy_super;
    use crate::{domain::Client, persistence::ClientRepository};
    use chrono::Utc;

    fn client(mode: Option<&str>) -> Client {
        Client {
            installer_media_id: None,
            id: crate::domain::ClientId::from_string("client").unwrap(),
            name: "Client".into(),
            mac: crate::domain::MacAddress::parse("00:11:22:33:44:55").unwrap(),
            ip: "192.0.2.1".parse().unwrap(),
            master: "pool/master".into(),
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            snapshot: None,
            block_store: None,
            target_iqn: None,
            writeback: None,
            last_modified: None,
            block_device: None,
            status: crate::domain::ClientStatus::Offline,
            mode: if mode == Some("super") {
                crate::domain::BootMode::Super
            } else {
                crate::domain::BootMode::Normal
            },
            pxe_mode: crate::domain::PxeMode::Uefi,
            keep_writeback: false,
            use_game_disk: false,
            game_disks: Vec::new(),
            chap_user: None,
            chap_secret: None,
            chap_enabled: false,
        }
    }

    #[test]
    fn only_super_mode_without_an_incomplete_session_is_legacy() {
        assert!(is_legacy_super(&client(Some("super")), false));
        assert!(!is_legacy_super(&client(Some("super")), true));
        assert!(!is_legacy_super(&client(None), false));
    }

    #[test]
    fn generated_menus_are_recognized_without_classifying_custom_menus() {
        assert!(crate::infrastructure::pxe::is_manager_generated_menu(
            "#!ipxe\n# Client-specific iPXE script generated by diskless-manager.\n"
        ));
        assert!(crate::infrastructure::pxe::is_manager_generated_menu(
            "#!ipxe\n# Super installer boot for pc\n"
        ));
        assert!(crate::infrastructure::pxe::is_manager_generated_menu(
            "#!ipxe\n# Super-session boot denied\n"
        ));
        assert!(!crate::infrastructure::pxe::is_manager_generated_menu(
            "#!ipxe\nmenu My custom boot\n"
        ));
    }

    #[tokio::test]
    async fn super_boot_policy_does_not_replace_custom_client_menus() {
        let root = tempfile::tempdir().unwrap();
        let mut settings = crate::core::config::Settings::default();
        settings.http.root_dir = root.path().to_string_lossy().into_owned();
        let mut client = client(Some("super"));
        client.target_iqn = Some("iqn.test:client".into());
        let relative = crate::infrastructure::pxe::client_mac_script_path(client.mac.as_str());
        let menu = root.path().join(relative);
        tokio::fs::create_dir_all(menu.parent().unwrap())
            .await
            .unwrap();

        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let clients = ClientRepository::new(pool);

        for (boot, expected) in [
            (
                crate::infrastructure::dhcp::SuperBootReservation::Deny,
                "Super-session boot denied",
            ),
            (
                crate::infrastructure::dhcp::SuperBootReservation::Installer {
                    media_type: crate::persistence::InstallerMediaType::LinuxIso,
                    firmware: crate::domain::PxeMode::Uefi,
                },
                "Super installer boot for client",
            ),
            (
                crate::infrastructure::dhcp::SuperBootReservation::WorkingClone,
                "Client-specific iPXE script generated by diskless-manager",
            ),
        ] {
            tokio::fs::write(&menu, "#!ipxe\necho custom boot menu\n")
                .await
                .unwrap();
            let script = super::render_client_menu(&clients, &client, &settings, Some(boot), false)
                .await
                .unwrap();
            let custom = tokio::fs::read_to_string(&menu).await.unwrap();
            assert!(script.contains(expected), "{script}");
            assert!(custom.contains("custom boot menu"), "{custom}");
        }
    }
}
