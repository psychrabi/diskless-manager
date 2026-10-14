//! Durable offline deadlines; independent of UI connections and ping status.
use super::storage_service::{OfflineDeletion, OfflineReplacement, OwnedGameClone, StorageService};
use crate::{
    domain::{
        storage::{ClientStorageSpec, StorageSource},
        Client,
    },
    persistence::ClientRepository,
    state::AppState,
};
use anyhow::{bail, Context, Result};
use std::{future::Future, pin::Pin, sync::Arc};

#[derive(Debug, Default, sqlx::FromRow)]
struct ResetState {
    fingerprint: String,
    offline_since: Option<i64>,
    completed: bool,
    retry_after: i64,
    operation: Option<String>,
}

#[derive(Debug, PartialEq)]
enum Decision {
    Cancel,
    Start,
    Wait,
    Delete,
}

const DELETION_JOURNAL_KIND: &str = "offline_deletion";
const DELETION_JOURNAL_VERSION: u64 = 1;

enum PendingJournal {
    Deletion(OfflineDeletion),
    Replacement(OfflineReplacement),
}

fn serialize_deletion_journal(operation: &OfflineDeletion) -> Result<String> {
    Ok(serde_json::to_string(&serde_json::json!({
        "kind": DELETION_JOURNAL_KIND,
        "version": DELETION_JOURNAL_VERSION,
        "operation": operation,
    }))?)
}

fn deserialize_pending_journal(value: serde_json::Value) -> Result<PendingJournal> {
    if value.get("kind").is_some() || value.get("version").is_some() {
        anyhow::ensure!(
            value.get("kind").is_some() && value.get("version").is_some(),
            "incomplete storage journal envelope"
        );
        anyhow::ensure!(
            value["kind"] == DELETION_JOURNAL_KIND,
            "unknown storage journal kind"
        );
        anyhow::ensure!(
            value["version"] == DELETION_JOURNAL_VERSION,
            "unsupported storage journal version"
        );
        let operation = value
            .get("operation")
            .cloned()
            .context("storage journal envelope has no operation")?;
        return Ok(PendingJournal::Deletion(serde_json::from_value(operation)?));
    }

    // Untagged journals predate the envelope; staged is the legacy replacement discriminator.
    if value.get("staged").is_some() {
        Ok(PendingJournal::Replacement(serde_json::from_value(value)?))
    } else {
        Ok(PendingJournal::Deletion(serde_json::from_value(value)?))
    }
}

fn decide(
    state: &ResetState,
    connected: Option<bool>,
    eligible: bool,
    now: i64,
    delay: i64,
) -> Decision {
    if !eligible || connected == Some(true) {
        return Decision::Cancel;
    }
    if connected.is_none() || state.completed {
        return Decision::Wait;
    }
    match state.offline_since {
        None => Decision::Start,
        Some(since) if now.saturating_sub(since) >= delay && now >= state.retry_after => {
            Decision::Delete
        }
        Some(_) => Decision::Wait,
    }
}

fn retry_delay(failures: i64) -> i64 {
    30 * (1_i64 << failures.clamp(0, 5))
}

fn storage_spec(client: &Client) -> Result<ClientStorageSpec> {
    let snapshot = client
        .snapshot
        .as_deref()
        .filter(|s| s.contains('@'))
        .context("client has no snapshot")?;
    let dataset = client
        .writeback
        .as_deref()
        .context("client has no persisted writeback dataset")?;
    if dataset == client.master
        || snapshot.split('@').next() == Some(dataset)
        || dataset.contains("..")
        || dataset.contains('@')
    {
        bail!("refusing to delete a master or invalid dataset");
    }
    Ok(ClientStorageSpec {
        client_id: client.id.to_string(),
        client_name: client.name.clone(),
        source: StorageSource::Snapshot(snapshot.to_owned()),
        dataset: dataset.to_owned(),
        backstore: client
            .block_store
            .clone()
            .context("client has no persisted backstore")?,
        target_iqn: client
            .target_iqn
            .clone()
            .context("client has no persisted target IQN")?,
        lun: 0,
        use_game_disk: client.use_game_disk,
        game_disks: Vec::new(),
        // Resolved by the caller; journals from older
        // builds deserialize this as None via serde default.
        chap: None,
    })
}

/// Caller must hold `state.client_mutations`; pending operations are rejected here.
pub async fn delete_writeback_locked(state: &AppState, client: &Client) -> Result<()> {
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM client_offline_resets WHERE client_id = ? AND operation IS NOT NULL",
    )
    .bind(client.id.as_str())
    .fetch_one(&state.db_pool)
    .await?;
    if pending != 0 {
        return Err(PendingRecoverySafetyError::PendingOperation.into());
    }
    let mut spec = storage_spec(client)?;
    let persisted_device = client
        .block_device
        .as_deref()
        .context("client has no persisted block device")?;
    anyhow::ensure!(
        persisted_device == spec.block_device().to_string_lossy(),
        "persisted block device does not match writeback dataset"
    );
    validate_unshared_storage(state, client, &spec).await?;
    let selected_games = if client.use_game_disk {
        crate::core::reconciliation::stored_game_selection(&state.db_pool, client.id.as_str())
            .await?
    } else {
        Vec::new()
    };
    let use_game_disk = client.use_game_disk;
    let storage = state.application.storage.clone();
    let client_id = client.id.to_string();
    let game_clones = tokio::task::spawn_blocking(move || {
        Ok::<_, anyhow::Error>(selected_owned_game_clones(
            storage.list_game_clones()?,
            &client_id,
            use_game_disk,
            &selected_games,
        ))
    })
    .await??;
    spec.chap = None;
    let mut operation = OfflineDeletion::new(spec, game_clones);
    persist_deletion(
        state,
        client.id.as_str(),
        client.mac.as_str(),
        &mut operation,
        false,
    )
    .await
}

