// Client management logic: helpers for client lookup, config, and deduplication.

use crate::{
    client, config::{get_config, read_config, write_config, Config}, dhcp::{create_dhcp_entry, update_dhcp_config}, iscsi::{cleanup_iscsi_target, setup_iscsi_target}, utils::{run_command, run_command_check}, zfs::{zfs_clone, zfs_destroy, zfs_exists}, ZFS_POOL
};

const IQN_BASE: &str = "iqn.2025-04.com.nsboot";
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

#[derive(Serialize, Deserialize, Clone, Debug)]
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

#[derive(Deserialize)]
pub struct DeprovisionRequest {
    pub mac: String,
    pub force: Option<bool>,
    pub keep_zfs: Option<bool>,
    pub dry_run: Option<bool>,
}

// #[tauri::command]
// pub async fn get_clients(client_id: Option<String>) -> Result<serde_json::Value, String> {
//     let mut config: Config = get_config();
//     use futures::future::join_all;

//     // Collect (index, mac, ip) tuples
//     let client_tuples: Vec<(usize, String, String)> = config
//         .clients
//         .iter()
//         .enumerate()
//         .map(|(i, c)| (i, c.mac.clone(), c.ip.clone()))
//         .collect();

//     // Spawn concurrent tasks to get status (Online / Leased / Offline)
//     let handles: Vec<_> = client_tuples
//         .into_iter()
//         .map(|(i, mac, ip)| {
//             tokio::task::spawn_blocking(move || (i, get_client_status_realtime(&mac, &ip)))
//         })
//         .collect();

//     // Wait for all tasks to finish
//     let results = join_all(handles).await;

//     // Update statuses in the original clients vector
//     for res in results {
//         if let Ok((i, status)) = res {
//             config.clients[i].status = Some(status);
//         } else if let Err(e) = res {
//             println!("[ERROR] spawn_blocking failed: {}", e);
//         }
//     }

//     // Update the config cache with new statuses
//     crate::config::set_config(&config);

//     // Discover dynamically provisioned clients (from DHCP leases) not yet in config
//     let mut combined_clients = config.clients.clone();
//     let existing_macs: std::collections::HashSet<String> = combined_clients
//         .iter()
//         .map(|c| c.mac.to_lowercase())
//         .collect();
//     let discovered = discover_dynamic_clients();
//     let mut new_clients_to_persist: Vec<Client> = Vec::new();
//     for mut c in discovered {
//         if !existing_macs.contains(&c.mac.to_lowercase()) {
//             // compute status for the discovered client
//             let status = get_client_status_realtime(&c.mac, &c.ip);
//             c.status = Some(status);
//             // Queue for persistence with transient status cleared (status is computed on read)
//             let mut to_save = c.clone();
//             to_save.status = None;
//             new_clients_to_persist.push(to_save);
//             combined_clients.push(c);
//         }
//     }

//     // Persist any newly discovered clients into config.json so they become managed entries
//     if !new_clients_to_persist.is_empty() {
//         let mut cfg_to_write = get_config();
//         // Deduplicate against MAC again in case of races
//         let cfg_macs: std::collections::HashSet<String> = cfg_to_write
//             .clients
//             .iter()
//             .map(|c| c.mac.to_lowercase())
//             .collect();
//         for client in new_clients_to_persist.into_iter() {
//             if !cfg_macs.contains(&client.mac.to_lowercase()) {
//                 cfg_to_write.clients.push(client);
//             }
//         }
//         // Best effort write; failures are logged
//         if let Err(e) = write_config(&cfg_to_write) {
//             println!("[WARN] Failed to persist discovered clients: {}", e);
//         } else {
//             crate::config::set_config(&cfg_to_write);
//         }
//     }

//     if let Some(id) = client_id {
//         let client = combined_clients
//             .iter()
//             .find(|c| c.id.eq_ignore_ascii_case(&id));
//         Ok(serde_json::json!(client))
//     } else {
//         Ok(serde_json::json!(combined_clients))
//     }
// }

