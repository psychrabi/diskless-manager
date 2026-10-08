use crate::config::get_config;
use crate::domain::provisioning::TargetIqn;
use crate::infrastructure::zfs::legacy::get_writeback_or_default_dataset;

fn configured_target_prefix() -> String {
    let settings = get_config().settings;

    settings
        .get("iscsi")
        .and_then(|iscsi| iscsi.get("target_prefix"))
        .and_then(|value| value.as_str())
        .or_else(|| {
            settings
                .get("iscsi_target_prefix")
                .and_then(|value| value.as_str())
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("iqn.2024-01.com.diskless")
        .to_string()
}

/// Storage resources calculated for a diskless client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientStoragePaths {
    /// ZFS dataset used for the client's boot/writeback volume.
    pub dataset: String,

    /// iSCSI Qualified Name assigned to the client.
    pub target_iqn: String,

    /// LIO/targetcli backstore owned by the client.
    pub backstore: String,
}

impl ClientStoragePaths {
    /// Build the default storage paths for a client.
    pub fn new(client_id: &str, _client_mac: &str) -> Self {
        let target_iqn = TargetIqn::for_client_name(&configured_target_prefix(), client_id);

        Self {
            dataset: get_writeback_or_default_dataset(client_id),
            target_iqn: target_iqn.as_str().to_string(),
            backstore: format!("block_{}", client_id.to_lowercase()),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_storage_paths_build_expected_target_iqn() {
        let paths = ClientStoragePaths::new("client_1", "00:11:22:33:44:55");

        assert_eq!(paths.target_iqn, "iqn.2024-01.com.diskless:client.client_1");
    }

    #[test]
    fn client_storage_paths_build_expected_backstore() {
        let paths = ClientStoragePaths::new("CLIENT_1", "00:11:22:33:44:55");

        assert_eq!(paths.backstore, "block_client_1");
    }
}
