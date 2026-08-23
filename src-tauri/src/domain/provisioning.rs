use std::net::IpAddr;

use super::{ClientId, ImageId, MacAddress};

#[derive(Debug, Clone)]
pub struct ProvisionClientRequest {
    pub client_id: ClientId,
    pub image_id: ImageId,
    pub ip: IpAddr,
    pub mac: MacAddress,
}

#[derive(Debug, Clone)]
pub struct ProvisioningPlan {
    pub client_id: ClientId,
    pub image_id: ImageId,
    pub client_dataset: String,
    pub target_iqn: String,
    pub block_device: String,
    pub ip: IpAddr,
    pub mac: MacAddress,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetIqn(String);

impl TargetIqn {
    /// Build a target IQN from a configured prefix and client name.
    ///
    /// Existing persisted IQNs must be reused by callers rather than
    /// regenerated. This constructor is for new targets.
    pub fn for_client_name(prefix: &str, client_name: &str) -> Self {
        let prefix = prefix.trim().trim_end_matches(':');
        let client_name = client_name.trim().to_lowercase();

        Self(format!("{prefix}:client.{client_name}"))
    }

    /// Build a target IQN from a configured prefix and client MAC.
    ///
    /// Kept for compatibility with older provisioning flows.
    pub fn for_client(prefix: &str, mac: &MacAddress) -> Self {
        let prefix = prefix.trim().trim_end_matches(':');
        let normalized = mac.to_string().to_lowercase().replace(':', "-");

        Self(format!("{prefix}:{normalized}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub struct ClientStorage {
    pub dataset: String,
    pub block_device: String,
    pub backstore: String,
    pub target_iqn: String,
    pub lun: u32,
}

pub struct ClientBootResources {
    pub storage: ClientStorage,
    pub dhcp_entry: String,
}

#[cfg(test)]
mod tests {
    use super::TargetIqn;

    #[test]
    fn target_iqn_uses_configured_prefix_and_client_name() {
        let iqn = TargetIqn::for_client_name("iqn.2024-01.com.diskless", "PC001");

        assert_eq!(iqn.as_str(), "iqn.2024-01.com.diskless:client.pc001");
    }

    #[test]
    fn target_iqn_trims_prefix_separator_and_client_whitespace() {
        let iqn = TargetIqn::for_client_name(" iqn.2024-01.com.diskless: ", " PC001 ");

        assert_eq!(iqn.as_str(), "iqn.2024-01.com.diskless:client.pc001");
    }
}