#[tauri::command]
pub async fn get_clients(token: String, client_id: Option<String>) -> Result<serde_json::Value, String> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;
    
    let mut config: Config = read_config();

    for client in config.clients.iter_mut() {
        client.status = Some(get_client_status_realtime(&client.mac,&client.ip));
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
fn get_client_status_realtime(mac: &str, ip: &str) -> String {
    let mac_norm = mac.to_lowercase();
    // let has_lease = has_active_dhcp_lease(&mac_norm, if ip.is_empty() { None } else { Some(ip) });

    // Consider ping reachability as Online
    let online = if ip.is_empty() || ip == "N/A" {
        false
    } else {
        match std::process::Command::new("ping").args(["-c", "1", "-W", "1", ip]).output() {
            Ok(out) => out.status.success(),
            Err(_) => false,
        }
    };

    if online {
        "Online".to_string()
    // } else if has_lease {
    //     // Lease present, but ping not responding yet
    //     "Leased".to_string()
    } else {
        "Offline".to_string()
    }
}

// fn has_active_dhcp_lease(mac_lower: &str, ip_opt: Option<&str>) -> bool {
//     use std::fs;
//     let leases_path = "/var/lib/dhcp/dhcpd.leases";
//     if let Ok(content) = fs::read_to_string(leases_path) {
//         // Very light parsing: look for a block that contains either the MAC or the IP with active state
//         // Split into simple blocks by 'lease ' occurrences
//         for block in content.split("lease ") {
//             let block_lc = block.to_lowercase();
//             if block_lc.contains(mac_lower) || ip_opt.map(|ip| block_lc.contains(ip)).unwrap_or(false) {
//                 if block_lc.contains("binding state active") {
//                     return true;
//                 }
//             }
//         }
//     } else {
//         // Fallback to dhcp-lease-list if available
//         if let Ok(output) = std::process::Command::new("dhcp-lease-list").output() {
//             let out = String::from_utf8_lossy(&output.stdout).to_lowercase();
//             if out.contains(mac_lower) {
//                 return true;
//             }
//         }
//     }
//     false
// }

fn discover_dynamic_clients() -> Vec<Client> {
    use std::collections::HashMap;
    use std::fs;
    let leases_path = "/var/lib/dhcp/dhcpd.leases";
    let mut mac_to_client: HashMap<String, Client> = HashMap::new();
    if let Ok(content) = fs::read_to_string(leases_path) {
        // Parse by blocks; last active block for a MAC wins
        for raw_block in content.split("lease ") {
            let block = raw_block.trim();
            if block.is_empty() { continue; }
            // first token is IP until space or '{'
            let ip = block
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_matches(|c: char| c == '{' || c.is_whitespace())
                .trim_end_matches(';')
                .to_string();
            let block_lc = block.to_lowercase();
            if !block_lc.contains("binding state active") { continue; }
            // extract mac
            let mac_lc = block_lc
                .lines()
                .find_map(|l| {
                    if l.contains("hardware ethernet") {
                        l.split("hardware ethernet")
                            .nth(1)
                            .map(|s| s.trim().trim_end_matches(';').to_string())
                    } else { None }
                })
                .unwrap_or_default();
            if mac_lc.is_empty() || ip.is_empty() { continue; }
            // extract hostname if any
            let hostname = block
                .lines()
                .find_map(|l| {
                    if l.contains("client-hostname") {
                        l.split('"').nth(1).map(|s| s.to_string())
                    } else { None }
                })
                .unwrap_or_else(|| mac_lc.replace(':', "").to_lowercase());
            // Build/overwrite ephemeral client for this MAC
            let name_upper = hostname.to_uppercase();
            let mac_upper = mac_lc.to_uppercase();
            mac_to_client.insert(
                mac_lc,
                Client {
                    // Use MAC as stable ID to avoid duplicate rows by hostname differences
                    id: mac_upper.to_lowercase(),
                    name: name_upper,
                    mac: mac_upper,
                    ip,
                    master: String::from(""),
                    snapshot: None,
                    block_store: None,
                    target_iqn: None,
                    writeback: None,
                    created_at: None,
                    last_modified: None,
                    block_device: None,
                    status: Some(String::from("Leased")),
                    mode: None,
                },
            );
        }
    }
    mac_to_client.into_values().collect()
}

fn get_client_by_id(client_id: &str) -> Option<Client> {
    let config = get_config();
    for c in config.clients {
        if c.id.eq_ignore_ascii_case(client_id) {
            return Some(c);
        }
    }
    None
}

fn check_duplicate_client(name: &str, mac: &str, ip: &str) -> Option<String> {
    let config: Value = match serde_json::to_value(get_config()) {
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

pub fn get_client_paths(client_id: &str, client_mac: &str) -> HashMap<String, String> {
    let clone = format!("{}/{}-disk", crate::ZFS_POOL, client_id.to_uppercase());
    let target_iqn = format!(
        "iqn.2025-04.com.nsboot:{}",
        client_mac.to_lowercase().replace(':', "-")
    );
    let block_store = format!("block_{}", client_id.to_lowercase());
    let mut map = HashMap::new();
    map.insert("clone".to_string(), clone);
    map.insert("target_iqn".to_string(), target_iqn);
    map.insert("block_store".to_string(), block_store);
    map
}

pub fn get_client_paths_with_master(client_id: &str, client_mac: &str, master: &str) -> HashMap<String, String> {
    let target_iqn = format!(
        "iqn.2025-04.com.nsboot:{}",
        client_mac.to_lowercase().replace(':', "-")
    );
    let block_store = format!("block_{}", client_id.to_lowercase());
    let mut map = HashMap::new();
    map.insert("target_iqn".to_string(), target_iqn);
    map.insert("block_store".to_string(), block_store);
    
    // Check if master is a fileIO image
    if master.contains("/var/lib/diskless/fileio/") && master.ends_with(".img") {
        // For fileIO images, create a copy in the same directory
        let fileio_dir = "/var/lib/diskless/fileio";
        let client_image = format!("{}/{}-{}-client.img", fileio_dir, client_id.to_lowercase(), client_mac.to_lowercase().replace(':', "-"));
        map.insert("clone".to_string(), client_image);
        map.insert("is_fileio".to_string(), "true".to_string());
    } else {
        // For ZFS images, create a clone
        let clone = format!("{}/{}-disk", crate::ZFS_POOL, client_id.to_uppercase());
        map.insert("clone".to_string(), clone);
        map.insert("is_fileio".to_string(), "false".to_string());
    }
    
    map
}

pub fn save_client_config(client_data: &Client) -> bool {
    let mut config: Value = match serde_json::to_value(get_config()) {
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
pub async fn remote_client(token: String, client_id: String) -> Result<serde_json::Value, String> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;
    print!("Remote client: {}", client_id);
    let client = get_client_by_id(&client_id).ok_or_else(|| "Client not found".to_string())?;

    let client_ip = client.ip.clone();
    if client_ip.is_empty() {
        return Err("Client IP not found".to_string());
    }

    // 2. Check if client is online
    let status = get_client_status_realtime(&client.mac, &client_ip);
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
    let mut config: Value = match serde_json::to_value(get_config()) {
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
pub async fn add_client(token: String, req: AddClientRequest) -> Result<serde_json::Value, String> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;
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

    // Get client paths based on master type
    let mut paths = get_client_paths_with_master(&name, &mac, &master);
    let is_fileio = paths.get("is_fileio").map(|s| s == "true").unwrap_or(false);

    // Create client image (ZFS clone or fileIO copy)
    if is_fileio {
        // For fileIO images, copy the master file
        if !snapshot.is_empty() {
            return Err("Snapshots are not supported for fileIO images".to_string());
        }
        copy_fileio_image(&master, &paths["clone"])?;
    } else {
        // For ZFS images, create clone
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
    }

    // Set up iSCSI target
    let block_device = if is_fileio {
        paths["clone"].clone()
    } else {
        format!("/dev/zvol/{}", &paths["clone"])
    };
    
    setup_iscsi_target(
        &paths["target_iqn"],
        &paths["block_store"],
        &block_device,
    )?;

    // Create DHCP entry (implement as needed)
    let dhcp_entry = create_dhcp_entry(&name, &mac, &ip, &paths["target_iqn"]);
    update_dhcp_config(&name, &dhcp_entry, true)?;

    let block_device = if is_fileio {
        paths["clone"].clone()
    } else {
        format!("/dev/zvol/{}", &paths["clone"])
    };

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
    token: String,
    client_id: String,
    data: serde_json::Value,
) -> Result<serde_json::Value, String> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;
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
    let current_paths = get_client_paths(&client_id, &client_info.mac);
    let current_is_fileio = client_info.master.contains("/var/lib/diskless/fileio/") && client_info.master.ends_with(".img");

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

    // Case 2: Name, master, or snapshot changed, or snapshot is set to use master directly (empty)
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
        let new_is_fileio = current_master.contains("/var/lib/diskless/fileio/") && current_master.ends_with(".img");

        if !current_master.is_empty() {            
            if new_is_fileio {
                // Handle fileIO image
                if !current_snapshot.is_empty() {
                    return Err("Snapshots are not supported for fileIO images".to_string());
                }
                
                // Clean up old client image
                let old_clone = current_paths.get("clone").cloned().unwrap_or_default();
                if !old_clone.is_empty() {
                    // Check if the old client was using fileIO
                    let old_is_fileio = client_info.master.contains("/var/lib/diskless/fileio/") && client_info.master.ends_with(".img");
                    
                  
                    
                    // Clean up iSCSI target
                    let old_target_iqn = current_paths.get("target_iqn").cloned();
                    let old_block_store = current_paths.get("block_store").cloned();
                    if old_target_iqn.is_some() || old_block_store.is_some() {
                        cleanup_iscsi_target(
                            old_target_iqn.as_deref().unwrap_or(""),
                            old_block_store.as_deref().unwrap_or(""),
                        )?;
                    }

                    if old_is_fileio {
                        // Delete old fileIO image
                        delete_fileio_image(&old_clone)?;
                    } else if zfs_exists(&old_clone) {
                        // Delete old ZFS clone
                        zfs_destroy(&old_clone)?;
                    }
                }
                
                // Create new fileIO client image
                let new_paths = get_client_paths_with_master(&new_name, &new_mac, current_master);
                copy_fileio_image(current_master, &new_paths["clone"])?;
                block_device = new_paths["clone"].clone();
            } else {
                // Handle ZFS image
                if !current_snapshot.is_empty() {
                    // Create new clone from snapshot
                    let old_clone = current_paths.get("clone").cloned().unwrap_or_default();
                    if !old_clone.is_empty() {
                        // Check if the old client was using fileIO
                        let old_is_fileio = client_info.master.contains("/var/lib/diskless/fileio/") && client_info.master.ends_with(".img");
                        
                       
                        
                        // Clean up iSCSI target
                        let old_target_iqn = current_paths.get("target_iqn").cloned();
                        let old_block_store = current_paths.get("block_store").cloned();
                        if old_target_iqn.is_some() || old_block_store.is_some() {
                            cleanup_iscsi_target(
                                old_target_iqn.as_deref().unwrap_or(""),
                                old_block_store.as_deref().unwrap_or(""),
                            )?;
                        }
                        if old_is_fileio {
                            // Delete old fileIO image
                            delete_fileio_image(&old_clone)?;
                        } else if zfs_exists(&old_clone) {
                            // Delete old ZFS clone
                            zfs_destroy(&old_clone)?;
                        }
                    }
                    let new_clone = format!("{}/{}-disk", ZFS_POOL, new_name);
                    zfs_clone(current_snapshot, &new_clone)?;
                    block_device = format!("/dev/zvol/{}", new_clone);
                } else {
                    // Use master directly
                    // Clean up old clone if it exists
                    let old_clone = current_paths.get("clone").cloned().unwrap_or_default();
                    if !old_clone.is_empty() {
                        // Check if the old client was using fileIO
                        let old_is_fileio = client_info.master.contains("/var/lib/diskless/fileio/") && client_info.master.ends_with(".img");
                        
                        if old_is_fileio {
                            // Delete old fileIO image
                            delete_fileio_image(&old_clone)?;
                        } else if zfs_exists(&old_clone) {
                            // Delete old ZFS clone
                            zfs_destroy(&old_clone)?;
                        }
                        
                        // Clean up iSCSI target
                        let old_target_iqn = current_paths.get("target_iqn").cloned();
                        let old_block_store = current_paths.get("block_store").cloned();
                        if old_target_iqn.is_some() || old_block_store.is_some() {
                            cleanup_iscsi_target(
                                old_target_iqn.as_deref().unwrap_or(""),
                                old_block_store.as_deref().unwrap_or(""),
                            )?;
                        }
                    }
                    block_device = format!("/dev/zvol/{}", current_master);
                }
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
        client_info.writeback = if new_is_fileio {
            Some(block_device.clone())
        } else {
            Some(format!("{}/{}-disk", ZFS_POOL, new_name))
        };
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
pub async fn delete_client(token: String, client_id: String) -> Result<serde_json::Value, String> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;
    let re = regex::Regex::new(r"^[\w-]+$").unwrap();
    if !re.is_match(&client_id) {
        return Err("Invalid client ID".to_string());
    }

      // Get current client info
    let mut client_info = match get_client_by_id(&client_id) {
        Some(info) => info,
        None => return Err(format!("Client {} not found", client_id)),
    };

    let mut errors = Vec::new();
    let paths = get_client_paths(&client_id, &client_info.mac);

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

    // Clean up client image (ZFS clone or fileIO copy)
    let is_fileio = client_info.master.contains("/var/lib/diskless/fileio/") && client_info.master.ends_with(".img");
    
    if is_fileio {
        // For fileIO images, delete the client copy
        if let Some(block_device) = &client_info.block_device {
            if std::path::Path::new(block_device).exists() {
                if let Err(e) = delete_fileio_image(block_device) {
                    errors.push(format!("Failed to delete fileIO image: {}", e));
                }
            }
        }
    } else {
        // For ZFS images, destroy the clone
        match run_command_check(&["zfs", "list", "-H", &paths["clone"]]) {
            0 => {
                if let Err(e) = run_command(&["zfs", "destroy", &paths["clone"]]) {
                    errors.push(format!("Failed to destroy ZFS clone: {}", e));
                }
            }
            _ => {} // ZFS clone does not exist, nothing to do
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
) -> Result<serde_json::Value, String> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;
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
pub async fn reset_client(token: String, client_id: String) -> Result<serde_json::Value, String> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;
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
    let current_paths = get_client_paths(&client_id, &client_info.mac);
    let target_iqn = current_paths.get("target_iqn").cloned().unwrap_or_default();
    let block_store = current_paths
        .get("block_store")
        .cloned()
        .unwrap_or_default();
    let clone = format!("{}/{}-disk", ZFS_POOL, client_id.to_uppercase());
    let is_fileio = client_info.master.contains("/var/lib/diskless/fileio/") && client_info.master.ends_with(".img");

    // 1. Clean up existing iSCSI resources
    if let Err(e) = cleanup_iscsi_target(&target_iqn, &block_store) {
        println!("Warning: Failed to clean up iSCSI target: {}", e);
    }

    // 2. Destroy existing client image (ZFS clone or fileIO copy)
    if is_fileio {
        // For fileIO images, delete the client copy
        if std::path::Path::new(&client_info.block_device.as_ref().unwrap_or(&String::new())).exists() {
            if let Err(e) = delete_fileio_image(&client_info.block_device.as_ref().unwrap_or(&String::new())) {
                return Err(format!("Failed to delete existing fileIO image: {}", e));
            }
        }
    } else {
        // For ZFS images, destroy the clone
        if zfs_exists(&clone) {
            if let Err(e) = zfs_destroy(&clone) {
                return Err(format!("Failed to destroy existing ZFS clone: {}", e));
            }
        }
    }

    // 3. Create new client image from master
    if is_fileio {
        // For fileIO images, copy from master
        if let Err(e) = copy_fileio_image(&client_info.master, &client_info.block_device.as_ref().unwrap_or(&String::new())) {
            return Err(format!("Failed to copy fileIO image: {}", e));
        }
    } else {
        // For ZFS images, create clone from snapshot
        let snapshot = match &client_info.snapshot {
            Some(s) if !s.is_empty() => s,
            _ => return Err("No snapshot found for client".to_string()),
        };

        if let Err(e) = zfs_clone(snapshot, &clone) {
            return Err(format!("Failed to create ZFS clone: {}", e));
        }
    }

    // 4. Setup new iSCSI target
    let block_device = if is_fileio {
        client_info.block_device.as_ref().unwrap_or(&String::new()).clone()
    } else {
        format!("/dev/zvol/{}", clone)
    };
    
    if let Err(e) = setup_iscsi_target(&target_iqn, &block_store, &block_device) {
        return Err(format!("Failed to set up iSCSI target: {}", e));
    }

    Ok(serde_json::json!({
        "message": format!("Client {} reset successfully", client_id.to_uppercase())
    }))
}

#[tauri::command]
pub async fn deprovision_client(token: String, req: DeprovisionRequest) -> Result<serde_json::Value, String> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;
    let mac = req.mac;
    let force = req.force.unwrap_or(false);
    let keep_zfs = req.keep_zfs.unwrap_or(false);
    let dry_run = req.dry_run.unwrap_or(false);

    // Validate MAC address format
    let mac_regex = regex::Regex::new(r"^([0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}$").unwrap();
    if !mac_regex.is_match(&mac) {
        return Err("Invalid MAC address format".to_string());
    }

    // Build command arguments
    let mut args = vec!["/usr/local/bin/deprovision_client.sh", &mac];
    
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
    let output = Command::new("sudo")
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to execute deprovision script: {}", e))?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(serde_json::json!({
            "success": true,
            "message": "Client deprovisioned successfully",
            "output": stdout.trim()
        }))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        Err(format!("Deprovisioning failed: {}\n{}", stderr, stdout))
    }
}

#[tauri::command]
pub async fn deprovision_client_by_id(token: String, client_id: String, force: Option<bool>, keep_zfs: Option<bool>) -> Result<serde_json::Value, String> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;
        
    // Get client by ID to extract MAC address
    let client = get_client_by_id(&client_id)
        .ok_or_else(|| format!("Client {} not found", client_id))?;

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
        if json_result.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
            // Remove client from configuration
            if !delete_client_config(&client_id) {
                println!("Warning: Failed to remove client {} from configuration", client_id);
            }
        }
    }

    Ok(result)
}