/// Reject boot-time repair when persisted resource identities are shared.
/// Direct master volumes are intentionally shared; their target and backstore are not.
pub(crate) async fn validate_boot_storage_owner(state: &AppState, client: &Client) -> Result<()> {
    let snapshot_backed = client
        .snapshot
        .as_deref()
        .is_some_and(|snapshot| !snapshot.trim().is_empty());
    let dataset = if snapshot_backed {
        client.writeback.as_deref().unwrap_or_default()
    } else {
        ""
    };
    let other: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM clients WHERE id <> ? AND (target_iqn = ? OR block_store = ? OR (? = 1 AND (block_device = ? OR writeback = ? OR master = ?)))",
    )
    .bind(client.id.as_str())
    .bind(client.target_iqn.as_deref().unwrap_or_default())
    .bind(client.block_store.as_deref().unwrap_or_default())
    .bind(i64::from(snapshot_backed))
    .bind(client.block_device.as_deref().unwrap_or_default())
    .bind(dataset)
    .bind(dataset)
    .fetch_one(&state.db_pool)
    .await?;
    if other != 0 {
        return Err(PendingRecoverySafetyError::SharedStorage.into());
    }
    Ok(())
}

fn selected_owned_game_clones(
    records: Vec<super::storage_service::GameCloneRecord>,
    client_id: &str,
    use_game_disk: bool,
    selected_masters: &[String],
) -> Vec<OwnedGameClone> {
    records
        .into_iter()
        .filter(|record| {
            record.client_id == client_id
                && use_game_disk
                && (selected_masters.is_empty()
                    || selected_masters.contains(&record.master_dataset))
        })
        .map(|record| OwnedGameClone {
            master_dataset: record.master_dataset,
            clone_dataset: record.clone_dataset,
        })
        .collect()
}

async fn validate_unshared_storage(
    state: &AppState,
    client: &Client,
    spec: &ClientStorageSpec,
) -> Result<()> {
    let other: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM clients WHERE id <> ? AND (block_device = ? OR block_store = ? OR writeback = ? OR target_iqn = ? OR master = ?)",
    )
    .bind(client.id.as_str())
    .bind(spec.block_device().to_string_lossy().as_ref())
    .bind(&spec.backstore)
    .bind(&spec.dataset)
    .bind(&spec.target_iqn)
    .bind(&spec.dataset)
    .fetch_one(&state.db_pool)
    .await?;
    if other != 0 {
        return Err(PendingRecoverySafetyError::SharedStorage.into());
    }
    Ok(())
}

fn validate_deletion_owner(client: &Client, operation: &OfflineDeletion) -> Result<()> {
    let expected = storage_spec(client)?;
    anyhow::ensure!(
        operation.spec.client_id == client.id.as_str()
            && operation.spec.client_name == expected.client_name
            && operation.spec.source == expected.source
            && operation.spec.dataset == expected.dataset
            && operation.spec.backstore == expected.backstore
            && operation.spec.target_iqn == expected.target_iqn
            && client.block_device.as_deref()
                == Some(expected.block_device().to_string_lossy().as_ref()),
        "recovery journal storage identity does not match the client record"
    );
    Ok(())
}

async fn persist_deletion(
    state: &AppState,
    client_id: &str,
    client_mac: &str,
    operation: &mut OfflineDeletion,
    already_journaled: bool,
) -> Result<()> {
    let settings = state.settings.read().await.clone();
    let workflow: Arc<dyn LifecycleDeletionWorkflow> = Arc::new(SystemLifecycleDeletionWorkflow(
        state.application.storage.clone(),
    ));
    persist_deletion_with(
        &state.db_pool,
        &settings,
        client_id,
        client_mac,
        operation,
        already_journaled,
        workflow,
    )
    .await
}

trait LifecycleDeletionWorkflow: Send + Sync + 'static {
    fn preflight(&self, operation: &OfflineDeletion) -> Result<()>;
    fn remove_menu<'a>(
        &'a self,
        settings: &'a crate::core::config::Settings,
        client_mac: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
    fn delete(&self, operation: &mut OfflineDeletion) -> Result<()>;
}

struct SystemLifecycleDeletionWorkflow(Arc<StorageService>);

impl LifecycleDeletionWorkflow for SystemLifecycleDeletionWorkflow {
    fn preflight(&self, operation: &OfflineDeletion) -> Result<()> {
        preflight_deletion(&self.0, operation)
    }

    fn remove_menu<'a>(
        &'a self,
        settings: &'a crate::core::config::Settings,
        client_mac: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(crate::infrastructure::dhcp::remove_client_ipxe_menu(
            settings, client_mac,
        ))
    }

    fn delete(&self, operation: &mut OfflineDeletion) -> Result<()> {
        self.0.delete_offline(operation)
    }
}

async fn persist_deletion_with(
    pool: &sqlx::SqlitePool,
    settings: &crate::core::config::Settings,
    client_id: &str,
    client_mac: &str,
    operation: &mut OfflineDeletion,
    already_journaled: bool,
    workflow: Arc<dyn LifecycleDeletionWorkflow>,
) -> Result<()> {
    if !already_journaled {
        let work = operation.clone();
        let check = workflow.clone();
        tokio::task::spawn_blocking(move || check.preflight(&work)).await??;
        sqlx::query("INSERT INTO client_offline_resets(client_id, fingerprint, operation) VALUES (?, '', ?) ON CONFLICT(client_id) DO UPDATE SET operation = excluded.operation")
            .bind(client_id)
            .bind(serialize_deletion_journal(operation)?)
            .execute(pool)
            .await?;
    }
    let work = operation.clone();
    let check = workflow.clone();
    tokio::task::spawn_blocking(move || check.preflight(&work)).await??;
    // Remove the stale boot script before its target can disappear; readiness republishes it after recovery.
    workflow.remove_menu(settings, client_mac).await?;
    let mut work = operation.clone();
    let delete = workflow.clone();
    let (result, updated) = tokio::task::spawn_blocking(move || {
        let result = delete.delete(&mut work);
        (result, work)
    })
    .await?;
    *operation = updated;
    save_deletion_progress(pool, client_id, operation, result.is_ok()).await?;
    result.map(|_| ())
}

async fn save_deletion_progress(
    pool: &sqlx::SqlitePool,
    client_id: &str,
    operation: &OfflineDeletion,
    succeeded: bool,
) -> Result<()> {
    sqlx::query("UPDATE client_offline_resets SET operation = CASE WHEN ? THEN NULL ELSE ? END, completed = ?, failures = CASE WHEN ? THEN 0 ELSE failures END, retry_after = CASE WHEN ? THEN 0 ELSE retry_after END, last_error = CASE WHEN ? THEN NULL ELSE last_error END WHERE client_id = ?")
        .bind(succeeded)
        .bind(serialize_deletion_journal(operation)?)
        .bind(if succeeded { 1 } else { 0 })
        .bind(succeeded)
        .bind(succeeded)
        .bind(succeeded)
        .bind(client_id)
        .execute(pool)
        .await?;
    Ok(())
}

