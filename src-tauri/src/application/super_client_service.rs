use crate::{
    application::{
        image_service::ImageService, installer_media_service::InstallerMediaService, StorageService,
    },
    config::RuntimeConfig,
    core::{config::Settings, image::ImageKind},
    domain::{
        client::{BootMode, Client, ClientId, ClientStatus},
        storage::{ClientStorageSpec, StorageSource},
    },
    infrastructure::iscsi::{ChapCredentials, IscsiProvisioner},
    persistence::{
        ClientRepository, InstallerMedia, InstallerMediaRepository, SuperSession,
        SuperSessionRepository, SuperSessionState,
    },
    state::ClientMutationLocks,
};
use anyhow::{bail, Context, Result};
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

/// Injectable worker boundary: success requires native checks and confirmed disk release.
trait VolumeSafetyGuard: Send + Sync {
    fn check<'a>(
        &'a self,
        profile: crate::core::config::WindowsServicingConfig,
        connection: crate::infrastructure::pxe::windows_iscsi_servicing::WindowsIscsiConnection,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>>;
}
struct RemoteVolumeSafetyGuard;
impl VolumeSafetyGuard for RemoteVolumeSafetyGuard {
    fn check<'a>(
        &'a self,
        profile: crate::core::config::WindowsServicingConfig,
        connection: crate::infrastructure::pxe::windows_iscsi_servicing::WindowsIscsiConnection,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            crate::infrastructure::pxe::RemoteWindowsServicer::new()
                .check_volume(
                    crate::infrastructure::pxe::RemoteWindowsCapabilitiesRequest {
                        host: profile.worker_host,
                        username: profile.worker_username,
                        password: None,
                        executable_path: Some(profile.helper_executable_path),
                    },
                    connection,
                )
                .await?;
            Ok(())
        })
    }
}

/// Coordinates clone-backed Super sessions. Normal sessions edit an isolated
/// working clone and never depend on installer media; verification-only sessions
/// additionally attach a read-only installer ISO for media boot testing.
#[derive(Clone)]
pub struct SuperClientService {
    clients: ClientRepository,
    media: InstallerMediaRepository,
    sessions: SuperSessionRepository,
    images: ImageService,
    storage: Arc<StorageService>,
    iscsi: Arc<dyn IscsiProvisioner>,
    config: RuntimeConfig,
    settings: Settings,
    media_service: InstallerMediaService,
    mutation_locks: ClientMutationLocks,
    volume_safety: Arc<dyn VolumeSafetyGuard>,
}

/// Dependencies required to construct a [`SuperClientService`].
pub struct SuperClientServiceDeps {
    pub clients: ClientRepository,
    pub media: InstallerMediaRepository,
    pub sessions: SuperSessionRepository,
    pub images: ImageService,
    pub storage: Arc<StorageService>,
    pub iscsi: Arc<dyn IscsiProvisioner>,
    pub config: RuntimeConfig,
    pub settings: Settings,
    pub media_service: InstallerMediaService,
    pub mutation_locks: ClientMutationLocks,
}

impl SuperClientService {
    pub fn new(deps: SuperClientServiceDeps) -> Self {
        Self {
            clients: deps.clients,
            media: deps.media,
            sessions: deps.sessions,
            images: deps.images,
            storage: deps.storage,
            iscsi: deps.iscsi,
            config: deps.config,
            settings: deps.settings,
            media_service: deps.media_service,
            mutation_locks: deps.mutation_locks,
            volume_safety: Arc::new(RemoteVolumeSafetyGuard),
        }
    }