#[tauri::command]
pub async fn get_deprovision_status(token: String, mac: String) -> Result<serde_json::Value, String> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;
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
        .args(&["list", "-H", "-o", "name", &format!("{}/{}", ZFS_POOL, client_vol)])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    status.insert("zfs_clone_exists".to_string(), serde_json::Value::Bool(zfs_exists));

    // Check iSCSI target (targetcli often requires sudo and may write to stderr)
    let iscsi_exists = {
        // First attempt: sudo targetcli ls
        let out1 = Command::new("sudo").args(["targetcli", "ls"]).output();
        if let Ok(o) = out1 {
            let txt = String::from_utf8_lossy(&o.stdout);
            let err = String::from_utf8_lossy(&o.stderr);
            let combined = format!("{}{}", txt, err);
            if combined.contains(&target_iqn) { true } else {
                // Fallback: sudo targetcli /iscsi ls
                match Command::new("sudo").args(["targetcli", "/iscsi", "ls"]).output() {
                    Ok(o2) => {
                        let txt2 = String::from_utf8_lossy(&o2.stdout);
                        let err2 = String::from_utf8_lossy(&o2.stderr);
                        let combined2 = format!("{}{}", txt2, err2);
                        combined2.contains(&target_iqn)
                    }
                    Err(_) => false,
                }
            }
        } else {
            false
        }
    };
    println!("iscsi_exists: {}", iscsi_exists);
    status.insert("iscsi_target_exists".to_string(), serde_json::Value::Bool(iscsi_exists));

    // Check PXE configuration
    let pxe_exists = std::path::Path::new(&pxe_file).exists();
    status.insert("pxe_config_exists".to_string(), serde_json::Value::Bool(pxe_exists));

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
        .args(&["-A", "10", mac, "/var/lib/dhcp/dhcpd.leases"])
        .output() {
        if let Ok(lease_content) = String::from_utf8(output.stdout) {
            if let Some(ip_line) = lease_content.lines().find(|line| line.contains("lease")) {
                if let Some(ip) = ip_line.split_whitespace().nth(1) {
                    let ip = ip.trim_end_matches(';');
                    if Command::new("ping")
                        .args(&["-c", "1", "-W", "2", ip])
                        .output()
                        .map(|output| output.status.success())
                        .unwrap_or(false) {
                        return true;
                    }
                }
            }
        }
    }

    false
}