fn preflight_deletion(storage: &StorageService, operation: &OfflineDeletion) -> Result<()> {
    storage.ensure_offline_deletion(operation)
}

pub async fn run(state: AppState) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        if let Err(error) = tick(&state).await {
            tracing::error!(%error, "offline reset coordinator failed");
        }
    }
}

async fn tick(state: &AppState) -> Result<()> {
    let repository = ClientRepository::new(state.db_pool.clone());
    let clients = repository.find_all().await?;
    let pending: std::collections::HashSet<String> = sqlx::query_scalar(
        "SELECT client_id FROM client_offline_resets WHERE operation IS NOT NULL OR offline_since IS NOT NULL OR failures > 0",
    ).fetch_all(&state.db_pool).await?.into_iter().collect();
    for client in clients {
        let eligible = !client.keep_writeback && client.has_snapshot();
        if !eligible && !pending.contains(client.id.as_str()) {
            continue;
        }
        let state = state.clone();
        tokio::spawn(async move { process_locked(state, client.id.to_string()).await }).await??;
    }
    Ok(())
}

async fn process_locked(state: AppState, client_id: String) -> Result<()> {
    // The detached child owns this guard through any spawn_blocking storage work.
    // Background polling must not queue behind user mutations or boot readiness.
    let Ok(_guard) = state.client_mutations.try_lock() else {
        return Ok(());
    };
    let id = crate::domain::ClientId::from_string(client_id.clone())?;
    let repository = ClientRepository::new(state.db_pool.clone());
    let current = repository
        .find_by_id(&id)
        .await?
        .with_context(|| format!("client '{client_id}' disappeared during lifecycle processing"))?;
    if let Err(error) = process(&state, &current).await {
        tracing::warn!(client_id, %error, "automatic client storage deletion deferred");
        let failures: Option<i64> =
            sqlx::query_scalar("SELECT failures FROM client_offline_resets WHERE client_id = ?")
                .bind(&client_id)
                .fetch_optional(&state.db_pool)
                .await?;
        if let Some(failures) = failures {
            sqlx::query("UPDATE client_offline_resets SET failures = failures + 1, retry_after = ?, last_error = ? WHERE client_id = ?")
                .bind(chrono::Utc::now().timestamp() + retry_delay(failures))
                .bind(format!("{error:#}"))
                .bind(&client_id)
                .execute(&state.db_pool)
                .await?;
        }
    }
    Ok(())
}

async fn process(state: &AppState, client: &Client) -> Result<()> {
    let settings = state.settings.read().await.clone();
    let now = chrono::Utc::now().timestamp();
    let eligible =
        !client.keep_writeback && client.snapshot.as_deref().is_some_and(|s| !s.is_empty());
    let fingerprint = lifecycle_fingerprint(state, client).await?;
    let saved = sqlx::query_as::<_, ResetState>(
        "SELECT fingerprint, offline_since, completed, failures, retry_after, operation FROM client_offline_resets WHERE client_id = ?",
    )
    .bind(client.id.as_str())
    .fetch_optional(&state.db_pool)
    .await?;
    let mut saved = saved.unwrap_or_default();
    let target = client.target_iqn.clone().unwrap_or_else(|| {
        format!(
            "{}:client.{}",
            settings.iscsi.target_prefix,
            client.name.trim().to_lowercase()
        )
    });
    let target_copy = target.clone();
    let storage = state.application.storage.clone();
    let connected =
        tokio::task::spawn_blocking(move || storage.target_connected_if_present(&target_copy))
            .await?
            .ok();

    // Finish a journal before clearing a deadline or accepting new settings.
    if let Some(journal) = saved.operation.as_deref() {
        match deserialize_pending_journal(serde_json::from_str(journal)?)? {
            PendingJournal::Deletion(mut operation) => {
                validate_deletion_owner(client, &operation)?;
                if now < saved.retry_after {
                    return Ok(());
                }
                validate_unshared_storage(state, client, &operation.spec).await?;
                persist_deletion(
                    state,
                    client.id.as_str(),
                    client.mac.as_str(),
                    &mut operation,
                    true,
                )
                .await?;
                return Ok(());
            }
            PendingJournal::Replacement(mut operation) => {
                validate_recovery_owner(client.id.as_str(), &operation)?;
                if connected == Some(true) {
                    if !operation.committed {
                        let storage = state.application.storage.clone();
                        let candidate = operation.clone();
                        if tokio::task::spawn_blocking(move || {
                            storage.replacement_is_attached(&candidate)
                        })
                        .await??
                        {
                            operation.committed = true;
                            sqlx::query(
                        "UPDATE client_offline_resets SET operation = ? WHERE client_id = ?",
                    )
                    .bind(serde_json::to_string(&operation)?)
                    .bind(client.id.as_str())
                    .execute(&state.db_pool)
                    .await?;
                        }
                    }
                    sqlx::query("UPDATE client_offline_resets SET offline_since = NULL, completed = 0, failures = 0, retry_after = 0 WHERE client_id = ?")
                .bind(client.id.as_str())
                .execute(&state.db_pool)
                .await?;
                    return Ok(());
                }
                if connected.is_none() {
                    return Ok(());
                }
                if now < saved.retry_after {
                    return Ok(());
                }
                let storage = state.application.storage.clone();
                let operations: Arc<dyn PendingRecoveryOperations> =
                    Arc::new(SystemPendingRecoveryOperations(storage));
                recover_pending_operation(&state.db_pool, client, &operation, operations).await?;
                saved.operation = None;
            }
        }
    }
    if saved.fingerprint != fingerprint {
        saved = ResetState {
            fingerprint: fingerprint.clone(),
            ..Default::default()
        };
        sqlx::query("INSERT INTO client_offline_resets (client_id, fingerprint) VALUES (?, ?) ON CONFLICT(client_id) DO UPDATE SET fingerprint = excluded.fingerprint, offline_since = NULL, completed = 0, failures = 0, retry_after = 0, last_error = NULL")
            .bind(client.id.as_str())
            .bind(&fingerprint)
            .execute(&state.db_pool)
            .await?;
    }
    match decide(
        &saved,
        connected,
        eligible,
        now,
        i64::from(settings.client_lifecycle.non_persistent_reset_delay_minutes) * 60,
    ) {
        Decision::Cancel => {
            sqlx::query("UPDATE client_offline_resets SET offline_since = NULL, completed = 0, failures = 0, retry_after = 0 WHERE client_id = ?")
                .bind(client.id.as_str())
                .execute(&state.db_pool)
                .await?;
        }
        Decision::Start => {
            sqlx::query("UPDATE client_offline_resets SET offline_since = ? WHERE client_id = ?")
                .bind(now)
                .bind(client.id.as_str())
                .execute(&state.db_pool)
                .await?;
        }
        Decision::Wait => {}
        Decision::Delete => {
            delete_writeback_locked(state, client).await?;
            tracing::info!(client_id = %client.id, "non-persistent client storage deletion completed");
        }
    }
    Ok(())
}

