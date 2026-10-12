//! Durable offline deadlines; independent of UI connections and ping status.
use super::storage_service::{OfflineReplacement, StorageService};
use crate::infrastructure::iscsi::reconcile::confirmed_target_connected as session_state;
use crate::{
    domain::{
        storage::{ClientStorageSpec, StorageSource},
        Client,
    },
    persistence::ClientRepository,
    state::AppState,
};
use anyhow::{bail, Context, Result};
use std::sync::Arc;

#[derive(Debug, Default, sqlx::FromRow)]
struct ResetState {
    fingerprint: String,
    offline_since: Option<i64>,
    completed: bool,
    failures: i64,
    retry_after: i64,
    operation: Option<String>,
}

#[derive(Debug, PartialEq)]
enum Decision {
    Cancel,
    Start,
    Wait,
    Reset,
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
            Decision::Reset
        }
        Some(_) => Decision::Wait,
    }
}

fn retry_delay(failures: i64) -> i64 {
    30 * (1_i64 << failures.clamp(0, 5))
}

fn storage_spec(client: &Client, prefix: &str) -> Result<ClientStorageSpec> {
    let snapshot = client
        .snapshot
        .as_deref()
        .filter(|s| s.contains('@'))
        .context("client has no snapshot")?;
    let device = client
        .block_device
        .as_deref()
        .or(client.block_store.as_deref())
        .context("client has no persisted disk")?;
    let dataset = device
        .strip_prefix("/dev/zvol/")
        .context("client disk is not a ZVOL")?;
    if dataset == client.master
        || snapshot.split('@').next() == Some(dataset)
        || dataset.contains("..")
        || dataset.contains('@')
    {
        bail!("refusing to reset a master or invalid dataset");
    }
    Ok(ClientStorageSpec {
        client_id: client.id.to_string(),
        client_name: client.name.clone(),
        source: StorageSource::Snapshot(snapshot.to_owned()),
        dataset: dataset.to_owned(),
        backstore: format!("block_{}", client.name.trim().to_lowercase()),
        target_iqn: client
            .target_iqn
            .clone()
            .unwrap_or_else(|| format!("{prefix}:client.{}", client.name.trim().to_lowercase())),
        lun: 0,
        use_game_disk: client.use_game_disk,
        // Boot-only spec: game clones are reset separately with the
        // resolved selection in the Reset branch below.
        game_disks: Vec::new(),
        // Resolved by the caller (Reset branch); journals from older
        // builds deserialize this as None via serde default.
        chap: None,
    })
}

async fn ensure_no_nvme_export(client: &Client) -> Result<()> {
    let device = client
        .block_device
        .as_deref()
        .or(client.block_store.as_deref())
        .context("client has no disk")?;
    let device = std::path::PathBuf::from(device);
    if tokio::task::spawn_blocking(move || {
        crate::infrastructure::nvmeof::block_device_is_exported(&device)
    })
    .await??
    {
        bail!("automatic reset deferred while an NVMe export exists");
    }
    Ok(())
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
    for client in clients {
        // Shares a lock with edits/deletion/NVMe export creation. Re-read after
        // acquiring it so an old snapshot of configuration cannot trigger a reset.
        let _guard = state.client_mutations.lock().await;
        let current = repository.find_by_id(&client.id).await?.with_context(|| {
            format!(
                "client '{}' disappeared during lifecycle processing",
                client.id
            )
        })?;
        if let Err(error) = process(state, &current).await {
            tracing::warn!(client_id = %client.id, %error, "automatic client reset deferred");
            let failures: Option<i64> = sqlx::query_scalar(
                "SELECT failures FROM client_offline_resets WHERE client_id = ?",
            )
            .bind(client.id.as_str())
            .fetch_optional(&state.db_pool)
            .await?;
            if let Some(failures) = failures {
                sqlx::query("UPDATE client_offline_resets SET failures = failures + 1, retry_after = ?, last_error = ? WHERE client_id = ?")
                    .bind(chrono::Utc::now().timestamp() + retry_delay(failures))
                    .bind(format!("{error:#}"))
                    .bind(client.id.as_str())
                    .execute(&state.db_pool)
                    .await?;
            }
        }
    }
    Ok(())
}

