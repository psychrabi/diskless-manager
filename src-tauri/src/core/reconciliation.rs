use crate::application::storage_service::{resolve_game_selection, StorageService};
use crate::core::provisioning::ClientStoragePaths;
use crate::domain::{
    storage::{ClientStorageSpec, StorageReconcileResult, StorageSource, StorageState},
    Client, ClientId,
};
use crate::infrastructure::iscsi::target_has_active_sessions;
use crate::persistence::ClientRepository;
use crate::state::AppState;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReconciliationOutcome {
    Ready,
    Partial,
    Missing,
    Error,
    Skipped,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReconciliationEntry {
    pub client_id: String,
    pub client_name: String,
    pub outcome: ReconciliationOutcome,
    pub message: String,
    pub target_iqn: Option<String>,
    pub dataset: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReconciliationSummary {
    pub checked: usize,
    pub ready: usize,
    pub partial: usize,
    pub missing: usize,
    pub errors: usize,
    pub skipped: usize,
    pub clients: Vec<ReconciliationEntry>,
}

impl ReconciliationSummary {
    fn new() -> Self {
        Self {
            checked: 0,
            ready: 0,
            partial: 0,
            missing: 0,
            errors: 0,
            skipped: 0,
            clients: Vec::new(),
        }
    }

    fn push(&mut self, entry: ReconciliationEntry) {
        self.checked += 1;

        match &entry.outcome {
            ReconciliationOutcome::Ready => self.ready += 1,
            ReconciliationOutcome::Partial => self.partial += 1,
            ReconciliationOutcome::Missing => self.missing += 1,
            ReconciliationOutcome::Error => self.errors += 1,
            ReconciliationOutcome::Skipped => self.skipped += 1,
        }

        self.clients.push(entry);
    }
}

/// Inspect persisted clients against the current ZFS and iSCSI state.
///
/// Inspection never changes infrastructure. Repair is a separate operation.
pub async fn inspect_storage(state: &AppState) -> anyhow::Result<ReconciliationSummary> {
    let clients = ClientRepository::new(state.db_pool.clone()).find_all().await?;

    let mut summary = ReconciliationSummary::new();

    for client in clients {
        match storage_spec_for_client(&state.db_pool, &client).await {
            Ok(Some(spec)) => match state.application.storage.reconcile_client_storage(&spec) {
                Ok(result) => summary.push(entry_from_result(&client, &spec, result)),
                Err(error) => summary.push(ReconciliationEntry {
                    client_id: client.id.to_string(),
                    client_name: client.name.clone(),
                    outcome: ReconciliationOutcome::Error,
                    message: error.to_string(),
                    target_iqn: Some(spec.target_iqn),
                    dataset: Some(spec.dataset),
                }),
            },
            Ok(None) => summary.push(ReconciliationEntry {
                client_id: client.id.to_string(),
                client_name: client.name.clone(),
                outcome: ReconciliationOutcome::Skipped,
                message: "Client has no storage configuration to reconcile".to_string(),
                target_iqn: client.target_iqn.clone(),
                dataset: client
                    .writeback
                    .clone()
                    .or_else(|| (!client.master.is_empty()).then(|| client.master.clone())),
            }),
            Err(error) => summary.push(ReconciliationEntry {
                client_id: client.id.to_string(),
                client_name: client.name.clone(),
                outcome: ReconciliationOutcome::Error,
                message: error.to_string(),
                target_iqn: client.target_iqn.clone(),
                dataset: client
                    .writeback
                    .clone()
                    .or_else(|| (!client.master.is_empty()).then(|| client.master.clone())),
            }),
        }
    }

    Ok(summary)
}

/// Refuse destructive storage repair while an initiator is connected.
pub(crate) fn ensure_storage_repair_safe(
    target_iqn: &str,
    active_sessions: bool,
) -> anyhow::Result<()> {
    if active_sessions {
        anyhow::bail!(
            "Client storage is in use: active iSCSI session on target '{}'. Disconnect the client before repair.",
            target_iqn
        );
    }

    Ok(())
}

/// Repair one persisted client's ZFS/iSCSI state to the desired configuration.
///
/// The operation is explicit. It does not run during application startup.
pub async fn repair_client_storage(
    state: &AppState,
    client_id: &str,
) -> anyhow::Result<ReconciliationEntry> {
    let id = ClientId::from_string(client_id.to_owned()).map_err(|error| anyhow::anyhow!(error))?;
    let client = ClientRepository::new(state.db_pool.clone())
        .find_by_id(&id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("client '{}' not found", client_id))?;
    let spec = storage_spec_for_client(&state.db_pool, &client).await?.ok_or_else(|| {
        anyhow::anyhow!(
            "client '{}' has no storage configuration to reconcile",
            client_id
        )
    })?;

    ensure_storage_repair_safe(
        &spec.target_iqn,
        target_has_active_sessions(&spec.target_iqn)?,
    )?;

    let storage = state
        .application
        .storage
        .reconcile_client_storage_in_place(&spec)?;

    Ok(ReconciliationEntry {
        client_id: client.id.to_string(),
        client_name: client.name,
        outcome: ReconciliationOutcome::Ready,
        message: format!("Storage reconciled successfully for '{}'", client_id),
        target_iqn: Some(storage.target_iqn().to_string()),
        dataset: Some(storage.dataset().to_string()),
    })
}

/// Stored per-client game master selection (possibly empty).
pub(crate) async fn stored_game_selection(
    pool: &sqlx::SqlitePool,
    client_id: &str,
) -> anyhow::Result<Vec<String>> {
    Ok(sqlx::query_scalar::<_, String>(
        "SELECT master_dataset FROM client_game_disks WHERE client_id = ?",
    )
    .bind(client_id)
    .fetch_all(pool)
    .await?)
}

/// Effective game masters: stored selection intersected with discovered
/// masters, or all discovered masters when the switch is on with an
/// empty selection. See `StorageService::resolve_game_selection`.
pub(crate) async fn resolved_game_selection(
    pool: &sqlx::SqlitePool,
    client_id: &str,
    use_game_disk: bool,
) -> anyhow::Result<Vec<String>> {
    let stored = stored_game_selection(pool, client_id).await?;
    let discovered = StorageService::discover_game_masters()?
        .into_iter()
        .map(|master| master.dataset)
        .collect::<Vec<_>>();
    Ok(resolve_game_selection(
        use_game_disk,
        &stored,
        &discovered,
    ))
}

/// Stored CHAP state: (enabled, username, secret).
pub(crate) async fn stored_chap_state(
    pool: &sqlx::SqlitePool,
    client_id: &str,
) -> anyhow::Result<(bool, Option<String>, Option<String>)> {
    let row: Option<(Option<i64>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT chap_enabled, chap_user, chap_secret FROM clients WHERE id = ?",
    )
    .bind(client_id)
    .fetch_optional(pool)
    .await?;
    Ok(match row {
        Some((enabled, user, secret)) => (enabled.unwrap_or(0) != 0, user, secret),
        None => (false, None, None),
    })
}

/// Read-only CHAP credentials: `Some` only when stored credentials exist.
/// Enforcement is decided by the caller from the client's `chap_enabled`
/// flag; storage and policy stay separate here.
pub(crate) async fn read_chap_credentials(
    pool: &sqlx::SqlitePool,
    client_id: &str,
) -> anyhow::Result<Option<crate::infrastructure::iscsi::ChapCredentials>> {
    let (_, user, secret) = stored_chap_state(pool, client_id).await?;
    Ok(match (user, secret) {
        (Some(username), Some(password))
            if !username.trim().is_empty() && !password.is_empty() =>
        {
            Some(crate::infrastructure::iscsi::ChapCredentials { username, password })
        }
        _ => None,
    })
}

/// Ensure CHAP credentials exist, generating and persisting them on first
/// use (and marking enforcement on, since generation only happens for
/// opted-in clients). Returns `None` when enforcement is off.
pub(crate) async fn ensure_chap_credentials(
    pool: &sqlx::SqlitePool,
    client_id: &str,
    client_name: &str,
    chap_enabled: bool,
) -> anyhow::Result<Option<crate::infrastructure::iscsi::ChapCredentials>> {
    use crate::infrastructure::iscsi::ChapCredentials;

    if !chap_enabled {
        return Ok(None);
    }
    if let Some(creds) = read_chap_credentials(pool, client_id).await? {
        return Ok(Some(creds));
    }
    let creds = ChapCredentials::generate(client_name)?;
    sqlx::query("UPDATE clients SET chap_user = ?, chap_secret = ?, chap_enabled = 1 WHERE id = ?")
        .bind(&creds.username)
        .bind(&creds.password)
        .bind(client_id)
        .execute(pool)
        .await?;
    tracing::info!(client_id = %client_id, user = %creds.username, "generated iSCSI CHAP credentials");
    Ok(Some(creds))
}

async fn storage_spec_for_client(
    pool: &sqlx::SqlitePool,
    client: &Client,
) -> anyhow::Result<Option<ClientStorageSpec>> {
    if !client.enabled || client.master.trim().is_empty() {
        return Ok(None);
    }

    let defaults = ClientStoragePaths::new(&client.name, client.mac.as_str());

    let target_iqn = client
        .target_iqn
        .clone()
        .unwrap_or_else(|| defaults.target_iqn.clone());

    let backstore = client
        .block_store
        .clone()
        .unwrap_or_else(|| defaults.backstore.clone());

    let use_game_disk = client.use_game_disk;
    let game_disks = resolved_game_selection(pool, client.id.as_str(), use_game_disk).await?;
    // Enforced targets converge here too: generate on first repair so an
    // enabled-but-credless client (e.g. pre-CHAP record) heals instead of
    // provisioning open. Inspect gains an idempotent one-time write; the
    // alternative is silently skipping enforcement.
    let chap = ensure_chap_credentials(
        pool,
        client.id.as_str(),
        &client.name,
        client.chap_enabled,
    )
    .await?;

    let source = match client.snapshot.as_deref().map(str::trim) {
        Some(snapshot) if !snapshot.is_empty() => {
            let dataset = client.writeback.clone().ok_or_else(|| {
                anyhow::anyhow!(
                    "client '{}' has a snapshot but no persisted writeback dataset",
                    client.id
                )
            })?;

            return Ok(Some(ClientStorageSpec {
                client_id: client.id.to_string(),
                source: StorageSource::Snapshot(snapshot.to_string()),
                dataset,
                backstore,
                target_iqn,
                lun: 0,
                use_game_disk,
                game_disks,
                chap: chap.clone(),
            }));
        }
        _ => StorageSource::ExistingVolume(client.master.clone()),
    };

    Ok(Some(ClientStorageSpec {
        client_id: client.id.to_string(),
        dataset: client.master.clone(),
        backstore,
        target_iqn,
        lun: 0,
        use_game_disk,
        game_disks,
        chap,
        source,
    }))
}

fn entry_from_result(
    client: &Client,
    spec: &ClientStorageSpec,
    result: StorageReconcileResult,
) -> ReconciliationEntry {
    let (outcome, message) = match result.state {
        StorageState::Ready => (
            ReconciliationOutcome::Ready,
            "ZFS and iSCSI match the persisted client configuration".to_string(),
        ),
        StorageState::Partial => (
            ReconciliationOutcome::Partial,
            "ZFS and iSCSI are out of sync".to_string(),
        ),
        StorageState::Missing => (
            ReconciliationOutcome::Missing,
            "Client storage is missing".to_string(),
        ),
        StorageState::InUse => (
            ReconciliationOutcome::Error,
            "Client storage is in use".to_string(),
        ),
        StorageState::Error => (
            ReconciliationOutcome::Error,
            "Client storage is in an error state".to_string(),
        ),
    };

    ReconciliationEntry {
        client_id: client.id.to_string(),
        client_name: client.name.clone(),
        outcome,
        message,
        target_iqn: Some(spec.target_iqn.clone()),
        dataset: Some(spec.dataset.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{BootMode, ClientStatus, MacAddress, PxeMode};

    #[test]
    fn summary_counts_outcomes() {
        let mut summary = ReconciliationSummary::new();

        summary.push(ReconciliationEntry {
            client_id: "1".to_string(),
            client_name: "PC001".to_string(),
            outcome: ReconciliationOutcome::Ready,
            message: "ready".to_string(),
            target_iqn: None,
            dataset: None,
        });

        summary.push(ReconciliationEntry {
            client_id: "2".to_string(),
            client_name: "PC002".to_string(),
            outcome: ReconciliationOutcome::Partial,
            message: "partial".to_string(),
            target_iqn: None,
            dataset: None,
        });

        assert_eq!(summary.checked, 2);
        assert_eq!(summary.ready, 1);
        assert_eq!(summary.partial, 1);
        assert_eq!(summary.missing, 0);
        assert_eq!(summary.errors, 0);
        assert_eq!(summary.skipped, 0);
    }

    #[tokio::test]
    async fn chap_credentials_generate_once_and_read_back() {
        let path = std::env::temp_dir().join(format!(
            "diskless-chap-{}.db",
            uuid::Uuid::new_v4()
        ));
        let url = format!("sqlite:{}?mode=rwc", path.display());
        let pool = sqlx::SqlitePool::connect(&url).await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO clients(id, name, mac, ip, master, created_at, updated_at) VALUES ('client', 'PC001', '00:11:22:33:44:55', '192.168.1.101', 'pool/master', 'now', 'now')")
            .execute(&pool)
            .await
            .unwrap();

        // Disabled: nothing generated, nothing stored.
        assert!(ensure_chap_credentials(&pool, "client", "PC001", false)
            .await
            .unwrap()
            .is_none());
        assert!(read_chap_credentials(&pool, "client").await.unwrap().is_none());

        // Enabled: generated once, then stable.
        let first = ensure_chap_credentials(&pool, "client", "PC001", true)
            .await
            .unwrap()
            .expect("credentials must be generated");
        assert_eq!(first.username, "chap-pc001");
        assert_eq!(first.password.len(), 12);
        let second = ensure_chap_credentials(&pool, "client", "PC001", true)
            .await
            .unwrap()
            .expect("credentials must persist");
        assert_eq!(first, second);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn missing_master_skips_storage_reconciliation() {
        let path = std::env::temp_dir().join(format!(
            "diskless-recon-{}.db",
            uuid::Uuid::new_v4()
        ));
        let url = format!("sqlite:{}?mode=rwc", path.display());
        let pool = sqlx::SqlitePool::connect(&url).await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let client = Client {
            id: ClientId::from_string("client-1").unwrap(),
            name: "PC001".to_string(),
            mac: MacAddress::parse("00:11:22:33:44:55").unwrap(),
            ip: "192.168.1.100".parse().unwrap(),
            master: String::new(),
            enabled: true,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            snapshot: None,
            block_store: None,
            target_iqn: None,
            writeback: None,
            last_modified: None,
            block_device: None,
            status: ClientStatus::Offline,
            mode: BootMode::Normal,
            pxe_mode: PxeMode::Uefi,
            keep_writeback: true,
            use_game_disk: false,
            game_disks: Vec::new(),
            chap_user: None,
            chap_secret: None,
            chap_enabled: false,
        };

        let spec = storage_spec_for_client(&pool, &client).await.unwrap();
        assert!(spec.is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn repair_policy_allows_disconnected_clients() {
        assert!(ensure_storage_repair_safe("iqn.test:client", false).is_ok());
    }

    #[test]
    fn repair_policy_rejects_connected_clients() {
        let error = ensure_storage_repair_safe("iqn.test:client", true).unwrap_err();
        let message = error.to_string();

        assert!(message.contains("active iSCSI session"));
        assert!(message.contains("Disconnect the client before repair"));
    }
}
