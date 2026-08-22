use crate::cmd::{run_command, run_command_async, run_command_output_no_sudo};
use crate::config::{get_config, get_zpool_name};
use crate::core::client::Client;
use crate::core::provisioning_transaction::ProvisioningTransaction;
use crate::dhcp::{create_dhcp_entry, update_dhcp_config};
use crate::domain::provisioning::TargetIqn;
use crate::domain::storage::{ClientStorageSpec, StorageSource};
use crate::error::AppError;
use crate::state::AppState;
use crate::zfs::get_writeback_or_default_dataset;
use log::{info, warn};
use sqlx::SqlitePool;

pub fn get_client_by_id(client_id: &str) -> Option<Client> {
    let config = get_config();

    for client in &config.clients {
        if client.id.eq_ignore_ascii_case(client_id) {
            return Some(client.clone());
        }
    }

    None
}

pub fn check_duplicate_client(name: &str, mac: &str, ip: &str) -> Option<String> {
    let config = get_config();

    for client in &config.clients {
        let client_name = client.name.to_lowercase();
        let client_ip = &client.ip;
        let client_mac = client.mac.to_uppercase();

        if name.to_lowercase() == client_name {
            return Some(format!("A client with name '{}' already exists", name));
        }

        if ip == client_ip {
            return Some(format!(
                "IP address {} is already in use by client '{}'",
                ip, client.name
            ));
        }

        if mac.to_uppercase() == client_mac {
            return Some(format!(
                "MAC address {} is already in use by client '{}'",
                mac, client.name
            ));
        }
    }

    None
}

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

    /// Replace the calculated dataset while preserving
    /// the iSCSI resources.
    pub fn with_dataset(mut self, dataset: impl Into<String>) -> Self {
        self.dataset = dataset.into();
        self
    }
}

pub fn get_client_paths(client_id: &str, client_mac: &str) -> ClientStoragePaths {
    get_client_paths_with_master(client_id, client_mac, "")
}

pub fn get_client_paths_with_master(
    client_id: &str,
    client_mac: &str,
    _master: &str,
) -> ClientStoragePaths {
    let mut paths = ClientStoragePaths::new(client_id, client_mac);

    if let Some(client) = get_client_by_id(client_id) {
        if let Some(target_iqn) = client.target_iqn.filter(|value| !value.trim().is_empty()) {
            paths.target_iqn = target_iqn;
        }
    }

    paths
}

pub async fn save_client_config(pool: &SqlitePool, client_data: &Client) -> bool {
    let mut cfg = get_config();
    let mut found = false;

    for client in cfg.clients.iter_mut() {
        if client.id.eq_ignore_ascii_case(&client_data.id) {
            *client = client_data.clone();
            found = true;
            break;
        }
    }

    if !found {
        cfg.clients.push(client_data.clone());
    }

    match crate::config::write_config(pool, &cfg).await {
        Ok(_) => {
            crate::config::set_config(&cfg);
            true
        }

        Err(error) => {
            warn!("Error saving client config: {}", error);
            false
        }
    }
}

pub async fn delete_client_config(pool: &SqlitePool, client_id: &str) -> bool {
    info!("Deleting client config: {}", client_id);

    let mut cfg = get_config();
    let before = cfg.clients.len();

    cfg.clients
        .retain(|client| !client.id.eq_ignore_ascii_case(client_id));

    if cfg.clients.len() == before {
        return true;
    }

    match crate::config::write_config(pool, &cfg).await {
        Ok(_) => {
            crate::config::set_config(&cfg);
            true
        }

        Err(error) => {
            warn!("Error writing config file: {}", error);
            false
        }
    }
}

pub struct AddClientProvisioningRequest {
    pub name: String,
    pub mac: String,
    pub ip: String,
    pub master: String,
    pub snapshot: String,
    pub keep_writeback: Option<bool>,
    pub use_game_disk: Option<bool>,
}

