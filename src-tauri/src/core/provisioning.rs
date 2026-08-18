use crate::cmd::{run_command, run_command_async, run_command_output_no_sudo};
use crate::config::{get_config, get_zpool_name};
use crate::core::client::Client;
use crate::dhcp::{create_dhcp_entry, update_dhcp_config};
use crate::domain::storage::{ClientStorageSpec, StorageSource};
use crate::error::AppError;
use crate::state::AppState;
use crate::zfs::{get_writeback_or_default_dataset, zfs_destroy};
use log::{error, info, warn};
use sqlx::SqlitePool;
use std::collections::HashMap;

pub fn get_client_by_id(client_id: &str) -> Option<Client> {
    let config = get_config();

    for c in &config.clients {
        if c.id.eq_ignore_ascii_case(client_id) {
            return Some(c.clone());
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

pub fn get_client_paths(client_id: &str, client_mac: &str) -> HashMap<String, String> {
    get_client_paths_with_master(client_id, client_mac, "")
}

pub fn get_client_paths_with_master(
    client_id: &str,
    client_mac: &str,
    _master: &str,
) -> HashMap<String, String> {
    // If a dataset with org.diskless:type=writeback exists, use it as the parent for client clones.
    let clone_path = get_writeback_or_default_dataset(client_id);

    let target_iqn = format!(
        "iqn.2025-04.local.diskless:{}",
        client_mac.to_lowercase().replace(':', "-")
    );
    let block_store = format!("block_{}", client_id.to_lowercase());
    let mut map = HashMap::new();
    map.insert("clone".to_string(), clone_path);
    map.insert("target_iqn".to_string(), target_iqn);
    map.insert("block_store".to_string(), block_store);
    map
}

pub async fn save_client_config(pool: &SqlitePool, client_data: &Client) -> bool {
    let mut cfg = get_config();
    let mut found = false;
    // match case-insensitively
    for c in cfg.clients.iter_mut() {
        if c.id.eq_ignore_ascii_case(&client_data.id) {
            *c = client_data.clone();
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
        Err(e) => {
            warn!("Error saving client config: {}", e);
            false
        }
    }
}

pub async fn delete_client_config(pool: &SqlitePool, client_id: &str) -> bool {
    info!("Deleting client config: {}", client_id);
    let mut cfg = get_config();
    let before = cfg.clients.len();
    cfg.clients
        .retain(|c| !c.id.eq_ignore_ascii_case(client_id));
    if cfg.clients.len() == before {
        return true;
    }
    match crate::config::write_config(pool, &cfg).await {
        Ok(_) => {
            crate::config::set_config(&cfg);
            true
        }
        Err(e) => {
            warn!("Error writing config file: {}", e);
            false
        }
    }
}

pub async fn add_client_provisioning(
    state: &AppState,
    name: String,
    mac: String,
    ip: String,
    master: String,
    snapshot: String,
    keep_writeback: Option<bool>,
    use_game_disk: Option<bool>,
) -> Result<serde_json::Value, AppError> {
    // If no master image is selected, only save to config files
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
        return Ok(
            serde_json::json!({ "message": format!("Client {} added to configuration (no image selected)", name) }),
        );
    }

    // Compute ZFS paths
    let mut paths = get_client_paths_with_master(&name, &mac, &master);

    // If a dataset with org.diskless:type=writeback exists, use it as the parent for client clones.
    if let Ok(pool_list) =
        run_command_output_no_sudo(&["zfs", "list", "-H", "-o", "name", "-r", &get_zpool_name()])
    {
        if let Some(parent) = pool_list.lines().filter(|l| !l.is_empty()).find_map(|ds| {
            match run_command_output_no_sudo(&[
                "zfs",
                "get",
                "-H",
                "-o",
                "value",
                "org.diskless:type",
                ds,
            ]) {
                Ok(v) if v.trim() == "writeback" => Some(ds.to_string()),
                _ => None,
            }
        }) {
            paths.insert(
                "clone".to_string(),
                format!("{}/{}-disk", parent, name.to_uppercase()),
            );
        } else {
            paths.insert(
                "clone".to_string(),
                format!("{}/{}-disk", get_zpool_name(), name.to_uppercase()),
            );
        }
    }
    warn!("Client paths: {:?}", paths);

    // Transaction tracking
    let mut rollback_clone: Option<String> = None;
    let mut rollback_target: Option<(String, String)> = None;
    let mut rollback_dhcp: bool = false;

    // Helper closure for rollback
    let perform_rollback = |r_clone: Option<String>,
                            r_target: Option<(String, String)>,
                            r_dhcp: bool,
                            client_id: String| async move {
        warn!("Rolling back provisioning for {}", client_id);

        if r_dhcp {
            if let Err(e) = update_dhcp_config(&client_id, "", false).await {
                error!("Rollback: Failed to remove DHCP entry: {}", e);
            }
        }

        if let Some((iqn, store)) = r_target {
            if let Err(e) = state
                .application
                .storage
                .remove_client_target(&iqn, Some(&store))
            {
                error!("Rollback: Failed to cleanup iSCSI target: {}", e);
            }
        }

        if let Some(clone) = r_clone {
            if let Err(e) = zfs_destroy(&clone) {
                error!("Rollback: Failed to destroy ZFS clone: {}", e);
            }
        }
    };

    // Step 1: Create client image (ZFS clone or use master directly)
    let mut used_master_directly = false;
    let clone_result = if !snapshot.is_empty() {
        // Use provided snapshot (full ZFS path: master@snapshot_name)
        run_command(&["zfs", "clone", &snapshot, &paths["clone"]])
    } else {
        // Use master volume directly
        paths.insert("clone".to_string(), master.clone());
        used_master_directly = true;
        Ok(())
    };

    if let Err(e) = clone_result {
        return Err(AppError::Command(format!(
            "Failed to create ZFS clone: {}",
            e
        )));
    }

    if !used_master_directly {
        rollback_clone = Some(paths["clone"].clone());
    }

    // Step 2: Set up iSCSI target through the application storage service.
    let block_device = format!("/dev/zvol/{}", &paths["clone"]);

    let storage_result = if use_game_disk.unwrap_or(false) {
        state
            .application
            .storage
            .create_client_storage_with_game_disks(
                &name,
                &paths["target_iqn"],
                &paths["block_store"],
                std::path::Path::new(&block_device),
            )
    } else {
        let storage_spec = ClientStorageSpec {
            client_id: name.clone(),
            source: if used_master_directly {
                StorageSource::ExistingVolume(master.clone())
            } else {
                StorageSource::Snapshot(snapshot.clone())
            },
            dataset: paths["clone"].clone(),
            backstore: paths["block_store"].clone(),
            target_iqn: paths["target_iqn"].clone(),
            lun: 0,
            use_game_disk: use_game_disk.unwrap_or(false),
        };

        state
            .application
            .storage
            .create_client_storage(&storage_spec)
            .map(|_| ())
    };

    if let Err(e) = storage_result {
        perform_rollback(rollback_clone, rollback_target, rollback_dhcp, name.clone()).await;

        return Err(AppError::Command(format!(
            "Failed to setup iSCSI target: {}",
            e
        )));
    }

    rollback_target = Some((paths["target_iqn"].clone(), paths["block_store"].clone()));

    // Step 3: Create DHCP entry
    let dhcp_entry = create_dhcp_entry(&name, &mac, &ip, &paths["target_iqn"]);
    if let Err(e) = update_dhcp_config(&name, &dhcp_entry, true).await {
        perform_rollback(rollback_clone, rollback_target, rollback_dhcp, name.clone()).await;
        return Err(AppError::Config(format!(
            "Failed to update DHCP config: {}",
            e
        )));
    }
    rollback_dhcp = true;

    // Step 4: Save client configuration to JSON file
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
        target_iqn: Some(paths["target_iqn"].clone()),
        block_device: Some(block_device.clone()),
        block_store: Some(paths["block_store"].clone()),
        writeback: if used_master_directly {
            None
        } else {
            Some(paths["clone"].clone())
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
        perform_rollback(rollback_clone, rollback_target, rollback_dhcp, name.clone()).await;
        return Err(AppError::Config(format!(
            "Failed to save client configuration for {}",
            name
        )));
    }

    // Step 5: Restart DHCP service
    if let Err(e) = run_command_async(&["systemctl", "restart", "isc-dhcp-server.service"]).await {
        warn!(
            "Failed to restart DHCP service after adding client {}: {}",
            name, e
        );
    }
    info!("Client {} added successfully", name);
    Ok(serde_json::json!({ "message": format!("Client {} added successfully", name) }))
}
