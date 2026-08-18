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
    pub fn for_client(mac: &MacAddress) -> Self {
        let normalized = mac.to_string().to_lowercase().replace(':', "-");

        Self(format!("iqn.2025-04.local.diskless:{normalized}"))
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
