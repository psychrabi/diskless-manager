//! Durable offline deadlines; independent of UI connections and ping status.
use super::storage_service::OfflineReplacement;
use crate::infrastructure::iscsi::reconcile::confirmed_target_connected as session_state;
use crate::{
    core::client::{Client, ClientManager},
    domain::storage::{ClientStorageSpec, StorageSource},
    state::AppState,
};
use anyhow::{bail, Context, Result};

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
        client_id: client.id.clone(),
        source: StorageSource::Snapshot(snapshot.to_owned()),
        dataset: dataset.to_owned(),
        backstore: format!("block_{}", client.name.trim().to_lowercase()),
        target_iqn: client
            .target_iqn
            .clone()
            .unwrap_or_else(|| format!("{prefix}:client.{}", client.name.trim().to_lowercase())),
        lun: 0,
        use_game_disk: client.use_game_disk.unwrap_or(false),
    })
}

fn ensure_no_nvme_export(client: &Client) -> Result<()> {
    let device = client
        .block_device
        .as_deref()
        .or(client.block_store.as_deref())
        .context("client has no disk")?;
    if crate::infrastructure::nvmeof::block_device_is_exported(std::path::Path::new(device))? {
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
    let clients = ClientManager::new(state.db_pool.clone()).list().await?;
    for client in clients {
        // Shares a lock with edits/deletion/NVMe export creation. Re-read after
        // acquiring it so an old snapshot of configuration cannot trigger a reset.
        let _guard = state.client_mutations.lock().await;
        let current = ClientManager::new(state.db_pool.clone())
            .get(&client.id)
            .await?;
        if let Err(error) = process(state, &current).await {
            tracing::warn!(client_id = %client.id, %error, "automatic client reset deferred");
            let failures: Option<i64> = sqlx::query_scalar(
                "SELECT failures FROM client_offline_resets WHERE client_id = ?",
            )
            .bind(&client.id)
            .fetch_optional(&state.db_pool)
            .await?;
            if let Some(failures) = failures {
                sqlx::query("UPDATE client_offline_resets SET failures = failures + 1, retry_after = ?, last_error = ? WHERE client_id = ?")
                    .bind(chrono::Utc::now().timestamp() + retry_delay(failures)).bind(format!("{error:#}")).bind(&client.id).execute(&state.db_pool).await?;
            }
        }
    }
    Ok(())
}

async fn process(state: &AppState, client: &Client) -> Result<()> {
    let settings = state.settings.read().await.clone();
    let now = chrono::Utc::now().timestamp();
    let eligible = client.keep_writeback == Some(false)
        && client.snapshot.as_deref().is_some_and(|s| !s.is_empty());
    let fingerprint = serde_json::to_string(&(
        client.snapshot.as_ref(),
        client.block_device.as_ref(),
        client.block_store.as_ref(),
        client.target_iqn.as_ref(),
        &client.name,
        client.keep_writeback,
    ))?;
    let saved = sqlx::query_as::<_, ResetState>("SELECT fingerprint, offline_since, completed, failures, retry_after, operation FROM client_offline_resets WHERE client_id = ?")
        .bind(&client.id).fetch_optional(&state.db_pool).await?;
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
        if connected == Some(true) {
            let mut operation: OfflineReplacement = serde_json::from_str(journal)?;
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
                    .bind(&client.id)
                    .execute(&state.db_pool)
                    .await?;
                }
            }
            sqlx::query("UPDATE client_offline_resets SET offline_since = NULL, completed = 0, failures = 0, retry_after = 0 WHERE client_id = ?")
                .bind(&client.id).execute(&state.db_pool).await?;
            return Ok(());
        }
        if connected.is_none() {
            return Ok(());
        }
        if now < saved.retry_after {
            return Ok(());
        }
        ensure_no_nvme_export(client)?;
        let operation: OfflineReplacement = serde_json::from_str(journal)?;
        let storage = state.application.storage.clone();
        tokio::task::spawn_blocking(move || storage.recover_offline(&operation)).await??;
        sqlx::query("UPDATE client_offline_resets SET operation = NULL WHERE client_id = ?")
            .bind(&client.id)
            .execute(&state.db_pool)
            .await?;
        saved.operation = None;
    }
    if saved.fingerprint != fingerprint {
        saved = ResetState {
            fingerprint: fingerprint.clone(),
            ..Default::default()
        };
        sqlx::query("INSERT INTO client_offline_resets (client_id, fingerprint) VALUES (?, ?) ON CONFLICT(client_id) DO UPDATE SET fingerprint = excluded.fingerprint, offline_since = NULL, completed = 0, failures = 0, retry_after = 0, last_error = NULL")
            .bind(&client.id).bind(&fingerprint).execute(&state.db_pool).await?;
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
                .bind(&client.id).execute(&state.db_pool).await?;
        }
        Decision::Start => {
            sqlx::query("UPDATE client_offline_resets SET offline_since = ? WHERE client_id = ?")
                .bind(now)
                .bind(&client.id)
                .execute(&state.db_pool)
                .await?;
        }
        Decision::Wait => {}
        Decision::Reset => {
            ensure_no_nvme_export(client)?;
            let spec = storage_spec(client, &settings.iscsi.target_prefix)?;
            // Shared datasets are never eligible even if a legacy record claims ownership.
            let other: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM clients WHERE id <> ? AND (block_device = ? OR block_store = ? OR master = ?)")
                .bind(&client.id).bind(spec.block_device().to_string_lossy().as_ref()).bind(spec.block_device().to_string_lossy().as_ref()).bind(&spec.dataset)
                .fetch_one(&state.db_pool).await?;
            if other != 0 {
                bail!("client disk is shared with another client");
            }
            let mut operation = OfflineReplacement::new(spec);
            sqlx::query("UPDATE client_offline_resets SET operation = ? WHERE client_id = ?")
                .bind(serde_json::to_string(&operation)?)
                .bind(&client.id)
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
                        .bind(serde_json::to_string(&operation)?).bind(&client.id).execute(&state.db_pool).await?;
                    tracing::info!(client_id = %client.id, "non-persistent clone reset completed");
                }
                Err(error) => {
                    sqlx::query("UPDATE client_offline_resets SET failures = failures + 1, retry_after = ?, last_error = ? WHERE client_id = ?")
                        .bind(now + retry_delay(saved.failures)).bind(format!("{error:#}")).bind(&client.id).execute(&state.db_pool).await?;
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
                .bind(&client.id)
                .execute(&state.db_pool)
                .await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn pending_deadline_and_replacement_journal_survive_database_restart() {
        let path = std::env::temp_dir().join(format!("diskless-reset-{}.db", uuid::Uuid::new_v4()));
        let url = format!("sqlite:{}?mode=rwc", path.display());
        let pool = sqlx::SqlitePool::connect(&url).await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO clients(id, name, mac, ip, master, created_at, updated_at) VALUES ('client', 'PC001', '00:11:22:33:44:55', '192.168.1.101', 'pool/master', 'now', 'now')").execute(&pool).await.unwrap();
        let op = OfflineReplacement::new(ClientStorageSpec {
            client_id: "client".into(),
            source: StorageSource::Snapshot("pool/master@ready".into()),
            dataset: "pool/pc001".into(),
            backstore: "block_pc001".into(),
            target_iqn: "iqn.test:pc001".into(),
            lun: 0,
            use_game_disk: false,
        });
        sqlx::query("INSERT INTO client_offline_resets(client_id, fingerprint, offline_since, retry_after, operation) VALUES ('client', 'unchanged', 100, 450, ?)")
            .bind(serde_json::to_string(&op).unwrap()).execute(&pool).await.unwrap();
        pool.close().await;
        let pool = sqlx::SqlitePool::connect(&url).await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let recovered = sqlx::query_as::<_, ResetState>("SELECT fingerprint, offline_since, completed, failures, retry_after, operation FROM client_offline_resets WHERE client_id = 'client'").fetch_one(&pool).await.unwrap();
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
        let mut client = ClientManager::new(pool.clone())
            .get("client")
            .await
            .unwrap();
        client.ip = "192.168.1.102".into();
        ClientManager::upsert_client(&pool, &client).await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM client_offline_resets WHERE client_id = 'client' AND offline_since = 100").fetch_one(&pool).await.unwrap();
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