async fn process(state: &AppState, client: &Client) -> Result<()> {
    let settings = state.settings.read().await.clone();
    let now = chrono::Utc::now().timestamp();
    let eligible =
        !client.keep_writeback && client.snapshot.as_deref().is_some_and(|s| !s.is_empty());
    let fingerprint = serde_json::to_string(&(
        client.snapshot.as_ref(),
        client.block_device.as_ref(),
        client.block_store.as_ref(),
        client.target_iqn.as_ref(),
        &client.name,
        client.keep_writeback,
    ))?;
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
    let connected = tokio::task::spawn_blocking(move || session_state(&target_copy))
        .await?
        .ok();

    // Finish a journal before clearing a deadline or accepting new settings.
    if let Some(journal) = saved.operation.as_deref() {
        let mut operation: OfflineReplacement = serde_json::from_str(journal)?;
        validate_recovery_owner(client.id.as_str(), &operation)?;
        if connected == Some(true) {
            if !operation.committed {
                let storage = state.application.storage.clone();
                let candidate = operation.clone();
                if tokio::task::spawn_blocking(move || storage.replacement_is_attached(&candidate))
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
        Decision::Reset => {
            ensure_no_nvme_export(client).await?;
            let mut spec = storage_spec(client, &settings.iscsi.target_prefix)?;
            // A rebuilt target must keep its authentication: resolve (and
            // generate, once) the client's CHAP credentials here.
            spec.chap = crate::core::reconciliation::ensure_chap_credentials(
                &state.db_pool,
                client.id.as_str(),
                &client.name,
                client.chap_enabled,
            )
            .await
            .map_err(|error| anyhow::anyhow!("failed to resolve CHAP credentials: {error:#}"))?;
            // Shared datasets are never eligible even if a legacy record claims ownership.
            let other: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM clients WHERE id <> ? AND (block_device = ? OR block_store = ? OR master = ?)",
            )
            .bind(client.id.as_str())
            .bind(spec.block_device().to_string_lossy().as_ref())
            .bind(spec.block_device().to_string_lossy().as_ref())
            .bind(&spec.dataset)
            .fetch_one(&state.db_pool)
            .await?;
            if other != 0 {
                bail!("client disk is shared with another client");
            }
            let mut operation = OfflineReplacement::new(spec);
            sqlx::query("UPDATE client_offline_resets SET operation = ? WHERE client_id = ?")
                .bind(serde_json::to_string(&operation)?)
                .bind(client.id.as_str())
                .execute(&state.db_pool)
                .await?;
            let storage = state.application.storage.clone();
            let work = operation.clone();
            let result = tokio::task::spawn_blocking(move || {
                if session_state(&work.spec.target_iqn)? {
                    bail!("client reconnected before reset");
                }
                storage.replace_offline(&work)
            })
            .await?;
            match result {
                Ok(()) => {
                    operation.committed = true;
                    sqlx::query("UPDATE client_offline_resets SET completed = 1, failures = 0, last_error = NULL, operation = ? WHERE client_id = ?")
                        .bind(serde_json::to_string(&operation)?)
                        .bind(client.id.as_str())
                        .execute(&state.db_pool)
                        .await?;
                    tracing::info!(client_id = %client.id, "non-persistent clone reset completed");
                    // Game clones follow the boot image lifetime: reset
                    // them now for non-persistent clients. Super clients
                    // never reach this branch (`eligible` is false when
                    // `keep_writeback` is set), so their clones persist.
                    if client.use_game_disk {
                        match crate::core::reconciliation::resolved_game_selection(
                            &state.db_pool,
                            &state.application.storage,
                            client.id.as_str(),
                            true,
                        )
                        .await
                        {
                            Ok(masters) => {
                                let storage = state.application.storage.clone();
                                let client_id = client.id.to_string();
                                let client_name = client.name.clone();
                                let game_target = target.clone();
                                let chap = operation.spec.chap.clone();
                                let result = tokio::task::spawn_blocking(move || {
                                    storage.reset_game_clones(
                                        &client_id,
                                        &client_name,
                                        &game_target,
                                        &masters,
                                        chap.as_ref(),
                                    )
                                })
                                .await?;
                                if let Err(error) = result {
                                    // The boot reset already committed; game
                                    // clones heal on the next ensure. Never
                                    // fail a boot reset over games.
                                    tracing::error!(client_id = %client.id, %error, "game clone reset failed; will retry on next ensure");
                                }
                            }
                            Err(error) => {
                                tracing::error!(client_id = %client.id, %error, "failed to resolve game selection for reset");
                            }
                        }
                    }
                }
                Err(error) => {
                    sqlx::query("UPDATE client_offline_resets SET failures = failures + 1, retry_after = ?, last_error = ? WHERE client_id = ?")
                        .bind(now + retry_delay(saved.failures))
                        .bind(format!("{error:#}"))
                        .bind(client.id.as_str())
                        .execute(&state.db_pool)
                        .await?;
                    tracing::error!(client_id = %client.id, %error, "clone reset failed; restoring previous clone");
                }
            }
            // Leave the journal intact when recovery fails; startup resumes it.
            let storage = state.application.storage.clone();
            tokio::task::spawn_blocking(move || {
                if session_state(&operation.spec.target_iqn)? {
                    bail!("client connected; cleanup deferred");
                }
                storage.recover_offline(&operation)
            })
            .await??;
            sqlx::query("UPDATE client_offline_resets SET operation = NULL WHERE client_id = ?")
                .bind(client.id.as_str())
                .execute(&state.db_pool)
                .await?;
        }
    }
    Ok(())
}

/// Manually retry a persisted storage recovery without starting a new reset.
#[derive(Debug, thiserror::Error)]
pub enum PendingRecoverySafetyError {
    #[error("client is connected; disconnect it before retrying recovery")]
    ClientConnected,
    #[error("client disk is exported over NVMe-oF; remove that export before retrying recovery")]
    NvmeExported,
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
    let operation: OfflineReplacement = serde_json::from_str(&journal)?;
    validate_recovery_owner(id, &operation)?;
    let client_id = crate::domain::ClientId::from_string(id.to_owned())?;
    let client = ClientRepository::new(state.db_pool.clone())
        .find_by_id(&client_id)
        .await?
        .context("client no longer exists")?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ClientId;

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
            Decision::Reset
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
    fn reset_requires_confirmed_offline_for_full_delay() {
        let state = ResetState {
            offline_since: Some(100),
            ..Default::default()
        };
        assert_eq!(decide(&state, Some(false), true, 399, 300), Decision::Wait);
        assert_eq!(decide(&state, Some(false), true, 400, 300), Decision::Reset);
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
        assert_eq!(decide(&state, Some(false), true, 250, 120), Decision::Reset);
    }

    #[test]
    fn successful_reset_is_not_repeated_until_next_connection() {
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
        assert_eq!(decide(&state, Some(false), true, 500, 300), Decision::Reset);
        assert_eq!(retry_delay(0), 30);
        assert_eq!(retry_delay(100), 960);
    }
}

#[cfg(test)]
mod pending_recovery_tests {
    use super::*;
    use crate::domain::storage::ClientStorageSpec;
    use std::{
        path::Path,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
    };

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
