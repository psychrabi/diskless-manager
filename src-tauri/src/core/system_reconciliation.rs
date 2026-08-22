use crate::{
    core::{dhcp_reconciliation, reconciliation},
    state::AppState,
};
use serde::Serialize;

/// Read-only reconciliation report for the managed diskless environment.
///
/// The report combines storage and DHCP drift without changing infrastructure.
#[derive(Debug, Clone, Serialize)]
pub struct SystemReconciliationSummary {
    pub storage: reconciliation::ReconciliationSummary,
    pub dhcp: dhcp_reconciliation::DhcpReconciliationSummary,
}

/// Inspect storage and DHCP state in one read-only operation.
pub async fn inspect_system_reconciliation(
    state: &AppState,
) -> anyhow::Result<SystemReconciliationSummary> {
    let storage = reconciliation::inspect_storage(state).await?;
    let dhcp = dhcp_reconciliation::inspect_dhcp(state).await?;

    Ok(SystemReconciliationSummary { storage, dhcp })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_contains_both_domains() {
        let storage = reconciliation::ReconciliationSummary {
            checked: 0,
            ready: 0,
            partial: 0,
            missing: 0,
            errors: 0,
            skipped: 0,
            clients: Vec::new(),
        };

        let dhcp = dhcp_reconciliation::DhcpReconciliationSummary {
            checked: 0,
            ready: 0,
            partial: 0,
            missing: 0,
            errors: 0,
            skipped: 0,
            clients: Vec::new(),
        };

        let summary = SystemReconciliationSummary { storage, dhcp };

        assert_eq!(summary.storage.checked, 0);
        assert_eq!(summary.dhcp.checked, 0);
    }
}
