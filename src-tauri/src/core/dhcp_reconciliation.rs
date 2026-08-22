use crate::{
    core::client::ClientManager,
    dhcp::{create_dhcp_entry, dhcp_entry_matches, format_client_name},
    state::AppState,
    DHCP_CLIENTS_PATH,
};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DhcpReconciliationOutcome {
    Ready,
    Partial,
    Missing,
    Error,
    Skipped,
}

#[derive(Debug, Clone, Serialize)]
pub struct DhcpReconciliationEntry {
    pub client_id: String,
    pub client_name: String,
    pub outcome: DhcpReconciliationOutcome,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DhcpReconciliationSummary {
    pub checked: usize,
    pub ready: usize,
    pub partial: usize,
    pub missing: usize,
    pub errors: usize,
    pub skipped: usize,
    pub clients: Vec<DhcpReconciliationEntry>,
}

impl DhcpReconciliationSummary {
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

    fn push(&mut self, entry: DhcpReconciliationEntry) {
        self.checked += 1;

        match &entry.outcome {
            DhcpReconciliationOutcome::Ready => self.ready += 1,
            DhcpReconciliationOutcome::Partial => self.partial += 1,
            DhcpReconciliationOutcome::Missing => self.missing += 1,
            DhcpReconciliationOutcome::Error => self.errors += 1,
            DhcpReconciliationOutcome::Skipped => self.skipped += 1,
        }

        self.clients.push(entry);
    }
}

fn classify_dhcp_entry(
    content: &str,
    client_name: &str,
    desired_entry: &str,
) -> DhcpReconciliationOutcome {
    if dhcp_entry_matches(content, client_name, desired_entry) {
        return DhcpReconciliationOutcome::Ready;
    }

    let host_name = format_client_name(client_name);
    let host_header = format!("host {host_name} {{");

    if content.lines().any(|line| line.trim() == host_header) {
        DhcpReconciliationOutcome::Partial
    } else {
        DhcpReconciliationOutcome::Missing
    }
}

pub async fn inspect_dhcp(state: &AppState) -> anyhow::Result<DhcpReconciliationSummary> {
    let manager = ClientManager::new(state.db_pool.clone());
    let clients = manager.list().await?;
    let content = tokio::fs::read_to_string(DHCP_CLIENTS_PATH)
        .await
        .unwrap_or_default();

    let mut summary = DhcpReconciliationSummary::new();

    for client in clients {
        if !client.enabled || client.master.trim().is_empty() {
            summary.push(DhcpReconciliationEntry {
                client_id: client.id,
                client_name: client.name,
                outcome: DhcpReconciliationOutcome::Skipped,
                message: "Client has no enabled DHCP configuration to reconcile".to_string(),
            });
            continue;
        }

        let target_iqn = match client.target_iqn.as_deref() {
            Some(value) if !value.trim().is_empty() => value,
            _ => {
                summary.push(DhcpReconciliationEntry {
                    client_id: client.id,
                    client_name: client.name,
                    outcome: DhcpReconciliationOutcome::Error,
                    message: "Client has no persisted iSCSI target IQN".to_string(),
                });
                continue;
            }
        };

        let desired = create_dhcp_entry(&client.name, &client.mac, &client.ip, target_iqn);
        let outcome = classify_dhcp_entry(&content, &client.name, &desired);

        let message = match outcome {
            DhcpReconciliationOutcome::Ready => {
                "DHCP entry matches the persisted client configuration".to_string()
            }
            DhcpReconciliationOutcome::Partial => {
                "DHCP host exists but does not match the persisted client configuration".to_string()
            }
            DhcpReconciliationOutcome::Missing => "DHCP entry is missing".to_string(),
            _ => unreachable!(),
        };

        summary.push(DhcpReconciliationEntry {
            client_id: client.id,
            client_name: client.name,
            outcome,
            message,
        });
    }

    Ok(summary)
}

pub async fn repair_client_dhcp(
    state: &AppState,
    client_id: &str,
) -> anyhow::Result<DhcpReconciliationEntry> {
    let manager = ClientManager::new(state.db_pool.clone());
    let client = manager.get(client_id).await?;

    if !client.enabled || client.master.trim().is_empty() {
        anyhow::bail!("client '{}' has no enabled DHCP configuration", client_id);
    }

    let target_iqn = client
        .target_iqn
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!("client '{}' has no persisted iSCSI target IQN", client_id)
        })?;

    let desired = create_dhcp_entry(&client.name, &client.mac, &client.ip, target_iqn);

    crate::dhcp::update_dhcp_config(&client.name, &desired, false)
        .await
        .map_err(anyhow::Error::msg)?;

    Ok(DhcpReconciliationEntry {
        client_id: client.id,
        client_name: client.name,
        outcome: DhcpReconciliationOutcome::Ready,
        message: "DHCP entry reconciled successfully".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DESIRED_ENTRY: &str = r#"host PC001 {
hardware ethernet 00:11:22:33:44:55;
fixed-address 192.168.1.100;
option host-name "PC001";
option root-path "iscsi:192.168.1.1::::iqn.test";
}"#;

    #[test]
    fn summary_counts_outcomes() {
        let mut summary = DhcpReconciliationSummary::new();

        summary.push(DhcpReconciliationEntry {
            client_id: "1".to_string(),
            client_name: "PC001".to_string(),
            outcome: DhcpReconciliationOutcome::Ready,
            message: "ready".to_string(),
        });

        summary.push(DhcpReconciliationEntry {
            client_id: "2".to_string(),
            client_name: "PC002".to_string(),
            outcome: DhcpReconciliationOutcome::Partial,
            message: "partial".to_string(),
        });

        assert_eq!(summary.checked, 2);
        assert_eq!(summary.ready, 1);
        assert_eq!(summary.partial, 1);
        assert_eq!(summary.missing, 0);
        assert_eq!(summary.errors, 0);
        assert_eq!(summary.skipped, 0);
    }

    #[test]
    fn classify_dhcp_entry_ready_when_block_matches() {
        assert_eq!(
            classify_dhcp_entry(
                &format!("# header\n{}\n# footer", DESIRED_ENTRY),
                "client_1",
                DESIRED_ENTRY,
            ),
            DhcpReconciliationOutcome::Ready
        );
    }

    #[test]
    fn classify_dhcp_entry_partial_when_host_exists_with_drift() {
        let drifted = DESIRED_ENTRY.replace("192.168.1.100", "192.168.1.101");

        assert_eq!(
            classify_dhcp_entry(&drifted, "client_1", DESIRED_ENTRY),
            DhcpReconciliationOutcome::Partial
        );
    }

    #[test]
    fn classify_dhcp_entry_missing_when_host_does_not_exist() {
        assert_eq!(
            classify_dhcp_entry("# no client entries\n", "client_1", DESIRED_ENTRY),
            DhcpReconciliationOutcome::Missing
        );
    }

    #[test]
    fn classify_dhcp_entry_tolerates_whitespace_in_header() {
        let content = DESIRED_ENTRY.replace("host PC001 {", "  host   PC001   {  ");

        assert_eq!(
            classify_dhcp_entry(&content, "client_1", DESIRED_ENTRY),
            DhcpReconciliationOutcome::Partial
        );
    }
}