pub fn copy_fileio_image(master_path: &str, client_path: &str) -> Result<(), String> {
    // Check if master file exists
    if !std::path::Path::new(master_path).exists() {
        return Err(format!("Master fileIO image not found: {}", master_path));
    }
    
    // Create directory if it doesn't exist
    let client_dir = std::path::Path::new(client_path).parent().unwrap();
    if !client_dir.exists() {
        std::fs::create_dir_all(client_dir)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }
    
    // Copy the file using cp command (preserves sparse file structure)
    let output = Command::new("sudo")
        .args(["cp", "--sparse=always", master_path, client_path])
        .output()
        .map_err(|e| format!("Failed to copy fileIO image: {}", e))?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to copy fileIO image: {}", stderr));
    }
    
    // Set proper permissions
    let chmod_output = Command::new("sudo")
        .args(["chmod", "644", client_path])
        .output()
        .map_err(|e| format!("Failed to set file permissions: {}", e))?;
    
    if !chmod_output.status.success() {
        println!("Warning: Failed to set file permissions for {}", client_path);
    }
    
    Ok(())
}

pub fn delete_fileio_image(image_path: &str) -> Result<(), String> {
    if std::path::Path::new(image_path).exists() {
        let output = Command::new("sudo")
            .args(["rm", "-f", image_path])
            .output()
            .map_err(|e| format!("Failed to delete fileIO image: {}", e))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Failed to delete fileIO image: {}", stderr));
        }
    }
    
    Ok(())
}