/// Provision a client and atomically clean up resources if any
/// provisioning step fails.
///
/// The transaction covers:
///
/// 1. ZFS clone creation
/// 2. iSCSI target creation
/// 3. DHCP configuration
/// 4. Client configuration persistence
pub async fn add_client_provisioning(
    state: &AppState,
    request: AddClientProvisioningRequest,
) -> Result<serde_json::Value, AppError> {
    let AddClientProvisioningRequest {
        name,
        mac,
        ip,
        master,
        snapshot,
        keep_writeback,
        use_game_disk,
    } = request;

    if master.is_empty() {
        let now = chrono::Utc::now();

        let client_data = Client {
            id: name.clone(),
            name: name.to_uppercase(),
            mac: mac.clone(),
            ip: ip.clone(),
            master: master.clone(),
            enabled: true,
            created_at: now,
            updated_at: now,
            snapshot: if snapshot.is_empty() {
                None
            } else {
                Some(snapshot.clone())
            },
            target_iqn: None,
            block_device: None,
            block_store: None,
            writeback: None,
            last_modified: Some(now.format("%Y-%m-%d %H:%M:%S").to_string()),
            status: None,
            mode: None,
            pxe_mode: Some("uefi".to_string()),
            keep_writeback: keep_writeback.or(Some(true)),
            use_game_disk,
        };

        if !save_client_config(&state.db_pool, &client_data).await {
            return Err(AppError::Config(format!(
                "Failed to save client configuration for {}",
                name
            )));
        }

        return Ok(serde_json::json!({
            "message": format!(
                "Client {} added to configuration (no image selected)",
                name
            )
        }));
    }

    if let Some(duplicate) = check_duplicate_client(&name, &mac, &ip) {
        return Err(AppError::Validation(duplicate));
    }

    let mut paths = get_client_paths_with_master(&name, &mac, &master);

    if let Ok(pool_list) =
        run_command_output_no_sudo(["zfs", "list", "-H", "-o", "name", "-r", &get_zpool_name()])
    {
        if let Some(parent) = pool_list
            .lines()
            .filter(|line| !line.is_empty())
            .find_map(|dataset| {
                match run_command_output_no_sudo([
                    "zfs",
                    "get",
                    "-H",
                    "-o",
                    "value",
                    "org.diskless:type",
                    dataset,
                ]) {
                    Ok(value) if value.trim() == "writeback" => Some(dataset.to_string()),
                    _ => None,
                }
            })
        {
            paths.dataset = format!("{}/{}-disk", parent, name.to_uppercase());
        } else {
            paths.dataset = format!("{}/{}-disk", get_zpool_name(), name.to_uppercase());
        }
    }

    info!("Client storage paths: {:?}", paths);

    let mut transaction = ProvisioningTransaction::new(state, name.clone());
    let used_master_directly;

    if snapshot.is_empty() {
        paths.dataset = master.clone();
        used_master_directly = true;
    } else {
        used_master_directly = false;

        if let Err(error) = run_command(["zfs", "clone", &snapshot, &paths.dataset]) {
            let provisioning_error = AppError::Command(format!(
                "Failed to create ZFS clone {} from {}: {}",
                paths.dataset, snapshot, error
            ));

            return Err(transaction.rollback_with_error(provisioning_error).await);
        }

        transaction.record_zfs_clone(paths.dataset.clone());
    }

    let block_device = format!("/dev/zvol/{}", paths.dataset);

    let storage_spec = ClientStorageSpec {
        client_id: name.clone(),
        source: if used_master_directly {
            StorageSource::ExistingVolume(master.clone())
        } else {
            StorageSource::ExistingClientVolume(paths.dataset.clone())
        },
        dataset: paths.dataset.clone(),
        backstore: paths.backstore.clone(),
        target_iqn: paths.target_iqn.clone(),
        lun: 0,
        use_game_disk: use_game_disk.unwrap_or(false),
    };

    let (_storage, iscsi_result) = match state
        .application
        .storage
        .create_client_storage_transaction(&storage_spec)
    {
        Ok(result) => result,
        Err(error) => {
            let provisioning_error = AppError::Command(format!(
                "Failed to setup iSCSI target '{}': {}",
                paths.target_iqn, error
            ));

            return Err(transaction.rollback_with_error(provisioning_error).await);
        }
    };

    transaction.record_iscsi_target(
        paths.target_iqn.clone(),
        iscsi_result.backstores_created.clone(),
    );

    let dhcp_entry = create_dhcp_entry(&name, &mac, &ip, &paths.target_iqn);

    if let Err(error) = update_dhcp_config(&name, &dhcp_entry, true).await {
        let provisioning_error =
            AppError::Config(format!("Failed to update DHCP config: {}", error));

        return Err(transaction.rollback_with_error(provisioning_error).await);
    }

    transaction.record_dhcp_entry(name.clone());

    let now = chrono::Utc::now();

    let client_data = Client {
        id: name.clone(),
        name: name.to_uppercase(),
        mac: mac.clone(),
        ip: ip.clone(),
        master: master.clone(),
        enabled: true,
        created_at: now,
        updated_at: now,
        snapshot: if used_master_directly {
            None
        } else {
            Some(snapshot.clone())
        },
        target_iqn: Some(paths.target_iqn.clone()),
        block_device: Some(block_device.clone()),
        block_store: Some(paths.backstore.clone()),
        writeback: if used_master_directly {
            None
        } else {
            Some(paths.dataset.clone())
        },
        last_modified: Some(now.format("%Y-%m-%d %H:%M:%S").to_string()),
        status: None,
        mode: if used_master_directly {
            Some("super".to_string())
        } else {
            None
        },
        pxe_mode: Some("uefi".to_string()),
        keep_writeback: keep_writeback.or(Some(true)),
        use_game_disk,
    };

    if !save_client_config(&state.db_pool, &client_data).await {
        let provisioning_error =
            AppError::Config(format!("Failed to save client configuration for {}", name));

        return Err(transaction.rollback_with_error(provisioning_error).await);
    }

    if let Err(error) = run_command_async(["systemctl", "restart", "isc-dhcp-server.service"]).await
    {
        warn!(
            "Failed to restart DHCP service after adding client {}: {}",
            name, error
        );
    }

    transaction.commit();

    info!("Client {} provisioning completed successfully", name);

    Ok(serde_json::json!({
        "message": format!("Client {} added successfully", name)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_storage_paths_build_expected_target_iqn() {
        let paths = ClientStoragePaths::new("client_1", "00:11:22:33:44:55");

        assert_eq!(
            paths.target_iqn,
            "iqn.2024-01.com.diskless:client.client_1"
        );
    }

    #[test]
    fn client_storage_paths_build_expected_backstore() {
        let paths = ClientStoragePaths::new("CLIENT_1", "00:11:22:33:44:55");

        assert_eq!(paths.backstore, "block_client_1");
    }

    #[test]
    fn client_storage_paths_can_replace_dataset() {
        let paths = ClientStoragePaths::new("client_1", "00:11:22:33:44:55")
            .with_dataset("tank/writeback/CLIENT_1-disk");

        assert_eq!(paths.dataset, "tank/writeback/CLIENT_1-disk");

        assert_eq!(
            paths.target_iqn,
            "iqn.2024-01.com.diskless:client.client_1"
        );

        assert_eq!(paths.backstore, "block_client_1");
    }
}