    /// Start an installer session, or an explicitly confirmed disposable verification session.
    pub async fn enter(
        &self,
        client_id: &str,
        verification_only: bool,
        confirmed: bool,
    ) -> Result<SuperSession> {
        if verification_only && !confirmed {
            bail!("verification-only sessions require explicit confirmation");
        }
        let guard = self.mutation_locks.lock_client(client_id).await;
        let service = self.clone();
        let client_id = client_id.to_owned();
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            let _guard = guard;
            runtime.block_on(async move {
                let session = service.enter_locked(&client_id, verification_only).await?;
                service
                    .restore_custom_menu_backup(&session.client_id)
                    .await?;
                Ok(session)
            })
        })
        .await
        .context("Super-session worker failed")?
    }

    pub async fn save(&self, client_id: &str) -> Result<()> {
        let guard = self.mutation_locks.lock_client(client_id).await;
        let service = self.clone();
        let operation_client_id = client_id.to_owned();
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            let _guard = guard;
            runtime.block_on(async move {
                service.finish_locked(&operation_client_id, true).await?;
                service
                    .restore_custom_menu_backup(&operation_client_id)
                    .await
            })
        })
        .await
        .context("Super-session save worker failed")?
    }

    pub async fn discard(&self, client_id: &str) -> Result<()> {
        let guard = self.mutation_locks.lock_client(client_id).await;
        let service = self.clone();
        let operation_client_id = client_id.to_owned();
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            let _guard = guard;
            runtime.block_on(async move {
                service.finish_locked(&operation_client_id, false).await?;
                service
                    .restore_custom_menu_backup(&operation_client_id)
                    .await
            })
        })
        .await
        .context("Super-session discard worker failed")?
    }

    /// Explicitly exit an unmanaged legacy Super client into the chosen image snapshot.
    pub async fn exit_legacy(&self, client_id: &str, selected_snapshot: &str) -> Result<Client> {
        let guard = self.mutation_locks.lock_client(client_id).await;
        let service = self.clone();
        let client_id = client_id.to_owned();
        let selected_snapshot = selected_snapshot.to_owned();
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            let _guard = guard;
            runtime.block_on(async move {
                let client = service
                    .exit_legacy_locked(&client_id, &selected_snapshot)
                    .await?;
                service.restore_custom_menu_backup(&client_id).await?;
                Ok(client)
            })
        })
        .await
        .context("legacy Super exit worker failed")?
    }

    async fn exit_legacy_locked(&self, client_id: &str, selected_snapshot: &str) -> Result<Client> {
        self.validate_no_conflicting_disk_owner(client_id).await?;
        let id = ClientId::from_string(client_id.to_owned()).map_err(anyhow::Error::from)?;
        let client = self
            .clients
            .find_by_id(&id)
            .await?
            .context("client does not exist")?;
        anyhow::ensure!(
            client.status != ClientStatus::Online,
            "client must be offline before legacy Super exit"
        );
        let target = client
            .target_iqn
            .as_deref()
            .context("client has no persisted target")?;
        anyhow::ensure!(
            self.iscsi
                .active_session_peer_ips(target, &client.name)?
                .is_empty(),
            "legacy Super exit requires no active iSCSI sessions"
        );
        let pending_storage_recovery: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM client_offline_resets WHERE client_id = ? AND operation IS NOT NULL",
        )
        .bind(client_id)
        .fetch_one(self.sessions.pool())
        .await?;
        anyhow::ensure!(
            pending_storage_recovery == 0,
            "client storage recovery is pending; retry that recovery first"
        );
        let pending: Option<(String,)> =
            sqlx::query_as("SELECT selected_snapshot FROM legacy_super_exits WHERE client_id = ?")
                .bind(client_id)
                .fetch_optional(self.sessions.pool())
                .await?;
        if client.mode != BootMode::Super {
            if let Some((chosen,)) = pending {
                anyhow::ensure!(
                    client.snapshot.as_deref() == Some(chosen.as_str())
                        && selected_snapshot == chosen,
                    "a different legacy exit is pending; retry it with its recorded snapshot"
                );
                let spec = self.normal_spec(&client).await?;
                let storage_state = self.storage.reconcile_client_storage(&spec)?;
                if storage_state.zfs_present {
                    self.storage.validate_unmounted(&spec.dataset)?;
                    anyhow::ensure!(
                        self.storage.clone_origin(&spec.dataset)?.as_deref()
                            == Some(chosen.as_str()),
                        "normal client clone does not match the recorded legacy-exit snapshot"
                    );
                }
                self.storage.ensure_client_storage_for_boot(&spec)?;
                let storage_state = self.storage.reconcile_client_storage(&spec)?;
                anyhow::ensure!(
                    storage_state.zfs_present && storage_state.iscsi_present,
                    "normal client storage is not ready for legacy-exit recovery"
                );
                anyhow::ensure!(
                    self.storage.clone_origin(&spec.dataset)?.as_deref() == Some(chosen.as_str()),
                    "recovered client clone does not match the recorded legacy-exit snapshot"
                );
                sqlx::query("DELETE FROM legacy_super_exits WHERE client_id = ?")
                    .bind(client_id)
                    .execute(self.sessions.pool())
                    .await?;
                return Ok(client);
            }
            bail!("client is not in legacy Super mode");
        }
        anyhow::ensure!(
            self.sessions
                .get_for_client(client_id)
                .await?
                .map_or(true, |session| session.state
                    == SuperSessionState::Completed),
            "client has a managed Super session; finish it through the Super-session controls"
        );
        anyhow::ensure!(
            selected_snapshot.starts_with(&format!("{}@", client.master)),
            "selected snapshot must belong to the client's assigned image"
        );
        self.storage
            .validate_unmounted(&client.master)
            .context("legacy Super master must be unmounted")?;
        let image = self.images.get(&client.master).await?;
        let snapshot_name = selected_snapshot
            .strip_prefix(&format!("{}@", client.master))
            .context("selected snapshot must belong to the client's assigned image")?;
        anyhow::ensure!(
            self.images
                .snapshots(&image.id)
                .await?
                .iter()
                .any(|snapshot| snapshot.name == snapshot_name),
            "selected snapshot does not exist"
        );

        let now = chrono::Utc::now().to_rfc3339();
        let prior_snapshot = client.snapshot.clone();
        if let Some((chosen,)) = pending.as_ref() {
            anyhow::ensure!(
                chosen == selected_snapshot,
                "a different legacy exit is already pending"
            );
        } else {
            sqlx::query("INSERT INTO legacy_super_exits(client_id, prior_master, prior_snapshot, selected_snapshot, state, last_error, created_at, updated_at) VALUES (?, ?, ?, ?, 'pending', NULL, ?, ?)")
                .bind(client_id).bind(&client.master).bind(&prior_snapshot).bind(selected_snapshot).bind(&now).bind(&now)
                .execute(self.sessions.pool()).await.context("failed to persist legacy-exit recovery record")?;
        }
        let mut normal = client.clone();
        normal.mode = BootMode::Normal;
        normal.snapshot = Some(selected_snapshot.to_owned());
        let spec = self.normal_spec(&normal).await?;
        let backstore = client
            .block_store
            .as_deref()
            .context("legacy client has no backstore")?;
        let storage = match self.storage.exit_legacy_super(
            target,
            backstore,
            &spec,
            selected_snapshot,
        ) {
            Ok(storage) => storage,
            Err(error) => {
                sqlx::query("UPDATE legacy_super_exits SET state = 'recovery_required', last_error = ?, updated_at = ? WHERE client_id = ?")
                    .bind(format!("{error:#}")).bind(chrono::Utc::now().to_rfc3339()).bind(client_id)
                    .execute(self.sessions.pool()).await?;
                return Err(error).context("legacy Super exit failed; recovery record retained");
            }
        };
        normal.block_device = Some(storage.volume.block_device.to_string_lossy().into_owned());
        normal.block_store = Some(storage.volume.backstore.clone());
        normal.target_iqn = Some(storage.volume.target_iqn.clone());
        normal.writeback = Some(storage.volume.dataset.clone());
        normal.updated_at = chrono::Utc::now();
        self.clients.update(&normal).await?;
        sqlx::query("DELETE FROM legacy_super_exits WHERE client_id = ?")
            .bind(client_id)
            .execute(self.sessions.pool())
            .await
            .context("normal assignment is ready but legacy-exit journal cleanup failed")?;
        Ok(normal)
    }

    /// Reconcile durable sessions after startup or on an explicit retry.
    pub async fn recover_incomplete(&self) -> Result<()> {
        let mut failures = Vec::new();
        for session in self.sessions.list_incomplete().await? {
            let client_id = session.client_id.clone();
            let guard = self.mutation_locks.lock_client(&session.client_id).await;
            let service = self.clone();
            let runtime = tokio::runtime::Handle::current();
            let result = tokio::task::spawn_blocking(move || {
                let _guard = guard;
                runtime.block_on(async move {
                    let recovery = service.recover_one(session).await;
                    let backup = service.restore_custom_menu_backup(&client_id).await;
                    match (recovery, backup) {
                        (Ok(()), Ok(())) => Ok(()),
                        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
                        (Err(error), Err(backup_error)) => Err(error).context(format!(
                            "custom menu restoration after recovery also failed: {backup_error:#}"
                        )),
                    }
                })
            })
            .await
            .context("Super-session recovery worker failed")?;
            if let Err(error) = result {
                failures.push(format!("{error:#}"));
            }
        }
        let legacy_exits: Vec<(String, String)> = sqlx::query_as(
            "SELECT client_id, selected_snapshot FROM legacy_super_exits ORDER BY created_at, client_id",
        )
        .fetch_all(self.sessions.pool())
        .await?;
        for (client_id, selected_snapshot) in legacy_exits {
            let guard = self.mutation_locks.lock_client(&client_id).await;
            let service = self.clone();
            let runtime = tokio::runtime::Handle::current();
            let recovery_client_id = client_id.clone();
            let result = tokio::task::spawn_blocking(move || {
                let _guard = guard;
                runtime.block_on(async move {
                    service
                        .exit_legacy_locked(&client_id, &selected_snapshot)
                        .await?;
                    service.restore_custom_menu_backup(&client_id).await
                })
            })
            .await
            .context("legacy Super exit recovery worker failed")?;
            if let Err(error) = result {
                tracing::warn!(client_id = %recovery_client_id, %error, "legacy Super exit recovery or custom menu restoration failed");
            }
        }
        let backed_up_menus: Vec<String> =
            sqlx::query_scalar("SELECT client_id FROM client_boot_menu_backups ORDER BY client_id")
                .fetch_all(self.sessions.pool())
                .await?;
        for client_id in backed_up_menus {
            let guard = self.mutation_locks.lock_client(&client_id).await;
            let service = self.clone();
            let runtime = tokio::runtime::Handle::current();
            let menu_client_id = client_id.clone();
            let result = tokio::task::spawn_blocking(move || {
                let _guard = guard;
                runtime.block_on(service.restore_custom_menu_backup(&client_id))
            })
            .await
            .context("custom menu backup recovery worker failed")?;
            if let Err(error) = result {
                failures.push(format!(
                    "client {menu_client_id} custom menu backup recovery failed: {error:#}"
                ));
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            bail!(
                "{} Super-session recovery operation(s) remain pending: {}",
                failures.len(),
                failures.join("; ")
            )
        }
    }

    pub async fn restore_custom_menu_backup(&self, client_id: &str) -> Result<()> {
        let id = ClientId::from_string(client_id.to_owned()).map_err(anyhow::Error::from)?;
        let client = self
            .clients
            .find_by_id(&id)
            .await?
            .context("session client disappeared while restoring its custom menu")?;
        let session = self.sessions.get_for_client(client_id).await?;
        if session
            .as_ref()
            .is_some_and(|session| session.state != SuperSessionState::Completed)
        {
            return Ok(());
        }
        crate::application::client_boot::restore_legacy_client_menu_backup(
            &self.clients,
            &client,
            &self.settings,
        )
        .await
    }

    async fn recover_one(&self, mut session: SuperSession) -> Result<()> {
        self.validate_no_conflicting_disk_owner(&session.client_id)
            .await?;
        if session.state == SuperSessionState::RecoveryRequired {
            let action = session
                .resume_state
                .context("recovery record has no resumable operation")?;
            anyhow::ensure!(
                self.sessions
                    .transition_state(&session.id, SuperSessionState::RecoveryRequired, action)
                    .await?,
                "recovery state changed"
            );
            session.state = action;
        }
        let client_id =
            ClientId::from_string(session.client_id.clone()).map_err(anyhow::Error::from)?;
        let client = self
            .clients
            .find_by_id(&client_id)
            .await?
            .context("session client was deleted")?;
        if session.state != SuperSessionState::Active {
            anyhow::ensure!(
                client.status == ClientStatus::Offline,
                "cannot recover Super session while client is online"
            );
        }
        let prior_image = self.images.get(&session.prior_image_id).await?;
        let (working_dataset, _) =
            session_working_clone_path(&prior_image.name, &session.working_clone)?;
        if session.working_clone != working_dataset {
            anyhow::ensure!(
                self.sessions
                    .set_working_clone(&session.id, session.state, &working_dataset)
                    .await?,
                "Super-session state changed while normalizing its working clone"
            );
            session.working_clone = working_dataset;
        }
        let result = match session.state {
            SuperSessionState::Preparing => {
                let prepared = async {
                    self.validate_prior_storage_safe(&client, &session).await?;
                    let media_id = session.installer_media_id.as_deref();
                    anyhow::ensure!(
                        !session.verification_only || media_id.is_some(),
                        "verification session installer media is missing"
                    );
                    if let Some(media_id) = media_id {
                        self.media_service.verify_integrity(media_id).await?;
                    }
                    let snapshot = session
                        .working_origin_snapshot
                        .as_deref()
                        .and_then(|v| v.rsplit_once('@').map(|(_, n)| n))
                        .context("session origin snapshot is missing")?;
                    self.prepare_storage(
                        &client,
                        &session.prior_image_id,
                        snapshot,
                        &session.working_clone,
                        media_id,
                    )
                    .await
                }
                .await;
                match prepared {
                    Ok(()) => {
                        anyhow::ensure!(
                            self.sessions
                                .activate_with_client_reservation(&session.id, &session.client_id)
                                .await?,
                            "session state changed during recovery"
                        );
                        return Ok(());
                    }
                    Err(error) => Err(error),
                }
            }
            SuperSessionState::Active => {
                self.verify_active_session(&client, &session).await?;
                anyhow::ensure!(
                    self.sessions
                        .set_client_boot_mode(&session.client_id, "super")
                        .await?,
                    "session client disappeared during recovery"
                );
                return Ok(());
            }
            SuperSessionState::Saving | SuperSessionState::Discarding => {
                let save = session.state == SuperSessionState::Saving;
                self.finish_storage(&session.client_id, &session, save)
                    .await
            }
            SuperSessionState::RecoveryRequired | SuperSessionState::Completed => Ok(()),
        };
        match result {
            Ok(()) => {
                let expected = session.state;
                self.sessions
                    .transition_state(&session.id, expected, SuperSessionState::Completed)
                    .await?;
                Ok(())
            }
            Err(error) => {
                match self
                    .sessions
                    .transition_state(
                        &session.id,
                        session.state,
                        SuperSessionState::RecoveryRequired,
                    )
                    .await
                {
                    Ok(true) => Err(error)
                        .context("Super-session recovery failed; durable record retained"),
                    Ok(false) => Err(error).context(
                        "Super-session recovery failed and its recovery state changed concurrently",
                    ),
                    Err(recovery_error) => Err(error).context(format!(
                        "Super-session recovery failed and could not persist recovery state: {recovery_error:#}"
                    )),
                }
            }
        }
    }

    async fn verify_active_session(&self, client: &Client, session: &SuperSession) -> Result<()> {
        let media = match session.installer_media_id.as_deref() {
            Some(media_id) => Some(self.media_service.verify_integrity(media_id).await?),
            None => None,
        };
        self.verify_active_session_target(client, session, media.as_ref())
            .await
    }

    /// Validate a verification session against the real LUN and backing-resource state.
    pub async fn verify_boot_verification_target(
        &self,
        client: &Client,
        session: &SuperSession,
        media: &InstallerMedia,
    ) -> Result<()> {
        anyhow::ensure!(
            session.state == SuperSessionState::Active
                && session.verification_only
                && session.client_id == client.id.as_ref()
                && session.installer_media_id.as_deref() == Some(media.id.as_str()),
            "verification session does not match the selected media"
        );
        let image = self.images.get(&client.master).await?;
        anyhow::ensure!(
            client.mode == BootMode::Super
                && session.prior_image_id == image.id
                && session.prior_snapshot.as_deref() == client.snapshot.as_deref(),
            "verification session does not match the current client assignment"
        );
        self.verify_active_session_target(client, session, Some(media))
            .await
    }

    async fn verify_active_session_target(
        &self,
        client: &Client,
        session: &SuperSession,
        media: Option<&InstallerMedia>,
    ) -> Result<()> {
        let mut spec = self.normal_spec(client).await?;
        spec.dataset = session.working_clone.clone();
        spec.source = StorageSource::ExistingVolume(session.working_clone.clone());
        spec.backstore = StorageService::super_session_working_backstore(&session.working_clone);
        spec.use_game_disk = false;
        spec.game_disks.clear();
        let installer_iso = media.map(|media| media.managed_path.clone());
        let desired = StorageService::build_super_session_iscsi_spec(&spec, installer_iso)?;
        self.storage.verify_super_session_target(&desired)?;
        Ok(())
    }

    async fn prior_assignment(&self, client: &Client, session: &SuperSession) -> Result<Client> {
        let image = self.images.get(&session.prior_image_id).await?;
        let mut prior = client.clone();
        prior.master = image.name;
        prior.snapshot = session.prior_snapshot.clone();
        Ok(prior)
    }

    async fn validate_prior_storage_safe(
        &self,
        client: &Client,
        session: &SuperSession,
    ) -> Result<()> {
        let target = client
            .target_iqn
            .as_deref()
            .context("client has no persisted iSCSI target")?;
        anyhow::ensure!(
            self.iscsi
                .active_session_peer_ips(target, &client.name)?
                .is_empty(),
            "client has an active iSCSI session"
        );
        let prior = self.prior_assignment(client, session).await?;
        let prior_storage = self.normal_spec(&prior).await?;
        self.storage
            .validate_unmounted(&prior_storage.dataset)
            .context("prior assigned client storage must be unmounted before recovery")
    }

    async fn validate_no_conflicting_disk_owner(&self, client_id: &str) -> Result<()> {
        let installation: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM installation_sessions WHERE client_id = ? AND state <> 'completed')")
            .bind(client_id).fetch_one(self.sessions.pool()).await?;
        anyhow::ensure!(!installation, "unfinished installation owns this client's disk; finish installer recovery before changing Super mode");
        let identity: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM client_identity_caches WHERE client_id = ?)",
        )
        .bind(client_id)
        .fetch_one(self.sessions.pool())
        .await?;
        anyhow::ensure!(
            !identity,
            "remove or restore the client identity cache before changing Super mode"
        );
        Ok(())
    }

    async fn enter_locked(&self, client_id: &str, verification_only: bool) -> Result<SuperSession> {
        self.validate_no_conflicting_disk_owner(client_id).await?;
        let id = ClientId::from_string(client_id.to_owned()).map_err(anyhow::Error::from)?;
        let client = self
            .clients
            .find_by_id(&id)
            .await?
            .context("client does not exist")?;
        anyhow::ensure!(
            client.enabled && client.status == ClientStatus::Offline,
            "client must be enabled and offline"
        );
        anyhow::ensure!(
            client.mode == BootMode::Normal,
            "client is already in Super mode"
        );
        let target_iqn = client
            .target_iqn
            .as_deref()
            .context("client has no persisted iSCSI target")?;
        anyhow::ensure!(
            self.iscsi
                .active_session_peer_ips(target_iqn, &client.name)?
                .is_empty(),
            "client has an active iSCSI session"
        );
        if let Some(existing) = self.sessions.get_for_client(client_id).await? {
            anyhow::ensure!(
                existing.state == SuperSessionState::Completed,
                "client already has an incomplete Super-session operation"
            );
        }
        let prior_storage = self.normal_spec(&client).await?;
        self.storage
            .validate_unmounted(&prior_storage.dataset)
            .context("assigned client storage must be unmounted before Super mode")?;
        anyhow::ensure!(
            !self.clients.has_other_storage_owner(&client).await?,
            "client storage is shared with another client"
        );
        self.storage
            .validate_client_target_for_replacement(&prior_storage)
            .context("assigned client target is not safe to replace")?;

        let image = self.images.get(&client.master).await?;
        anyhow::ensure!(
            image.status == "ready" && matches!(image.kind, ImageKind::Master | ImageKind::Clone),
            "assigned image is not a ready image revision"
        );
        // Normal Super sessions boot the isolated working clone. Installer media
        // is only meaningful for an explicit verification session that tests an
        // ISO boot path, so it must never gate starting a normal session.
        let media = if verification_only {
            let media = self
                .media
                .get_default()
                .await?
                .context("no default installer media is configured")?;
            Some(self.media_service.verify_integrity(&media.id).await?)
        } else {
            None
        };
        crate::application::client_boot::backup_custom_client_menu(
            &self.clients,
            &client,
            &self.settings,
        )
        .await?;

        let session_id = Uuid::new_v4().to_string();
        let source_snapshot_name = match client
            .snapshot
            .as_deref()
            .filter(|snapshot| !snapshot.trim().is_empty())
        {
            Some(assigned) => {
                let (source, name) = assigned
                    .split_once('@')
                    .context("assigned snapshot is malformed")?;
                anyhow::ensure!(
                    source == image.name,
                    "assigned snapshot does not belong to the assigned image"
                );
                name.to_owned()
            }
            None => format!("super-origin-{}", session_id.replace('-', "")),
        };
        let origin_snapshot = format!("{}@{}", image.name, source_snapshot_name);
        let working_name = format!("super-{}", session_id.replace('-', ""));
        let (working_dataset, _) = session_working_clone_path(&image.name, &working_name)?;
        let now = Utc::now();
        let mut session = SuperSession {
            id: session_id.clone(),
            client_id: client_id.to_owned(),
            prior_image_id: image.id.clone(),
            prior_snapshot: client.snapshot.clone(),
            working_origin_snapshot: Some(origin_snapshot),
            working_clone: working_dataset.clone(),
            installer_media_id: media.as_ref().map(|media| media.id.clone()),
            verification_only,
            state: SuperSessionState::Preparing,
            resume_state: None,
            created_at: now,
            updated_at: now,
        };
        self.sessions
            .insert(&session)
            .await
            .context("failed to persist Super-session before storage changes")?;

        let result = self
            .prepare_storage(
                &client,
                &image.id,
                &source_snapshot_name,
                &working_dataset,
                media.as_ref().map(|media| media.id.as_str()),
            )
            .await;
        match result {
            Ok(()) => {
                if self
                    .sessions
                    .activate_with_client_reservation(&session.id, client_id)
                    .await?
                {
                    session.state = SuperSessionState::Active;
                    Ok(session)
                } else {
                    self.sessions
                        .transition_state(
                            &session.id,
                            SuperSessionState::Preparing,
                            SuperSessionState::RecoveryRequired,
                        )
                        .await?;
                    bail!("Super-session state changed during activation")
                }
            }
            Err(error) => {
                let cleanup = self.cleanup_failed_entry(&client, &session).await;
                let next = if cleanup.is_ok() {
                    SuperSessionState::Completed
                } else {
                    SuperSessionState::RecoveryRequired
                };
                let transition = self
                    .sessions
                    .transition_state(&session.id, SuperSessionState::Preparing, next)
                    .await;
                match transition {
                    Ok(true) => {}
                    Ok(false) => {
                        return Err(error).context(
                            "Super-session activation failed; session state changed concurrently",
                        )
                    }
                    Err(recovery_error) => {
                        return Err(error).context(format!(
                            "Super-session activation failed and could not persist final state: {recovery_error:#}"
                        ))
                    }
                }
                match cleanup {
                    Ok(()) => {
                        Err(error).context("Super-session activation failed and was rolled back")
                    }
                    Err(cleanup_error) => Err(error).context(format!(
                        "activation failed; cleanup requires recovery: {cleanup_error:#}"
                    )),
                }
            }
        }
    }

    async fn prepare_storage(
        &self,
        client: &Client,
        image_id: &str,
        snapshot_name: &str,
        working_clone: &str,
        media_id: Option<&str>,
    ) -> Result<()> {
        let source = self.images.get(image_id).await?;
        let (working_path, working_name) = session_working_clone_path(&source.name, working_clone)?;
        self.ensure_snapshot(image_id, snapshot_name)
            .await
            .context("failed to pin assigned image before clone")?;
        let working = if let Some(existing) = self.images.get_optional(&working_path).await? {
            existing
        } else {
            self.images
                .clone_image(image_id, snapshot_name, &working_name)
                .await
                .context("failed to create session-owned working clone")?
        };
        let mut spec = crate::application::client_storage_mapping::storage_spec_for_values(
            &self.config,
            &self.settings,
            client.id.to_string(),
            &client.name,
            &client.master,
            client.snapshot.as_deref(),
            client.use_game_disk,
        )
        .await
        .map_err(anyhow::Error::msg)?;
        spec.target_iqn = client
            .target_iqn
            .clone()
            .context("client target is missing")?;
        spec.dataset = working.name.clone();
        spec.source = StorageSource::ExistingVolume(working.name.clone());
        spec.backstore = format!("super_{}", working_name);
        spec.use_game_disk = false;
        spec.game_disks.clear();
        spec.chap = chap(client);
        let installer_iso = match media_id {
            Some(media_id) => Some(
                self.media_service
                    .verify_integrity(media_id)
                    .await?
                    .managed_path,
            ),
            None => None,
        };
        let target = StorageService::build_super_session_iscsi_spec(&spec, installer_iso.clone())?;
        anyhow::ensure!(
            !self.clients.has_other_storage_owner(client).await?,
            "client storage is shared with another client"
        );
        if self.iscsi.inspect_target(&target)?.is_ready() {
            return self.storage.verify_super_session_target(&target);
        }
        let normal = self.normal_spec(client).await?;
        if let Err(normal_error) = self.storage.validate_client_target_for_replacement(&normal) {
            self.storage
                .validate_super_session_target_for_replacement(&target)
                .with_context(|| format!("assigned client target is unsafe: {normal_error:#}"))?;
        }
        self.storage
            .create_super_session_target(&spec, installer_iso)
            .context("failed to publish verified Super-session storage")?;
        self.storage
            .verify_super_session_target(&target)
            .context("published Super-session target did not verify")
    }

    async fn cleanup_failed_entry(&self, client: &Client, session: &SuperSession) -> Result<()> {
        let target = client
            .target_iqn
            .as_deref()
            .context("client target is missing")?;
        let backstores = StorageService::super_session_backstores(
            &session.client_id,
            &session.working_clone,
            session.installer_media_id.is_some(),
        );
        self.storage
            .remove_super_session_target(target, &backstores)?;
        let normal = self.normal_spec(client).await?;
        self.storage
            .ensure_client_storage_for_boot(&normal)
            .context("failed to restore normal client target")?;
        if let Some(working) = self.images.get_optional(&session.working_clone).await? {
            self.images
                .delete(&working.id)
                .await
                .context("failed to remove session-owned working clone")?;
        }
        self.remove_session_origin_snapshot(session).await?;
        Ok(())
    }

    async fn finish_locked(&self, client_id: &str, save: bool) -> Result<()> {
        self.validate_no_conflicting_disk_owner(client_id).await?;
        let session = self
            .sessions
            .get_for_client(client_id)
            .await?
            .context("client has no Super session")?;
        anyhow::ensure!(
            session.state == SuperSessionState::Active,
            "Super session is not active and requires recovery"
        );
        anyhow::ensure!(
            !save || !session.verification_only,
            "verification-only sessions cannot be saved"
        );
        let expected = if save {
            SuperSessionState::Saving
        } else {
            SuperSessionState::Discarding
        };
        anyhow::ensure!(
            self.sessions
                .transition_state(&session.id, SuperSessionState::Active, expected)
                .await?,
            "Super-session state changed before operation started"
        );
        let result = self.finish_storage(client_id, &session, save).await;
        match result {
            Ok(()) => {
                anyhow::ensure!(
                    self.sessions
                        .transition_state(&session.id, expected, SuperSessionState::Completed)
                        .await?,
                    "Super-session state changed before completion"
                );
                Ok(())
            }
            Err(error) => {
                match self
                    .sessions
                    .transition_state(&session.id, expected, SuperSessionState::RecoveryRequired)
                    .await
                {
                    Ok(true) => Err(error)
                        .context("Super-session operation failed; recovery record retained"),
                    Ok(false) => Err(error).context(
                        "Super-session operation failed and its recovery state changed concurrently",
                    ),
                    Err(recovery_error) => Err(error).context(format!(
                        "Super-session operation failed and could not persist recovery state: {recovery_error:#}"
                    )),
                }
            }
        }
    }

    async fn check_windows_candidate(
        &self,
        session: &SuperSession,
        revision: &crate::core::image::Image,
    ) -> Result<()> {
        use crate::infrastructure::{
            iscsi::IscsiTargetSpec,
            pxe::windows_iscsi_servicing::{WindowsIscsiConnection, SERVICING_IQN_PREFIX},
        };
        let profile = self.settings.windows_servicing.clone();
        anyhow::ensure!(
            !profile.worker_host.trim().is_empty() && !profile.worker_username.trim().is_empty() && !profile.iscsi_portal.trim().is_empty(),
            "configure the Windows servicing worker SSH host, username and iSCSI portal in System Settings before saving Windows"
        );
        let job = Uuid::parse_str(&session.id)
            .context("Windows save requires a UUID Super session identifier")?;
        let target = format!("{SERVICING_IQN_PREFIX}{job}");
        let backstore = format!("super_check_{}", job.simple());
        self.storage.validate_unmounted(&revision.name)?;
        let chap = ChapCredentials::generate(&format!("super-check-{job}"))?;
        let mut spec = IscsiTargetSpec::new(&target, &backstore, &revision.path, 0);
        spec.chap = Some(chap.clone());
        // Deterministic names permit safe retry after a process interruption. Only
        // matching candidate resources may be removed, never another target's LUNs.
        if self.iscsi.target_exists(&target)? {
            let state = self.iscsi.inspect_target(&spec)?;
            let actual = self.iscsi.list_target_luns(&target)?;
            anyhow::ensure!(
                actual.len() == 1
                    && actual[0].backstore == backstore
                    && state.luns.len() == 1
                    && state.luns[0].backstore == backstore
                    && state.luns[0].block_device_matches,
                "Windows safety target ownership mismatch; inspect retained Super recovery"
            );
            self.iscsi
                .remove_target_with_backstores(&target, std::slice::from_ref(&backstore))?;
            self.iscsi.remove_target(&target)?;
        }
        let size = self.images.get_info(&revision.id).await?.virtual_size;
        let connection = WindowsIscsiConnection {
            portal: profile.iscsi_portal.clone(),
            target_iqn: target.clone(),
            chap_username: chap.username,
            chap_password: chap.password,
            expected_size_bytes: size,
        };
        connection.validate()?;
        self.iscsi.create_target_transaction(&spec)?;
        // On error retain candidate, target and Super recovery record: a timed-out
        // SSH command may still own a mounted hive on the Windows worker.
        self.volume_safety.check(profile, connection).await?;
        // The application provisioner refuses this removal while any target
        // session exists, covering dynamic worker ACLs as well as named ACLs.
        self.iscsi
            .remove_target_with_backstores(&target, std::slice::from_ref(&backstore))?;
        self.iscsi.remove_target(&target)?;
        self.storage.validate_unmounted(&revision.name)?;
        Ok(())
    }

    async fn finish_storage(
        &self,
        client_id: &str,
        session: &SuperSession,
        save: bool,
    ) -> Result<()> {
        anyhow::ensure!(
            !save || !session.verification_only,
            "verification-only sessions cannot be saved"
        );
        let client_key =
            ClientId::from_string(client_id.to_owned()).map_err(anyhow::Error::from)?;
        let mut client = self
            .clients
            .find_by_id(&client_key)
            .await?
            .context("client does not exist")?;
        anyhow::ensure!(
            client.status == ClientStatus::Offline,
            "client must be offline"
        );
        let target = client
            .target_iqn
            .as_deref()
            .context("client target is missing")?
            .to_owned();
        anyhow::ensure!(
            self.iscsi
                .active_session_peer_ips(&target, &client.name)?
                .is_empty(),
            "client has an active iSCSI session"
        );
        if !save {
            client = self.prior_assignment(&client, session).await?;
        }
        let owned = StorageService::super_session_backstores(
            client_id,
            &session.working_clone,
            session.installer_media_id.is_some(),
        );
        self.storage.remove_super_session_target(&target, &owned)?;

        if save {
            let image_parent = session
                .working_clone
                .rsplit_once('/')
                .map(|(parent, _)| parent)
                .context("working clone has no dataset parent")?;
            let revision_slug = format!("super-revision-{}", session.id.replace('-', ""));
            let revision_name = format!("{image_parent}/{revision_slug}");
            let revision = if let Some(image) = self.images.get_optional(&revision_name).await? {
                image
            } else {
                let working = self
                    .images
                    .get(&session.working_clone)
                    .await
                    .context("working clone is missing")?;
                let snapshot_name = format!("super-save-{}", session.id.replace('-', ""));
                self.ensure_snapshot(&working.id, &snapshot_name).await?;
                let clone = self
                    .images
                    .clone_image(&working.id, &snapshot_name, &revision_slug)
                    .await?;
                self.images
                    .update(
                        &clone.id,
                        crate::core::image::UpdateImageRequest {
                            name: None,
                            os_type: None,
                            description: None,
                            status: Some("preparing".into()),
                        },
                    )
                    .await?;
                clone
            };
            if revision.kind == ImageKind::Clone {
                self.images
                    .update(
                        &revision.id,
                        crate::core::image::UpdateImageRequest {
                            name: None,
                            os_type: None,
                            description: None,
                            status: Some("preparing".into()),
                        },
                    )
                    .await?;
                if revision.os_type == crate::core::image::OsType::Windows {
                    self.check_windows_candidate(session, &revision).await?;
                }
                self.ensure_snapshot(
                    &revision.id,
                    &format!("published-{}", session.id.replace('-', "")),
                )
                .await?;
                self.images.promote_clone(&revision.name).await?;
                self.images.publish_serviced_clone(&revision.id).await?;
            }
            let revision = self.images.get(&revision.id).await?;
            self.ensure_snapshot(
                &revision.id,
                &format!("published-{}", session.id.replace('-', "")),
            )
            .await?;
            client.master = revision.name.clone();
            client.snapshot = Some(format!(
                "{}@published-{}",
                revision.name,
                session.id.replace('-', "")
            ));
            let parent = client
                .writeback
                .as_deref()
                .and_then(|path| path.rsplit_once('/').map(|(parent, _)| parent))
                .unwrap_or("pool/clients");
            client.writeback = Some(format!("{parent}/super-{}", session.id.replace('-', "")));
            client.block_store = Some(format!("super_saved_{}", session.id.replace('-', "")));
        }

        let mut normal = self.normal_spec(&client).await?;
        if save {
            normal.dataset = client
                .writeback
                .clone()
                .context("saved client writeback is missing")?;
            normal.backstore = client
                .block_store
                .clone()
                .context("saved client backstore is missing")?;
        }
        self.storage
            .ensure_client_storage_for_boot(&normal)
            .context("failed to rebuild normal client storage")?;
        anyhow::ensure!(
            self.storage.reconcile_client_storage(&normal)?.state
                == crate::domain::storage::StorageState::Ready,
            "normal client storage did not verify ready"
        );
        if save {
            client.mode = BootMode::Normal;
            client.updated_at = Utc::now();
            self.clients
                .update(&client)
                .await
                .context("failed to persist saved image assignment")?;
        } else {
            client.mode = BootMode::Normal;
            client.updated_at = Utc::now();
            self.clients
                .update(&client)
                .await
                .context("failed to restore normal boot mode")?;
        }
        if let Some(working) = self.images.get_optional(&session.working_clone).await? {
            if save {
                self.remove_session_save_snapshot(&working.id, session)
                    .await?;
            }
            self.images
                .delete(&working.id)
                .await
                .context("failed to remove session-owned working clone")?;
        }
        self.remove_session_origin_snapshot(session).await?;
        Ok(())
    }

    async fn remove_session_save_snapshot(
        &self,
        working_id: &str,
        session: &SuperSession,
    ) -> Result<()> {
        let snapshot_name = format!("super-save-{}", session.id.replace('-', ""));
        if let Some(snapshot) = self
            .images
            .snapshots(working_id)
            .await?
            .into_iter()
            .find(|snapshot| snapshot.name == snapshot_name)
        {
            self.images.delete(&snapshot.id).await?;
        }
        Ok(())
    }

    async fn remove_session_origin_snapshot(&self, session: &SuperSession) -> Result<()> {
        if session
            .prior_snapshot
            .as_deref()
            .is_some_and(|snapshot| !snapshot.trim().is_empty())
        {
            return Ok(());
        }
        let expected = format!("super-origin-{}", session.id.replace('-', ""));
        let name = session
            .working_origin_snapshot
            .as_deref()
            .and_then(|snapshot| snapshot.rsplit_once('@').map(|(_, name)| name))
            .context("session working origin snapshot is missing")?;
        anyhow::ensure!(
            name == expected,
            "refusing to remove non-session origin snapshot"
        );
        if let Some(snapshot) = self
            .images
            .snapshots(&session.prior_image_id)
            .await?
            .into_iter()
            .find(|snapshot| snapshot.name == expected)
        {
            self.images.delete(&snapshot.id).await?;
        }
        Ok(())
    }

    async fn ensure_snapshot(&self, image_id: &str, name: &str) -> Result<()> {
        if !self
            .images
            .snapshots(image_id)
            .await?
            .iter()
            .any(|snapshot| snapshot.name == name)
        {
            self.images.create_snapshot(image_id, name).await?;
        }
        Ok(())
    }

    async fn normal_spec(&self, client: &Client) -> Result<ClientStorageSpec> {
        let mut spec = crate::application::client_storage_mapping::storage_spec_for_values(
            &self.config,
            &self.settings,
            client.id.to_string(),
            &client.name,
            &client.master,
            client.snapshot.as_deref(),
            client.use_game_disk,
        )
        .await
        .map_err(anyhow::Error::msg)?;
        spec.target_iqn = client
            .target_iqn
            .clone()
            .context("client target is missing")?;
        if let Some(writeback) = client.writeback.as_ref() {
            spec.dataset = writeback.clone();
        }
        spec.backstore = client
            .block_store
            .clone()
            .context("client normal backstore is missing")?;
        spec.game_disks = self.clients.game_selection(&client.id).await?;
        spec.chap = chap(client);
        Ok(spec)
    }
}

