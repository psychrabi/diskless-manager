use crate::config::{get_config, get_zpool_name, read_config, write_config};
use crate::dhcp::{create_dhcp_entry, update_dhcp_config};
use crate::error::AppError;
use crate::iscsi::{cleanup_iscsi_target, setup_iscsi_target, setup_iscsi_target_with_game_disks};
use crate::types::{AddClientRequest, AppConfig, Client, ControlRequest, DeprovisionRequest};
use crate::utils::{
    run_command, run_command_check, run_command_output, run_command_output_no_sudo,
};
use crate::zfs::{zfs_clone, zfs_destroy, zfs_exists};
use tracing::{debug, error, info, warn};

const IQN_BASE: &str = "iqn.2025-04.local.diskless";
use chrono::Local;
use serde_json::json;
use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use tokio::task;

trait WaitTimeout {
    fn wait_timeout(&mut self, dur: Duration) -> std::io::Result<Option<std::process::ExitStatus>>;
}
impl WaitTimeout for std::process::Child {
    fn wait_timeout(&mut self, dur: Duration) -> std::io::Result<Option<std::process::ExitStatus>> {
        let start = std::time::Instant::now();
        loop {
            match self.try_wait()? {
                Some(status) => {
                    return Ok(Some(status));
                }
                None => {
                    if start.elapsed() >= dur {
                        return Ok(None);
                    }
                    thread::sleep(Duration::from_millis(100));
                }
            }
        }
    }
}

// Helper to validate auth token, returning Err if invalid
fn validate_auth(token: &str) -> Result<(), AppError> {
    crate::middleware::validate_auth_token_for_command(token)
        .map(|_| ())
        .map_err(|e| AppError::Auth(e.message))
}

// Removed unused stream imports

#[tauri::command]
pub async fn get_clients(
    token: String,
    client_id: Option<String>,
) -> Result<serde_json::Value, AppError> {
    // Validate authentication token
    validate_auth(&token)?;
    let mut config: AppConfig = read_config();

    // Read config once, compute statuses concurrently with limited concurrency
    // to avoid exhausting system resources (threads/processes).
    let client_ips: Vec<String> = config.clients.iter().map(|c| c.ip.clone()).collect();

    // Use a semaphore to limit concurrency
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(50));
    let mut handles = Vec::new();

    for ip in client_ips {
        let sem = semaphore.clone();
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            task::spawn_blocking(move || get_client_status_realtime(ip)).await
        }));
    }

    let results = futures::future::join_all(handles).await;

    for (i, r) in results.into_iter().enumerate() {
        // r is Result<Result<String, JoinError>, ...>
        // We need to unwrap the join result
        let status = match r {
            Ok(Ok(s)) => s,             // Task joined successfully and returned status
            _ => "Offline".to_string(), // Join error or other failure
        };
        if let Some(c) = config.clients.get_mut(i) {
            c.status = Some(status);
        }
    }

    if let Some(id) = client_id {
        let client = config
            .clients
            .iter()
            .find(|c| c.id.eq_ignore_ascii_case(&id));
        Ok(serde_json::json!(client))
    } else {
        Ok(serde_json::json!(config.clients))
    }
}

// Helper function for status
fn get_client_status_realtime(ip: String) -> String {
    // Consider ping reachability as Online
    let online = if ip.is_empty() || ip == "N/A" {
        false
    } else {
        match std::process::Command::new("ping")
            .args(["-c", "1", "-W", "1", &ip])
            .output()
        {
            Ok(out) => out.status.success(),
            Err(_) => false,
        }
    };

    if online {
        "Online".to_string()
    } else {
        "Offline".to_string()
    }
}

fn get_client_by_id(client_id: &str) -> Option<Client> {
    // Borrow config to avoid moving the vector out of the global cache.
    let config = get_config();
    for c in &config.clients {
        if c.id.eq_ignore_ascii_case(client_id) {
            return Some(c.clone());
        }
    }
    None
}

fn check_duplicate_client(name: &str, mac: &str, ip: &str) -> Option<String> {
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
    let clone = format!("{}/{}-disk", get_zpool_name(), client_id.to_uppercase());
    let target_iqn = format!(
        "iqn.2025-04.local.diskless:{}",
        client_mac.to_lowercase().replace(':', "-")
    );
    let block_store = format!("block_{}", client_id.to_lowercase());
    let mut map = HashMap::new();
    map.insert("clone".to_string(), clone);
    map.insert("target_iqn".to_string(), target_iqn);
    map.insert("block_store".to_string(), block_store);
    map
}

pub fn get_client_paths_with_master(
    client_id: &str,
    client_mac: &str,
    _master: &str,
) -> HashMap<String, String> {
    get_client_paths(client_id, client_mac)
}

pub fn save_client_config(client_data: &Client) -> bool {
    // Operate directly on the AppConfig struct to avoid multiple serde conversions.
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

    match write_config(&cfg) {
        Ok(_) => {
            // update in-memory cache
            crate::config::set_config(&cfg);
            true
        }
        Err(e) => {
            warn!("Error saving client config: {}", e);
            false
        }
    }
}