pub(crate) async fn lifecycle_fingerprint(state: &AppState, client: &Client) -> Result<String> {
    let mut games =
        crate::core::reconciliation::stored_game_selection(&state.db_pool, client.id.as_str())
            .await?;
    games.sort();
    Ok(serde_json::to_string(&(
        &client.master,
        &client.snapshot,
        &client.writeback,
        &client.block_device,
        &client.block_store,
        &client.target_iqn,
        &client.name,
        client.keep_writeback,
        client.use_game_disk,
        games,
    ))?)
}

/// Manually retry a persisted storage recovery without starting a new reset.
#[derive(Debug, thiserror::Error)]
pub enum PendingRecoverySafetyError {
    #[error("client is connected; disconnect it before retrying recovery")]
    ClientConnected,
    #[error("client disk is exported over NVMe-oF; remove that export before retrying recovery")]
    NvmeExported,
    #[error("client has a pending storage operation")]
    PendingOperation,
    #[error("client storage is shared with another client")]
    SharedStorage,
}

#[derive(Debug, thiserror::Error)]
enum PendingRecoveryError {
    #[error(transparent)]
    Safety(#[from] PendingRecoverySafetyError),
    #[error("storage recovery failed: {0:#}")]
    Attempt(#[source] anyhow::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

trait PendingRecoveryOperations: Send + Sync + 'static {
    fn target_connected(&self, target_iqn: &str) -> Result<bool>;
    fn device_exported(&self, device: &std::path::Path) -> Result<bool>;
    fn recover_offline(&self, operation: &OfflineReplacement) -> Result<()>;
}

struct SystemPendingRecoveryOperations(Arc<StorageService>);

impl PendingRecoveryOperations for SystemPendingRecoveryOperations {
    fn target_connected(&self, target_iqn: &str) -> Result<bool> {
        crate::infrastructure::iscsi::reconcile::target_has_active_sessions(target_iqn)
    }

    fn device_exported(&self, device: &std::path::Path) -> Result<bool> {
        crate::infrastructure::nvmeof::block_device_is_exported(device).map_err(Into::into)
    }

    fn recover_offline(&self, operation: &OfflineReplacement) -> Result<()> {
        self.0.recover_offline(operation)
    }
}

fn validate_recovery_owner(client_id: &str, operation: &OfflineReplacement) -> Result<()> {
    anyhow::ensure!(
        operation.spec.client_id == client_id,
        "recovery journal client mismatch"
    );
    Ok(())
}

async fn recover_pending_operation(
    pool: &sqlx::SqlitePool,
    client: &Client,
    operation: &OfflineReplacement,
    operations: Arc<dyn PendingRecoveryOperations>,
) -> std::result::Result<(), PendingRecoveryError> {
    validate_recovery_owner(client.id.as_str(), operation).map_err(PendingRecoveryError::Other)?;
    let device = client
        .block_device
        .as_deref()
        .or(client.block_store.as_deref())
        .ok_or_else(|| PendingRecoveryError::Other(anyhow::anyhow!("client has no disk")))?;
    let device = std::path::PathBuf::from(device);
    let operation = operation.clone();
    let result = tokio::task::spawn_blocking(move || {
        if operations
            .target_connected(&operation.spec.target_iqn)
            .map_err(PendingRecoveryError::Other)?
        {
            return Err(PendingRecoverySafetyError::ClientConnected.into());
        }
        if operations
            .device_exported(&device)
            .map_err(PendingRecoveryError::Other)?
        {
            return Err(PendingRecoverySafetyError::NvmeExported.into());
        }
        operations
            .recover_offline(&operation)
            .map_err(PendingRecoveryError::Attempt)
    })
    .await
    .map_err(|error| PendingRecoveryError::Other(error.into()))?;
    result?;
    sqlx::query(
        "UPDATE client_offline_resets SET operation = NULL WHERE client_id = ? AND operation IS NOT NULL",
    )
    .bind(client.id.as_str())
    .execute(pool)
    .await
    .map_err(|error| PendingRecoveryError::Other(error.into()))?;
    Ok(())
}

async fn record_manual_recovery_failure(
    pool: &sqlx::SqlitePool,
    client_id: &str,
    failures: i64,
    error: &anyhow::Error,
) -> Result<()> {
    sqlx::query("UPDATE client_offline_resets SET failures = failures + 1, retry_after = ?, last_error = ? WHERE client_id = ? AND operation IS NOT NULL")
        .bind(chrono::Utc::now().timestamp() + retry_delay(failures))
        .bind(format!("{error:#}"))
        .bind(client_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn retry_pending_recovery(state: &AppState, id: &str) -> Result<bool> {
    let Some((journal, failures)) = sqlx::query_as::<_, (Option<String>, i64)>(
        "SELECT operation, failures FROM client_offline_resets WHERE client_id = ?",
    )
    .bind(id)
    .fetch_optional(&state.db_pool)
    .await?
    else {
        return Ok(false);
    };
    let Some(journal) = journal else {
        return Ok(false);
    };
    let parsed: serde_json::Value = serde_json::from_str(&journal)?;
    let client_id = crate::domain::ClientId::from_string(id.to_owned())?;
    let client = ClientRepository::new(state.db_pool.clone())
        .find_by_id(&client_id)
        .await?
        .context("client no longer exists")?;
    match deserialize_pending_journal(parsed)? {
        PendingJournal::Deletion(mut operation) => {
            validate_deletion_owner(&client, &operation)?;
            validate_unshared_storage(state, &client, &operation.spec).await?;
            if let Err(error) =
                persist_deletion(state, id, client.mac.as_str(), &mut operation, true).await
            {
                record_manual_recovery_failure(&state.db_pool, id, failures, &error).await?;
                return Err(error).context("storage deletion retry failed");
            }
            sqlx::query("UPDATE client_offline_resets SET failures = 0, retry_after = 0, last_error = NULL WHERE client_id = ? AND operation IS NULL")
            .bind(id)
            .execute(&state.db_pool)
            .await?;
            Ok(true)
        }
        PendingJournal::Replacement(operation) => {
            validate_recovery_owner(id, &operation)?;
            let storage = state.application.storage.clone();
            let operations: Arc<dyn PendingRecoveryOperations> =
                Arc::new(SystemPendingRecoveryOperations(storage));
            match recover_pending_operation(&state.db_pool, &client, &operation, operations).await {
                Ok(()) => {}
                Err(PendingRecoveryError::Attempt(error)) => {
                    record_manual_recovery_failure(&state.db_pool, id, failures, &error).await?;
                    return Err(error).context("storage recovery retry failed");
                }
                Err(PendingRecoveryError::Safety(error)) => return Err(error.into()),
                Err(error) => return Err(error.into()),
            }

            sqlx::query("UPDATE client_offline_resets SET failures = 0, retry_after = 0, last_error = NULL WHERE client_id = ? AND operation IS NULL")
        .bind(id)
        .execute(&state.db_pool)
        .await?;
            Ok(true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::domain::ClientId;

    #[tokio::test]
    async fn background_processing_skips_busy_mutation_lock() {
        let (state, _, _, _) = crate::api::security_tests::setup().await;
        let guard = state.client_mutations.clone().lock_owned().await;
        // No client lookup or system inspection should run while mutations are busy.
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            process_locked(state, "missing".into()),
        )
        .await
        .unwrap()
        .unwrap();
        drop(guard);
    }

    #[test]
    fn journal_envelope_and_legacy_formats_decode_safely() {
        let replacement = OfflineReplacement::new(ClientStorageSpec {
            client_id: "client".into(),
            client_name: "pc".into(),
            source: StorageSource::Snapshot("pool/master@ready".into()),
            dataset: "pool/client".into(),
            backstore: "block_client".into(),
            target_iqn: "iqn.test:client".into(),
            lun: 0,
            use_game_disk: false,
            game_disks: Vec::new(),
            chap: None,
        });
        let encoded =
            serialize_deletion_journal(&OfflineDeletion::new(replacement.spec.clone(), Vec::new()))
                .unwrap();
        assert!(matches!(
            deserialize_pending_journal(serde_json::from_str(&encoded).unwrap()).unwrap(),
            PendingJournal::Deletion(_)
        ));
        assert!(matches!(
            deserialize_pending_journal(serde_json::to_value(&replacement).unwrap()).unwrap(),
            PendingJournal::Replacement(_)
        ));
        let deletion = OfflineDeletion::new(replacement.spec.clone(), Vec::new());
        assert!(matches!(
            deserialize_pending_journal(serde_json::to_value(deletion).unwrap()).unwrap(),
            PendingJournal::Deletion(_)
        ));
        for value in [
            serde_json::json!({"kind": "future_operation", "version": 1, "operation": {}}),
            serde_json::json!({"kind": DELETION_JOURNAL_KIND, "version": 2, "operation": {}}),
        ] {
            assert!(deserialize_pending_journal(value).is_err());
        }
    }

    #[test]
    fn clone_inventory_includes_only_selected_client_owned_games() {
        let records = vec![
            crate::application::storage_service::GameCloneRecord {
                master_dataset: "pool/games/a".into(),
                client_id: "client".into(),
                clone_dataset: "pool/games/client-a".into(),
            },
            crate::application::storage_service::GameCloneRecord {
                master_dataset: "pool/games/b".into(),
                client_id: "client".into(),
                clone_dataset: "pool/games/client-b".into(),
            },
            crate::application::storage_service::GameCloneRecord {
                master_dataset: "pool/games/a".into(),
                client_id: "other".into(),
                clone_dataset: "pool/games/other-a".into(),
            },
        ];
        let selected =
            selected_owned_game_clones(records.clone(), "client", true, &["pool/games/b".into()]);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].clone_dataset, "pool/games/client-b");
        assert_eq!(
            selected_owned_game_clones(records.clone(), "client", true, &[]).len(),
            2,
            "an empty selection means all owned game clones when game disks are enabled"
        );
        assert!(selected_owned_game_clones(records, "client", false, &[]).is_empty());
    }

    #[tokio::test]
    async fn pending_deadline_and_replacement_journal_survive_database_restart() {
        let path = std::env::temp_dir().join(format!("diskless-reset-{}.db", uuid::Uuid::new_v4()));
        let url = format!("sqlite:{}?mode=rwc", path.display());
        let pool = sqlx::SqlitePool::connect(&url).await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO clients(id, name, mac, ip, master, created_at, updated_at) VALUES ('client', 'PC001', '00:11:22:33:44:55', '192.168.1.101', 'pool/master', '2026-09-13T00:00:00+00:00', '2026-09-13T00:00:00+00:00')")
            .execute(&pool)
            .await
            .unwrap();
        let op = OfflineReplacement::new(ClientStorageSpec {
            client_id: "client".into(),
            client_name: "PC001".into(),
            source: StorageSource::Snapshot("pool/master@ready".into()),
            dataset: "pool/pc001".into(),
            backstore: "block_pc001".into(),
            target_iqn: "iqn.test:pc001".into(),
            lun: 0,
            use_game_disk: false,
            game_disks: Vec::new(),
            chap: None,
        });
        sqlx::query("INSERT INTO client_offline_resets(client_id, fingerprint, offline_since, retry_after, operation) VALUES ('client', 'unchanged', 100, 450, ?)")
            .bind(serde_json::to_string(&op).unwrap())
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let pool = sqlx::SqlitePool::connect(&url).await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let recovered = sqlx::query_as::<_, ResetState>("SELECT fingerprint, offline_since, completed, failures, retry_after, operation FROM client_offline_resets WHERE client_id = 'client'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            decide(&recovered, Some(false), true, 449, 300),
            Decision::Wait
        );
        assert_eq!(
            decide(&recovered, Some(false), true, 450, 300),
            Decision::Delete
        );
        let restored: OfflineReplacement =
            serde_json::from_str(recovered.operation.as_deref().unwrap()).unwrap();
        assert_eq!(restored.backup, op.backup);
        assert_eq!(restored.spec.dataset, "pool/pc001");
        assert!(!restored.committed);

        let repository = ClientRepository::new(pool.clone());
        let client_id = ClientId::from_string("client").unwrap();
        let mut client = repository
            .find_by_id(&client_id)
            .await
            .unwrap()
            .expect("client should exist");
        client.ip = "192.168.1.102".parse().unwrap();
        repository.update(&client).await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM client_offline_resets WHERE client_id = 'client' AND offline_since = 100")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1, "a client edit must preserve the reset journal");
        pool.close().await;
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn delete_requires_confirmed_offline_for_full_delay() {
        let state = ResetState {
            offline_since: Some(100),
            ..Default::default()
        };
        assert_eq!(decide(&state, Some(false), true, 399, 300), Decision::Wait);
        assert_eq!(
            decide(&state, Some(false), true, 400, 300),
            Decision::Delete
        );
        assert_eq!(decide(&state, None, true, 400, 300), Decision::Wait);
        assert_eq!(decide(&state, Some(true), true, 400, 300), Decision::Cancel);
        assert_eq!(
            decide(&state, Some(false), false, 400, 300),
            Decision::Cancel
        );
    }

    #[test]
    fn settings_changes_apply_to_pending_deadline() {
        let state = ResetState {
            offline_since: Some(100),
            ..Default::default()
        };
        assert_eq!(decide(&state, Some(false), true, 250, 300), Decision::Wait);
        assert_eq!(
            decide(&state, Some(false), true, 250, 120),
            Decision::Delete
        );
    }

    #[test]
    fn successful_deletion_is_not_repeated_until_next_connection() {
        let state = ResetState {
            offline_since: Some(100),
            completed: true,
            ..Default::default()
        };
        assert_eq!(decide(&state, Some(false), true, 1000, 300), Decision::Wait);
        assert_eq!(
            decide(&state, Some(true), true, 1000, 300),
            Decision::Cancel
        );
        assert_eq!(
            decide(&ResetState::default(), Some(false), true, 1000, 300),
            Decision::Start
        );
    }

    #[test]
    fn retries_respect_deadline_and_backoff_is_bounded() {
        let state = ResetState {
            offline_since: Some(100),
            retry_after: 500,
            ..Default::default()
        };
        assert_eq!(decide(&state, Some(false), true, 499, 300), Decision::Wait);
        assert_eq!(
            decide(&state, Some(false), true, 500, 300),
            Decision::Delete
        );
        assert_eq!(retry_delay(0), 30);
        assert_eq!(retry_delay(100), 960);
    }

    #[tokio::test]
    async fn cancelling_coordinator_wait_keeps_mutation_lock_until_storage_work_finishes() {
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        let operation_lock = lock.clone();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let coordinator = tokio::spawn(async move {
            let child = tokio::spawn(async move {
                let _guard = operation_lock.lock().await;
                let _ = started_tx.send(());
                tokio::task::spawn_blocking(|| {
                    std::thread::sleep(std::time::Duration::from_millis(100))
                })
                .await
                .unwrap();
            });
            child.await.unwrap();
        });

        started_rx.await.unwrap();
        coordinator.abort();
        assert!(lock.try_lock().is_err());
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        assert!(lock.try_lock().is_ok());
    }
}

#[cfg(test)]
mod pending_recovery_tests {
    use super::*;
    use crate::domain::storage::ClientStorageSpec;

    type ClientMetadataRow = (
        Option<String>,
        Option<String>,
        Option<String>,
        i64,
        i64,
        Option<String>,
        Option<String>,
    );
    use std::{
        path::Path,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
    };

    struct FakeLifecycleWorkflow {
        pool: sqlx::SqlitePool,
        fail_preflight: AtomicBool,
        fail_menu: AtomicBool,
        fail_delete: AtomicBool,
        calls: std::sync::Mutex<Vec<&'static str>>,
    }

    impl LifecycleDeletionWorkflow for FakeLifecycleWorkflow {
        fn preflight(&self, _: &OfflineDeletion) -> Result<()> {
            self.calls.lock().unwrap().push("preflight");
            if self.fail_preflight.load(Ordering::SeqCst) {
                bail!("injected preflight failure");
            }
            Ok(())
        }

        fn remove_menu<'a>(
            &'a self,
            _: &'a crate::core::config::Settings,
            _: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
            Box::pin(async move {
                let journal: Option<String> = sqlx::query_scalar(
                    "SELECT operation FROM client_offline_resets WHERE client_id = 'client'",
                )
                .fetch_optional(&self.pool)
                .await?
                .flatten();
                anyhow::ensure!(journal.is_some(), "intent must precede menu removal");
                self.calls.lock().unwrap().push("menu");
                if self.fail_menu.load(Ordering::SeqCst) {
                    bail!("injected menu removal failure");
                }
                Ok(())
            })
        }

        fn delete(&self, operation: &mut OfflineDeletion) -> Result<()> {
            self.calls.lock().unwrap().push("delete");
            if !operation.removed_datasets.contains(&operation.spec.dataset) {
                operation
                    .removed_datasets
                    .push(operation.spec.dataset.clone());
            }
            if self.fail_delete.load(Ordering::SeqCst) {
                bail!("injected partial deletion failure");
            }
            Ok(())
        }
    }

    fn deletion_operation() -> OfflineDeletion {
        OfflineDeletion::new(
            ClientStorageSpec {
                client_id: "client".into(),
                client_name: "pc".into(),
                source: StorageSource::Snapshot("pool/master@ready".into()),
                dataset: "pool/clients/pc".into(),
                backstore: "block_pc".into(),
                target_iqn: "iqn.test:pc".into(),
                lun: 0,
                use_game_disk: true,
                game_disks: vec!["pool/games/game".into()],
                chap: None,
            },
            vec![OwnedGameClone {
                master_dataset: "pool/games/game".into(),
                clone_dataset: "pool/games/client-game".into(),
            }],
        )
    }

    async fn deletion_test_pool() -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO clients(id, name, mac, ip, master, snapshot, writeback, block_device, block_store, target_iqn, keep_writeback, use_game_disk, chap_enabled, chap_user, chap_secret, created_at, updated_at) VALUES ('client', 'pc', '00:11:22:33:44:55', '192.168.1.101', 'pool/master', 'pool/master@ready', 'pool/clients/pc', '/dev/zvol/pool/clients/pc', 'block_pc', 'iqn.test:pc', 1, 1, 1, 'synthetic-user', 'synthetic-secret', '2026-09-13T00:00:00+00:00', '2026-09-13T00:00:00+00:00')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO client_game_disks(client_id, master_dataset) VALUES ('client', 'pool/games/game')")
            .execute(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn journaled_deletion_orders_safety_menu_and_storage_and_keeps_metadata() {
        let pool = deletion_test_pool().await;
        let workflow = Arc::new(FakeLifecycleWorkflow {
            pool: pool.clone(),
            fail_preflight: AtomicBool::new(true),
            fail_menu: AtomicBool::new(false),
            fail_delete: AtomicBool::new(false),
            calls: std::sync::Mutex::new(Vec::new()),
        });
        let settings = crate::core::config::Settings::default();
        let mut operation = deletion_operation();

        assert!(persist_deletion_with(
            &pool,
            &settings,
            "client",
            "00:11:22:33:44:55",
            &mut operation,
            false,
            workflow.clone(),
        )
        .await
        .is_err());
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM client_offline_resets WHERE client_id = 'client'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            count, 0,
            "failed safety preflight must not journal or mutate"
        );
        assert_eq!(workflow.calls.lock().unwrap().as_slice(), ["preflight"]);

        workflow.fail_preflight.store(false, Ordering::SeqCst);
        workflow.fail_menu.store(true, Ordering::SeqCst);
        assert!(persist_deletion_with(
            &pool,
            &settings,
            "client",
            "00:11:22:33:44:55",
            &mut operation,
            false,
            workflow.clone(),
        )
        .await
        .is_err());
        assert_eq!(
            workflow.calls.lock().unwrap().as_slice(),
            ["preflight", "preflight", "preflight", "menu"]
        );
        let pending: Option<String> = sqlx::query_scalar(
            "SELECT operation FROM client_offline_resets WHERE client_id = 'client'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(pending.is_some(), "menu failure retains durable intent");

        workflow.fail_menu.store(false, Ordering::SeqCst);
        workflow.fail_delete.store(true, Ordering::SeqCst);
        assert!(persist_deletion_with(
            &pool,
            &settings,
            "client",
            "00:11:22:33:44:55",
            &mut operation,
            true,
            workflow.clone(),
        )
        .await
        .is_err());
        let pending: Option<String> = sqlx::query_scalar(
            "SELECT operation FROM client_offline_resets WHERE client_id = 'client'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let PendingJournal::Deletion(restored) =
            deserialize_pending_journal(serde_json::from_str(pending.as_deref().unwrap()).unwrap())
                .unwrap()
        else {
            panic!("expected deletion journal");
        };
        assert_eq!(restored.removed_datasets, ["pool/clients/pc"]);

        workflow.fail_delete.store(false, Ordering::SeqCst);
        persist_deletion_with(
            &pool,
            &settings,
            "client",
            "00:11:22:33:44:55",
            &mut restored.clone(),
            true,
            workflow.clone(),
        )
        .await
        .unwrap();
        let cleared: Option<String> = sqlx::query_scalar(
            "SELECT operation FROM client_offline_resets WHERE client_id = 'client'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(cleared.is_none());
        let metadata: ClientMetadataRow = sqlx::query_as(
            "SELECT snapshot, writeback, target_iqn, keep_writeback, use_game_disk, chap_user, chap_secret FROM clients WHERE id = 'client'",
        ).fetch_one(&pool).await.unwrap();
        assert_eq!(
            metadata,
            (
                Some("pool/master@ready".into()),
                Some("pool/clients/pc".into()),
                Some("iqn.test:pc".into()),
                1,
                1,
                Some("synthetic-user".into()),
                Some("synthetic-secret".into())
            )
        );
        let selected: Vec<String> = sqlx::query_scalar(
            "SELECT master_dataset FROM client_game_disks WHERE client_id = 'client'",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(selected, ["pool/games/game"]);
        assert!(workflow
            .calls
            .lock()
            .unwrap()
            .ends_with(&["preflight", "menu", "delete"]));
    }

    #[tokio::test]
    async fn failed_deletion_progress_is_durable_without_changing_client_metadata() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO clients(id, name, mac, ip, master, snapshot, writeback, block_device, block_store, target_iqn, keep_writeback, created_at, updated_at) VALUES ('client', 'pc', '00:11:22:33:44:55', '192.168.1.101', 'pool/master', 'pool/master@ready', 'pool/clients/pc', '/dev/zvol/pool/clients/pc', 'block_pc', 'iqn.test:pc', 1, '2026-09-13T00:00:00+00:00', '2026-09-13T00:00:00+00:00')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO client_offline_resets(client_id, fingerprint, failures, retry_after, operation) VALUES ('client', 'persisted', 2, 123, '{}')")
            .execute(&pool).await.unwrap();

        let mut operation = OfflineDeletion::new(
            ClientStorageSpec {
                client_id: "client".into(),
                client_name: "pc".into(),
                source: StorageSource::Snapshot("pool/master@ready".into()),
                dataset: "pool/clients/pc".into(),
                backstore: "block_pc".into(),
                target_iqn: "iqn.test:pc".into(),
                lun: 0,
                use_game_disk: false,
                game_disks: Vec::new(),
                chap: None,
            },
            Vec::new(),
        );
        // Mirrors a fake storage adapter that completed one deletion step then failed.
        operation.removed_datasets.push("pool/clients/pc".into());
        save_deletion_progress(&pool, "client", &operation, false)
            .await
            .unwrap();

        let (journal, completed, failures, retry_after): (Option<String>, i64, i64, i64) =
            sqlx::query_as("SELECT operation, completed, failures, retry_after FROM client_offline_resets WHERE client_id = 'client'")
                .fetch_one(&pool).await.unwrap();
        let PendingJournal::Deletion(restored) =
            deserialize_pending_journal(serde_json::from_str(journal.as_deref().unwrap()).unwrap())
                .unwrap()
        else {
            panic!("expected deletion journal");
        };
        assert_eq!(restored.removed_datasets, operation.removed_datasets);
        assert_eq!((completed, failures, retry_after), (0, 2, 123));
        let metadata: (Option<String>, Option<String>, Option<String>, i64) = sqlx::query_as(
            "SELECT snapshot, writeback, target_iqn, keep_writeback FROM clients WHERE id = 'client'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            metadata,
            (
                Some("pool/master@ready".into()),
                Some("pool/clients/pc".into()),
                Some("iqn.test:pc".into()),
                1
            )
        );
    }

    #[derive(Default)]
    struct FakeRecoveryOperations {
        connected: bool,
        exported: bool,
        fail_recovery: bool,
        recovery_called: AtomicBool,
    }

    impl PendingRecoveryOperations for FakeRecoveryOperations {
        fn target_connected(&self, _: &str) -> Result<bool> {
            Ok(self.connected)
        }

        fn device_exported(&self, _: &Path) -> Result<bool> {
            Ok(self.exported)
        }

        fn recover_offline(&self, _: &OfflineReplacement) -> Result<()> {
            self.recovery_called.store(true, Ordering::SeqCst);
            if self.fail_recovery {
                bail!("injected recovery failure");
            }
            Ok(())
        }
    }

    fn operation(client_id: &str) -> OfflineReplacement {
        OfflineReplacement::new(ClientStorageSpec {
            client_id: client_id.to_owned(),
            client_name: "client".into(),
            source: StorageSource::Snapshot("pool/master@ready".into()),
            dataset: "pool/client".into(),
            backstore: "block_client".into(),
            target_iqn: "iqn.test:client".into(),
            lun: 0,
            use_game_disk: false,
            game_disks: Vec::new(),
            chap: None,
        })
    }

    fn client(id: &str) -> Client {
        Client {
            id: crate::domain::ClientId::from_string(id.to_owned()).unwrap(),
            name: "client".into(),
            mac: crate::domain::MacAddress::parse("00:11:22:33:44:55").unwrap(),
            ip: "192.168.1.101".parse().unwrap(),
            master: "pool/master".into(),
            enabled: true,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            snapshot: None,
            block_store: None,
            target_iqn: Some("iqn.test:client".into()),
            writeback: None,
            last_modified: None,
            block_device: Some("/dev/zvol/pool/client".into()),
            status: crate::domain::ClientStatus::Offline,
            mode: crate::domain::BootMode::Normal,
            pxe_mode: crate::domain::PxeMode::Uefi,
            keep_writeback: false,
            use_game_disk: false,
            game_disks: Vec::new(),
            chap_user: None,
            chap_secret: None,
            chap_enabled: false,
        }
    }

    async fn pending_pool(operation: &OfflineReplacement) -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO clients(id, name, mac, ip, master, created_at, updated_at) VALUES ('client', 'PC001', '00:11:22:33:44:55', '192.168.1.101', 'pool/master', '2026-09-13T00:00:00+00:00', '2026-09-13T00:00:00+00:00')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO client_offline_resets(client_id, fingerprint, failures, retry_after, operation, last_error) VALUES ('client', 'fingerprint', 2, 123, ?, 'previous failure')")
            .bind(serde_json::to_string(operation).unwrap())
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn shared_recovery_clears_only_the_journal_after_success() {
        let operation = operation("client");
        let pool = pending_pool(&operation).await;
        let operations = Arc::new(FakeRecoveryOperations::default());

        recover_pending_operation(&pool, &client("client"), &operation, operations.clone())
            .await
            .unwrap();

        let state: (Option<String>, i64, i64, Option<String>) = sqlx::query_as(
            "SELECT operation, failures, retry_after, last_error FROM client_offline_resets WHERE client_id = 'client'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(state.0, None);
        assert_eq!(
            (state.1, state.2, state.3.as_deref()),
            (2, 123, Some("previous failure"))
        );
        assert!(operations.recovery_called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn shared_recovery_rejects_unsafe_or_foreign_journals_without_mutation() {
        for (client_id, connected, exported, expected_safety_error) in [
            ("client", true, false, Some(true)),
            ("client", false, true, Some(false)),
            ("another-client", false, false, None),
        ] {
            let operation = operation(client_id);
            let pool = pending_pool(&operation).await;
            let operations = Arc::new(FakeRecoveryOperations {
                connected,
                exported,
                ..Default::default()
            });

            let result =
                recover_pending_operation(&pool, &client("client"), &operation, operations.clone())
                    .await;

            match (expected_safety_error, result) {
                (Some(true), Err(error)) => {
                    assert!(matches!(
                        &error,
                        PendingRecoveryError::Safety(PendingRecoverySafetyError::ClientConnected)
                    ));
                }
                (Some(false), Err(error)) => {
                    assert!(matches!(
                        &error,
                        PendingRecoveryError::Safety(PendingRecoverySafetyError::NvmeExported)
                    ));
                }
                (None, Err(error)) => assert!(error.to_string().contains("client mismatch")),
                _ => panic!("recovery unexpectedly ran"),
            }
            let remaining: Option<String> = sqlx::query_scalar(
                "SELECT operation FROM client_offline_resets WHERE client_id = 'client'",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert!(remaining.is_some());
            assert!(!operations.recovery_called.load(Ordering::SeqCst));
        }
    }

    #[tokio::test]
    async fn failed_storage_recovery_retains_its_journal() {
        let operation = operation("client");
        let pool = pending_pool(&operation).await;
        let operations = Arc::new(FakeRecoveryOperations {
            fail_recovery: true,
            ..Default::default()
        });

        let result =
            recover_pending_operation(&pool, &client("client"), &operation, operations).await;

        assert!(matches!(result, Err(PendingRecoveryError::Attempt(_))));
        let remaining: Option<String> = sqlx::query_scalar(
            "SELECT operation FROM client_offline_resets WHERE client_id = 'client'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(remaining.is_some());
    }

    #[tokio::test]
    async fn manual_storage_failure_records_backoff_without_clearing_journal() {
        let operation = operation("client");
        let pool = pending_pool(&operation).await;
        let before = chrono::Utc::now().timestamp();

        record_manual_recovery_failure(&pool, "client", 2, &anyhow::anyhow!("recovery failed"))
            .await
            .unwrap();

        let state: (Option<String>, i64, i64, Option<String>) = sqlx::query_as(
            "SELECT operation, failures, retry_after, last_error FROM client_offline_resets WHERE client_id = 'client'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(state.0.is_some());
        assert_eq!(state.1, 3);
        assert!(
            (before + retry_delay(2)..=chrono::Utc::now().timestamp() + retry_delay(2))
                .contains(&state.2)
        );
        assert_eq!(state.3.as_deref(), Some("recovery failed"));
    }
}
