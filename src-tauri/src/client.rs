//! Client management logic: helpers for client lookup, config, and deduplication.

use crate::{
    config::{read_config, write_config, Config},
    dhcp::{create_dhcp_entry, update_dhcp_config},
    iscsi::{cleanup_iscsi_target, setup_iscsi_target},
    utils::{run_command, run_command_check},
    zfs::{zfs_clone, zfs_destroy, zfs_exists},
    ZFS_POOL,
};
use chrono::Local;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Stdio;
use std::thread;
use std::time::Duration;
use std::{collections::HashMap, process::Command};

trait WaitTimeout {
    fn wait_timeout(&mut self, dur: Duration) -> std::io::Result<Option<std::process::ExitStatus>>;
}
impl WaitTimeout for std::process::Child {
    fn wait_timeout(&mut self, dur: Duration) -> std::io::Result<Option<std::process::ExitStatus>> {
        let start = std::time::Instant::now();
        loop {
            match self.try_wait()? {
                Some(status) => return Ok(Some(status)),
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

#[derive(Serialize, Deserialize, Clone)]
pub struct Client {
    pub id: String,
    pub name: String,
    pub mac: String,
    pub ip: String,
    pub master: String,
    pub snapshot: Option<String>,
    pub block_store: Option<String>,
    pub target_iqn: Option<String>,
    pub writeback: Option<String>,
    pub created_at: Option<String>,
    pub last_modified: Option<String>,
    pub block_device: Option<String>,
    pub status: Option<String>,
    pub mode: Option<String>,
}

#[derive(Deserialize)]
pub struct AddClientRequest {
    pub name: String,
    pub mac: String,
    pub ip: String,
    pub master: String,
    pub snapshot: Option<String>,
}

#[derive(Deserialize)]
pub struct ControlRequest {
    pub action: String,
    pub make_super: Option<bool>, // for toggleSuper
}

#[tauri::command]
pub async fn get_clients(client_id: Option<String>) -> Result<serde_json::Value, String> {
    let mut config: Config = read_config();

    for client in config.clients.iter_mut() {
        client.status = Some(get_client_status(&client.ip));
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
fn get_client_status(ip: &str) -> String {
    if ip.is_empty() || ip == "N/A" {
        return "Unknown".to_string();
    }
    let output = std::process::Command::new("ping")
        .args(["-c", "1", "-W", "1", ip])
        .output();
    match output {
        Ok(out) if out.status.success() => "Online".to_string(),
        _ => "Offline".to_string(),
    }
}

fn get_client_by_id(client_id: &str) -> Option<Client> {
    let config = read_config();
    for c in config.clients {
        if c.id.eq_ignore_ascii_case(client_id) {
            return Some(c);
        }
    }
    None
}

fn check_duplicate_client(name: &str, mac: &str, ip: &str) -> Option<String> {
    let config: Value = match serde_json::to_value(read_config()) {
        Ok(cfg) => cfg,
        Err(e) => {
            println!("Error parsing config file: {}", e);
            return Some("Error checking for existing clients".to_string());
        }
    };
    let clients = config.get("clients").and_then(|v| v.as_array());
    if clients.is_none() {
        return None;
    }
    for client in clients.unwrap() {
        let client_name = client
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        let client_ip = client.get("ip").and_then(|v| v.as_str()).unwrap_or("");
        let client_mac = client
            .get("mac")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_uppercase();
        if name.to_lowercase() == client_name {
            return Some(format!("A client with name '{}' already exists", name));
        }
        if ip == client_ip {
            return Some(format!(
                "IP address {} is already in use by client '{}'",
                ip,
                client.get("name").and_then(|v| v.as_str()).unwrap_or("")
            ));
        }
        if mac.to_uppercase() == client_mac {
            return Some(format!(
                "MAC address {} is already in use by client '{}'",
                mac,
                client.get("name").and_then(|v| v.as_str()).unwrap_or("")
            ));
        }
    }
    None
}

pub fn get_client_paths(client_id: &str) -> HashMap<String, String> {
    let clone = format!("{}/{}-disk", crate::ZFS_POOL, client_id.to_uppercase());
    let target_iqn = format!(
        "iqn.2025-04.com.nsboot:{}",
        client_id.to_lowercase().replace('_', "")
    );
    let block_store = format!("block_{}", client_id.to_lowercase());
    let mut map = HashMap::new();
    map.insert("clone".to_string(), clone);
    map.insert("target_iqn".to_string(), target_iqn);
    map.insert("block_store".to_string(), block_store);
    map
}

pub fn save_client_config(client_data: &Client) -> bool {
    let mut config: Value = match serde_json::to_value(read_config()) {
        Ok(val) => val,
        Err(_) => json!({
            "clients": [],
            "masters": {},
            "services": {},
            "settings": {}
        }),
    };

    // Ensure all required fields exist
    if !config.is_object() {
        config = json!({
            "clients": [],
            "masters": {},
            "services": {},
            "settings": {}
        });
    }
    if !config.get("clients").is_some() {
        config["clients"] = json!([]);
    }
    if !config.get("masters").is_some() {
        config["masters"] = json!({});
    }
    if !config.get("services").is_some() {
        config["services"] = json!({});
    }
    if !config.get("settings").is_some() {
        config["settings"] = json!({});
    }

    let clients = config.get_mut("clients").and_then(|v| v.as_array_mut());
    let mut updated = false;

    if let Some(clients_arr) = clients {
        for c in clients_arr.iter_mut() {
            if c.get("id") == Some(&json!(client_data.id)) {
                *c = serde_json::to_value(client_data).unwrap();
                updated = true;
                break;
            }
        }
        if !updated {
            clients_arr.push(serde_json::to_value(client_data).unwrap());
        }
    } else {
        config["clients"] = json!([client_data]);
    }

    // Convert serde_json::Value back to Config before writing
    let config_struct: Config = match serde_json::from_value(config) {
        Ok(cfg) => cfg,
        Err(e) => {
            println!("Error converting config to struct: {}", e);
            return false;
        }
    };
    match write_config(&config_struct) {
        Ok(_) => true,
        Err(e) => {
            println!("Error saving client config: {}", e);
            false
        }
    }
}

#[tauri::command]
pub async fn remote_client(client_id: String) -> Result<serde_json::Value, String> {
    print!("Remote client: {}", client_id);
    let client = get_client_by_id(&client_id).ok_or_else(|| "Client not found".to_string())?;

    let client_ip = client.ip.clone();
    if client_ip.is_empty() {
        return Err("Client IP not found".to_string());
    }

    // 2. Check if client is online
    let status = get_client_status(&client_ip);
    if status != "Online" {
        return Err("Client is not online".to_string());
    }

    // 3. Launch remote desktop (xfreerdp)
    match launch_remote_desktop(&client_ip, "diskless") {
        Ok(_) => Ok(serde_json::json!({
            "message": format!("Remote desktop connection initiated to {}", client_id),
            "ip": client_ip
        })),
        Err(e) => Err(format!("Failed to launch remote desktop: {}", e)),
    }
}

// Helper: Launch xfreerdp with fallback
fn launch_remote_desktop(client_ip: &str, username: &str) -> Result<(), String> {
    let rdp_command = [
        "xfreerdp",
        &format!("/v:{}", client_ip),
        &format!("/u:{}", username),
        "/p:1",
        "/cert-ignore",
        "/w:1920",
        "/h:1080",
        "/dynamic-resolution",
        "+clipboard",
        "/gdi:sw",
        "/network:auto",
        "/bpp:32",
        "/sec:nla",
        "/timeout:20000",
    ];

    let mut child = Command::new(rdp_command[0])
        .args(&rdp_command[1..])
        .spawn()
        .map_err(|e| format!("Failed to launch xfreerdp: {}", e))?;

    // Wait briefly to check for immediate failures
    let result = child.wait_timeout(Duration::from_secs(5)).unwrap_or(None);

    if let Some(status) = result {
        if !status.success() {
            // Try fallback
            let fallback_command = [
                "xfreerdp",
                &format!("/v:{}", client_ip),
                &format!("/u:{}", username),
                "/p:1",
                "/cert-ignore",
                "/w:1366",
                "/h:768",
                "/dynamic-resolution",
                "+clipboard",
                "/gdi:sw",
                "/network:auto",
                "/bpp:24",
                "/sec:nla",
                "/timeout:20000",
            ];
            let mut fallback_child = Command::new(fallback_command[0])
                .args(&fallback_command[1..])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("Fallback xfreerdp failed: {}", e))?;

            let fallback_result = fallback_child
                .wait_timeout(Duration::from_secs(5))
                .unwrap_or(None);

            if let Some(fallback_status) = fallback_result {
                if !fallback_status.success() {
                    return Err("Both RDP attempts failed".to_string());
                }
            }
        }
    }
    // If process didn't exit immediately, assume success
    Ok(())
}

pub fn delete_client_config(client_id: &str) -> bool {
    println!("Deleting client config: {}", client_id);
    let mut config: Value = match serde_json::to_value(read_config()) {
        Ok(cfg) => cfg,
        Err(e) => {
            println!("Error serializing config: {}", e);
            return false;
        }
    };
    let clients = config.get_mut("clients").and_then(|v| v.as_array_mut());
    if clients.is_none() {
        return true;
    }
    let client_id_lower = client_id.to_lowercase();
    let new_clients: Vec<Value> = clients
        .unwrap()
        .drain(..)
        .filter(|c| {
            c.get("id")
                .and_then(|id| id.as_str())
                .map(|id| id.to_lowercase())
                != Some(client_id_lower.clone())
        })
        .collect();
    config["clients"] = Value::Array(new_clients);
    // Convert serde_json::Value back to Config before writing
    let config_struct: Config = match serde_json::from_value(config) {
        Ok(cfg) => cfg,
        Err(e) => {
            println!("Error converting config to struct: {}", e);
            return false;
        }
    };
    match write_config(&config_struct) {
        Ok(_) => true,
        Err(e) => {
            println!("Error writing config file: {}", e);
            false
        }
    }
}

#[tauri::command]
pub async fn add_client(req: AddClientRequest) -> Result<serde_json::Value, String> {
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
        return Err("Missing required fields: name, mac, ip".to_string());
    }
    if master.is_empty() {
        return Err("Master image is required".to_string());
    }

    // Check for duplicates (implement as needed)
    if let Some(dup) = check_duplicate_client(&name, &mac, &ip) {
        return Err(dup);
    }

    // Get client paths (implement as needed)
    let mut paths = get_client_paths(&name);

    // Create ZFS clone
    if !snapshot.is_empty() {
        // Use provided snapshot
        run_command(&["zfs", "clone", &snapshot, &paths["clone"]])?;
    } else {
        // Check if base snapshot exists
        let base_snapshot = format!("{}@base", master);
        let result = run_command_check(&["zfs", "list", "-H", "-t", "snapshot", &base_snapshot]);
        if result == 0 {
            // Create new snapshot for this client
            let snapshot_name = format!("{}@{}_base", master, name);
            run_command(&["zfs", "snapshot", &snapshot_name])?;
            run_command(&["zfs", "clone", &snapshot_name, &paths["clone"]])?;
        } else {
            // Use master volume directly
            paths.insert("clone".to_string(), master.clone());
        }
    }

    // Set up iSCSI target (implement as needed)
    setup_iscsi_target(
        &paths["target_iqn"],
        &paths["block_store"],
        &format!("/dev/zvol/{}", &paths["clone"]),
    )?;

    // Create DHCP entry (implement as needed)
    let dhcp_entry = create_dhcp_entry(&name, &mac, &ip, &paths["target_iqn"]);
    update_dhcp_config(&name, &dhcp_entry, true)?;

    let block_device = format!("/dev/zvol/{}", &paths["clone"]);

    // Save client configuration to JSON file (implement as needed)
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
        target_iqn: Some(paths["target_iqn"].clone()),
        block_device: Some(block_device.clone()),
        block_store: Some(paths["block_store"].clone()),
        writeback: Some(paths["clone"].clone()),
        created_at: Some(now.clone()),
        last_modified: Some(now.clone()),
        status: None,
        mode: None,
    };
    if !save_client_config(&client_data) {
        println!("Warning: Failed to save client configuration for {}", name);
    }

    // Restart DHCP service
    run_command(&["systemctl", "restart", "isc-dhcp-server.service"])?;

    Ok(serde_json::json!({ "message": format!("Client {} added successfully", name) }))
}

#[tauri::command]
pub async fn edit_client(
    client_id: String,
    data: serde_json::Value,
) -> Result<serde_json::Value, String> {
    // Validate client_id format
    if !regex::Regex::new(r"^[\w-]+$").unwrap().is_match(&client_id) {
        return Err("Invalid client ID".to_string());
    }

    // Get current client info
    let mut client_info = match get_client_by_id(&client_id) {
        Some(info) => info,
        None => return Err(format!("Client {} not found", client_id)),
    };

    // Get current paths
    let current_paths = get_client_paths(&client_id);

    // Extract new client details
    let new_name = data
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(&client_info.name)
        .trim()
        .to_string();
    let new_mac = data
        .get("mac")
        .and_then(|v| v.as_str())
        .unwrap_or(&client_info.mac)
        .trim()
        .to_uppercase();
    let new_ip = data
        .get("ip")
        .and_then(|v| v.as_str())
        .unwrap_or(&client_info.ip)
        .trim()
        .to_string();
    let new_master = data
        .get("master")
        .and_then(|v| v.as_str())
        .unwrap_or(&client_info.master)
        .trim()
        .to_string();
    let new_snapshot = data
        .get("snapshot")
        .and_then(|v| v.as_str())
        .unwrap_or(client_info.snapshot.as_deref().unwrap_or(""))
        .trim()
        .to_string();

    // Validate inputs (implement as needed)
    if new_name.is_empty() || new_mac.is_empty() || new_ip.is_empty() {
        return Err("Missing required fields: name, mac, ip".to_string());
    }
    if new_master.is_empty() {
        return Err("Master image is required".to_string());
    }

    // Detect changes
    let name_changed = new_name != client_info.name;
    let mac_changed = new_mac != client_info.mac;
    let ip_changed = new_ip != client_info.ip;
    let master_changed = new_master != client_info.master;
    let snapshot_changed = new_snapshot != client_info.snapshot.clone().unwrap_or_default();

    // Case 1: Only MAC or IP changed
    if (mac_changed || ip_changed) && !(name_changed || master_changed || snapshot_changed) {
        client_info.mac = new_mac.clone();
        client_info.ip = new_ip.clone();
        client_info.last_modified = Some(Local::now().format("%Y-%m-%d %H:%M:%S").to_string());

        // Update DHCP config
        let dhcp_entry = create_dhcp_entry(
            &client_info.name,
            &new_mac,
            &new_ip,
            client_info.target_iqn.as_deref().unwrap_or(""),
        );
        update_dhcp_config(&client_id, &dhcp_entry, false)?;

        // Save updated config
        save_client_config(&client_info);
        return Ok(
            serde_json::json!({"message": format!("Successfully updated client {}", client_id)}),
        );
    }

    // Case 2: Name, master, or snapshot changed
    if name_changed || master_changed || snapshot_changed {
        let new_target_iqn = format!("iqn.2025-04.com.nsboot:{}", new_name.to_lowercase());
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

        if !current_master.is_empty() {
            if !current_snapshot.is_empty() {
                // Create new clone from snapshot
                let old_clone = current_paths.get("clone").cloned().unwrap_or_default();
                if !old_clone.is_empty() && zfs_exists(&old_clone) {
                    // Update iSCSI target
                    let old_target_iqn = current_paths.get("target_iqn").cloned();
                    let old_block_store = current_paths.get("block_store").cloned();
                    if old_target_iqn.is_some() || old_block_store.is_some() {
                        cleanup_iscsi_target(
                            old_target_iqn.as_deref().unwrap_or(""),
                            old_block_store.as_deref().unwrap_or(""),
                        )?;
                    }
                    zfs_destroy(&old_clone)?;
                }
                let new_clone = format!("{}/{}-disk", ZFS_POOL, new_name);
                zfs_clone(current_snapshot, &new_clone)?;
                block_device = format!("/dev/zvol/{}", new_clone);
            } else {
                // Use master directly
                block_device = format!("/dev/zvol/{}", current_master);
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
        client_info.writeback = Some(format!("{}/{}-disk", ZFS_POOL, new_name));
        client_info.last_modified = Some(Local::now().format("%Y-%m-%d %H:%M:%S").to_string());

        // Update DHCP config
        let dhcp_entry = create_dhcp_entry(&new_name, &new_mac, &new_ip, &new_target_iqn);
        update_dhcp_config(&client_id, &dhcp_entry, false)?;

        setup_iscsi_target(&new_target_iqn, &new_block_store, &block_device)?;

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
pub async fn delete_client(client_id: String) -> Result<serde_json::Value, String> {
    let re = regex::Regex::new(r"^[\w-]+$").unwrap();
    if !re.is_match(&client_id) {
        return Err("Invalid client ID".to_string());
    }

    let mut errors = Vec::new();
    let paths = get_client_paths(&client_id);

    // Clean up DHCP configuration
    if let Err(e) = update_dhcp_config(&client_id, "", false)
        .and_then(|_| run_command(&["systemctl", "restart", "isc-dhcp-server.service"]))
    {
        errors.push(format!("Failed to clean up DHCP config: {}", e));
    }

    // Clean up iSCSI target
    if let Err(e) = cleanup_iscsi_target(&paths["target_iqn"], &paths["block_store"]) {
        errors.push(format!("Failed to clean up iSCSI target: {}", e));
    }

    // Clean up ZFS clone
    match run_command_check(&["zfs", "list", "-H", &paths["clone"]]) {
        0 => {
            if let Err(e) = run_command(&["zfs", "destroy", &paths["clone"]]) {
                errors.push(format!("Failed to destroy ZFS clone: {}", e));
            }
        }
        _ => {} // ZFS clone does not exist, nothing to do
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
    client_id: String,
    req: ControlRequest,
) -> Result<serde_json::Value, String> {
    let client =
        get_client_by_id(&client_id).ok_or_else(|| format!("Client {} not found", client_id))?;

    let mac = client.mac.clone();
    let ip = client.ip.clone();
    let name = client.name.clone();

    match req.action.as_str() {
        "wake" => {
            if mac.is_empty() {
                return Err(format!("MAC address not found for '{}'", name));
            }
            let output = Command::new("wakeonlan")
                .arg(&mac)
                .output()
                .map_err(|e| e.to_string())?;
            if !output.status.success() {
                return Err(format!(
                    "Wake-on-LAN failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            Ok(
                serde_json::json!({ "message": format!("Wake-on-LAN command sent to {} ({})", name, ip) }),
            )
        }
        "reboot" => {
            if ip.is_empty() {
                return Err(format!("IP address not found for '{}'", client_id));
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
                .map_err(|e| e.to_string())?;
            if !output.status.success() {
                return Err(format!(
                    "Failed to reboot client: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            Ok(
                serde_json::json!({ "message": format!("Reboot command sent to {} ({})", name, ip) }),
            )
        }
        "shutdown" => {
            if ip.is_empty() {
                return Err(format!("IP address not found for '{}'", client_id));
            }
            let output = Command::new("net")
                .args(["rpc", "shutdown", "-S", &ip, "-U", "diskless%1"])
                .output()
                .map_err(|e| e.to_string())?;
            if !output.status.success() {
                return Err(format!(
                    "Failed to shutdown client: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            Ok(
                serde_json::json!({ "message": format!("Shutdown command sent to {} ({})", name, ip) }),
            )
        }
        "toggleSuper" => {
            // Implement ZFS promote/clone logic here, using req.make_super
            // Example stub:
            let is_super = req.make_super.unwrap_or(false);
            if is_super {
                // Promote logic
                Ok(
                    serde_json::json!({ "message": format!("Super Client enabled for {}", client_id) }),
                )
            } else {
                // Demote logic
                Ok(
                    serde_json::json!({ "message": format!("Super Client disabled for {}", client_id) }),
                )
            }
        }
        "edit" => Ok(
            serde_json::json!({ "message": format!("Placeholder: Edit Client {} not implemented.", client_id) }),
        ),
        _ => Err(format!("Invalid action: {}", req.action)),
    }
}

#[tauri::command]
pub async fn reset_client(client_id: String) -> Result<serde_json::Value, String> {
    // Validate client ID
    let re = regex::Regex::new(r"^[\w-]+$").unwrap();
    if !re.is_match(&client_id) {
        return Err("Invalid client ID".to_string());
    }

    // Fetch client info
    let client_info = match get_client_by_id(&client_id) {
        Some(info) => info,
        None => return Err(format!("Client {} not found", client_id)),
    };

    // Get paths for the client
    let current_paths = get_client_paths(&client_id);
    let target_iqn = current_paths.get("target_iqn").cloned().unwrap_or_default();
    let block_store = current_paths
        .get("block_store")
        .cloned()
        .unwrap_or_default();
    let clone = format!("{}/{}-disk", ZFS_POOL, client_id.to_uppercase());

    // 1. Clean up existing iSCSI resources
    if let Err(e) = cleanup_iscsi_target(&target_iqn, &block_store) {
        println!("Warning: Failed to clean up iSCSI target: {}", e);
    }

    // 2. Destroy existing ZFS clone if it exists
    if zfs_exists(&clone) {
        if let Err(e) = zfs_destroy(&clone) {
            return Err(format!("Failed to destroy existing ZFS clone: {}", e));
        }
    }

    // 3. Create new clone from snapshot
    let snapshot = match &client_info.snapshot {
        Some(s) if !s.is_empty() => s,
        _ => return Err("No snapshot found for client".to_string()),
    };

    if let Err(e) = zfs_clone(snapshot, &clone) {
        return Err(format!("Failed to create ZFS clone: {}", e));
    }

    // 4. Setup new iSCSI target
    let block_device = format!("/dev/zvol/{}", clone);
    if let Err(e) = setup_iscsi_target(&target_iqn, &block_store, &block_device) {
        return Err(format!("Failed to set up iSCSI target: {}", e));
    }

    Ok(serde_json::json!({
        "message": format!("Client {} reset successfully", client_id.to_uppercase())
    }))
}