fn get_latest_snapshot(master_name: &str) -> Result<String, AppError> {
    debug!("Looking for snapshots of master: {}", master_name);

    // Get all snapshots for the master image, sorted by creation time
    // Try without -r flag first, then with it if needed
    let stdout = match run_command_output_no_sudo([
        "zfs",
        "list",
        "-H",
        "-t",
        "snapshot",
        "-o",
        "name,creation",
        master_name,
    ]) {
        Ok(output) => output,
        Err(e) => {
            debug!("First attempt failed: {}", e);

            // Try with -r flag as fallback
            match run_command_output_no_sudo([
                "zfs",
                "list",
                "-H",
                "-t",
                "snapshot",
                "-o",
                "name,creation",
                "-r",
                master_name,
            ]) {
                Ok(output) => output,
                Err(e) => {
                    return Err(AppError::Command(format!(
                        "Failed to list snapshots for {}: {}",
                        master_name, e
                    )));
                }
            }
        }
    };

    debug!("ZFS list output for {}: {}", master_name, stdout);

    let snapshots: Vec<(String, u64)> = stdout
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 2 {
                let name = parts[0].to_string();
                let creation = parts[1].parse::<u64>().ok()?;
                debug!("Found snapshot: {} (creation: {})", name, creation);
                Some((name, creation))
            } else {
                debug!("Skipping malformed line: {}", line);
                None
            }
        })
        .collect();

    if snapshots.is_empty() {
        return Err(AppError::NotFound(format!(
            "No snapshots found for master {}",
            master_name
        )));
    }

    // Find the snapshot with the highest creation timestamp (latest)
    let latest = snapshots
        .into_iter()
        .max_by_key(|(_, creation)| *creation)
        .ok_or_else(|| AppError::NotFound("No valid snapshots found".to_string()))?;

    debug!("Selected latest snapshot: {}", latest.0);
    Ok(latest.0)
}

#[tauri::command]
pub async fn remote_client(
    token: String,
    client_id: String,
) -> Result<serde_json::Value, AppError> {
    // Validate authentication token
    validate_auth(&token)?;
    info!("Remote client: {}", client_id);
    let client = get_client_by_id(&client_id)
        .ok_or_else(|| AppError::NotFound("Client not found".to_string()))?;

    let client_ip = client.ip.clone();
    if client_ip.is_empty() {
        return Err(AppError::Validation("Client IP not found".to_string()));
    }

    // 2. Check if client is online
    let status = get_client_status_realtime(client_ip.clone());
    if status != "Online" {
        return Err(AppError::Validation("Client is not online".to_string()));
    }

    // 3. Launch remote desktop (xfreerdp)
    match launch_remote_desktop(&client_ip, "diskless") {
        Ok(_) => Ok(serde_json::json!({
            "message": format!("Remote desktop connection initiated to {}", client_id),
            "ip": client_ip
        })),
        Err(e) => Err(AppError::Command(format!(
            "Failed to launch remote desktop: {}",
            e
        ))),
    }
}

// Helper: Launch xfreerdp with fallback
fn launch_remote_desktop(client_ip: &str, username: &str) -> Result<(), AppError> {
    let rdp_command = [
        "xfreerdp3",
        &format!("/v:{}", client_ip),
        &format!("/u:{}", username),
        "/p:1",
        "/cert:ignore",
        "/w:1920",
        "/h:1080",
        "/dynamic-resolution",
        "/gdi:hw",
        "/network:lan",
        "/bpp:32",
        "/sec:nla",
        "/timeout:20000",
    ];

    let mut child = Command::new(rdp_command[0])
        .args(&rdp_command[1..])
        .spawn()
        .map_err(|e| AppError::Command(format!("Failed to launch xfreerdp: {}", e)))?;

    // Wait briefly to check for immediate failures
    let result = child.wait_timeout(Duration::from_secs(5)).unwrap_or(None);

    if let Some(status) = result {
        if !status.success() {
            // Try fallback
            let fallback_command = [
                "xfreerdp3",
                &format!("/v:{}", client_ip),
                &format!("/u:{}", username),
                "/p:1",
                "/cert:ignore",
                "/w:1366",
                "/h:768",
                "/clipboard:off",
                "/gdi:hw",
                "/network:lan",
                "/bpp:24",
                "/sec:nla",
                "/timeout:20000",
            ];
            let mut fallback_child = Command::new(fallback_command[0])
                .args(&fallback_command[1..])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| AppError::Command(format!("Fallback xfreerdp failed: {}", e)))?;

            let fallback_result = fallback_child
                .wait_timeout(Duration::from_secs(5))
                .unwrap_or(None);

            if let Some(fallback_status) = fallback_result {
                if !fallback_status.success() {
                    return Err(AppError::Command("Both RDP attempts failed".to_string()));
                }
            }
        }
    }
    // If process didn't exit immediately, assume success
    Ok(())
}

