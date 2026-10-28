//! Client-bound installation sessions. Partitioning is performed only by scoped, identity-gated boot payloads.
use crate::{
    core::image::CreateImageRequest,
    domain::Client,
    infrastructure::iscsi::{ChapCredentials, IscsiLunSpec, IscsiTargetSpec},
    infrastructure::pxe::installation::{self, RenderContext},
    persistence::repositories::installation::{InstallationRepository, InstallationSession},
    state::AppState,
};
use anyhow::{ensure, Context, Result};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::json;
use std::{net::IpAddr, path::PathBuf};
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateInstallation {
    pub recipe: String,
    #[serde(default)]
    pub media_id: String,
    pub image_name: Option<String>,
    pub size_gb: Option<u64>,
    #[serde(default)]
    pub assign_on_ready: bool,
    pub confirm_erase: bool,
    pub username: Option<String>,
    pub password_hash: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallationReport {
    pub state: String,
    pub progress: i64,
    pub message: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdaptationInventory {
    #[serde(default)]
    pub boot_nic_ids: Vec<String>,
    #[serde(default)]
    pub boot_nic_mac: Option<String>,
}
#[derive(Clone)]
pub struct InstallationService {
    state: AppState,
    pub repository: InstallationRepository,
}
impl InstallationService {
    pub fn new(state: AppState) -> Self {
        Self {
            repository: InstallationRepository::new(state.db_pool.clone()),
            state,
        }
    }
    pub fn resume_saving(&self, id: String) {
        let service = self.clone();
        tokio::spawn(async move {
            if service.save_after_installer(&id).await.is_err() {
                tracing::error!(session_id = %id, "failed to resume installation save");
            }
        });
    }
    pub fn resume_published_cleanup(&self, id: String) {
        let service = self.clone();
        tokio::spawn(async move {
            if service.cleanup_published(&id).await.is_err() {
                if let Err(record_error) =
                    service.repository.record_published_cleanup_error(&id).await
                {
                    tracing::warn!(session_id = %id, %record_error, "failed to record published image cleanup error");
                } else {
                    tracing::warn!(session_id = %id, "published image cleanup remains pending");
                }
            }
        });
    }
    pub async fn sweep_deadlines(&self) -> Result<()> {
        for session in self.repository.expired_sessions().await? {
            let (state, message) = match session.state.as_str() {
                "verifying" => ("verification_failed", "Normal boot verification deadline expired"),
                "waiting" => ("cancelled", "Installer boot window expired; session credentials were revoked"),
                _ => ("recovery_required", "Installation phase deadline expired; verify the client is powered off before cleanup"),
            };
            if self
                .repository
                .transition(&session, state, session.progress, message)
                .await
                .is_ok()
                && state == "cancelled"
            {
                if let Err(error) = self.close_recovery(&session.id).await {
                    tracing::warn!(session_id = %session.id, %error, "expired installer cleanup remains pending");
                }
            }
        }
        Ok(())
    }
    async fn cleanup_published(&self, id: &str) -> Result<()> {
        let initial = self
            .repository
            .get(id)
            .await?
            .context("installation session does not exist")?;
        let _guard = self
            .state
            .client_mutations
            .lock_client_and_resource(&initial.client_id, "verified-image-factory")
            .await;
        let session = self
            .repository
            .get(id)
            .await?
            .context("installation session does not exist")?;
        ensure!(
            session.state == "published" && session.closed_at.is_none(),
            "published cleanup is not pending"
        );
        let deadline = Utc::now() + Duration::minutes(30);
        let target = session
            .verification_target_iqn
            .clone()
            .context("published verification target is missing")?;
        loop {
            let target_check = target.clone();
            let active = tokio::task::spawn_blocking(move || {
                crate::infrastructure::iscsi::target_has_active_sessions(&target_check)
            })
            .await??;
            let client = self.client(&session.client_id).await?;
            if !active && !client.is_online() {
                break;
            }
            ensure!(
                Utc::now() < deadline,
                "client has not powered off after verification; cleanup remains pending"
            );
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
        let storage = self.state.application.storage.clone();
        let target_to_remove = target.clone();
        tokio::task::spawn_blocking(move || storage.remove_owned_session_target(&target_to_remove))
            .await??;
        if let Some(candidate) = session.verification_image_id.as_deref() {
            self.state
                .application
                .images
                .delete_session_image(candidate, "verifying")
                .await?;
        }
        if let (Some(draft), Some(snapshot)) = (
            session.draft_image_id.as_deref(),
            session.verification_snapshot.as_deref(),
        ) {
            self.state
                .application
                .images
                .delete_session_snapshot(draft, snapshot)
                .await?;
        }
        self.remove_adaptation_bundle(&session.id).await;
        let assignment_error = if session.assign_on_ready {
            match self.assign_published_image(&session).await {
                Ok(()) => None,
                Err(error) => {
                    tracing::warn!(session_id = %session.id, %error, "published image assignment failed");
                    Some(error.to_string().chars().take(512).collect::<String>())
                }
            }
        } else {
            None
        };
        self.repository
            .finish_published_cleanup(&session, assignment_error.as_deref())
            .await
    }
    /// Bundles live outside the Apache-served boot root so they stay token-scoped.
    fn driver_bundle_root(&self) -> std::path::PathBuf {
        self.state
            .config_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(|parent| parent.join("installer-bundles"))
            .unwrap_or_else(|| std::env::temp_dir().join("diskless-manager-installer-bundles"))
    }
    /// Booted clients reach the scoped installer API on the enrollment
    /// listener, not on the Apache vhost that serves kernel and initrd files.
    fn installer_api_base(settings: &crate::core::config::Settings) -> String {
        format!(
            "http://{}:{}",
            settings.http.server_ip,
            crate::infrastructure::pxe::ENROLL_PORT
        )
    }
    /// Cleanup failure bookkeeping must never replace the error it describes.
    async fn record_cleanup_failure(&self, id: &str, message: &str) {
        if let Err(error) = self.repository.record_cleanup_failure(id, message).await {
            tracing::warn!(session_id = %id, %error, "failed to record installation cleanup failure");
        }
    }
    /// Driver bundles are disposable; failure to remove one must not block cleanup.
    async fn remove_adaptation_bundle(&self, id: &str) {
        let root = self.state.settings.read().await.http.root_dir.clone();
        let bundle_root = self.driver_bundle_root();
        let session_id = id.to_owned();
        if let Ok(Err(error)) = tokio::task::spawn_blocking(move || {
            crate::infrastructure::pxe::NetworkDriverInjectionPlugin::new(root)
                .with_bundle_directory(bundle_root)
                .remove_session_bundle(&session_id)
        })
        .await
        {
            tracing::warn!(session_id = %id, %error, "failed to remove installation driver bundle");
        }
    }
    /// Assign the published image through the normal reconciliation path after the
    /// verification client is off. Failure is recorded on the session but never
    /// unpublishes or removes the image.
    async fn assign_published_image(&self, session: &InstallationSession) -> Result<()> {
        let image_id = session
            .published_image_id
            .as_deref()
            .context("published image is missing")?;
        let image = self.state.application.images.get(image_id).await?;
        ensure!(
            image.status == "ready",
            "published image is not ready for assignment"
        );
        let client = self.client(&session.client_id).await?;
        self.ensure_offline(&client).await?;
        self.repository
            .assign_published_image(&session.id, &image.name)
            .await?;
        let state = self.state.clone();
        let client_id = session.client_id.clone();
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            runtime.block_on(crate::core::reconciliation::repair_client_storage(
                &state, &client_id,
            ))
        })
        .await
        .context("published image storage reconciliation task failed")??;
        Ok(())
    }
    /// The owned task retains mutation guards if an HTTP timeout drops the waiting request.
    pub async fn create(
        &self,
        client: String,
        request: CreateInstallation,
    ) -> Result<InstallationSession> {
        let service = self.clone();
        tokio::spawn(async move { service.prepare(&client, request).await }).await?
    }
    async fn prepare(&self, id: &str, request: CreateInstallation) -> Result<InstallationSession> {
        validate_request(&request)?;
        let _guard = self
            .state
            .client_mutations
            .lock_client_and_resource(id, "verified-image-factory")
            .await;
        let client = self.client(id).await?;
        self.validate_client(&client).await?;
        ensure!(
            self.repository.active(id).await?.is_none(),
            "client has an unfinished installation session; close recovery before retrying"
        );
        self.ensure_offline(&client).await?;
        let media_id = if request.media_id.is_empty() {
            client
                .installer_media_id
                .as_deref()
                .context("select installer media")?
        } else {
            request.media_id.as_str()
        };
        let media = self
            .state
            .installer_media_service()
            .await
            .verify_integrity(media_id)
            .await?;
        self.prerequisites(&request.recipe, &client, &media).await?;
        let assigned = self
            .state
            .application
            .images
            .get_optional(&client.master)
            .await?;
        let size_gb = request
            .size_gb
            .or_else(|| assigned.as_ref().map(|image| image.size_gb))
            .context("size_gb is required when the client has no assigned image")?;
        ensure!(size_gb >= 8, "draft image must be at least 8 GiB");
        let image_name = if request.recipe == "interactive" {
            format!("manual-{}", Uuid::new_v4().simple())
        } else {
            request
                .image_name
                .clone()
                .unwrap_or_else(|| format!("factory-{}", Uuid::new_v4().simple()))
        };
        let os_type = if request.recipe == "windows" {
            "windows".to_owned()
        } else {
            assigned
                .as_ref()
                .map(|image| image.os_type.to_string())
                .filter(|_| request.recipe == "interactive")
                .unwrap_or_else(|| "linux".to_owned())
        };
        let now = Utc::now();
        let id_value = Uuid::new_v4().to_string();
        let short = id_value.replace('-', "");
        let short = &short[..12];
        let prefix = self.state.settings.read().await.iscsi.target_prefix.clone();
        let target_iqn = format!("{prefix}:install.{short}");
        let chap = if client.chap_enabled {
            Some(ChapCredentials {
                username: client
                    .chap_user
                    .clone()
                    .context("client CHAP username is missing")?,
                password: client
                    .chap_secret
                    .clone()
                    .context("client CHAP secret is missing")?,
            })
        } else {
            None
        };
        let kind = if request.recipe == "interactive" {
            "manual"
        } else {
            "factory"
        };
        ensure!(
            kind != "manual" || !request.assign_on_ready,
            "manual sessions cannot publish or assign images"
        );
        let session = InstallationSession {
            id: id_value.clone(),
            client_id: id.to_owned(),
            kind: kind.to_owned(),
            recipe: request.recipe.clone(),
            media_id: media.id,
            media_checksum: media.checksum,
            image_name: Some(image_name.clone()),
            size_gb: Some(i64::try_from(size_gb)?),
            prior_master: Some(client.master.clone()),
            prior_snapshot: client.snapshot.clone(),
            target_iqn: target_iqn.clone(),
            source_ip: client.ip.to_string(),
            firmware: client.pxe_mode.as_str().to_owned(),
            assign_on_ready: request.assign_on_ready,
            token: format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()),
            config_json: json!({"username":request.username,"password_hash":request.password_hash})
                .to_string(),
            state: "preparing".to_owned(),
            progress: 0,
            error: None,
            assignment_state: if request.assign_on_ready {
                "pending"
            } else {
                "not_requested"
            }
            .into(),
            cleanup_state: "pending".into(),
            phase_deadline: Some((now + Duration::minutes(10)).to_rfc3339()),
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
            expires_at: (now
                + if kind == "manual" {
                    Duration::hours(24)
                } else {
                    Duration::minutes(10)
                })
            .to_rfc3339(),
            ..Default::default()
        };
        self.repository.insert(&session).await?;
        let draft = match self
            .state
            .application
            .images
            .create_draft(CreateImageRequest {
                name: image_name,
                os_type,
                size_gb,
                format: None,
                description: Some(format!("Installation session {}", id_value)),
            })
            .await
        {
            Ok(draft) => draft,
            Err(error) => {
                self.repository
                    .fail_preparation(&id_value, &error.to_string(), true)
                    .await?;
                return Err(error).context("failed to create the session draft image");
            }
        };
        let requested_bytes = size_gb
            .checked_mul(1024 * 1024 * 1024)
            .and_then(|bytes| i64::try_from(bytes).ok())
            .context("draft image size is too large")?;
        self.repository
            .set_draft(&id_value, &draft.id, requested_bytes)
            .await?;
        let bytes = match self.disk_bytes(&draft.id).await {
            Ok(bytes) => bytes,
            Err(error) => {
                self.repository
                    .fail_preparation(
                        &id_value,
                        "Draft storage could not be verified; recovery retains the draft",
                        false,
                    )
                    .await?;
                return Err(error).context("failed to verify session draft storage");
            }
        };
        if let Err(error) = self
            .repository
            .set_expected_bytes(&id_value, &draft.id, i64::try_from(bytes)?)
            .await
        {
            self.repository
                .fail_preparation(
                    &id_value,
                    "Draft size metadata could not be recorded; recovery retains the draft",
                    false,
                )
                .await?;
            return Err(error).context("failed to record session draft size");
        }
        let target = IscsiTargetSpec::with_luns(
            &target_iqn,
            vec![
                IscsiLunSpec::new(0, format!("install_{short}"), &draft.path),
                IscsiLunSpec::readonly_file(
                    1,
                    format!("install_media_{short}"),
                    media.managed_path,
                ),
            ],
        )?;
        let mut target = target;
        target.initiator_iqn = Some(crate::infrastructure::pxe::client_initiator_iqn(
            &client.name,
        ));
        target.chap = chap;
        let storage = self.state.application.storage.clone();
        let existing_target_iqn = target_iqn.clone();
        let target_preexisted = tokio::task::spawn_blocking(move || {
            storage.session_target_exists(&existing_target_iqn)
        })
        .await??;
        let storage = self.state.application.storage.clone();
        let owned_target = target.clone();
        if let Err(error) =
            tokio::task::spawn_blocking(move || storage.create_owned_session_target(&owned_target))
                .await?
        {
            let storage = self.state.application.storage.clone();
            let target_cleanup = if target_preexisted {
                Ok(())
            } else {
                tokio::task::spawn_blocking(move || {
                    storage.remove_owned_session_target(&target_iqn)
                })
                .await?
            };
            let cleanup = target_cleanup.is_ok()
                && self
                    .state
                    .application
                    .images
                    .delete_session_image(&draft.id, "draft")
                    .await
                    .is_ok();
            self.repository
                .fail_preparation(
                    &id_value,
                    "Session target creation failed; preparation rollback was attempted",
                    cleanup,
                )
                .await?;
            return Err(error).context("failed to create verified installer target");
        }
        let state = if kind == "manual" {
            "manual"
        } else {
            "waiting"
        };
        let session = self
            .repository
            .get(&id_value)
            .await?
            .context("prepared installation session disappeared")?;
        self.repository
            .transition(
                &session,
                state,
                if kind == "manual" { 0 } else { 5 },
                "Installer target is ready",
            )
            .await?;
        self.repository
            .get(&id_value)
            .await?
            .context("created installation session disappeared")
    }
    async fn client(&self, id: &str) -> Result<Client> {
        self.state
            .application
            .clients
            .get_by_string(id)
            .await?
            .context("client does not exist")
    }
    async fn validate_client(&self, c: &Client) -> Result<()> {
        ensure!(c.enabled, "client must be enabled");
        ensure!(
            c.chap_user.as_ref().map_or(true, |v| safe_word(v))
                && c.chap_secret.as_ref().map_or(true, |v| safe_word(v)),
            "invalid client CHAP credentials"
        );
        let pending:i64=sqlx::query_scalar("SELECT COUNT(*) FROM client_offline_resets WHERE client_id=? AND operation IS NOT NULL").bind(c.id.as_str()).fetch_one(&self.state.db_pool).await?;
        ensure!(pending == 0, "client has pending storage recovery");
        let legacy_exit: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM legacy_super_exits WHERE client_id=?")
                .bind(c.id.as_str())
                .fetch_one(&self.state.db_pool)
                .await?;
        ensure!(legacy_exit == 0, "client has pending legacy Super recovery");
        let identity_cache: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM client_identity_caches WHERE client_id=?")
                .bind(c.id.as_str())
                .fetch_one(&self.state.db_pool)
                .await?;
        ensure!(
            identity_cache == 0,
            "remove or restore the client identity cache before starting image factory"
        );
        if let Some(s) = self
            .state
            .application
            .super_sessions
            .get_for_client(c.id.as_str())
            .await?
        {
            ensure!(
                s.state == crate::persistence::SuperSessionState::Completed,
                "client has an unfinished Super session"
            );
        }
        Ok(())
    }
    async fn ensure_offline(&self, c: &Client) -> Result<()> {
        ensure!(
            !c.is_online(),
            "client must be powered off before preparing or closing installation"
        );
        let iqn = c.target_iqn.clone();
        let storage = self.state.application.storage.clone();
        let dataset = if c
            .snapshot
            .as_deref()
            .is_some_and(|snapshot| !snapshot.trim().is_empty())
        {
            c.writeback
                .as_deref()
                .filter(|dataset| !dataset.trim().is_empty())
                .unwrap_or(&c.master)
                .to_owned()
        } else {
            c.master.clone()
        };
        tokio::task::spawn_blocking(move || -> Result<()> {
            if let Some(iqn) = iqn.as_deref().filter(|iqn| !iqn.trim().is_empty()) {
                ensure!(
                    !crate::infrastructure::iscsi::target_has_active_sessions(iqn)?,
                    "client has active iSCSI sessions; power it off first"
                );
            }
            if !dataset.trim().is_empty() && dataset != "pending" {
                storage.validate_unmounted(&dataset)?;
            }
            Ok(())
        })
        .await?
    }
    /// The managed volume's virtual size is exactly what LIO exports for the LUN, so it
    /// identifies the installation disk without depending on a local device node.
    async fn disk_bytes(&self, image_id: &str) -> Result<u64> {
        let bytes = self
            .state
            .application
            .images
            .get_info(image_id)
            .await?
            .virtual_size;
        ensure!(
            bytes >= 8 * 1024 * 1024 * 1024,
            "installation disk must be at least 8 GiB"
        );
        Ok(bytes)
    }
    async fn prerequisites(
        &self,
        recipe: &str,
        c: &Client,
        media: &crate::persistence::InstallerMedia,
    ) -> Result<()> {
        use crate::persistence::InstallerMediaType;
        if recipe == "interactive" {
            return Ok(());
        }
        ensure!(
            c.pxe_mode == crate::domain::PxeMode::Uefi,
            "unattended recipes support UEFI amd64 only; use interactive mode for BIOS"
        );
        if recipe == "windows" {
            ensure!(
                media.media_type == InstallerMediaType::WindowsIso,
                "Windows unattended requires cataloged Windows ISO media"
            );
            let root =
                PathBuf::from(&self.state.settings.read().await.http.root_dir).join("boot/winpe");
            for name in [
                "wimboot",
                "BCD",
                "boot.sdi",
                "boot-installer.wim",
                "installer-v1.json",
            ] {
                ensure!(tokio::fs::metadata(root.join(name)).await.is_ok_and(|m|m.is_file() && m.len()>0),"dedicated WinPE installer prerequisite missing: {name}; see installer/BUILD-WINPE.md");
            }
            let manifest: serde_json::Value =
                serde_json::from_slice(&tokio::fs::read(root.join("installer-v1.json")).await?)?;
            ensure!(
                manifest["architecture"] == "amd64"
                    && manifest["powershell"] == true
                    && manifest["storage_wmi"] == true,
                "WinPE installer manifest must attest amd64, PowerShell and StorageWMI"
            );
            installation::member_size(&media.managed_path, "sources/install.wim").await?;
        } else {
            ensure!(
                media.media_type == InstallerMediaType::LinuxIso,
                "Linux unattended requires cataloged Linux ISO media"
            );
            for asset in ["vmlinuz", "initrd"] {
                installation::member_size(
                    &media.managed_path,
                    &installation::media_member(recipe, asset)
                        .context("unsupported Linux recipe")?,
                )
                .await?;
            }
            if recipe == "ubuntu" {
                let info = installation::member_text(&media.managed_path, ".disk/info").await?;
                ensure!(
                    info.contains("Ubuntu-Server 24.04") && info.contains("amd64"),
                    "Ubuntu recipe supports Ubuntu Server 24.04 amd64 live-server ISO only"
                );
            }
            if recipe == "rhel" {
                let info = installation::member_text(&media.managed_path, ".treeinfo").await?;
                let values = info
                    .lines()
                    .filter_map(|l| l.split_once('='))
                    .map(|(k, v)| (k.trim(), v.trim()))
                    .collect::<Vec<_>>();
                ensure!(
                    values.contains(&("arch", "x86_64"))
                        && values
                            .iter()
                            .any(|(k, v)| *k == "version" && (v.starts_with("9.") || *v == "9")),
                    "RHEL recipe supports RHEL 9 x86_64 full DVD installation media only"
                );
                for member in [".treeinfo", "images/install.img", "repodata/repomd.xml"] {
                    installation::member_size(&media.managed_path, member).await?;
                }
            }
        }
        Ok(())
    }
    /// Called under the caller's existing readiness/mutation guard, after target readiness.
    /// A pending recovery denies boot; returning None means no durable session owns the client.
    pub async fn boot_for_client(&self, id: &str) -> Result<Option<String>> {
        let Some(s) = self.repository.active(id).await? else {
            return Ok(None);
        };
        if matches!(s.state.as_str(), "published" | "completed") {
            return Ok(None);
        }
        if s.state == "saving" {
            return Ok(Some("#!ipxe\necho Image save is in progress. The server will retry this boot shortly.\nsleep 10\nreboot\n".to_owned()));
        }
        if s.state == "verifying" {
            let client = self.client(id).await?;
            let settings = self.state.settings.read().await;
            let server = settings.dhcp.next_server_ip.parse::<IpAddr>()?;
            let chap = if client.chap_enabled {
                format!(
                    "set username {}\nset password {}\n",
                    client.chap_user.as_deref().unwrap_or(""),
                    client.chap_secret.as_deref().unwrap_or("")
                )
            } else {
                String::new()
            };
            let target = s
                .verification_target_iqn
                .as_deref()
                .context("verification target is missing")?;
            return Ok(Some(format!(
                "#!ipxe\ndhcp\nset initiator-iqn {}\n{}set keep-san 1\nsanboot iscsi:{}:::0:{}\n",
                crate::infrastructure::pxe::client_initiator_iqn(&client.name),
                chap,
                server,
                target
            )));
        }
        if matches!(
            s.state.as_str(),
            "failed" | "cancelled" | "verification_failed" | "recovery_required"
        ) {
            return Ok(Some("#!ipxe\necho This installation session is stopped. An administrator must retry or close recovery.\nshell\n".to_owned()));
        }
        self.validate_session(&s, None).await?;
        let c = self.client(id).await?;
        let script = self.text(&s, &c, "boot").await?;
        if s.state == "waiting" {
            self.repository
                .transition(&s, "installing", 10, "Scoped installer boot dispatched")
                .await?;
        }
        Ok(Some(script))
    }
    async fn validate_session(&self, s: &InstallationSession, ip: Option<IpAddr>) -> Result<()> {
        ensure!(
            matches!(
                s.state.as_str(),
                "waiting" | "installing" | "adapting" | "manual"
            ),
            "installation is stopped; administrator recovery is required"
        );
        ensure!(
            chrono::DateTime::parse_from_rfc3339(&s.expires_at)? > Utc::now(),
            "installation boot credential expired; close recovery before retrying"
        );
        if let Some(ip) = ip {
            ensure!(
                s.source_ip.parse::<IpAddr>()? == ip,
                "installation source address does not match assigned client"
            );
        }
        let c = self.client(&s.client_id).await?;
        self.validate_client(&c).await?;
        ensure!(
            c.ip.to_string() == s.source_ip && c.pxe_mode.as_str() == s.firmware,
            "installation client boot identity changed; recovery required"
        );
        let media = self
            .state
            .application
            .installer_media
            .get(&s.media_id)
            .await?
            .context("installation media missing")?;
        ensure!(
            media.checksum == s.media_checksum,
            "installation media revision changed"
        );
        ensure!(
            self.disk_bytes(
                s.draft_image_id
                    .as_deref()
                    .context("installation draft is missing")?
            )
            .await?
                == s.expected_bytes
                    .context("installation has no expected disk size")? as u64,
            "installation disk size changed"
        );
        Ok(())
    }
    /// Scope checks precede opening any file, rendering credentials, or accepting progress.
    pub async fn authorize(
        &self,
        id: &str,
        token: &str,
        ip: IpAddr,
    ) -> Result<InstallationSession> {
        let s = self
            .repository
            .get(id)
            .await?
            .context("installation session does not exist")?;
        ensure!(
            constant_token_eq(&s.token, token),
            "invalid installation credential"
        );
        self.validate_session(&s, Some(ip)).await?;
        Ok(s)
    }
    pub async fn report(
        &self,
        id: &str,
        token: &str,
        ip: IpAddr,
        r: InstallationReport,
    ) -> Result<()> {
        let first = self
            .repository
            .get(id)
            .await?
            .context("installation session does not exist")?;
        if first.state == "saving" && matches!(r.state.as_str(), "installed" | "installing")
            || first.state == "adapting" && r.state == "installing"
        {
            ensure!(
                constant_token_eq(&first.token, token) && first.source_ip.parse::<IpAddr>()? == ip,
                "invalid installation credential"
            );
            return Ok(());
        }
        let _guard = self
            .state
            .client_mutations
            .lock_client(&first.client_id)
            .await;
        let s = self.authorize(id, token, ip).await?;
        if r.state == "installed" {
            ensure!(
                s.kind == "factory" && matches!(s.state.as_str(), "installing" | "adapting"),
                "installation is not ready to save"
            );
            self.repository
                .transition(
                    &s,
                    "saving",
                    s.progress.max(88),
                    "Installer acknowledged completed installation and adaptation",
                )
                .await?;
            let service = self.clone();
            let session_id = id.to_owned();
            tokio::spawn(async move {
                if service.save_after_installer(&session_id).await.is_err() {
                    tracing::error!(
                        session_id,
                        "failed to save installation draft for verification"
                    );
                }
            });
            return Ok(());
        }
        ensure!(
            matches!(r.state.as_str(), "installing" | "adapting" | "failed"),
            "unsupported installer report state"
        );
        // Client messages are untrusted and never logged or reflected as diagnostics outside this session.
        self.repository
            .transition(
                &s,
                &r.state,
                if r.state == "failed" {
                    r.progress.max(s.progress)
                } else {
                    r.progress
                },
                &r.message,
            )
            .await
    }

    async fn save_after_installer(&self, id: &str) -> Result<()> {
        let initial = self
            .repository
            .get(id)
            .await?
            .context("installation session does not exist")?;
        let _guard = self
            .state
            .client_mutations
            .lock_client_and_resource(&initial.client_id, "verified-image-factory")
            .await;
        let result = self.save_after_installer_locked(id).await;
        if result.is_err() {
            self.repository.record_save_error(id, "Draft save or verification-target preparation failed; resources remain reserved").await?;
        }
        result
    }

    async fn save_after_installer_locked(&self, id: &str) -> Result<()> {
        let mut session = self
            .repository
            .get(id)
            .await?
            .context("installation session does not exist")?;
        ensure!(
            session.state == "saving" && session.kind == "factory" && !session.legacy,
            "session is not an active factory save"
        );
        let deadline = session
            .phase_deadline
            .as_deref()
            .context("save deadline missing")?
            .parse::<chrono::DateTime<Utc>>()?;
        let draft_id = session
            .draft_image_id
            .as_deref()
            .context("draft image is missing")?;
        let draft = self.state.application.images.get(draft_id).await?;
        loop {
            ensure!(
                Utc::now() < deadline,
                "installer target did not disconnect before the save deadline"
            );
            let client = self.client(&session.client_id).await?;
            let target = session.target_iqn.clone();
            let active = tokio::task::spawn_blocking(move || {
                crate::infrastructure::iscsi::target_has_active_sessions(&target)
            })
            .await??;
            let storage = self.state.application.storage.clone();
            let dataset = draft.name.clone();
            let unmounted =
                tokio::task::spawn_blocking(move || storage.validate_unmounted(&dataset))
                    .await?
                    .is_ok();
            if !active && !client.is_online() && unmounted {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
        let short = session.id.replace('-', "");
        let snapshot_name = if let Some(snapshot) = session.verification_snapshot.clone() {
            snapshot
        } else {
            let snapshot = format!("factory-{}-{}", &short[..12], session.attempt + 1);
            self.state
                .application
                .images
                .create_draft_snapshot(&draft.id, &snapshot)
                .await?;
            self.repository
                .record_verification_snapshot(&session, &snapshot)
                .await?;
            session = self
                .repository
                .get(id)
                .await?
                .context("installation session disappeared during save")?;
            snapshot
        };
        let candidate = if let Some(candidate_id) = session.verification_image_id.as_deref() {
            self.state.application.images.get(candidate_id).await?
        } else {
            let candidate_name = format!("verify-{}-{}", &short[..12], session.attempt);
            let candidate = self
                .state
                .application
                .images
                .create_verification_candidate(&draft.id, &snapshot_name, &candidate_name)
                .await?;
            self.repository
                .record_verification_candidate(&session, &candidate.id)
                .await?;
            session = self
                .repository
                .get(id)
                .await?
                .context("installation session disappeared during save")?;
            candidate
        };
        let target_iqn = if let Some(target) = session.verification_target_iqn.clone() {
            target
        } else {
            let target = format!(
                "{}:verify.{}.{}",
                self.state.settings.read().await.iscsi.target_prefix,
                &short[..12],
                session.attempt
            );
            let storage = self.state.application.storage.clone();
            let check = target.clone();
            ensure!(
                !tokio::task::spawn_blocking(move || storage.session_target_exists(&check))
                    .await??,
                "verification target IQN is already in use"
            );
            self.repository
                .set_verification_target(&session, &target)
                .await?;
            session = self
                .repository
                .get(id)
                .await?
                .context("installation session disappeared during save")?;
            target
        };
        let client = self.client(&session.client_id).await?;
        let chap = if client.chap_enabled {
            Some(ChapCredentials {
                username: client
                    .chap_user
                    .clone()
                    .context("client CHAP username is missing")?,
                password: client
                    .chap_secret
                    .clone()
                    .context("client CHAP secret is missing")?,
            })
        } else {
            None
        };
        let mut target = IscsiTargetSpec::with_luns(
            &target_iqn,
            vec![IscsiLunSpec::new(
                0,
                format!("verify_{}_{}", &short[..12], session.attempt),
                &candidate.path,
            )],
        )?;
        target.initiator_iqn = Some(crate::infrastructure::pxe::client_initiator_iqn(
            &client.name,
        ));
        target.chap = chap;
        let storage = self.state.application.storage.clone();
        let target_iqn_check = target_iqn.clone();
        let target_exists =
            tokio::task::spawn_blocking(move || storage.session_target_exists(&target_iqn_check))
                .await??;
        if !target_exists {
            let storage = self.state.application.storage.clone();
            tokio::task::spawn_blocking(move || storage.create_owned_session_target(&target))
                .await??;
        } else {
            let storage = self.state.application.storage.clone();
            tokio::task::spawn_blocking(move || storage.verify_owned_session_target(&target))
                .await??;
        }
        session = self
            .repository
            .get(id)
            .await?
            .context("installation session disappeared during save")?;
        self.repository
            .transition(
                &session,
                "verifying",
                90,
                "Normal boot verification target is ready",
            )
            .await
    }
    pub async fn cancel(&self, id: &str) -> Result<()> {
        let s = self
            .repository
            .get(id)
            .await?
            .context("installation session does not exist")?;
        let _guard = self.state.client_mutations.lock_client(&s.client_id).await;
        let s = self
            .repository
            .get(id)
            .await?
            .context("installation session does not exist")?;
        self.repository.transition(&s,"cancelled",s.progress,"Boot credentials revoked; power off client before closing recovery. A running installer cannot be remotely stopped.").await
    }
    pub async fn retry(&self, id: &str) -> Result<()> {
        let s = self
            .repository
            .get(id)
            .await?
            .context("installation session does not exist")?;
        let _guard = self
            .state
            .client_mutations
            .lock_client_and_resource(&s.client_id, "verified-image-factory")
            .await;
        let s = self
            .repository
            .get(id)
            .await?
            .context("installation session does not exist")?;
        let client = self.client(&s.client_id).await?;
        self.ensure_offline(&client).await?;
        if s.state == "saving" {
            self.repository.refresh_save_deadline(id).await?;
        } else {
            ensure!(
                s.state == "verification_failed",
                "only save or verification failures can be retried"
            );
            if let Some(target) = s.verification_target_iqn.as_deref() {
                let target = target.to_owned();
                let storage = self.state.application.storage.clone();
                tokio::task::spawn_blocking(move || storage.remove_owned_session_target(&target))
                    .await??;
            }
            if let Some(candidate) = s.verification_image_id.as_deref() {
                self.state
                    .application
                    .images
                    .delete_session_image(candidate, "verifying")
                    .await?;
            }
            self.repository.retry_verification(&s).await?;
        }
        self.resume_saving(id.to_owned());
        Ok(())
    }
    pub async fn close_recovery(&self, id: &str) -> Result<()> {
        let service = self.clone();
        let id = id.to_owned();
        tokio::spawn(async move {
            let s = service.repository.get(&id).await?.context("installation session does not exist")?;
            if s.state == "published" && s.closed_at.is_none() {
                service.resume_published_cleanup(id);
                return Ok(());
            }
            let _guard = service.state.client_mutations.lock_client_and_resource(&s.client_id, "verified-image-factory").await;
            let s = service.repository.get(&id).await?.context("installation session does not exist")?;
            ensure!(matches!(s.state.as_str(), "manual" | "failed" | "cancelled" | "verification_failed" | "recovery_required"), "installation must be stopped before cleanup");
            let c = service.client(&s.client_id).await?;
            service.ensure_offline(&c).await?;
            if s.legacy {
                return service.repository.transition(&s, "completed", 100, "Legacy session archived after offline recovery; no storage cleanup was performed").await;
            }
            if let Some(target) = s.verification_target_iqn.as_deref() {
                let target = target.to_owned();
                let storage = service.state.application.storage.clone();
                if let Err(error) = tokio::task::spawn_blocking(move || storage.remove_owned_session_target(&target)).await? {
                    service.record_cleanup_failure(&s.id, "Verification target cleanup failed; resources remain reserved").await;
                    return Err(error).context("verification target cleanup failed");
                }
            }
            let target = s.target_iqn.clone();
            let storage = service.state.application.storage.clone();
            if let Err(error) = tokio::task::spawn_blocking(move || storage.remove_owned_session_target(&target)).await? {
                service.record_cleanup_failure(&s.id, "Session target cleanup failed; resources remain reserved").await;
                return Err(error).context("session target cleanup failed");
            }
            if let Some(candidate) = s.verification_image_id.as_deref() {
                if let Err(error) = service.state.application.images.delete_session_image(candidate, "verifying").await {
                    service.record_cleanup_failure(&s.id, "Verification image cleanup failed; resources remain reserved").await;
                    return Err(error).context("verification image cleanup failed");
                }
            }
            if let (Some(draft), Some(snapshot)) = (s.draft_image_id.as_deref(), s.verification_snapshot.as_deref()) {
                if let Err(error) = service.state.application.images.delete_session_snapshot(draft, snapshot).await {
                    service.record_cleanup_failure(&s.id, "Draft snapshot cleanup failed; resources remain reserved").await;
                    return Err(error).context("draft snapshot cleanup failed");
                }
            }
            if let Some(draft) = s.draft_image_id.as_deref() {
                let image = service.state.application.images.get(draft).await?;
                if image.status == "draft" {
                    if let Err(error) = service.state.application.images.delete_session_image(draft, "draft").await {
                        service.record_cleanup_failure(&s.id, "Draft cleanup failed; resources remain reserved").await;
                        return Err(error).context("draft image cleanup failed");
                    }
                }
            }
            service.remove_adaptation_bundle(&s.id).await;
            service.repository.transition(&s,"completed",100,"Administrator verified client offline and closed session; disk and assignment retained").await
        }).await?
    }
    async fn text(&self, s: &InstallationSession, c: &Client, asset: &str) -> Result<String> {
        let settings = self.state.settings.read().await.clone();
        let server = settings
            .dhcp
            .next_server_ip
            .parse::<IpAddr>()
            .context("boot server address must be an IP address")?
            .to_string();
        let base = settings.http.boot_base_url();
        let api = Self::installer_api_base(&settings);
        let config: serde_json::Value = serde_json::from_str(&s.config_json)?;
        let context = RenderContext {
            session: s,
            client: c,
            base: &base,
            api: &api,
            server: &server,
            username: config["username"].as_str().unwrap_or(""),
            password_hash: config["password_hash"].as_str().unwrap_or(""),
        };
        Ok(match asset {
            "boot"=>context.boot(),
            "startnet.cmd" if s.recipe=="windows"=>"@echo off\r\nwpeinit\r\npowershell.exe -NoProfile -ExecutionPolicy Bypass -File X:\\Windows\\System32\\install.ps1\r\n".to_owned(),
            "install.ps1" if s.recipe=="windows"=>context.windows_script(),
            "linux-finalize.sh" if matches!(s.recipe.as_str(), "ubuntu" | "rhel") => context.linux_finalize(),
            "disk-gate.sh" if matches!(s.recipe.as_str(),"ubuntu"|"rhel")=>context.linux_gate(),
            "user-data" if s.recipe=="ubuntu"=>context.ubuntu(),
            "meta-data" if s.recipe=="ubuntu"=>format!("instance-id: {}\nlocal-hostname: diskless-client\n",s.id),
            "ks.cfg" if s.recipe=="rhel"=>context.kickstart(),
            _=>anyhow::bail!("unsupported installation payload"),
        })
    }
    /// Bounded WinPE adaptation manifest: the server selects cataloged driver
    /// packages for the observed boot NIC and returns only the session-scoped
    /// bundle, boot-arm, and reporter configuration.
    pub async fn adaptation_manifest(
        &self,
        id: &str,
        token: &str,
        ip: IpAddr,
        request: AdaptationInventory,
    ) -> Result<serde_json::Value> {
        let session = self.authorize(id, token, ip).await?;
        ensure!(
            session.kind == "factory" && session.recipe == "windows",
            "adaptation manifest is only available for Windows factory sessions"
        );
        ensure!(
            matches!(session.state.as_str(), "installing" | "adapting"),
            "installation is not ready for adaptation"
        );
        ensure!(
            request.boot_nic_ids.len() <= 64,
            "too many boot NIC hardware IDs"
        );
        ensure!(
            request
                .boot_nic_mac
                .as_deref()
                .map_or(true, |mac| mac.len() <= 32
                    && !mac.chars().any(char::is_control)),
            "invalid boot NIC MAC address"
        );
        let boot_nic_ids =
            crate::infrastructure::pxe::normalize_hardware_ids(&request.boot_nic_ids)
                .map_err(anyhow::Error::msg)?;
        let root = self.state.settings.read().await.http.root_dir.clone();
        let bundle_root = self.driver_bundle_root();
        let session_id = session.id.clone();
        let (packages, bundle) = tokio::task::spawn_blocking(move || {
            let plugin = crate::infrastructure::pxe::NetworkDriverInjectionPlugin::new(root)
                .with_bundle_directory(bundle_root);
            let catalog = plugin.list()?;
            let target = crate::infrastructure::pxe::ImageDriverTarget {
                architecture: Some("amd64".to_owned()),
                boot_nic_ids,
                ..Default::default()
            };
            let packages =
                crate::infrastructure::pxe::select_compatible_for_image(&catalog, &target)
                    .into_iter()
                    .map(|selected| selected.driver_id)
                    .collect::<Vec<_>>();
            let bundle = if packages.is_empty() {
                None
            } else {
                Some(plugin.build_session_bundle(&session_id, &packages)?)
            };
            Ok::<_, anyhow::Error>((packages, bundle))
        })
        .await??;
        if packages.is_empty() {
            self.repository
                .record_event(
                    &session,
                    "No cataloged boot NIC driver matched; adaptation continues with in-box drivers",
                    "server",
                )
                .await?;
        }
        let settings = self.state.settings.read().await.clone();
        let base = Self::installer_api_base(&settings);
        Ok(json!({
            "driver_bundle_url": bundle.as_ref().map(|_| format!("{base}/api/installer/{id}/{token}/adaptation/drivers.zip")),
            "driver_bundle_sha256": bundle.as_ref().map(|bundle| bundle.sha256.clone()),
            "driver_bundle_bytes": bundle.as_ref().map(|bundle| bundle.bytes),
            "driver_packages": packages,
            "boot_arm": crate::infrastructure::pxe::WindowsBootArmConfig::default(),
            "reporter": {
                "server_url": base,
                "export_package": false,
                "installation_verification": true
            }
        }))
    }

    /// Serve the exact bundle the manifest advertised for this session.
    pub async fn adaptation_bundle(
        &self,
        id: &str,
        token: &str,
        ip: IpAddr,
    ) -> Result<axum::body::Body> {
        let session = self.authorize(id, token, ip).await?;
        ensure!(
            session.kind == "factory" && session.recipe == "windows",
            "adaptation bundle is only available for Windows factory sessions"
        );
        ensure!(
            matches!(session.state.as_str(), "installing" | "adapting"),
            "installation is not ready for adaptation"
        );
        let root = self.state.settings.read().await.http.root_dir.clone();
        let bundle_root = self.driver_bundle_root();
        let session_id = session.id.clone();
        let bundle = tokio::task::spawn_blocking(move || {
            crate::infrastructure::pxe::NetworkDriverInjectionPlugin::new(root)
                .with_bundle_directory(bundle_root)
                .existing_session_bundle(&session_id)
        })
        .await??
        .context("no driver bundle was selected for this installation")?;
        let file = tokio::fs::File::open(bundle.path).await?;
        let stream = futures::stream::try_unfold(file, |mut file| async move {
            use tokio::io::AsyncReadExt;
            let mut bytes = vec![0u8; 64 * 1024];
            let read = file.read(&mut bytes).await?;
            if read == 0 {
                return Ok::<_, std::io::Error>(None);
            }
            bytes.truncate(read);
            Ok(Some((bytes, file)))
        });
        Ok(axum::body::Body::from_stream(stream))
    }
    pub async fn payload(
        &self,
        id: &str,
        token: &str,
        ip: IpAddr,
        asset: &str,
    ) -> Result<axum::body::Body> {
        let s = self.authorize(id, token, ip).await?;
        let c = self.client(&s.client_id).await?;
        if asset == "media.iso" && s.recipe == "ubuntu" {
            let media = self
                .state
                .application
                .installer_media
                .get(&s.media_id)
                .await?
                .context("installation media missing")?;
            let file = tokio::fs::File::open(media.managed_path).await?;
            let stream = futures::stream::try_unfold(file, |mut file| async move {
                use tokio::io::AsyncReadExt;
                let mut bytes = vec![0u8; 64 * 1024];
                let n = file.read(&mut bytes).await?;
                if n == 0 {
                    return Ok::<_, std::io::Error>(None);
                }
                bytes.truncate(n);
                Ok(Some((bytes, file)))
            });
            return Ok(axum::body::Body::from_stream(stream));
        }
        if let Some(member) = installation::media_member(&s.recipe, asset) {
            let media = self
                .state
                .application
                .installer_media
                .get(&s.media_id)
                .await?
                .context("installation media missing")?;
            return installation::member_body(&media.managed_path, &member).await;
        }
        Ok(axum::body::Body::from(self.text(&s, &c, asset).await?))
    }
}
fn safe_word(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 240
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".:_-".contains(&c))
}
fn constant_token_eq(a: &str, b: &str) -> bool {
    if a.len() != 64 || b.len() != 64 {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |diff, (x, y)| diff | (x ^ y))
        == 0
}
fn validate_request(r: &CreateInstallation) -> Result<()> {
    ensure!(
        r.confirm_erase,
        "explicit disk erase confirmation is required"
    );
    if let Some(size_gb) = r.size_gb {
        ensure!(size_gb >= 8, "draft image must be at least 8 GiB");
    }
    if let Some(name) = r.image_name.as_deref() {
        crate::validation::validate_zfs_name(name)?;
    }
    if r.recipe == "interactive" {
        ensure!(
            !r.assign_on_ready,
            "manual sessions cannot publish or assign images"
        );
        ensure!(
            r.username.is_none() && r.password_hash.is_none(),
            "manual sessions do not accept unattended credentials"
        );
        ensure!(
            r.image_name.is_none(),
            "manual sessions do not accept a published image name"
        );
        return Ok(());
    }
    ensure!(
        matches!(
            r.recipe.as_str(),
            "interactive" | "windows" | "ubuntu" | "rhel"
        ),
        "unsupported installation recipe"
    );
    ensure!(
        r.image_name
            .as_deref()
            .is_some_and(|name| !name.trim().is_empty()),
        "factory recipe requires an image name"
    );
    if matches!(r.recipe.as_str(), "ubuntu" | "rhel") {
        let user = r
            .username
            .as_deref()
            .context("Linux recipe requires username")?;
        ensure!(
            !user.is_empty()
                && user.len() <= 32
                && user.bytes().enumerate().all(|(i, v)| v.is_ascii_lowercase()
                    || v == b'_'
                    || (i > 0 && (v.is_ascii_digit() || v == b'-'))),
            "invalid Linux username"
        );
        let hash = r
            .password_hash
            .as_deref()
            .context("Linux recipe requires a SHA-512 crypt password hash")?;
        ensure!(
            hash.starts_with("$6$")
                && hash.len() >= 90
                && hash.len() <= 200
                && hash
                    .bytes()
                    .all(|v| v.is_ascii_alphanumeric() || b"$./".contains(&v)),
            "password_hash must be a SHA-512 crypt hash, never a plaintext password"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_state() -> AppState {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::raw_sql(
            "INSERT INTO clients (id,name,mac,ip,master,enabled,created_at,updated_at)\n             VALUES ('client-a','Client A','02:00:00:00:00:01','192.0.2.10','pool/master',1,'now','now');\n             INSERT INTO images (id,name,os_type,size_gb,path,created_at,updated_at)\n             VALUES ('image-a','pool/master','windows',20,'/dev/zvol/pool/master','now','now');\n             INSERT INTO installer_media (id,display_name,media_type,managed_path,byte_size,checksum,validation_state,is_default,created_at,updated_at)\n             VALUES ('media-a','Media','windows_iso','/managed/media.iso',2048,'sha','valid',0,'now','now');",
        )
        .execute(&pool)
        .await
        .unwrap();
        AppState {
            config: crate::config::RuntimeConfig::default(),
            client_mutations: crate::state::ClientMutationLocks::default(),
            settings: std::sync::Arc::new(tokio::sync::RwLock::new(
                crate::core::config::Settings::default(),
            )),
            db_pool: pool.clone(),
            config_path: std::path::PathBuf::new(),
            client_ips: std::sync::Arc::new(tokio::sync::RwLock::new(Vec::new())),
            metrics_collector: std::sync::Arc::new(crate::metrics::MetricsCollector::default()),
            ssh_executor: std::sync::Arc::new(crate::ssh_executor::SshExecutor::new()),
            application: std::sync::Arc::new(crate::application::ApplicationServices::new(pool)),
        }
    }

    async fn session_row(state: &AppState, session_state: &str, expires_at: &str) {
        sqlx::query("INSERT INTO installation_sessions (id,client_id,kind,recipe,media_id,media_checksum,draft_image_id,size_gb,expected_bytes,prior_master,target_iqn,source_ip,firmware,token,config_json,state,progress,phase_deadline,created_at,updated_at) VALUES ('session-a','client-a','factory','windows','media-a','sha','image-a',20,1073741824,'pool/master','iqn.test:install','192.0.2.10','uefi','token','{}',?,0,?,'now','now')")
            .bind(session_state)
            .bind(expires_at)
            .execute(&state.db_pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn stopped_or_expired_sessions_deny_boot_instead_of_falling_back_to_the_generic_menu() {
        let state = test_state().await;
        session_row(&state, "failed", "2999-01-01T00:00:00Z").await;
        let script = InstallationService::new(state.clone())
            .boot_for_client("client-a")
            .await
            .unwrap()
            .expect("stopped sessions must return a denial script");
        assert!(script.contains("administrator must retry or close recovery"));
        sqlx::query(
            "UPDATE installation_sessions SET state='waiting', phase_deadline='2000-01-01T00:00:00Z'",
        )
        .execute(&state.db_pool)
        .await
        .unwrap();
        let error = InstallationService::new(state)
            .boot_for_client("client-a")
            .await
            .unwrap_err();
        assert!(format!("{error:#}").contains("expired"));
    }

    #[tokio::test]
    async fn expired_destructive_phase_revokes_credentials_and_keeps_recovery_reserved() {
        let state = test_state().await;
        session_row(&state, "installing", "2000-01-01T00:00:00Z").await;
        InstallationService::new(state.clone())
            .sweep_deadlines()
            .await
            .unwrap();
        let session = InstallationRepository::new(state.db_pool.clone())
            .get("session-a")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(session.state, "recovery_required");
        assert!(session.closed_at.is_none());
        assert!(session.token.is_empty());
    }

    #[test]
    fn requests_require_erasure_confirmation_and_do_not_accept_plaintext_or_yaml_injection() {
        let mut r = CreateInstallation {
            recipe: "ubuntu".into(),
            media_id: "media-a".into(),
            image_name: Some("factory-image".into()),
            size_gb: Some(20),
            assign_on_ready: false,
            confirm_erase: true,
            username: Some("operator".into()),
            password_hash: Some(format!("$6$salt${}", "a".repeat(86))),
        };
        assert!(validate_request(&r).is_ok());
        r.confirm_erase = false;
        assert!(validate_request(&r).is_err());
        r.confirm_erase = true;
        r.password_hash = Some("plaintext-password".into());
        assert!(validate_request(&r).is_err());
        r.password_hash = Some(format!("$6$salt${}", "a".repeat(86)));
        r.username = Some("operator\n  storage: {}".into());
        assert!(validate_request(&r).is_err());
        r.recipe = "unknown".into();
        assert!(validate_request(&r).is_err());
    }
    #[test]
    fn bounded_unguessable_tokens_compare_without_prefix_acceptance() {
        let token = "a".repeat(64);
        assert!(constant_token_eq(&token, &token));
        assert!(!constant_token_eq(&token, &"a".repeat(63)));
        assert!(!constant_token_eq(&token, &format!("{}b", "a".repeat(63))));
        assert!(!constant_token_eq("", ""));
    }
}