fn chap(client: &Client) -> Option<ChapCredentials> {
    if !client.chap_enabled {
        return None;
    }
    Some(ChapCredentials {
        username: client.chap_user.clone()?,
        password: client.chap_secret.clone()?,
    })
}

fn session_working_clone_path(source_image: &str, working_clone: &str) -> Result<(String, String)> {
    let parent = source_image
        .rsplit_once('/')
        .map(|(parent, _)| parent)
        .context("assigned image has no dataset parent")?;
    let (working_parent, name) = working_clone
        .rsplit_once('/')
        .unwrap_or((parent, working_clone));
    anyhow::ensure!(
        working_parent == parent && !name.is_empty(),
        "session working clone must be a child of the assigned image dataset"
    );
    Ok((format!("{parent}/{name}"), name.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        core::image::{Image, ImageFormat, OsType},
        domain::client::{CreateClient, MacAddress, PxeMode},
        infrastructure::{
            image::{ImageBackend, ImageBackendInfo},
            iscsi::{IscsiLunBacking, IscsiLunState, IscsiTargetSpec, IscsiTargetState},
        },
        persistence::{
            repositories::{
                image::ImageRepository, InstallerMedia, InstallerMediaType,
                StructuralValidationState,
            },
            SuperSessionState,
        },
    };
    use chrono::Utc;
    use sha2::{Digest, Sha256};
    use std::{
        collections::{HashMap, HashSet},
        path::{Path, PathBuf},
        sync::{
            atomic::{AtomicBool, Ordering},
            Mutex,
        },
    };

    #[derive(Default)]
    struct FakeImages {
        datasets: Mutex<HashSet<String>>,
        snapshots: Mutex<HashSet<String>>,
        origins: Mutex<HashMap<String, String>>,
        mounted: AtomicBool,
    }

    impl ImageBackend for FakeImages {
        fn validate_unmounted(&self, _: &str) -> Result<()> {
            if self.mounted.load(Ordering::SeqCst) {
                anyhow::bail!("injected mounted client volume")
            }
            Ok(())
        }
        fn settle_device_changes(&self) -> Result<()> {
            Ok(())
        }
        fn clone_origin(&self, name: &str) -> Result<Option<String>> {
            Ok(self.origins.lock().unwrap().get(name).cloned())
        }
        fn exists(&self, name: &str) -> Result<bool> {
            Ok(self.datasets.lock().unwrap().contains(name)
                || self.snapshots.lock().unwrap().contains(name))
        }
        fn create_volume(&self, name: &str, _: u64) -> Result<()> {
            self.datasets.lock().unwrap().insert(name.to_owned());
            Ok(())
        }
        fn destroy(&self, name: &str) -> Result<()> {
            self.datasets.lock().unwrap().remove(name);
            self.origins.lock().unwrap().remove(name);
            Ok(())
        }
        fn rename(&self, old: &str, new: &str) -> Result<()> {
            if self.datasets.lock().unwrap().remove(old) {
                self.datasets.lock().unwrap().insert(new.into());
            }
            Ok(())
        }
        fn clone_image(&self, source: &str, destination: &str) -> Result<()> {
            self.datasets.lock().unwrap().insert(destination.into());
            self.origins
                .lock()
                .unwrap()
                .insert(destination.into(), source.into());
            Ok(())
        }
        fn promote_clone(&self, name: &str) -> Result<()> {
            self.origins.lock().unwrap().remove(name);
            Ok(())
        }
        fn create_snapshot(&self, dataset: &str, snapshot: &str) -> Result<()> {
            self.snapshots
                .lock()
                .unwrap()
                .insert(format!("{dataset}@{snapshot}"));
            Ok(())
        }
        fn destroy_snapshot(&self, dataset: &str, snapshot: &str) -> Result<()> {
            self.snapshots
                .lock()
                .unwrap()
                .remove(&format!("{dataset}@{snapshot}"));
            Ok(())
        }
        fn rollback_snapshot(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn resize(&self, _: &str, _: u64) -> Result<()> {
            Ok(())
        }
        fn import_raw(&self, _: &Path, _: &str, _: u64) -> Result<()> {
            Ok(())
        }
        fn verify(&self, name: &str) -> Result<bool> {
            self.exists(name)
        }
        fn info(&self, _: &str) -> Result<ImageBackendInfo> {
            Ok(ImageBackendInfo {
                virtual_size: 50,
                actual_size: 50,
                format: ImageFormat::Raw,
                backing_file: None,
                snapshots: Vec::new(),
            })
        }
        fn set_os_type(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn image_parent(&self) -> Result<String> {
            Ok("pool/images".into())
        }
    }

    #[derive(Default)]
    struct FakeIscsi {
        target: Mutex<Option<IscsiTargetSpec>>,
        fail_create_once: AtomicBool,
        extra_after_create: AtomicBool,
        active_session: AtomicBool,
        seen_lun_counts: Mutex<Vec<usize>>,
    }
    impl IscsiProvisioner for FakeIscsi {
        fn active_session_peer_ips(&self, _: &str, _: &str) -> Result<Vec<std::net::IpAddr>> {
            Ok(if self.active_session.load(Ordering::SeqCst) {
                vec!["192.0.2.99".parse().unwrap()]
            } else {
                vec![]
            })
        }
        fn create_target(&self, spec: &IscsiTargetSpec) -> Result<()> {
            if self.fail_create_once.swap(false, Ordering::SeqCst) {
                anyhow::bail!("injected target create failure");
            }
            self.seen_lun_counts.lock().unwrap().push(spec.luns.len());
            let mut created = spec.clone();
            if self.extra_after_create.swap(false, Ordering::SeqCst) {
                let mut extra = created.luns[1].clone();
                extra.lun = 2;
                extra.backstore = "stale_game_lun".into();
                created.luns.push(extra);
            }
            *self.target.lock().unwrap() = Some(created);
            Ok(())
        }
        fn remove_target(&self, _: &str) -> Result<()> {
            *self.target.lock().unwrap() = None;
            Ok(())
        }
        fn remove_target_with_backstores(&self, _: &str, _: &[String]) -> Result<()> {
            *self.target.lock().unwrap() = None;
            Ok(())
        }
        fn target_exists(&self, _: &str) -> Result<bool> {
            Ok(self.target.lock().unwrap().is_some())
        }
        fn set_chap_auth(&self, _: &str, _: Option<&ChapCredentials>) -> Result<()> {
            Ok(())
        }
        fn list_target_luns(&self, _: &str) -> Result<Vec<IscsiLunState>> {
            Ok(self
                .target
                .lock()
                .unwrap()
                .as_ref()
                .map(|s| {
                    s.luns
                        .iter()
                        .map(|l| IscsiLunState {
                            lun: l.lun,
                            backstore: l.backstore.clone(),
                            exists: true,
                            backstore_exists: true,
                            block_device_matches: true,
                        })
                        .collect()
                })
                .unwrap_or_default())
        }
        fn inspect_target(&self, spec: &IscsiTargetSpec) -> Result<IscsiTargetState> {
            let ready = self.target.lock().unwrap().as_ref().is_some_and(|current| {
                spec.luns.iter().all(|wanted| {
                    current.luns.iter().any(|actual| {
                        actual.lun == wanted.lun && actual.backstore == wanted.backstore
                    })
                })
            });
            Ok(IscsiTargetState::from_luns(
                spec.target_iqn.clone(),
                ready,
                spec.luns
                    .iter()
                    .map(|lun| IscsiLunState {
                        lun: lun.lun,
                        backstore: lun.backstore.clone(),
                        exists: ready,
                        backstore_exists: ready,
                        block_device_matches: ready,
                    })
                    .collect(),
                ready,
            ))
        }
        fn verify_named_file_lun_mappings(&self, spec: &IscsiTargetSpec) -> Result<()> {
            if spec
                .luns
                .iter()
                .any(|lun| lun.backing == IscsiLunBacking::File)
            {
                anyhow::ensure!(
                    spec.initiator_iqn.is_some(),
                    "named file LUN mapping is missing"
                );
                let target = self.target.lock().unwrap();
                let actual = target.as_ref().context("target missing")?;
                anyhow::ensure!(
                    actual.initiator_iqn == spec.initiator_iqn,
                    "named file LUN mapping is missing"
                );
            }
            Ok(())
        }
        fn reconcile(&self, spec: &IscsiTargetSpec) -> Result<()> {
            self.create_target(spec)
        }
    }

    async fn fixture() -> (
        SuperClientService,
        ClientRepository,
        SuperSessionRepository,
        Arc<FakeImages>,
        Arc<FakeIscsi>,
        Client,
        tempfile::TempDir,
    ) {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let clients = ClientRepository::new(pool.clone());
        let media = InstallerMediaRepository::new(pool.clone());
        let sessions = SuperSessionRepository::new(pool.clone());
        let images_backend = Arc::new(FakeImages::default());
        images_backend
            .datasets
            .lock()
            .unwrap()
            .insert("pool/master".into());
        images_backend
            .datasets
            .lock()
            .unwrap()
            .insert("pool/client-writeback".into());
        images_backend
            .origins
            .lock()
            .unwrap()
            .insert("pool/client-writeback".into(), "pool/master@v1".into());
        images_backend
            .snapshots
            .lock()
            .unwrap()
            .insert("pool/master@v1".into());
        let image_repo = ImageRepository::new(pool.clone());
        let images = ImageService::with_backend(image_repo.clone(), images_backend.clone());
        let master = Image {
            id: "master-id".into(),
            name: "pool/master".into(),
            kind: ImageKind::Master,
            os_type: OsType::Linux,
            size_gb: 50,
            path: PathBuf::from("/dev/zvol/pool/master"),
            format: ImageFormat::Raw,
            status: "ready".into(),
            description: None,
            parent_id: None,
            source_snapshot: None,
            checksum: None,
            is_default: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        image_repo.insert(&master).await.unwrap();
        let snapshot = Image {
            id: "snap-id".into(),
            name: "v1".into(),
            kind: ImageKind::Snapshot,
            os_type: OsType::Linux,
            size_gb: 50,
            path: PathBuf::from("/dev/zvol/pool/master"),
            format: ImageFormat::Raw,
            status: "ready".into(),
            description: None,
            parent_id: Some(master.id.clone()),
            source_snapshot: None,
            checksum: None,
            is_default: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        image_repo.insert(&snapshot).await.unwrap();
        let working = Image {
            id: "working-id".into(),
            name: "pool/super-working-session1".into(),
            kind: ImageKind::Clone,
            os_type: OsType::Linux,
            size_gb: 50,
            path: PathBuf::from("/dev/zvol/pool/super-working-session1"),
            format: ImageFormat::Raw,
            status: "ready".into(),
            description: None,
            parent_id: Some(master.id.clone()),
            source_snapshot: Some("v1".into()),
            checksum: None,
            is_default: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        image_repo.insert(&working).await.unwrap();
        images_backend
            .datasets
            .lock()
            .unwrap()
            .insert(working.name.clone());
        images_backend
            .origins
            .lock()
            .unwrap()
            .insert(working.name.clone(), "pool/master@v1".into());
        let mut client = Client::create(CreateClient {
            installer_media_id: None,
            enabled: None,
            name: "client-a".into(),
            mac: "02:00:00:00:00:01".into(),
            ip: "192.0.2.1".into(),
            master: "pool/master".into(),
            snapshot: Some("pool/master@v1".into()),
            block_store: Some("block_client_a".into()),
            block_device: Some("/dev/zvol/pool/client-writeback".into()),
            target_iqn: Some("iqn.test:client-a".into()),
            pxe_mode: PxeMode::default(),
            keep_writeback: true,
            use_game_disk: false,
            game_disks: vec![],
            chap_enabled: false,
        })
        .unwrap();
        client.status = ClientStatus::Offline;
        client.writeback = Some("pool/client-writeback".into());
        clients.insert(&client).await.unwrap();
        let now = Utc::now();
        let media_root = tempfile::tempdir().unwrap();
        let media_dir = media_root.path().join("installer-media");
        tokio::fs::create_dir_all(&media_dir).await.unwrap();
        let iso_bytes = b"synthetic installer media";
        let media_path = media_dir.join("media-1.iso");
        tokio::fs::write(&media_path, iso_bytes).await.unwrap();
        let iso = InstallerMedia {
            id: "media-1".into(),
            display_name: "Installer ISO".into(),
            media_type: InstallerMediaType::WindowsIso,
            managed_path: media_path,
            byte_size: iso_bytes.len() as u64,
            checksum: format!("{:x}", Sha256::digest(iso_bytes)),
            validation_state: StructuralValidationState::Valid,
            is_default: true,
            created_at: now,
            updated_at: now,
        };
        media.insert(&iso).await.unwrap();
        let iscsi = Arc::new(FakeIscsi::default());
        *iscsi.target.lock().unwrap() = Some(IscsiTargetSpec::new(
            "iqn.test:client-a",
            "block_client_a",
            "/dev/zvol/pool/client-writeback",
            0,
        ));
        let storage = Arc::new(
            StorageService::new(images_backend.clone(), iscsi.clone()).with_test_game_masters(
                vec![crate::application::storage_service::GameMaster {
                    dataset: "pool/game-games".into(),
                    disk_type: "game".into(),
                    size_bytes: Some(50 * 1024 * 1024 * 1024),
                }],
            ),
        );
        let media_service =
            InstallerMediaService::new(media_root.path(), 1_000_000, media, sessions.clone());
        let runtime_config = RuntimeConfig::default();
        crate::config::set_config(
            &runtime_config,
            &crate::types::AppConfig {
                settings: serde_json::json!({"zpool_name": "pool"}),
                ..crate::types::AppConfig::default()
            },
        );
        let mut settings = Settings::default();
        settings.http.root_dir = media_root.path().to_string_lossy().into_owned();
        let service = SuperClientService::new(SuperClientServiceDeps {
            clients: clients.clone(),
            media: InstallerMediaRepository::new(pool),
            sessions: sessions.clone(),
            images,
            storage,
            iscsi: iscsi.clone(),
            config: runtime_config,
            settings,
            media_service,
            mutation_locks: ClientMutationLocks::default(),
        });
        (
            service,
            clients,
            sessions,
            images_backend,
            iscsi,
            client,
            media_root,
        )
    }

    #[test]
    fn working_clone_paths_are_normalized_and_confined_to_the_source_dataset_parent() {
        assert_eq!(
            super::session_working_clone_path("pool/images/windows", "super-session-1").unwrap(),
            (
                "pool/images/super-session-1".into(),
                "super-session-1".into()
            )
        );
        assert!(super::session_working_clone_path(
            "pool/images/windows",
            "pool/other/super-session-1"
        )
        .is_err());
    }

    fn active_session(client: &Client, state: SuperSessionState) -> SuperSession {
        let now = Utc::now();
        SuperSession {
            id: "session-1".into(),
            client_id: client.id.to_string(),
            prior_image_id: "master-id".into(),
            prior_snapshot: client.snapshot.clone(),
            working_origin_snapshot: Some("pool/master@v1".into()),
            working_clone: "pool/super-working-session1".into(),
            installer_media_id: Some("media-1".into()),
            verification_only: false,
            state,
            resume_state: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn restarted_service(service: &SuperClientService) -> SuperClientService {
        SuperClientService::new(SuperClientServiceDeps {
            clients: service.clients.clone(),
            media: service.media.clone(),
            sessions: service.sessions.clone(),
            images: service.images.clone(),
            storage: service.storage.clone(),
            iscsi: service.iscsi.clone(),
            config: service.config.clone(),
            settings: service.settings.clone(),
            media_service: service.media_service.clone(),
            mutation_locks: service.mutation_locks.clone(),
        })
    }

    #[tokio::test]
    async fn entry_refuses_to_replace_a_client_target_with_unowned_luns() {
        let (service, _, sessions, _, iscsi, client, _media_root) = fixture().await;
        let mut existing = iscsi.target.lock().unwrap().clone().unwrap();
        existing
            .luns
            .push(crate::infrastructure::iscsi::IscsiLunSpec::new(
                1,
                "foreign_backstore",
                "/dev/zvol/foreign-volume",
            ));
        *iscsi.target.lock().unwrap() = Some(existing.clone());

        let result = service.enter(client.id.as_str(), true, true).await;

        assert!(result.is_err());
        assert_eq!(*iscsi.target.lock().unwrap(), Some(existing));
        assert!(iscsi.seen_lun_counts.lock().unwrap().is_empty());
        assert!(sessions
            .get_for_client(client.id.as_str())
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn entry_refuses_storage_identifiers_shared_with_another_client() {
        let (service, clients, sessions, _, iscsi, client, _media_root) = fixture().await;
        let other = Client::create(CreateClient {
            installer_media_id: None,
            enabled: None,
            name: "client-b".into(),
            mac: "02:00:00:00:00:02".into(),
            ip: "192.0.2.2".into(),
            master: "pool/master".into(),
            snapshot: Some("pool/master@v1".into()),
            block_store: Some(client.block_store.clone().unwrap()),
            block_device: Some("/dev/zvol/pool/other-writeback".into()),
            target_iqn: client.target_iqn.clone(),
            pxe_mode: PxeMode::default(),
            keep_writeback: true,
            use_game_disk: false,
            game_disks: vec![],
            chap_enabled: false,
        })
        .unwrap();
        clients.insert(&other).await.unwrap();
        let target_before = iscsi.target.lock().unwrap().clone();

        let result = service.enter(client.id.as_str(), true, true).await;

        assert!(result.is_err());
        assert_eq!(*iscsi.target.lock().unwrap(), target_before);
        assert!(iscsi.seen_lun_counts.lock().unwrap().is_empty());
        assert!(sessions
            .get_for_client(client.id.as_str())
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn completed_session_keeps_custom_menu_across_restart() {
        let (service, clients, _, _, _, client, media_root) = fixture().await;
        let relative = crate::infrastructure::pxe::client_mac_script_path(client.mac.as_str());
        let menu = media_root.path().join(relative);
        tokio::fs::create_dir_all(menu.parent().unwrap())
            .await
            .unwrap();
        let original = "#!ipxe\necho diagnostic recovery menu\n";
        tokio::fs::write(&menu, original).await.unwrap();

        service.enter(client.id.as_str(), true, true).await.unwrap();
        assert_eq!(tokio::fs::read_to_string(&menu).await.unwrap(), original);
        service
            .finish_locked(client.id.as_str(), false)
            .await
            .unwrap();
        restarted_service(&service)
            .recover_incomplete()
            .await
            .unwrap();

        assert_eq!(tokio::fs::read_to_string(&menu).await.unwrap(), original);
        assert!(clients
            .boot_menu_backup(client.id.as_str())
            .await
            .unwrap()
            .is_none());
    }

    #[test]
    fn completed_session_cannot_resume_storage_mutation() {
        assert!(!SuperSessionState::Completed.allows(SuperSessionState::Active));
        assert!(SuperSessionState::Active.allows(SuperSessionState::Saving));
    }

    #[tokio::test]
    async fn verification_session_requires_explicit_confirmation_before_storage_changes() {
        let (service, _, sessions, _, iscsi, client, _media_root) = fixture().await;

        assert!(service
            .enter(client.id.as_str(), true, false)
            .await
            .is_err());
        assert!(sessions.list_incomplete().await.unwrap().is_empty());
        assert_eq!(
            iscsi.seen_lun_counts.lock().unwrap().len(),
            0,
            "unconfirmed verification must not create a target"
        );
    }

    #[tokio::test]
    async fn legacy_exit_failure_is_recovered_after_service_restart() {
        let (service, clients, sessions, _disks, iscsi, mut client, _media_root) = fixture().await;
        client.mode = BootMode::Super;
        client.snapshot = None;
        client.writeback = None;
        clients.update(&client).await.unwrap();
        *iscsi.target.lock().unwrap() = Some(IscsiTargetSpec::new(
            "iqn.test:client-a",
            "block_client_a",
            "/dev/zvol/pool/master",
            0,
        ));
        iscsi.fail_create_once.store(true, Ordering::SeqCst);

        assert!(service
            .exit_legacy(client.id.as_str(), "pool/master@v1")
            .await
            .is_err());
        let recovery_state: String =
            sqlx::query_scalar("SELECT state FROM legacy_super_exits WHERE client_id = ?")
                .bind(client.id.as_str())
                .fetch_one(sessions.pool())
                .await
                .unwrap();
        assert_eq!(recovery_state, "recovery_required");

        restarted_service(&service)
            .recover_incomplete()
            .await
            .unwrap();
        let recovered = clients.find_by_id(&client.id).await.unwrap().unwrap();
        assert_eq!(recovered.mode, BootMode::Normal);
        assert_eq!(recovered.snapshot.as_deref(), Some("pool/master@v1"));
        let assigned_dataset = service.normal_spec(&recovered).await.unwrap().dataset;
        assert_eq!(
            recovered.writeback.as_deref(),
            Some(assigned_dataset.as_str())
        );
        let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM legacy_super_exits")
            .fetch_one(sessions.pool())
            .await
            .unwrap();
        assert_eq!(remaining, 0);
    }

    #[tokio::test]
    async fn legacy_exit_accepts_ready_saved_status_when_client_is_offline() {
        let (service, clients, _sessions, _disks, iscsi, mut client, _media_root) = fixture().await;
        client.mode = BootMode::Super;
        client.snapshot = None;
        client.writeback = None;
        client.status = ClientStatus::Ready;
        clients.update(&client).await.unwrap();
        *iscsi.target.lock().unwrap() = Some(IscsiTargetSpec::new(
            "iqn.test:client-a",
            "block_client_a",
            "/dev/zvol/pool/master",
            0,
        ));

        let exited = service
            .exit_legacy(client.id.as_str(), "pool/master@v1")
            .await
            .expect("offline Ready client should be allowed to exit legacy Super mode");

        assert_eq!(exited.mode, BootMode::Normal);
        assert_eq!(exited.snapshot.as_deref(), Some("pool/master@v1"));
    }

    #[tokio::test]
    async fn legacy_exit_recovery_repairs_a_missing_target_after_assignment_commit() {
        let (service, clients, sessions, _disks, iscsi, client, _media_root) = fixture().await;
        *iscsi.target.lock().unwrap() = None;
        sqlx::query("INSERT INTO legacy_super_exits(client_id, prior_master, prior_snapshot, selected_snapshot, state, created_at, updated_at) VALUES (?, ?, ?, ?, 'pending', ?, ?)")
            .bind(client.id.as_str())
            .bind(&client.master)
            .bind(&client.snapshot)
            .bind("pool/master@v1")
            .bind(Utc::now().to_rfc3339())
            .bind(Utc::now().to_rfc3339())
            .execute(sessions.pool())
            .await
            .unwrap();

        service.recover_incomplete().await.unwrap();

        let state = service
            .storage
            .reconcile_client_storage(&service.normal_spec(&client).await.unwrap())
            .unwrap();
        assert!(state.zfs_present && state.iscsi_present);
        assert!(iscsi.target.lock().unwrap().is_some());
        let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM legacy_super_exits")
            .fetch_one(sessions.pool())
            .await
            .unwrap();
        assert_eq!(remaining, 0);
        assert_eq!(
            clients.find_by_id(&client.id).await.unwrap().unwrap().mode,
            BootMode::Normal
        );
    }

    #[tokio::test]
    async fn legacy_exit_refuses_active_session_and_pending_storage_recovery_before_journaling() {
        let (service, clients, sessions, disks, iscsi, mut client, _media_root) = fixture().await;
        client.mode = BootMode::Super;
        client.snapshot = None;
        client.writeback = None;
        clients.update(&client).await.unwrap();
        *iscsi.target.lock().unwrap() = Some(IscsiTargetSpec::new(
            "iqn.test:client-a",
            "block_client_a",
            "/dev/zvol/pool/master",
            0,
        ));
        iscsi.active_session.store(true, Ordering::SeqCst);

        assert!(service
            .exit_legacy(client.id.as_str(), "pool/master@v1")
            .await
            .unwrap_err()
            .to_string()
            .contains("active iSCSI"));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM legacy_super_exits")
                .fetch_one(sessions.pool())
                .await
                .unwrap(),
            0
        );
        assert!(disks.exists("pool/client-writeback").unwrap());
        assert!(iscsi.target.lock().unwrap().is_some());

        iscsi.active_session.store(false, Ordering::SeqCst);
        sqlx::query("INSERT INTO client_offline_resets(client_id, fingerprint, operation) VALUES (?, '', '{}')")
            .bind(client.id.as_str())
            .execute(sessions.pool())
            .await
            .unwrap();
        assert!(service
            .exit_legacy(client.id.as_str(), "pool/master@v1")
            .await
            .unwrap_err()
            .to_string()
            .contains("storage recovery is pending"));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM legacy_super_exits")
                .fetch_one(sessions.pool())
                .await
                .unwrap(),
            0
        );
        assert!(disks.exists("pool/client-writeback").unwrap());
        assert!(iscsi.target.lock().unwrap().is_some());
    }

    #[tokio::test]
    async fn preparing_recovery_checks_for_active_iscsi_before_reusing_storage() {
        let (service, _, sessions, _, iscsi, client, _media_root) = fixture().await;
        iscsi.active_session.store(true, Ordering::SeqCst);
        sessions
            .insert(&active_session(&client, SuperSessionState::Preparing))
            .await
            .unwrap();

        let error = service.recover_incomplete().await.unwrap_err().to_string();
        assert!(error.contains("active iSCSI session"), "{error}");
        let recovery = sessions
            .get_for_client(client.id.as_str())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(recovery.state, SuperSessionState::RecoveryRequired);
    }

    #[tokio::test]
    async fn preparing_recovery_reuses_existing_working_clone_by_dataset_name() {
        let (service, _, sessions, backend, iscsi, client, _media_root) = fixture().await;
        let mut session = active_session(&client, SuperSessionState::Preparing);
        session.working_clone = "super-working-session1".into();
        sessions.insert(&session).await.unwrap();
        assert!(service
            .images
            .get_optional("pool/super-working-session1")
            .await
            .unwrap()
            .is_some());
        assert!(backend
            .datasets
            .lock()
            .unwrap()
            .contains("pool/super-working-session1"));

        service.recover_incomplete().await.unwrap();

        let recovered = sessions
            .get_for_client(client.id.as_str())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(recovered.state, SuperSessionState::Active);
        assert_eq!(recovered.working_clone, "pool/super-working-session1");
        let target = iscsi.target.lock().unwrap().clone().unwrap();
        assert_eq!(target.luns.len(), 2);
        assert!(target.luns[0].backstore.contains("super-working-session1"));
    }

    #[tokio::test]
    async fn preparing_recovery_checks_prior_volume_is_unmounted() {
        let (service, _, sessions, backend, _, client, _media_root) = fixture().await;
        backend.mounted.store(true, Ordering::SeqCst);
        sessions
            .insert(&active_session(&client, SuperSessionState::Preparing))
            .await
            .unwrap();

        let error = service.recover_incomplete().await.unwrap_err().to_string();
        assert!(
            error.contains("prior assigned client storage must be unmounted"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn preparing_recovery_refuses_to_replace_a_target_with_unowned_luns() {
        let (service, _, sessions, _, iscsi, client, _media_root) = fixture().await;
        let mut target = iscsi.target.lock().unwrap().clone().unwrap();
        target
            .luns
            .push(crate::infrastructure::iscsi::IscsiLunSpec::new(
                1,
                "foreign_backstore",
                "/dev/zvol/foreign-volume",
            ));
        *iscsi.target.lock().unwrap() = Some(target.clone());
        sessions
            .insert(&active_session(&client, SuperSessionState::Preparing))
            .await
            .unwrap();

        let error = service.recover_incomplete().await.unwrap_err().to_string();

        assert!(
            error.contains("unexpected or missing LUN mappings"),
            "{error}"
        );
        assert_eq!(*iscsi.target.lock().unwrap(), Some(target));
        assert!(iscsi.seen_lun_counts.lock().unwrap().is_empty());
        assert_eq!(
            sessions
                .get_for_client(client.id.as_str())
                .await
                .unwrap()
                .unwrap()
                .state,
            SuperSessionState::RecoveryRequired
        );
    }

    #[tokio::test]
    async fn preparing_recovery_refuses_storage_shared_with_another_client() {
        let (service, clients, sessions, _, iscsi, client, _media_root) = fixture().await;
        let other = Client::create(CreateClient {
            installer_media_id: None,
            enabled: None,
            name: "client-b".into(),
            mac: "02:00:00:00:00:02".into(),
            ip: "192.0.2.2".into(),
            master: "pool/master".into(),
            snapshot: Some("pool/master@v1".into()),
            block_store: client.block_store.clone(),
            block_device: Some("/dev/zvol/pool/other-writeback".into()),
            target_iqn: client.target_iqn.clone(),
            pxe_mode: PxeMode::default(),
            keep_writeback: true,
            use_game_disk: false,
            game_disks: vec![],
            chap_enabled: false,
        })
        .unwrap();
        clients.insert(&other).await.unwrap();
        let target = iscsi.target.lock().unwrap().clone();
        sessions
            .insert(&active_session(&client, SuperSessionState::Preparing))
            .await
            .unwrap();

        let error = service.recover_incomplete().await.unwrap_err().to_string();

        assert!(error.contains("shared with another client"), "{error}");
        assert_eq!(*iscsi.target.lock().unwrap(), target);
        assert!(iscsi.seen_lun_counts.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn preparing_recovery_replaces_its_own_partial_super_target() {
        let (service, _, sessions, _, iscsi, client, _media_root) = fixture().await;
        let session = active_session(&client, SuperSessionState::Preparing);
        let mut spec = service.normal_spec(&client).await.unwrap();
        spec.dataset = session.working_clone.clone();
        spec.source = StorageSource::ExistingVolume(session.working_clone.clone());
        spec.backstore = StorageService::super_session_working_backstore(&session.working_clone);
        spec.use_game_disk = false;
        spec.game_disks.clear();
        let media = service.media.get("media-1").await.unwrap().unwrap();
        let mut partial =
            StorageService::build_super_session_iscsi_spec(&spec, Some(media.managed_path))
                .unwrap();
        partial.luns.pop();
        *iscsi.target.lock().unwrap() = Some(partial);
        sessions.insert(&session).await.unwrap();

        service.recover_incomplete().await.unwrap();

        assert_eq!(
            sessions
                .get_for_client(client.id.as_str())
                .await
                .unwrap()
                .unwrap()
                .state,
            SuperSessionState::Active
        );
        assert_eq!(iscsi.target.lock().unwrap().as_ref().unwrap().luns.len(), 2);
        assert_eq!(iscsi.seen_lun_counts.lock().unwrap().as_slice(), &[2]);
    }

    #[tokio::test]
    async fn disabled_completed_client_stays_denied_until_reenabled() {
        let (service, clients, _, _, _, client, media_root) = fixture().await;
        let menu = media_root
            .path()
            .join(crate::infrastructure::pxe::client_mac_script_path(
                client.mac.as_str(),
            ));
        tokio::fs::create_dir_all(menu.parent().unwrap())
            .await
            .unwrap();
        let original = "#!ipxe\necho original client menu\n";
        tokio::fs::write(&menu, original).await.unwrap();

        service.enter(client.id.as_str(), true, true).await.unwrap();
        service
            .finish_locked(client.id.as_str(), false)
            .await
            .unwrap();
        let mut disabled = clients.find_by_id(&client.id).await.unwrap().unwrap();
        disabled.enabled = false;
        clients.update(&disabled).await.unwrap();
        service
            .restore_custom_menu_backup(client.id.as_str())
            .await
            .unwrap();

        assert_eq!(tokio::fs::read_to_string(&menu).await.unwrap(), original);
        assert!(clients
            .boot_menu_backup(client.id.as_str())
            .await
            .unwrap()
            .is_none());

        disabled.enabled = true;
        clients.update(&disabled).await.unwrap();
        service
            .restore_custom_menu_backup(client.id.as_str())
            .await
            .unwrap();

        assert_eq!(tokio::fs::read_to_string(menu).await.unwrap(), original);
        assert!(clients
            .boot_menu_backup(client.id.as_str())
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn normal_entry_does_not_require_a_boot_verification_record() {
        let (service, clients, sessions, _, iscsi, client, media_root) = fixture().await;

        let menu = media_root
            .path()
            .join(crate::infrastructure::pxe::client_mac_script_path(
                &client.mac.to_string(),
            ));
        tokio::fs::create_dir_all(menu.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(
            &menu,
            crate::infrastructure::pxe::render_client_script(
                &client.name,
                client.target_iqn.as_deref().unwrap(),
                80,
            ),
        )
        .await
        .unwrap();

        let normal_menu = tokio::fs::read_to_string(&menu).await.unwrap();
        let session = service
            .enter(client.id.as_str(), false, false)
            .await
            .unwrap();
        let updated = clients.find_by_id(&client.id).await.unwrap().unwrap();
        let target = iscsi.target.lock().unwrap().clone().unwrap();
        assert_eq!(session.state, SuperSessionState::Active);
        assert_eq!(
            session.working_clone,
            format!("pool/super-{}", session.id.replace('-', ""))
        );
        assert_eq!(tokio::fs::read_to_string(&menu).await.unwrap(), normal_menu);
        assert_eq!(updated.mode, BootMode::Super);
        assert!(session.installer_media_id.is_none());
        assert_eq!(target.luns.len(), 1);
        assert_eq!(target.luns[0].lun, 0);
        assert_eq!(
            target.luns[0].block_device,
            PathBuf::from(format!("/dev/zvol/{}", session.working_clone))
        );
        assert_eq!(
            target.luns[0].backstore,
            format!("super_super-{}", session.id.replace('-', ""))
        );
        assert_eq!(
            sessions
                .get_for_client(client.id.as_str())
                .await
                .unwrap()
                .unwrap()
                .state,
            SuperSessionState::Active
        );

        // The shared dispatcher reads the active Super-session state per boot.
        tokio::fs::write(
            &menu,
            crate::infrastructure::pxe::render_client_script(
                &client.name,
                client.target_iqn.as_deref().unwrap(),
                80,
            ),
        )
        .await
        .unwrap();
        service
            .restore_custom_menu_backup(client.id.as_str())
            .await
            .unwrap();
        assert_eq!(tokio::fs::read_to_string(&menu).await.unwrap(), normal_menu);

        // Disabling a client during an active session must fail closed.
        let mut disabled = updated;
        disabled.enabled = false;
        clients.update(&disabled).await.unwrap();
        service
            .restore_custom_menu_backup(client.id.as_str())
            .await
            .unwrap();
        assert_eq!(tokio::fs::read_to_string(menu).await.unwrap(), normal_menu);
    }

    #[tokio::test]
    async fn normal_entry_does_not_require_installer_media() {
        let (service, _, _, _, iscsi, client, _media_root) = fixture().await;
        sqlx::query("UPDATE installer_media SET is_default = 0")
            .execute(service.sessions.pool())
            .await
            .unwrap();

        let session = service
            .enter(client.id.as_str(), false, false)
            .await
            .unwrap();

        assert!(session.installer_media_id.is_none());
        let target = iscsi.target.lock().unwrap().clone().unwrap();
        assert_eq!(target.luns.len(), 1);
        assert_eq!(target.luns[0].lun, 0);
    }

    #[tokio::test]
    async fn clone_only_active_session_is_left_in_place_on_recovery() {
        let (service, clients, sessions, _, iscsi, mut client, _media_root) = fixture().await;
        client.mode = BootMode::Super;
        client.status = ClientStatus::Online;
        clients.update(&client).await.unwrap();
        let mut session = active_session(&client, SuperSessionState::Active);
        session.installer_media_id = None;
        let mut spec = service.normal_spec(&client).await.unwrap();
        spec.dataset = session.working_clone.clone();
        spec.source = StorageSource::ExistingVolume(session.working_clone.clone());
        spec.backstore = StorageService::super_session_working_backstore(&session.working_clone);
        spec.use_game_disk = false;
        spec.game_disks.clear();
        *iscsi.target.lock().unwrap() =
            Some(StorageService::build_super_session_iscsi_spec(&spec, None).unwrap());
        iscsi.active_session.store(true, Ordering::SeqCst);
        sessions.insert(&session).await.unwrap();

        service.recover_incomplete().await.unwrap();

        assert_eq!(iscsi.seen_lun_counts.lock().unwrap().len(), 0);
        assert_eq!(
            sessions
                .get_for_client(client.id.as_str())
                .await
                .unwrap()
                .unwrap()
                .state,
            SuperSessionState::Active
        );
        assert_eq!(iscsi.target.lock().unwrap().as_ref().unwrap().luns.len(), 1);
    }

    #[tokio::test]
    async fn active_recovery_rejects_installer_lun_on_clone_only_session() {
        let (service, _, sessions, _, iscsi, client, _media_root) = fixture().await;
        let mut session = active_session(&client, SuperSessionState::Active);
        session.installer_media_id = None;
        let mut spec = service.normal_spec(&client).await.unwrap();
        spec.dataset = session.working_clone.clone();
        spec.source = StorageSource::ExistingVolume(session.working_clone.clone());
        spec.backstore = StorageService::super_session_working_backstore(&session.working_clone);
        spec.use_game_disk = false;
        spec.game_disks.clear();
        let media = service.media.get("media-1").await.unwrap().unwrap();
        *iscsi.target.lock().unwrap() = Some(
            StorageService::build_super_session_iscsi_spec(&spec, Some(media.managed_path))
                .unwrap(),
        );
        sessions.insert(&session).await.unwrap();

        let error = service.recover_incomplete().await.unwrap_err();
        assert!(
            format!("{error:#}").contains("exactly the expected LUNs"),
            "{error:#}"
        );
    }

    #[tokio::test]
    async fn clone_only_discard_restores_assignment_without_installer_cleanup() {
        let (service, clients, sessions, backend, iscsi, client, _media_root) = fixture().await;
        backend
            .datasets
            .lock()
            .unwrap()
            .insert("pool/super-working-session1".into());
        let mut session = active_session(&client, SuperSessionState::Active);
        session.installer_media_id = None;
        sessions.insert(&session).await.unwrap();

        service.discard(client.id.as_str()).await.unwrap();

        let restored = clients.find_by_id(&client.id).await.unwrap().unwrap();
        assert_eq!(restored.master, client.master);
        assert_eq!(restored.snapshot, client.snapshot);
        assert!(sessions.list_incomplete().await.unwrap().is_empty());
        let target = iscsi.target.lock().unwrap().clone().unwrap();
        assert_eq!(target.luns.len(), 1);
        let normal = service.normal_spec(&restored).await.unwrap();
        assert_eq!(target.luns[0].backstore, normal.backstore);
        assert_eq!(
            sessions
                .get_for_client(client.id.as_str())
                .await
                .unwrap()
                .unwrap()
                .installer_media_id,
            None
        );
    }

    #[tokio::test]
    async fn entry_rejects_media_integrity_mismatch_before_persisting_session() {
        let (service, _, sessions, _, iscsi, client, media_root) = fixture().await;
        tokio::fs::write(
            media_root.path().join("installer-media/media-1.iso"),
            b"changed media",
        )
        .await
        .unwrap();

        assert!(service.enter(client.id.as_str(), true, true).await.is_err());
        assert!(sessions.list_incomplete().await.unwrap().is_empty());
        assert_eq!(iscsi.seen_lun_counts.lock().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn healthy_active_session_with_live_client_is_left_in_place_on_recovery() {
        let (service, clients, sessions, _, iscsi, mut client, _media_root) = fixture().await;
        client.status = ClientStatus::Online;
        clients.update(&client).await.unwrap();
        let mut spec = service.normal_spec(&client).await.unwrap();
        spec.dataset = "pool/super-working-session1".into();
        spec.source = StorageSource::ExistingVolume(spec.dataset.clone());
        spec.backstore = StorageService::super_session_working_backstore(&spec.dataset);
        spec.use_game_disk = false;
        spec.game_disks.clear();
        let media = service.media.get("media-1").await.unwrap().unwrap();
        *iscsi.target.lock().unwrap() = Some(
            StorageService::build_super_session_iscsi_spec(&spec, Some(media.managed_path))
                .unwrap(),
        );
        iscsi.active_session.store(true, Ordering::SeqCst);
        sessions
            .insert(&active_session(&client, SuperSessionState::Active))
            .await
            .unwrap();

        service.recover_incomplete().await.unwrap();
        assert_eq!(iscsi.seen_lun_counts.lock().unwrap().len(), 0);
        assert_eq!(
            sessions
                .get_for_client(client.id.as_str())
                .await
                .unwrap()
                .unwrap()
                .state,
            SuperSessionState::Active
        );
    }

    #[tokio::test]
    async fn active_recovery_rejects_extra_target_luns() {
        let (service, _, sessions, _, iscsi, client, _media_root) = fixture().await;
        let mut spec = service.normal_spec(&client).await.unwrap();
        spec.dataset = "pool/super-working-session1".into();
        spec.source = StorageSource::ExistingVolume(spec.dataset.clone());
        spec.backstore = StorageService::super_session_working_backstore(&spec.dataset);
        spec.use_game_disk = false;
        spec.game_disks.clear();
        let media = service.media.get("media-1").await.unwrap().unwrap();
        let mut target =
            StorageService::build_super_session_iscsi_spec(&spec, Some(media.managed_path))
                .unwrap();
        let mut extra = target.luns[1].clone();
        extra.lun = 2;
        extra.backstore = "stale_game_lun".into();
        target.luns.push(extra);
        *iscsi.target.lock().unwrap() = Some(target);
        sessions
            .insert(&active_session(&client, SuperSessionState::Active))
            .await
            .unwrap();

        let result = service.recover_incomplete().await;
        assert!(
            result.is_err(),
            "active target with an extra LUN was accepted"
        );
    }

    #[tokio::test]
    async fn boot_verification_rejects_missing_named_file_lun_mapping() {
        let (service, clients, sessions, _, iscsi, mut client, _media_root) = fixture().await;
        client.mode = BootMode::Super;
        clients.update(&client).await.unwrap();
        let media = service.media.get("media-1").await.unwrap().unwrap();
        let mut session = active_session(&client, SuperSessionState::Active);
        session.verification_only = true;
        sessions.insert(&session).await.unwrap();

        let mut spec = service.normal_spec(&client).await.unwrap();
        spec.dataset = session.working_clone.clone();
        spec.source = StorageSource::ExistingVolume(session.working_clone.clone());
        spec.backstore = StorageService::super_session_working_backstore(&session.working_clone);
        spec.use_game_disk = false;
        spec.game_disks.clear();
        let mut target =
            StorageService::build_super_session_iscsi_spec(&spec, Some(media.managed_path.clone()))
                .unwrap();
        target.initiator_iqn = None;
        *iscsi.target.lock().unwrap() = Some(target);

        assert!(
            service
                .verify_boot_verification_target(&client, &session, &media)
                .await
                .is_err(),
            "verification must reject a missing named ACL mapping"
        );
    }

    #[tokio::test]
    async fn entry_rejects_extra_lun_after_target_creation_before_activation() {
        let (service, clients, sessions, _, iscsi, client, _media_root) = fixture().await;
        iscsi.extra_after_create.store(true, Ordering::SeqCst);

        let error = service
            .enter(client.id.as_str(), true, true)
            .await
            .unwrap_err();
        let error = format!("{error:#}");
        assert!(error.contains("exactly the expected LUNs"), "{error}");
        assert_eq!(
            clients.find_by_id(&client.id).await.unwrap().unwrap().mode,
            BootMode::Normal
        );
        assert!(sessions.list_incomplete().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn failed_entry_removes_only_its_generated_origin_snapshot() {
        let (service, clients, sessions, _, iscsi, mut client, _media_root) = fixture().await;
        client.snapshot = None;
        client.writeback = None;
        clients.update(&client).await.unwrap();
        iscsi.fail_create_once.store(true, Ordering::SeqCst);

        let error = service
            .enter(client.id.as_str(), true, true)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("activation failed"), "{error:#}");
        let session = sessions
            .get_for_client(client.id.as_str())
            .await
            .unwrap()
            .unwrap();
        let (dataset, snapshot_name) = session
            .working_origin_snapshot
            .as_deref()
            .unwrap()
            .split_once('@')
            .unwrap();
        assert_eq!(dataset, "pool/master");
        assert!(!service
            .images
            .snapshots(&session.prior_image_id)
            .await
            .unwrap()
            .iter()
            .any(|snapshot| snapshot.name == snapshot_name));
    }

    #[tokio::test]
    async fn discard_removes_generated_origin_snapshot() {
        let (service, clients, _, _, _, mut client, _media_root) = fixture().await;
        client.snapshot = None;
        client.writeback = None;
        clients.update(&client).await.unwrap();

        let session = service.enter(client.id.as_str(), true, true).await.unwrap();
        let snapshot_name = session
            .working_origin_snapshot
            .as_deref()
            .unwrap()
            .split_once('@')
            .unwrap()
            .1
            .to_owned();
        assert!(service
            .images
            .snapshots(&session.prior_image_id)
            .await
            .unwrap()
            .iter()
            .any(|snapshot| snapshot.name == snapshot_name));

        service.discard(client.id.as_str()).await.unwrap();
        assert!(!service
            .images
            .snapshots(&session.prior_image_id)
            .await
            .unwrap()
            .iter()
            .any(|snapshot| snapshot.name == snapshot_name));
    }

    struct FakeVolumeSafety {
        fail: bool,
        calls: Mutex<Vec<String>>,
    }
    impl VolumeSafetyGuard for FakeVolumeSafety {
        fn check<'a>(
            &'a self,
            _profile: crate::core::config::WindowsServicingConfig,
            connection: crate::infrastructure::pxe::windows_iscsi_servicing::WindowsIscsiConnection,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
            Box::pin(async move {
                connection.validate()?;
                self.calls.lock().unwrap().push(connection.target_iqn);
                anyhow::ensure!(!self.fail, "injected native replay failure");
                Ok(())
            })
        }
    }

    #[tokio::test]
    async fn super_refuses_identity_and_unfinished_installation_owners() {
        let (service, _, sessions, _, _, client, _root) = fixture().await;
        sqlx::query("INSERT INTO installation_sessions(id,client_id,media_id,image_id,master,media_checksum,target_iqn,expected_bytes,source_ip,firmware,recipe,token,state,created_at,updated_at,expires_at) VALUES ('install',?,'media-1','master-id','pool/master','synthetic','iqn.test:client-a',50,'192.0.2.1','bios','interactive','synthetic','failed','now','now','later')")
            .bind(client.id.as_str()).execute(sessions.pool()).await.unwrap();
        let error = service
            .enter(client.id.as_str(), true, true)
            .await
            .unwrap_err();
        assert!(format!("{error:#}").contains("unfinished installation"));
        assert!(service
            .finish_locked(client.id.as_str(), true)
            .await
            .is_err());
        let pending = active_session(&client, SuperSessionState::Saving);
        assert!(service.recover_one(pending).await.is_err());
        sqlx::query("UPDATE installation_sessions SET state = 'completed'")
            .execute(sessions.pool())
            .await
            .unwrap();
        sqlx::query("INSERT INTO client_identity_caches(client_id,id,profile_key,source_revision,dataset,snapshot,replacement,state,created_at,updated_at) VALUES (?,'cache','profile','pool/master@v1','pool/cache','pool/cache@identity','pool/replacement','ready','now','now')")
            .bind(client.id.as_str()).execute(sessions.pool()).await.unwrap();
        let error = service
            .enter(client.id.as_str(), true, true)
            .await
            .unwrap_err();
        assert!(format!("{error:#}").contains("identity cache"));
        assert!(service
            .finish_locked(client.id.as_str(), false)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn windows_save_requires_worker_and_retains_unpublished_candidate() {
        let (service, clients, sessions, backend, _, client, _root) = fixture().await;
        sqlx::query(
            "UPDATE images SET os_type = 'windows' WHERE name = 'pool/super-working-session1'",
        )
        .execute(sessions.pool())
        .await
        .unwrap();
        backend
            .datasets
            .lock()
            .unwrap()
            .insert("pool/super-working-session1".into());
        sessions
            .insert(&active_session(&client, SuperSessionState::Active))
            .await
            .unwrap();
        let error = service.save(client.id.as_str()).await.unwrap_err();
        assert!(format!("{error:#}").contains("configure the Windows servicing worker"));
        let candidate = service
            .images
            .get("pool/super-revision-session1")
            .await
            .unwrap();
        assert_eq!(candidate.status, "preparing");
        assert!(service
            .images
            .snapshots(&candidate.id)
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            clients
                .find_by_id(&client.id)
                .await
                .unwrap()
                .unwrap()
                .master,
            client.master
        );
        assert_eq!(
            sessions
                .get_for_client(client.id.as_str())
                .await
                .unwrap()
                .unwrap()
                .state,
            SuperSessionState::RecoveryRequired
        );
    }

    #[tokio::test]
    async fn windows_save_native_failure_is_retryable_before_publication() {
        let (mut service, clients, sessions, backend, _, client, _root) = fixture().await;
        sqlx::query(
            "UPDATE images SET os_type = 'windows' WHERE name = 'pool/super-working-session1'",
        )
        .execute(sessions.pool())
        .await
        .unwrap();
        backend
            .datasets
            .lock()
            .unwrap()
            .insert("pool/super-working-session1".into());
        let mut session = active_session(&client, SuperSessionState::Active);
        session.id = "12345678-1234-1234-1234-123456789abc".into();
        sessions.insert(&session).await.unwrap();
        service.settings.windows_servicing.worker_host = "worker.example.test".into();
        service.settings.windows_servicing.worker_username = "servicer".into();
        service.settings.windows_servicing.iscsi_portal = "192.0.2.1".into();
        let guard = Arc::new(FakeVolumeSafety {
            fail: true,
            calls: Mutex::new(Vec::new()),
        });
        service.volume_safety = guard.clone();
        let error = service.save(client.id.as_str()).await.unwrap_err();
        assert!(format!("{error:#}").contains("injected native replay failure"));
        assert_eq!(guard.calls.lock().unwrap().len(), 1);
        let candidate = service
            .images
            .get(&format!(
                "pool/super-revision-{}",
                session.id.replace('-', "")
            ))
            .await
            .unwrap();
        assert_eq!(candidate.status, "preparing");
        assert!(service
            .images
            .snapshots(&candidate.id)
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            clients
                .find_by_id(&client.id)
                .await
                .unwrap()
                .unwrap()
                .master,
            client.master
        );
        let retry = Arc::new(FakeVolumeSafety {
            fail: false,
            calls: Mutex::new(Vec::new()),
        });
        service.volume_safety = retry.clone();
        service.recover_incomplete().await.unwrap();
        assert_eq!(
            retry.calls.lock().unwrap().as_slice(),
            guard.calls.lock().unwrap().as_slice()
        );
        assert_eq!(
            clients
                .find_by_id(&client.id)
                .await
                .unwrap()
                .unwrap()
                .master,
            candidate.name
        );
        assert!(sessions.list_incomplete().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn save_publishes_independent_revision_and_only_reassigns_its_client() {
        let (service, clients, sessions, backend, iscsi, client, media_root) = fixture().await;
        let mut other = client.clone();
        other.id = ClientId::new();
        other.name = "client-b".into();
        other.mac = MacAddress::parse("02:00:00:00:00:02").unwrap();
        other.ip = "192.0.2.2".parse().unwrap();
        other.target_iqn = Some("iqn.test:client-b".into());
        clients.insert(&other).await.unwrap();
        backend
            .datasets
            .lock()
            .unwrap()
            .insert("pool/super-working-session1".into());
        backend.origins.lock().unwrap().insert(
            "pool/super-working-session1".into(),
            "pool/master@v1".into(),
        );
        sessions
            .insert(&active_session(&client, SuperSessionState::Active))
            .await
            .unwrap();
        let menu = media_root
            .path()
            .join(crate::infrastructure::pxe::client_mac_script_path(
                &client.mac.to_string(),
            ));
        tokio::fs::create_dir_all(menu.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(
            &menu,
            crate::infrastructure::pxe::render_super_boot_denied(),
        )
        .await
        .unwrap();
        service.save(client.id.as_str()).await.unwrap();
        let retained = tokio::fs::read_to_string(menu).await.unwrap();
        assert!(retained.contains("Super-session boot denied"));
        let updated = clients.find_by_id(&client.id).await.unwrap().unwrap();
        let unchanged = clients.find_by_id(&other.id).await.unwrap().unwrap();
        assert!(updated.master.contains("super-revision-session1"));
        assert_ne!(updated.writeback, client.writeback);
        assert_eq!(unchanged.master, other.master);
        assert_eq!(unchanged.writeback, other.writeback);
        assert_eq!(backend.origins.lock().unwrap().get(&updated.master), None);
        assert!(sessions.list_incomplete().await.unwrap().is_empty());
        assert_eq!(iscsi.seen_lun_counts.lock().unwrap().last(), Some(&1));
    }

    #[tokio::test]
    async fn discard_restores_prior_assignment_and_isolates_another_client() {
        let (service, clients, sessions, backend, iscsi, client, _media_root) = fixture().await;
        let mut other = client.clone();
        other.id = ClientId::new();
        other.name = "client-b".into();
        other.mac = MacAddress::parse("02:00:00:00:00:02").unwrap();
        other.ip = "192.0.2.2".parse().unwrap();
        other.target_iqn = Some("iqn.test:client-b".into());
        clients.insert(&other).await.unwrap();
        let mut diverged = client.clone();
        diverged.master = "pool/changed-during-session".into();
        diverged.snapshot = Some("pool/changed-during-session@later".into());
        clients.update(&diverged).await.unwrap();
        clients
            .set_game_selection(&client.id, &["pool/game-games".into()])
            .await
            .unwrap();
        let game_clone = crate::application::storage_service::game_clone_dataset(
            client.id.as_str(),
            "pool/game-games",
        );
        backend
            .datasets
            .lock()
            .unwrap()
            .insert("pool/super-working-session1".into());
        backend.datasets.lock().unwrap().insert(game_clone.clone());
        sessions
            .insert(&active_session(&client, SuperSessionState::Active))
            .await
            .unwrap();
        service.discard(client.id.as_str()).await.unwrap();
        let restored = clients.find_by_id(&client.id).await.unwrap().unwrap();
        let unchanged = clients.find_by_id(&other.id).await.unwrap().unwrap();
        assert_eq!(restored.master, client.master);
        assert_eq!(restored.snapshot, client.snapshot);
        assert_eq!(restored.writeback, client.writeback);
        assert!(service
            .images
            .snapshots("master-id")
            .await
            .unwrap()
            .iter()
            .any(|snapshot| snapshot.name == "v1"));
        assert_eq!(unchanged.master, other.master);
        assert_eq!(unchanged.writeback, other.writeback);
        assert_eq!(iscsi.seen_lun_counts.lock().unwrap().last(), Some(&1));
        assert!(!restored.use_game_disk);
        assert_eq!(
            clients.game_selection(&client.id).await.unwrap(),
            vec!["pool/game-games"]
        );
        assert!(backend.datasets.lock().unwrap().contains(&game_clone));
        assert!(sessions.list_incomplete().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn discard_restores_selected_game_lun_when_game_disk_is_enabled() {
        let (service, clients, sessions, backend, iscsi, mut client, media_root) = fixture().await;
        client.use_game_disk = true;
        clients.update(&client).await.unwrap();
        clients
            .set_game_selection(&client.id, &["pool/game-games".into()])
            .await
            .unwrap();
        backend
            .datasets
            .lock()
            .unwrap()
            .insert("pool/super-working-session1".into());
        sessions
            .insert(&active_session(&client, SuperSessionState::Active))
            .await
            .unwrap();
        let menu = media_root
            .path()
            .join(crate::infrastructure::pxe::client_mac_script_path(
                &client.mac.to_string(),
            ));
        tokio::fs::create_dir_all(menu.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(
            &menu,
            crate::infrastructure::pxe::render_super_boot_denied(),
        )
        .await
        .unwrap();

        service.discard(client.id.as_str()).await.unwrap();
        let retained = tokio::fs::read_to_string(menu).await.unwrap();
        assert!(retained.contains("Super-session boot denied"));
        assert_eq!(iscsi.seen_lun_counts.lock().unwrap().last(), Some(&2));
        assert!(backend.datasets.lock().unwrap().contains(
            &crate::application::storage_service::game_clone_dataset(
                client.id.as_str(),
                "pool/game-games"
            )
        ));
    }

    #[tokio::test]
    async fn save_target_failure_is_retried_from_durable_saving_state() {
        let (service, clients, sessions, backend, iscsi, client, _media_root) = fixture().await;
        backend
            .datasets
            .lock()
            .unwrap()
            .insert("pool/super-working-session1".into());
        backend.origins.lock().unwrap().insert(
            "pool/super-working-session1".into(),
            "pool/master@v1".into(),
        );
        sessions
            .insert(&active_session(&client, SuperSessionState::Active))
            .await
            .unwrap();
        iscsi.fail_create_once.store(true, Ordering::SeqCst);

        assert!(service.save(client.id.as_str()).await.is_err());
        let recovery = sessions
            .get_for_client(client.id.as_str())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(recovery.state, SuperSessionState::RecoveryRequired);
        assert_eq!(recovery.resume_state, Some(SuperSessionState::Saving));
        restarted_service(&service)
            .recover_incomplete()
            .await
            .unwrap();
        assert!(sessions.list_incomplete().await.unwrap().is_empty());
        assert!(clients
            .find_by_id(&client.id)
            .await
            .unwrap()
            .unwrap()
            .master
            .contains("super-revision-session1"));
    }

    #[tokio::test]
    async fn target_failure_retains_recovery_record_and_retry_completes_discard() {
        let (service, clients, sessions, backend, iscsi, client, _media_root) = fixture().await;
        backend
            .datasets
            .lock()
            .unwrap()
            .insert("pool/super-working-session1".into());
        sessions
            .insert(&active_session(&client, SuperSessionState::Active))
            .await
            .unwrap();
        iscsi.fail_create_once.store(true, Ordering::SeqCst);
        assert!(service.discard(client.id.as_str()).await.is_err());
        let recovery = sessions
            .get_for_client(client.id.as_str())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(recovery.state, SuperSessionState::RecoveryRequired);
        assert_eq!(recovery.resume_state, Some(SuperSessionState::Discarding));
        restarted_service(&service)
            .recover_incomplete()
            .await
            .unwrap();
        assert!(sessions.list_incomplete().await.unwrap().is_empty());
        assert_eq!(
            clients.find_by_id(&client.id).await.unwrap().unwrap().mode,
            BootMode::Normal
        );
    }

    #[tokio::test]
    async fn reports_when_failure_cannot_be_marked_for_recovery() {
        let (service, _clients, sessions, backend, iscsi, client, _media_root) = fixture().await;
        backend
            .datasets
            .lock()
            .unwrap()
            .insert("pool/super-working-session1".into());
        backend.origins.lock().unwrap().insert(
            "pool/super-working-session1".into(),
            "pool/master@v1".into(),
        );
        sessions
            .insert(&active_session(&client, SuperSessionState::Active))
            .await
            .unwrap();
        sqlx::query("CREATE TRIGGER reject_super_recovery BEFORE UPDATE OF state ON super_sessions WHEN NEW.state = 'recovery_required' BEGIN SELECT RAISE(ABORT, 'forced recovery journal failure'); END")
            .execute(sessions.pool())
            .await
            .unwrap();
        iscsi.fail_create_once.store(true, Ordering::SeqCst);

        let error = service.save(client.id.as_str()).await.unwrap_err();
        let detail = format!("{error:#}");
        assert!(
            detail.contains("could not persist recovery state"),
            "{detail}"
        );
        assert!(
            detail.contains("forced recovery journal failure"),
            "{detail}"
        );
        assert_eq!(
            sessions
                .get_for_client(client.id.as_str())
                .await
                .unwrap()
                .unwrap()
                .state,
            SuperSessionState::Saving,
            "the durable in-progress state remains retryable"
        );
    }
}