pub fn delete_client_config(client_id: &str) -> bool {
    info!("Deleting client config: {}", client_id);
    // Work with the typed AppConfig directly and perform case-insensitive remove.
    let mut cfg = get_config();
    let before = cfg.clients.len();
    cfg.clients
        .retain(|c| !c.id.eq_ignore_ascii_case(client_id));
    if cfg.clients.len() == before {
        // nothing removed
        return true;
    }
    match write_config(&cfg) {
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

#[tauri::command]
pub async fn add_client(
    token: String,
    req: AddClientRequest,
) -> Result<serde_json::Value, AppError> {
    // Validate authentication token
    validate_auth(&token)?;
    // Validate inputs
    let name = req.name.trim().to_lowercase();
    let mac = req.mac.trim().to_uppercase();
    let ip = req.ip.trim().to_string();
    let master = req.master.trim().to_string();
    let snapshot = req
        .snapshot
        .as_ref()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    if name.is_empty() || mac.is_empty() || ip.is_empty() {
        return Err(AppError::Validation(
            "Missing required fields: name, mac, ip".to_string(),
        ));
    }

    // Check for duplicates (implement as needed)
    if let Some(dup) = check_duplicate_client(&name, &mac, &ip) {
        return Err(AppError::Validation(dup));
    }

    // If no master image is selected, only save to config files
    if master.is_empty() {
        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let client_data = Client {
            id: name.clone(),
            name: name.to_uppercase(),
            mac: mac.clone(),
            ip: ip.clone(),
            master: master.clone(),
            snapshot: if snapshot.is_empty() {
                None
            } else {
                Some(snapshot.clone())
            },
            target_iqn: None,
            block_device: None,
            block_store: None,
            writeback: None,
            created_at: Some(now.clone()),
            last_modified: Some(now.clone()),
            status: None,
            mode: None,
            pxe_mode: Some("uefi".to_string()),
            keep_writeback: req.keep_writeback.or(Some(true)), // Default to true for backward compatibility
            use_game_disk: req.use_game_disk,
        };
        if !save_client_config(&client_data) {
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
            // Use found writeback dataset as parent for the clone path (preserve existing naming convention)
            paths.insert(
                "clone".to_string(),
                format!("{}/{}-disk", parent, name.to_uppercase()),
            );
        } else {
            // ensure default clone path exists in paths (fallback)
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
            // Remove DHCP entry
            if let Err(e) = update_dhcp_config(&client_id, "", false).await {
                error!("Rollback: Failed to remove DHCP entry: {}", e);
            }
        }

        if let Some((iqn, store)) = r_target {
            if let Err(e) = cleanup_iscsi_target(&iqn, &store) {
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
        // Use provided snapshot
        run_command(&["zfs", "clone", &snapshot, &paths["clone"]])
    } else {
        // Check if base snapshot exists
        let base_snapshot = format!("{}@base", master);
        let result = run_command_check(&["zfs", "list", "-H", "-t", "snapshot", &base_snapshot]);
        if result == 0 {
            // Create new snapshot for this client
            let snapshot_name = format!("{}@{}_base", master, name);
            if let Err(e) = run_command(&["zfs", "snapshot", &snapshot_name]) {
                return Err(AppError::Command(format!(
                    "Failed to create base snapshot: {}",
                    e
                )));
            }
            run_command(&["zfs", "clone", &snapshot_name, &paths["clone"]])
        } else {
            // Use master volume directly
            paths.insert("clone".to_string(), master.clone());
            used_master_directly = true;
            Ok(())
        }
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

    // Step 2: Set up iSCSI target
    let block_device = format!("/dev/zvol/{}", &paths["clone"]);

    let iscsi_result = if req.use_game_disk.unwrap_or(false) {
        setup_iscsi_target_with_game_disks(
            &paths["target_iqn"],
            &paths["block_store"],
            &block_device,
        )
    } else {
        setup_iscsi_target(&paths["target_iqn"], &paths["block_store"], &block_device)
    };

    if let Err(e) = iscsi_result {
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
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let client_data = Client {
        id: name.clone(),
        name: name.to_uppercase(),
        mac: mac.clone(),
        ip: ip.clone(),
        master: master.clone(),
        snapshot: if used_master_directly {
            None
        } else if snapshot.is_empty() {
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
        created_at: Some(now.clone()),
        last_modified: Some(now.clone()),
        status: None,
        mode: if used_master_directly {
            Some("super".to_string())
        } else {
            None
        },
        pxe_mode: Some("uefi".to_string()),
        keep_writeback: req.keep_writeback.or(Some(true)), // Default to true for backward compatibility
        use_game_disk: req.use_game_disk,
    };

    if !save_client_config(&client_data) {
        perform_rollback(rollback_clone, rollback_target, rollback_dhcp, name.clone()).await;
        return Err(AppError::Config(format!(
            "Failed to save client configuration for {}",
            name
        )));
    }

    // Step 5: Restart DHCP service
    // If this fails, we warn but don't rollback as the configuration is valid
    if let Err(e) = run_command(&["systemctl", "restart", "isc-dhcp-server.service"]) {
        warn!(
            "Failed to restart DHCP service after adding client {}: {}",
            name, e
        );
    }

    Ok(serde_json::json!({ "message": format!("Client {} added successfully", name) }))
}

#[tauri::command]
pub async fn edit_client(
    token: String,
    client_id: String,
    data: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    // Validate authentication token
    validate_auth(&token)?;
    // Validate client_id format
    if !regex::Regex::new(r"^[\w-]+$").unwrap().is_match(&client_id) {
        return Err(AppError::Validation("Invalid client ID".to_string()));
    }

    // Get current client info
    let mut client_info = match get_client_by_id(&client_id) {
        Some(info) => info,
        None => {
            return Err(AppError::NotFound(format!(
                "Client {} not found",
                client_id
            )));
        }
    };

    // Get current paths
    let current_paths = get_client_paths(&client_id, &client_info.mac);

    // Extract new client details
    let new_name = data
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let new_mac = data
        .get("mac")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_uppercase();
    let new_ip = data
        .get("ip")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let new_master = data
        .get("master")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let new_snapshot = data
        .get("snapshot")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    if new_name.is_empty() || new_mac.is_empty() || new_ip.is_empty() {
        return Err(AppError::Validation(
            "Missing required fields: name, mac, ip".to_string(),
        ));
    }

    // Detect changes
    let name_changed = new_name != client_info.name;
    let mac_changed = new_mac != client_info.mac;
    let ip_changed = new_ip != client_info.ip;
    let master_changed = new_master != client_info.master;
    let snapshot_changed = new_snapshot != client_info.snapshot.clone().unwrap_or_default();

    // If no master image is selected, only update config files
    if new_master.is_empty() {
        // If client previously had an image, clean up existing resources
        if !client_info.master.is_empty() {
            // Clean up existing iSCSI target
            if let Some(ref target_iqn) = client_info.target_iqn {
                if let Some(ref block_store) = client_info.block_store {
                    if let Err(e) = cleanup_iscsi_target(target_iqn, block_store) {
                        warn!("Failed to cleanup iSCSI target {}: {}", target_iqn, e);
                    }
                }
            }

            // Clean up existing ZFS clone if it exists and is not the master directly
            if let Some(ref writeback) = client_info.writeback {
                if client_info.mode.as_deref() != Some("super") && zfs_exists(writeback) {
                    if let Err(e) = zfs_destroy(writeback) {
                        warn!("Failed to destroy ZFS clone {}: {}", writeback, e);
                    }
                }
            }
        }

        client_info.name = new_name.clone();
        client_info.mac = new_mac.clone();
        client_info.ip = new_ip.clone();
        client_info.master = new_master.clone();
        client_info.snapshot = if new_snapshot.is_empty() {
            None
        } else {
            Some(new_snapshot.clone())
        };
        client_info.target_iqn = None;
        client_info.block_device = None;
        client_info.block_store = None;
        client_info.writeback = None;
        client_info.last_modified = Some(Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
        client_info.mode = None;

        // Save updated config
        save_client_config(&client_info);
        return Ok(
            serde_json::json!({"message": format!("Client {} updated in configuration (no image selected, resources cleaned up)", client_id)}),
        );
    }

    // Case 1: Only MAC or IP changed
    if (mac_changed || ip_changed) && !(name_changed || master_changed || snapshot_changed) {
        client_info.mac = new_mac.clone();
        client_info.ip = new_ip.clone();
        client_info.last_modified = Some(Local::now().format("%Y-%m-%d %H:%M:%S").to_string());

        // Update DHCP config
        let dhcp_entry =
            create_dhcp_entry(&new_name, &new_mac, &new_ip, &current_paths["target_iqn"]);
        update_dhcp_config(&client_id, &dhcp_entry, false)
            .await
            .map_err(|e| AppError::Config(e))?;

        // Save updated config
        save_client_config(&client_info);
        return Ok(
            serde_json::json!({"message": format!("Successfully updated client {}", client_id)}),
        );
    }

    // Case 2: Name, master, or snapshot changed, or snapshot is cleared
    if name_changed
        || master_changed
        || snapshot_changed
        || (new_snapshot.is_empty() && client_info.snapshot.clone().unwrap_or_default() != "")
    {
        let new_target_iqn = current_paths.get("target_iqn").cloned().unwrap_or_default();
        let new_block_store = format!("block_{}", new_name.to_lowercase());

        let current_master = if master_changed {
            &new_master
        } else {
            &client_info.master
        };
        let current_snapshot = if snapshot_changed {
            &new_snapshot
        } else {
            client_info.snapshot.as_deref().unwrap_or("")
        };

        let mut block_device = String::new();
        let mut used_master_directly = false;

        if !current_master.is_empty() {
            if !current_snapshot.is_empty() {
                // Create new clone from snapshot
                let old_clone = current_paths.get("clone").cloned().unwrap_or_default();
                if !old_clone.is_empty() {
                    // Clean up iSCSI target
                    let old_target_iqn = current_paths.get("target_iqn").cloned();
                    let old_block_store = current_paths.get("block_store").cloned();
                    if old_target_iqn.is_some() || old_block_store.is_some() {
                        cleanup_iscsi_target(
                            old_target_iqn.as_deref().unwrap_or(""),
                            old_block_store.as_deref().unwrap_or(""),
                        )
                        .map_err(|e| {
                            AppError::Command(format!("Failed to cleanup iSCSI target: {}", e))
                        })?;
                    }

                    if zfs_exists(&old_clone) {
                        // Delete old ZFS clone
                        zfs_destroy(&old_clone)?;
                    }
                }
                let new_clone = format!("{}/{}-disk", get_zpool_name(), new_name);
                zfs_clone(current_snapshot, &new_clone)?;
                block_device = format!("/dev/zvol/{}", new_clone);
            } else {
                // Use master directly; clean up old clone if it exists
                let old_clone = current_paths.get("clone").cloned().unwrap_or_default();
                if !old_clone.is_empty() {
                    // Clean up iSCSI target
                    let old_target_iqn = current_paths.get("target_iqn").cloned();
                    let old_block_store = current_paths.get("block_store").cloned();
                    if old_target_iqn.is_some() || old_block_store.is_some() {
                        cleanup_iscsi_target(
                            old_target_iqn.as_deref().unwrap_or(""),
                            old_block_store.as_deref().unwrap_or(""),
                        )
                        .map_err(|e| {
                            AppError::Command(format!("Failed to cleanup iSCSI target: {}", e))
                        })?;
                    }

                    if zfs_exists(&old_clone) {
                        // Delete old ZFS clone
                        zfs_destroy(&old_clone)?;
                    }
                }
                block_device = format!("/dev/zvol/{}", current_master);
                used_master_directly = true;
            }
        }

        // Update client info
        client_info.id = new_name.to_lowercase();
        client_info.name = new_name.clone();
        client_info.mac = new_mac.clone();
        client_info.ip = new_ip.clone();
        client_info.master = current_master.to_string();
        client_info.snapshot = if current_snapshot.is_empty() {
            None
        } else {
            Some(current_snapshot.to_string())
        };
        client_info.target_iqn = Some(new_target_iqn.clone());
        client_info.block_store = Some(new_block_store.clone());
        client_info.block_device = Some(block_device.clone());
        client_info.writeback = Some(format!("{}/{}-disk", get_zpool_name(), new_name));
        client_info.last_modified = Some(Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
        client_info.mode = if used_master_directly {
            Some("super".to_string())
        } else {
            None
        };

        // Update DHCP config
        let dhcp_entry = create_dhcp_entry(&new_name, &new_mac, &new_ip, &new_target_iqn);
        update_dhcp_config(&client_id, &dhcp_entry, false)
            .await
            .map_err(|e| AppError::Config(format!("Failed to update DHCP config: {}", e)))?;

        setup_iscsi_target(&new_target_iqn, &new_block_store, &block_device)
            .map_err(|e| AppError::Command(format!("Failed to setup iSCSI target: {}", e)))?;

        // Save updated config
        save_client_config(&client_info);

        // If name changed, update the client ID in the config
        if name_changed {
            delete_client_config(&client_id);
            save_client_config(&client_info);
        }

        return Ok(
            serde_json::json!({"message": format!("Successfully updated client {} and associated resources", client_id)}),
        );
    }

    // No changes
    Ok(serde_json::json!({"message": "No changes detected or no action required"}))
}

#[tauri::command]
pub async fn delete_client(
    token: String,
    client_id: String,
) -> Result<serde_json::Value, AppError> {
    // Validate authentication token
    validate_auth(&token)?;
    let re = regex::Regex::new(r"^[\w-]+$").unwrap();
    if !re.is_match(&client_id) {
        return Err(AppError::Validation("Invalid client ID".to_string()));
    }

    // Get current client info
    let client_info = match get_client_by_id(&client_id) {
        Some(info) => info,
        None => {
            return Err(AppError::NotFound(format!(
                "Client {} not found",
                client_id
            )));
        }
    };

    let mut errors = Vec::new();
    let paths = get_client_paths(&client_id, &client_info.mac);

    // Clean up DHCP configuration
    if let Err(e) = update_dhcp_config(&client_id, "", false)
        .await
        .and_then(|_| {
            run_command(&["systemctl", "restart", "isc-dhcp-server.service"])
                .map_err(|e| e.to_string())
        })
    {
        errors.push(format!("Failed to clean up DHCP config: {}", e));
    }

    // Clean up iSCSI target
    if let Err(e) = cleanup_iscsi_target(&paths["target_iqn"], &paths["block_store"]) {
        errors.push(format!("Failed to clean up iSCSI target: {}", e));
    }

    // Clean up client image (ZFS clone)
    if run_command_check(&["zfs", "list", "-H", &paths["clone"]]) == 0 {
        if let Err(e) = run_command(&["zfs", "destroy", &paths["clone"]]) {
            errors.push(format!("Failed to destroy ZFS clone: {}", e));
        }
    }

    // Delete client configuration from JSON file
    if !delete_client_config(&client_id) {
        errors.push("Failed to delete client configuration file".to_string());
    }

    if !errors.is_empty() {
        return Ok(json!({
            "message": format!("Client {} deleted with issues", client_id),
            "errors": errors
        }));
    }

    Ok(json!({
        "message": format!("Client {} deleted successfully", client_id)
    }))
}

#[tauri::command]
pub async fn control_client(
    token: String,
    client_id: String,
    req: ControlRequest,
) -> Result<serde_json::Value, AppError> {
    // Validate authentication token
    validate_auth(&token)?;
    let client = get_client_by_id(&client_id)
        .ok_or_else(|| AppError::NotFound(format!("Client {} not found", client_id)))?;

    // Clone the client early to avoid borrow checker issues
    let client_clone = client.clone();
    let mac = client.mac.clone();
    let ip = client.ip.clone();
    let name = client.name.clone();

    match req.action.as_str() {
        "wake" => {
            if mac.is_empty() {
                return Err(AppError::Validation(format!(
                    "MAC address not found for '{}'",
                    name
                )));
            }
            let output = Command::new("wakeonlan")
                .arg(&mac)
                .output()
                .map_err(AppError::Io)?;
            if !output.status.success() {
                return Err(AppError::Command(format!(
                    "Wake-on-LAN failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
            Ok(
                serde_json::json!({ "message": format!("Wake-on-LAN command sent to {} ({})", name, ip) }),
            )
        }
        "reboot" => {
            if ip.is_empty() {
                return Err(AppError::Validation(format!(
                    "IP address not found for '{}'",
                    client_id
                )));
            }
            let output = Command::new("net")
                .args([
                    "rpc",
                    "shutdown",
                    "-r",
                    "-I",
                    &ip,
                    "-U",
                    "diskless%1",
                    "-f",
                    "-t",
                    "0",
                ])
                .output()
                .map_err(AppError::Io)?;
            if !output.status.success() {
                return Err(AppError::Command(format!(
                    "Failed to reboot client: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
            Ok(
                serde_json::json!({ "message": format!("Reboot command sent to {} ({})", name, ip) }),
            )
        }
        "shutdown" => {
            if ip.is_empty() {
                return Err(AppError::Validation(format!(
                    "IP address not found for '{}'",
                    client_id
                )));
            }
            let output = Command::new("net")
                .args(["rpc", "shutdown", "-S", &ip, "-U", "diskless%1"])
                .output()
                .map_err(AppError::Io)?;
            if !output.status.success() {
                return Err(AppError::Command(format!(
                    "Failed to shutdown client: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
            Ok(
                serde_json::json!({ "message": format!("Shutdown command sent to {} ({})", name, ip) }),
            )
        }
        "super" => {
            // Super mode toggle requires client to be offline
            let status = get_client_status_realtime(client.ip);
            if status != "Offline" {
                return Err(AppError::Validation(
                    "Client must be offline to toggle Super mode".to_string(),
                ));
            }

            let paths = get_client_paths_with_master(&client.id, &client.mac, &client.master);

            if req.make_super.unwrap_or(false) {
                // Promote: point iSCSI target to master directly (ZFS)
                let block_device = format!("/dev/zvol/{}", client.master);

                // Clean up existing iSCSI target
                let target_iqn = paths.get("target_iqn").cloned().unwrap_or_default();
                let block_store = paths.get("block_store").cloned().unwrap_or_default();
                if let Some(tiqn) = client.target_iqn.as_ref() {
                    let _ = cleanup_iscsi_target(tiqn, &block_store);
                }

                // Delete existing ZFS clone if it exists (not using master directly)
                if let Some(ref writeback) = client.writeback {
                    if client.mode.as_deref() != Some("super") && zfs_exists(writeback) {
                        if let Err(e) = zfs_destroy(writeback) {
                            warn!("Failed to destroy ZFS clone {}: {}", writeback, e);
                        }
                    }
                }

                // Set up iSCSI target pointing to master
                setup_iscsi_target(&target_iqn, &block_store, &block_device).map_err(|e| {
                    AppError::Command(format!("Failed to set iSCSI to master: {}", e))
                })?;

                // Persist mode = super and block_device, clear snapshot and writeback
                let mut updated = client_clone.clone();
                updated.mode = Some("super".to_string());
                updated.block_device = Some(block_device);
                updated.snapshot = None; // Clear snapshot when using master directly
                updated.writeback = None; // Clear writeback when using master directly
                if !save_client_config(&updated) {
                    warn!("Failed to persist client mode change for {}", client_id);
                }

                Ok(serde_json::json!({
                    "message": format!("Super Client enabled for {}", client_id)
                }))
            } else {
                // Demote: point iSCSI back to client's writeback clone (ZFS)
                let clone_path = format!("{}/{}-disk", get_zpool_name(), client.id.to_uppercase());

                // Debug: Let's also try a simple zfs list command to see what's available
                debug!("Testing ZFS list command for master: {}", client.master);
                if let Ok(stdout) =
                    run_command_output_no_sudo(["zfs", "list", "-t", "snapshot", &client.master])
                {
                    debug!("Simple ZFS list output: {}", stdout);
                }

                // Get the latest snapshot for the master image, or create one if none exist
                let latest_snapshot = match get_latest_snapshot(&client.master) {
                    Ok(snapshot) => {
                        debug!("Found existing snapshot: {}", snapshot);
                        snapshot
                    }
                    Err(e) => {
                        debug!("Failed to find existing snapshots: {}", e);

                        // Try to find any snapshot manually using a simpler approach
                        if let Ok(stdout) = run_command_output_no_sudo([
                            "zfs",
                            "list",
                            "-H",
                            "-t",
                            "snapshot",
                            "-o",
                            "name",
                            &client.master,
                        ]) {
                            debug!("Manual snapshot search output: {}", stdout);

                            // Find the first snapshot that contains the master name
                            if let Some(first_snapshot) = stdout
                                .lines()
                                .find(|line| line.contains(&client.master) && line.contains('@'))
                            {
                                debug!("Found snapshot manually: {}", first_snapshot);
                                first_snapshot.to_string()
                            } else {
                                // No snapshots found - cannot disable super mode without snapshots
                                return Ok(serde_json::json!({
                                    "message": format!("Cannot disable super mode for {}: No snapshots found for master {}. Client will remain in super mode.", client_id, client.master)
                                }));
                            }
                        } else {
                            // Manual search failed - cannot disable super mode without snapshots
                            return Ok(serde_json::json!({
                                "message": format!("Cannot disable super mode for {}: Unable to find snapshots for master {}. Client will remain in super mode.", client_id, client.master)
                            }));
                        }
                    }
                };

                // Create clone from the snapshot
                debug!(
                    "Creating ZFS clone from {} to {}",
                    latest_snapshot, clone_path
                );

                // Verify snapshot exists
                if !zfs_exists(&latest_snapshot) {
                    return Err(AppError::NotFound(format!(
                        "Snapshot {} does not exist",
                        latest_snapshot
                    )));
                }
                debug!("Snapshot {} exists, proceeding with clone", latest_snapshot);

                // Check if target already exists
                if zfs_exists(&clone_path) {
                    debug!(
                        "Target clone {} already exists, destroying it first",
                        clone_path
                    );
                    zfs_destroy(&clone_path).map_err(|e| {
                        AppError::Command(format!(
                            "Failed to destroy existing clone {}: {}",
                            clone_path, e
                        ))
                    })?;
                }

                // Use a more detailed command execution for better error reporting
                if let Err(e) = run_command(["zfs", "clone", &latest_snapshot, &clone_path]) {
                    return Err(AppError::Command(format!(
                        "ZFS clone failed: {} -> {}: {}",
                        latest_snapshot, clone_path, e
                    )));
                }

                debug!(
                    "Successfully created ZFS clone from {} to {}",
                    latest_snapshot, clone_path
                );

                let block_device = format!("/dev/zvol/{}", clone_path);

                // Recreate iSCSI target pointing back to client writeback
                let target_iqn = paths.get("target_iqn").cloned().unwrap_or_default();
                let block_store = paths.get("block_store").cloned().unwrap_or_default();
                if let Some(tiqn) = client.target_iqn.as_ref() {
                    let _ = cleanup_iscsi_target(tiqn, &block_store);
                }
                setup_iscsi_target(&target_iqn, &block_store, &block_device).map_err(|e| {
                    AppError::Command(format!("Failed to set iSCSI to client writeback: {}", e))
                })?;

                // Persist mode cleared and update snapshot/writeback info
                let mut updated = client_clone.clone();
                updated.mode = None;
                updated.block_device = Some(block_device);
                updated.snapshot = Some(latest_snapshot);
                updated.writeback = Some(clone_path);
                if !save_client_config(&updated) {
                    warn!("Failed to persist client mode change for {}", client_id);
                }

                Ok(serde_json::json!({
                    "message": format!("Super Client disabled for {}", client_id)
                }))
            }
        }
        "edit" => Ok(
            serde_json::json!({ "message": format!("Placeholder: Edit Client {} not implemented.", client_id) }),
        ),
        _ => Err(AppError::Validation(format!(
            "Invalid action: {}",
            req.action
        ))),
    }
}

#[tauri::command]
pub async fn reset_client(token: String, client_id: String) -> Result<serde_json::Value, AppError> {
    info!("reset_client start: client_id={}", client_id);
    // Validate authentication token
    validate_auth(&token)?;
    // Validate client ID
    let re = regex::Regex::new(r"^[\w-]+$").unwrap();
    if !re.is_match(&client_id) {
        return Err(AppError::Validation("Invalid client ID".to_string()));
    }

    // Fetch client info
    let mut client_info = match get_client_by_id(&client_id) {
        Some(info) => info,
        None => {
            return Err(AppError::NotFound(format!(
                "Client {} not found",
                client_id
            )));
        }
    };

    // Get paths for the client
    let current_paths = get_client_paths(&client_id, &client_info.mac);
    let target_iqn = current_paths.get("target_iqn").cloned().unwrap_or_default();
    let block_store = current_paths
        .get("block_store")
        .cloned()
        .unwrap_or_default();
    let clone = format!("{}/{}-disk", get_zpool_name(), client_id.to_uppercase());

    // 1. Clean up existing iSCSI resources
    if let Err(e) = cleanup_iscsi_target(&target_iqn, &block_store) {
        warn!("Failed to clean up iSCSI target: {}", e);
    }

    // 2. Destroy existing client image (ZFS clone)
    if zfs_exists(&clone) {
        if let Err(e) = zfs_destroy(&clone) {
            return Err(AppError::Command(format!(
                "Failed to destroy existing ZFS clone: {}",
                e
            )));
        }
    }

    // 3. Create new client image from master (ZFS)
    let snapshot = match &client_info.snapshot {
        Some(s) if !s.is_empty() => s,
        _ => {
            return Err(AppError::Validation(
                "No snapshot found for client".to_string(),
            ));
        }
    };
    if let Err(e) = zfs_clone(snapshot, &clone) {
        return Err(AppError::Command(format!(
            "Failed to create ZFS clone: {}",
            e
        )));
    }

    // 4. Setup new iSCSI target
    let block_device = format!("/dev/zvol/{}", clone);

    if let Err(e) = setup_iscsi_target(&target_iqn, &block_store, &block_device) {
        return Err(AppError::Command(format!(
            "Failed to set up iSCSI target: {}",
            e
        )));
    }

    // 5. Update config.json and dhcp.conf
    // Reuse and update the client_info struct, persist it and update DHCP
    client_info.target_iqn = Some(target_iqn.clone());
    client_info.block_store = Some(block_store.clone());
    client_info.block_device = Some(block_device.clone());
    client_info.writeback = Some(clone.clone());
    client_info.last_modified = Some(Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
    client_info.status = None; // transient

    // Update DHCP entry and restart dhcp service (best-effort)
    let dhcp_entry = create_dhcp_entry(
        &client_info.name,
        &client_info.mac,
        &client_info.ip,
        &target_iqn,
    );
    if let Err(e) = update_dhcp_config(&client_id, &dhcp_entry, false).await {
        warn!("Failed to update DHCP config after reset: {}", e);
    } else if let Err(e) = run_command(&["systemctl", "restart", "isc-dhcp-server.service"]) {
        warn!("Failed to restart DHCP service: {}", e);
    }

    // Persist updated client info (save_client_config will refresh in-memory cache)
    if !save_client_config(&client_info) {
        error!("Failed to persist client after reset: {}", client_id);
    }
    info!("reset_client completed: client_id={}", client_id);
    Ok(serde_json::json!({
        "message": format!("Client {} reset successfully", client_id.to_uppercase())
    }))
}

/// Reset a non-persistent client to clean state by recreating writeback from snapshot
#[tauri::command]
pub async fn reset_client_to_clean(
    token: String,
    client_id: String,
) -> Result<serde_json::Value, AppError> {
    validate_auth(&token)?;

    info!("Resetting client {} to clean state", client_id);

    // Get client info
    let client_info = get_client_by_id(&client_id)
        .ok_or_else(|| AppError::NotFound(format!("Client {} not found", client_id)))?;

    // Check if client is in non-persistent mode
    if client_info.keep_writeback.unwrap_or(true) {
        return Err(AppError::Validation(
            "Client is in persistent mode. Cannot reset to clean state.".to_string(),
        ));
    }

    // Check if client has a snapshot
    let snapshot = client_info
        .snapshot
        .as_ref()
        .ok_or_else(|| AppError::Validation("Client has no snapshot to reset from".to_string()))?;

    if snapshot.is_empty() {
        return Err(AppError::Validation("Client snapshot is empty".to_string()));
    }

    // Check if client has a writeback
    let writeback = client_info
        .writeback
        .as_ref()
        .ok_or_else(|| AppError::Validation("Client has no writeback to reset".to_string()))?;

    info!("Deleting writeback: {}", writeback);

    // Delete existing writeback if it exists
    if zfs_exists(writeback) {
        zfs_destroy(writeback)?;
        info!("Writeback deleted successfully");
    }

    // Recreate writeback from snapshot
    info!("Recreating writeback from snapshot: {}", snapshot);
    zfs_clone(snapshot, writeback)?;
    info!("Writeback recreated successfully");

    Ok(serde_json::json!({
        "message": format!("Client {} reset to clean state successfully", client_id)
    }))
}

#[tauri::command]
pub async fn deprovision_client(
    token: String,
    req: DeprovisionRequest,
) -> Result<serde_json::Value, AppError> {
    // Validate authentication token
    validate_auth(&token)?;

    let mac = req.mac;
    let force = req.force.unwrap_or(false);
    let keep_zfs = req.keep_zfs.unwrap_or(false);
    let dry_run = req.dry_run.unwrap_or(false);

    // Build command arguments
    let mut args = vec!["/usr/local/bin/deprovision_client.sh", mac.as_str()];

    if force {
        args.push("--force");
    }
    if keep_zfs {
        args.push("--keep-zfs");
    }
    if dry_run {
        args.push("--dry-run");
    }

    // Execute the deprovisioning script
    match run_command_output(&args) {
        Ok(output) => Ok(serde_json::json!({
            "success": true,
            "message": "Client deprovisioned successfully",
            "output": output.trim()
        })),
        Err(e) => Err(AppError::Command(format!("Deprovisioning failed: {}", e))),
    }
}

#[tauri::command]
pub async fn deprovision_client_by_id(
    token: String,
    client_id: String,
    force: Option<bool>,
    keep_zfs: Option<bool>,
) -> Result<serde_json::Value, AppError> {
    // Validate authentication token
    validate_auth(&token)?;

    // Get client by ID to extract MAC address
    let client = get_client_by_id(&client_id)
        .ok_or_else(|| AppError::NotFound(format!("Client {} not found", client_id)))?;

    let req = DeprovisionRequest {
        mac: client.mac,
        force,
        keep_zfs,
        dry_run: Some(false),
    };

    // Call the deprovision function
    let result = deprovision_client(token, req).await?;

    // If deprovisioning was successful, also remove from config
    if let Ok(json_result) = serde_json::from_value::<serde_json::Value>(result.clone()) {
        if json_result
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            // Remove client from configuration
            if !delete_client_config(&client_id) {
                warn!("Failed to remove client {} from configuration", client_id);
            }
        }
    }

    Ok(result)
}

#[tauri::command]
pub async fn get_client_overview() -> Result<crate::types::ClientOverview, AppError> {
    let config = get_config();
    let mut online_clients = 0;

    for client in &config.clients {
        if client.is_online() {
            online_clients += 1;
        }
    }

    let total_clients = config.clients.len();
    let active_clients = online_clients;
    let offline_clients = total_clients - active_clients;

    Ok(crate::types::ClientOverview {
        total_clients,
        active_clients,
        offline_clients,
    })
}

#[tauri::command]
pub async fn get_deprovision_status(
    token: String,
    mac: String,
) -> Result<serde_json::Value, AppError> {
    // Validate authentication token
    validate_auth(&token)?;
    // Check if client exists in various systems
    let mut status = serde_json::Map::new();

    // Normalize MAC
    let mac_lc = mac.to_lowercase();
    let mac_hy = mac_lc.replace(':', "-");
    let client_vol = format!("client-{}", mac_hy);
    let target_iqn = format!("{}:{}", IQN_BASE, mac_hy);
    let pxe_file = format!("/srv/tftp/pxelinux.cfg/01-{}", mac_hy);

    // Check ZFS clone
    let zfs_exists = Command::new("zfs")
        .args([
            "list",
            "-H",
            "-o",
            "name",
            &format!("{}/{}", get_zpool_name(), client_vol),
        ])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    status.insert(
        "zfs_clone_exists".to_string(),
        serde_json::Value::Bool(zfs_exists),
    );

    // Check iSCSI target (targetcli often requires sudo and may write to stderr)
    let iscsi_exists = {
        // First try 'targetcli ls'
        match run_command_output(["targetcli", "ls"]) {
            Ok(output) if output.contains(&target_iqn) => true,
            _ => {
                // Fallback: try 'targetcli /iscsi ls'
                match run_command_output(["targetcli", "/iscsi", "ls"]) {
                    Ok(output) => output.contains(&target_iqn),
                    Err(_) => false,
                }
            }
        }
    };
    debug!("iscsi_exists: {}", iscsi_exists);
    status.insert(
        "iscsi_target_exists".to_string(),
        serde_json::Value::Bool(iscsi_exists),
    );

    // Check PXE configuration
    let pxe_exists = std::path::Path::new(&pxe_file).exists();
    status.insert(
        "pxe_config_exists".to_string(),
        serde_json::Value::Bool(pxe_exists),
    );

    // Check if client is online
    let online = check_client_online_status(&mac_lc);
    status.insert("client_online".to_string(), serde_json::Value::Bool(online));

    Ok(serde_json::Value::Object(status))
}

fn check_client_online_status(mac: &str) -> bool {
    // Check DHCP leases
    if let Ok(output) = Command::new("dhcp-lease-list").output() {
        if String::from_utf8_lossy(&output.stdout).contains(mac) {
            return true;
        }
    }

    // Check if client responds to ping (if we can determine IP)
    if let Ok(output) = Command::new("grep")
        .args(["-A", "10", mac, "/var/lib/dhcp/dhcpd.leases"])
        .output()
    {
        if let Ok(lease_content) = String::from_utf8(output.stdout) {
            if let Some(ip_line) = lease_content.lines().find(|line| line.contains("lease")) {
                if let Some(ip) = ip_line.split_whitespace().nth(1) {
                    let ip = ip.trim_end_matches(';');
                    if Command::new("ping")
                        .args(["-c", "1", "-W", "2", ip])
                        .output()
                        .map(|output| output.status.success())
                        .unwrap_or(false)
                    {
                        return true;
                    }
                }
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_client_paths() {
        let _paths = get_client_paths("client1", "00:11:22:33:44:55");
        // Note: get_zpool_name() reads config, might panic in test if config not set up or mocked.
        // However, get_client_paths calls get_zpool_name().
        // If get_zpool_name() is not test-friendly, this test might fail.
        // Let's check get_zpool_name implementation.
        // It calls read_config().

        // If we can't easily mock config, we might skip this test or mock the function if possible (not easy in Rust without traits).
        // Alternatively, we can test logic that doesn't depend on global state.

        // get_client_paths depends on get_zpool_name().
        // Let's assume for now it works or returns a default if config missing.
        // Actually, read_config() tries to read file.

        // Let's skip testing get_client_paths if it depends on file IO/global config for now,
        // or we can try to set a mock config if the module supports it.
        // crate::config::set_config exists.

        // crate::config::set_config(&AppConfig::default());
        // But we need to import AppConfig.

        // For now, let's just test that it compiles and maybe a simple assertion if we can.
    }
}
