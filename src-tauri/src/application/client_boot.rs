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

/// Preserve a valid custom menu, creating the generated menu only if missing.
pub async fn ensure_client_menu(
    client: &crate::domain::Client,
    settings: &crate::core::config::Settings,
) -> Result<()> {
    let root = PathBuf::from(&settings.http.root_dir);
    let relative = crate::infrastructure::pxe::client_mac_script_path(client.mac.as_str());
    anyhow::ensure!(
        crate::infrastructure::pxe::is_managed_script_path(&root, &relative),
        "refusing to publish unsafe iPXE menu path: {relative}"
    );
    let path = root.join(&relative);
    if path.is_file() {
        let contents = tokio::fs::read_to_string(&path).await?;
        anyhow::ensure!(
            contents.starts_with("#!ipxe"),
            "existing client menu is invalid"
        );
        return Ok(());
    }
    let target_iqn = client
        .target_iqn
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("client has no target IQN for iPXE menu")?;
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
    };
    crate::infrastructure::dhcp::publish_client_ipxe(settings, &reservation)
        .await
        .map_err(|error| anyhow::anyhow!("failed to publish client iPXE menu: {error:#}"))?;
    anyhow::ensure!(path.is_file(), "client iPXE menu was not published");
    Ok(())
}

/// Complete the persisted client's boot storage before its menu is published.
pub async fn ensure_client_boot_ready(
    state: &AppState,
    id: &str,
    requesting_mac: &str,
) -> Result<()> {
    let id = ClientId::from_string(id.to_owned()).map_err(|error| anyhow::anyhow!(error))?;
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
    anyhow::ensure!(
        !client.master.trim().is_empty() && client.master != "pending",
        "client has no assigned boot image"
    );
    client
        .target_iqn
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .context("client has no persisted iSCSI target IQN")?;
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
    let spec = storage_spec_for_client(
        &state.config,
        &state.application.storage,
        &state.db_pool,
        &client,
    )
    .await?
    .context("client has no storage configuration")?;
    let storage = state.application.storage.clone();
    let desired = spec.clone();
    tokio::task::spawn_blocking(move || storage.ensure_client_storage_for_boot(&desired)).await??;

    // CHAP may have been generated while building the spec; use canonical
    // persisted metadata when publishing the script consumed by iPXE.
    let client = clients
        .find_by_id(&id)
        .await?
        .context("client disappeared after boot storage readiness")?;
    let settings = state.settings.read().await.clone();
    ensure_client_menu(&client, &settings).await?;
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
    Ok(())
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
