use crate::core::client::{Client, ClientManager};
use crate::core::provisioning::ClientStoragePaths;
use crate::domain::storage::{
    ClientStorageSpec, StorageReconcileResult, StorageSource, StorageState,
};
use crate::infrastructure::iscsi::target_has_active_sessions;
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
    let manager = ClientManager::new(state.db_pool.clone());
    let clients = manager.list().await?;

    let mut summary = ReconciliationSummary::new();

    for client in clients {
        match storage_spec_for_client(&client) {
            Ok(Some(spec)) => match state.application.storage.reconcile_client_storage(&spec) {
                Ok(result) => summary.push(entry_from_result(&client, &spec, result)),
                Err(error) => summary.push(ReconciliationEntry {
                    client_id: client.id.clone(),
                    client_name: client.name.clone(),
                    outcome: ReconciliationOutcome::Error,
                    message: error.to_string(),
                    target_iqn: Some(spec.target_iqn),
                    dataset: Some(spec.dataset),
                }),
            },
            Ok(None) => summary.push(ReconciliationEntry {
                client_id: client.id.clone(),
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
                client_id: client.id.clone(),
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

/// Repair one persisted client's ZFS/iSCSI state to the desired configuration.
///
/// The operation is explicit. It does not run during application startup.
pub async fn repair_client_storage(
    state: &AppState,
    client_id: &str,
) -> anyhow::Result<ReconciliationEntry> {
    let manager = ClientManager::new(state.db_pool.clone());
    let client = manager.get(client_id).await?;
    let spec = storage_spec_for_client(&client)?.ok_or_else(|| {
        anyhow::anyhow!(
            "client '{}' has no storage configuration to reconcile",
            client_id
        )
    })?;

    if target_has_active_sessions(&spec.target_iqn)? {
        return Ok(ReconciliationEntry {
            client_id: client.id,
            client_name: client.name,
            outcome: ReconciliationOutcome::Error,
            message: format!(
                "Client storage is in use: active iSCSI session on target '{}'. Disconnect the client before repair.",
                spec.target_iqn
            ),
            target_iqn: Some(spec.target_iqn),
            dataset: Some(spec.dataset),
        });
    }

    let storage = state
        .application
        .storage
        .reconcile_client_storage_in_place(&spec)?;

    Ok(ReconciliationEntry {
        client_id: client.id,
        client_name: client.name,
        outcome: ReconciliationOutcome::Ready,
        message: format!("Storage reconciled successfully for '{}'", client_id),
        target_iqn: Some(storage.target_iqn().to_string()),
        dataset: Some(storage.dataset().to_string()),
    })
}

fn storage_spec_for_client(client: &Client) -> anyhow::Result<Option<ClientStorageSpec>> {
    if !client.enabled || client.master.trim().is_empty() {
        return Ok(None);
    }

    let defaults = ClientStoragePaths::new(&client.name, &client.mac);

    let target_iqn = client
        .target_iqn
        .clone()
        .unwrap_or_else(|| defaults.target_iqn.clone());

    let backstore = client
        .block_store
        .clone()
        .unwrap_or_else(|| defaults.backstore.clone());

    let source = match client.snapshot.as_deref().map(str::trim) {
        Some(snapshot) if !snapshot.is_empty() => {
            let dataset = client.writeback.clone().ok_or_else(|| {
                anyhow::anyhow!(
                    "client '{}' has a snapshot but no persisted writeback dataset",
                    client.id
                )
            })?;

            return Ok(Some(ClientStorageSpec {
                client_id: client.id.clone(),
                source: StorageSource::Snapshot(snapshot.to_string()),
                dataset,
                backstore,
                target_iqn,
                lun: 0,
                use_game_disk: client.use_game_disk.unwrap_or(false),
            }));
        }
        _ => StorageSource::ExistingVolume(client.master.clone()),
    };

    Ok(Some(ClientStorageSpec {
        client_id: client.id.clone(),
        dataset: client.master.clone(),
        backstore,
        target_iqn,
        lun: 0,
        use_game_disk: client.use_game_disk.unwrap_or(false),
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
        client_id: client.id.clone(),
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

    #[test]
    fn missing_master_skips_storage_reconciliation() {
        let client = Client {
            id: "client-1".to_string(),
            name: "PC001".to_string(),
            mac: "00:11:22:33:44:55".to_string(),
            ip: "192.168.1.100".to_string(),
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
            status: None,
            mode: None,
            pxe_mode: None,
            keep_writeback: Some(true),
            use_game_disk: Some(false),
        };

        assert!(storage_spec_for_client(&client).unwrap().is_none());
    }
}
