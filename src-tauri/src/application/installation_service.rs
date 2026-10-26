//! Client-bound installation sessions. Partitioning is performed only by scoped, identity-gated boot payloads.
use crate::{
    domain::Client,
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
        let client = self.client(id).await?;
        let _guard = self
            .state
            .client_mutations
            .lock_client_and_resource(id, &format!("installation-master:{}", client.master))
            .await;
        let locked_master = client.master.clone();
        let client = self.client(id).await?;
        ensure!(
            client.master == locked_master,
            "client assignment changed while acquiring installation lock; retry"
        );
        self.validate_client(&client).await?;
        ensure!(
            self.repository.active(id).await?.is_none(),
            "client has an unfinished installation session; close recovery before retrying"
        );
        self.ensure_offline(&client).await?;
        let media = self
            .state
            .installer_media_service()
            .await
            .verify_integrity(
                client
                    .installer_media_id
                    .as_deref()
                    .context("client has no assigned installation media")?,
            )
            .await?;
        self.prerequisites(&request.recipe, &client, &media).await?;
        // Reuse readiness and both iSCSI backends rather than constructing a separate target.
        let spec = crate::core::reconciliation::storage_spec_for_client(
            &self.state.config,
            &self.state.application.storage,
            &self.state.db_pool,
            &client,
        )
        .await?
        .context("client has no storage configuration")?;
        let mut spec = spec;
        spec.installer_iso = Some(media.managed_path.clone());
        let storage = self.state.application.storage.clone();
        let dataset = client.master.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            storage.validate_unmounted(&dataset)?;
            storage.ensure_client_storage_for_boot(&spec)?;
            Ok(())
        })
        .await??;
        let client = self.client(id).await?;
        let image = self.state.application.images.get(&client.master).await?;
        let bytes = self.disk_bytes(&image.id).await?;
        let now = Utc::now();
        let session = InstallationSession {
            id: Uuid::new_v4().to_string(),
            client_id: id.to_owned(),
            media_id: media.id,
            image_id: image.id,
            master: client.master.clone(),
            media_checksum: media.checksum,
            target_iqn: client
                .target_iqn
                .clone()
                .context("missing client target IQN")?,
            expected_bytes: i64::try_from(bytes)?,
            source_ip: client.ip.to_string(),
            firmware: client.pxe_mode.as_str().to_owned(),
            recipe: request.recipe,
            token: format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()),
            config_json: json!({"username":request.username,"password_hash":request.password_hash})
                .to_string(),
            state: "ready".to_owned(),
            progress: 0,
            error: None,
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
            expires_at: (now + Duration::hours(24)).to_rfc3339(),
        };
        self.repository.insert(&session).await?;
        Ok(session)
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
        ensure!(c.enabled && c.installer_media_id.is_some() && c.snapshot.is_none() && c.writeback.is_none() && !c.use_game_disk && c.game_disks.is_empty() && !c.is_super_mode(),"installation requires an enabled, exclusive direct-master client without writeback, game disks or Super mode");
        ensure!(
            c.block_device.as_deref() == Some(format!("/dev/zvol/{}", c.master).as_str()),
            "installation block device differs from assigned master"
        );
        ensure!(
            c.target_iqn.as_ref().is_some_and(|v| safe_word(v)),
            "invalid installation target IQN"
        );
        ensure!(
            c.chap_user.as_ref().map_or(true, |v| safe_word(v))
                && c.chap_secret.as_ref().map_or(true, |v| safe_word(v)),
            "invalid client CHAP credentials"
        );
        crate::application::client_lifecycle::validate_boot_storage_owner(&self.state, c).await?;
        let others: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM clients WHERE master=? AND id<>?")
                .bind(&c.master)
                .bind(c.id.as_str())
                .fetch_one(&self.state.db_pool)
                .await?;
        ensure!(
            others == 0,
            "installation master is assigned to another client"
        );
        let pending:i64=sqlx::query_scalar("SELECT COUNT(*) FROM client_offline_resets WHERE client_id=? AND operation IS NOT NULL").bind(c.id.as_str()).fetch_one(&self.state.db_pool).await?;
        ensure!(pending == 0, "client has pending storage recovery");
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
        let iqn = c.target_iqn.clone().context("client has no target IQN")?;
        let storage = self.state.application.storage.clone();
        let dataset = c.master.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            ensure!(
                !crate::infrastructure::iscsi::target_has_active_sessions(&iqn)?,
                "client has active iSCSI sessions; power it off first"
            );
            storage.validate_unmounted(&dataset)?;
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
        self.validate_session(&s, None).await?;
        let c = self.client(id).await?;
        let script = self.text(&s, &c, "boot").await?;
        if s.state == "ready" {
            self.repository
                .transition(&s, "booting", 0, "Scoped installer boot dispatched")
                .await?;
        }
        Ok(Some(script))
    }
    async fn validate_session(&self, s: &InstallationSession, ip: Option<IpAddr>) -> Result<()> {
        ensure!(
            matches!(s.state.as_str(), "ready" | "booting" | "installing"),
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
            c.master == s.master
                && c.installer_media_id.as_deref() == Some(s.media_id.as_str())
                && c.target_iqn.as_deref() == Some(s.target_iqn.as_str())
                && c.ip.to_string() == s.source_ip
                && c.pxe_mode.as_str() == s.firmware,
            "installation client assignment changed; recovery required"
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
            self.disk_bytes(&s.image_id).await? == s.expected_bytes as u64,
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
        let _guard = self
            .state
            .client_mutations
            .lock_client(&first.client_id)
            .await;
        let s = self.authorize(id, token, ip).await?;
        ensure!(matches!(r.state.as_str(),"installing"|"failed"),"boot reports may only record installing or failed; completion requires offline administrator validation");
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
    pub async fn close_recovery(&self, id: &str) -> Result<()> {
        let service = self.clone();
        let id = id.to_owned();
        tokio::spawn(async move {
            let s=service.repository.get(&id).await?.context("installation session does not exist")?;
            let _guard=service.state.client_mutations.lock_client_and_resource(&s.client_id,&format!("installation-master:{}",s.master)).await;
            let s=service.repository.get(&id).await?.context("installation session does not exist")?;
            ensure!(matches!(s.state.as_str(),"installing"|"failed"|"cancelled"|"recovery_required"),"installation must have started or stopped before offline completion");
            let c=service.client(&s.client_id).await?;
            ensure!(c.master==s.master && c.target_iqn.as_deref()==Some(s.target_iqn.as_str()),"client target changed; retain recovery and repair assignment first");
            service.ensure_offline(&c).await?;
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
        let base = format!(
            "{}://{}:{}",
            if settings.http.tls_enabled {
                "https"
            } else {
                "http"
            },
            settings.http.server_ip,
            settings.http.port
        );
        let config: serde_json::Value = serde_json::from_str(&s.config_json)?;
        let context = RenderContext {
            session: s,
            client: c,
            base: &base,
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
    ensure!(
        matches!(
            r.recipe.as_str(),
            "interactive" | "windows" | "ubuntu" | "rhel"
        ),
        "unsupported installation recipe"
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
        sqlx::query("INSERT INTO installation_sessions (id,client_id,media_id,image_id,master,media_checksum,target_iqn,expected_bytes,source_ip,firmware,recipe,token,config_json,state,progress,created_at,updated_at,expires_at) VALUES ('session-a','client-a','media-a','image-a','pool/master','sha','iqn.test:client-a',1073741824,'192.0.2.10','uefi','windows','token','{}',?,0,'now','now',?)")
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
        let error = InstallationService::new(state.clone())
            .boot_for_client("client-a")
            .await
            .unwrap_err();
        assert!(format!("{error:#}").contains("administrator recovery"));
        sqlx::query(
            "UPDATE installation_sessions SET state='ready', expires_at='2000-01-01T00:00:00Z'",
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

    #[test]
    fn requests_require_erasure_confirmation_and_do_not_accept_plaintext_or_yaml_injection() {
        let mut r = CreateInstallation {
            recipe: "ubuntu".into(),
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
