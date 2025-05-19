use serde::{Deserialize, Serialize};
use std::fs;
use std::process::{Command, Stdio};
use std::time::Duration;
use std::thread;
use std::collections::HashMap;


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

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    pub clients: Vec<Client>,
    pub masters: serde_json::Value,
    pub settings: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Snapshot {
    pub name: String,
    pub created: String,
    pub used: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Master {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub snapshots: Vec<Snapshot>,
}

#[derive(Deserialize)]
pub struct ControlRequest {
    pub action: String,
    pub make_super: Option<bool>, // for toggleSuper
}

#[derive(Deserialize)]
pub struct ServiceControlRequest {
    pub action: String,
}


#[tauri::command]
async fn get_masters(zfs_pool: String) -> Result<Vec<Master>, String> {
    // 1. Get default master from config
    let config: serde_json::Value = fs::read_to_string("config.json")
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let default_master = config
        .get("settings")
        .and_then(|s| s.get("default_master"))
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();

    // 2. List all datasets
    let output = Command::new("sudo")
        .args([
            "zfs",
            "list",
            "-H",
            "-t",
            "filesystem,volume",
            "-o",
            "name,creation,used",
            "-r",
            &zfs_pool,
        ])
        .output()
        .map_err(|e| format!("Failed to run zfs list: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    let all_datasets = parse_zfs_list(&String::from_utf8_lossy(&output.stdout));

    // 3. Find master datasets and master snapshots
    let mut master_names = vec![];
    for ds in &all_datasets {
        if ds.name.to_lowercase().ends_with("-master") {
            master_names.push(ds.name.clone());
            continue;
        }
        // Check snapshots of this dataset
        let snap_out = Command::new("sudo")
            .args([
                "zfs",
                "list",
                "-H",
                "-t",
                "snapshot",
                "-o",
                "name",
                "-r",
                &ds.name,
            ])
            .output();
        if let Ok(snap_out) = snap_out {
            if snap_out.status.success() {
                for snap in String::from_utf8_lossy(&snap_out.stdout).lines() {
                    if snap.to_lowercase().ends_with("-master") {
                        master_names.push(snap.to_string());
                    }
                }
            }
        }
    }
    master_names.sort();
    master_names.dedup();

    // 4. For each master, get its snapshots
    let mut masters_data = vec![];
    for master_name in &master_names {
        let mut snapshots = vec![];
        let snap_out = Command::new("sudo")
            .args([
                "zfs",
                "list",
                "-H",
                "-t",
                "snapshot",
                "-o",
                "name,creation,used",
                "-r",
                master_name,
            ])
            .output();
        if let Ok(snap_out) = snap_out {
            if snap_out.status.success() {
                snapshots = parse_zfs_list(&String::from_utf8_lossy(&snap_out.stdout));
            }
        }
        masters_data.push(Master {
            id: master_name.clone(),
            name: master_name.clone(),
            is_default: master_name == &default_master,
            snapshots,
        });
    }
    Ok(masters_data)
}

// Helper: Parse output of 'zfs list -H -o name,creation,used'
fn parse_zfs_list(output: &str) -> Vec<Snapshot> {
    output
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() == 3 {
                Some(Snapshot {
                    name: parts[0].to_string(),
                    created: parts[1].to_string(),
                    used: parts[2].to_string(),
                })
            } else {
                None
            }
        })
        .collect()
}

#[tauri::command]
async fn get_services(zfs_pool: String) -> Result<serde_json::Value, String> {
    let mut statuses = HashMap::new();

    // Systemd services
    let service_map = vec![
        ("iscsi", "target.service"),
        ("dhcp", "isc-dhcp-server.service"),
        ("tftp", "tftpd-hpa.service"),
    ];

    for (key, service_name) in service_map {
        let output = std::process::Command::new("systemctl")
            .args(["is-active", service_name])
            .output()
            .map_err(|e| e.to_string())?;
        let status = if output.status.success() {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        } else {
            "inactive".to_string()
        };
        statuses.insert(key, serde_json::json!({
            "name": service_name.trim_end_matches(".service"),
            "status": status
        }));
    }

    // ZFS pool health check
    let zfs_status = match std::process::Command::new("zpool")
        .args(["status", &zfs_pool])
        .output()
    {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let pool_state = stdout
                .lines()
                .find_map(|line| {
                    if line.trim_start().starts_with("state:") {
                        Some(line.split(':').nth(1).unwrap_or("").trim())
                    } else {
                        None
                    }
                })
                .unwrap_or("unknown");
            let status = if pool_state == "ONLINE" { "active" } else { "degraded" };
            status.to_string()
        }
        Ok(_) => "error".to_string(),
        Err(_) => "error".to_string(),
    };
    statuses.insert(
        "zfs",
        serde_json::json!({
            "name": format!("ZFS Pool ({})", zfs_pool),
            "status": zfs_status
        }),
    );

    Ok(serde_json::to_value(statuses).unwrap())
}

#[tauri::command]
async fn get_clients(client_id: Option<String>) -> Result<serde_json::Value, String> {
    let data = fs::read_to_string("config.json").map_err(|e| e.to_string())?;
    let mut config: Config = serde_json::from_str(&data).map_err(|e| e.to_string())?;

    // Add status to each client
    for client in config.clients.iter_mut() {
        client.status = Some(get_client_status(&client.ip));
    }

    if let Some(id) = client_id {
        let client = config.clients.iter().find(|c| c.id.eq_ignore_ascii_case(&id));
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

#[tauri::command]
async fn control_client(client_id: String, req: ControlRequest) -> Result<serde_json::Value, String> {
    // Load client info (implement get_client_by_id as needed)
    let client = get_client_by_id(&client_id)
        .ok_or_else(|| format!("Client {} not found", client_id))?;

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
            Ok(serde_json::json!({ "message": format!("Wake-on-LAN command sent to {} ({})", name, ip) }))            
        }
        "reboot" => {
            if ip.is_empty() {
                return Err(format!("IP address not found for '{}'", client_id));
            }
            let output = Command::new("net")
                .args([
                    "rpc", "shutdown", "-r", "-I", &ip, "-U", "diskless%1", "-f", "-t", "0"
                ])
                .output()
                .map_err(|e| e.to_string())?;
            if !output.status.success() {
                return Err(format!(
                    "Failed to reboot client: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            Ok(serde_json::json!({ "message": format!("Reboot command sent to {} ({})", name, ip) }))
        }
        "shutdown" => {
            if ip.is_empty() {
                return Err(format!("IP address not found for '{}'", client_id));
            }
            let output = Command::new("net")
                .args([
                    "rpc", "shutdown", "-S", &ip, "-U", "diskless%1"
                ])
                .output()
                .map_err(|e| e.to_string())?;
            if !output.status.success() {
                return Err(format!(
                    "Failed to shutdown client: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            Ok(serde_json::json!({ "message": format!("Shutdown command sent to {} ({})", name, ip) }))
        }
        "toggleSuper" => {
            // Implement ZFS promote/clone logic here, using req.make_super
            // Example stub:
            let is_super = req.make_super.unwrap_or(false);
            if is_super {
                // Promote logic
                Ok(serde_json::json!({ "message": format!("Super Client enabled for {}", client_id) }))
            } else {
                // Demote logic
                Ok(serde_json::json!({ "message": format!("Super Client disabled for {}", client_id) }))
            }
        }
        "edit" => Ok(serde_json::json!({ "message": format!("Placeholder: Edit Client {} not implemented.", client_id) })),
        _ => Err(format!("Invalid action: {}", req.action)),
    }
}

// Helper: Find client by id (implement as needed)
fn get_client_by_id(client_id: &str) -> Option<Client> {
    // Load from config.json or your data source
    // Example:
    let data = std::fs::read_to_string("config.json").ok()?;
    let config: serde_json::Value = serde_json::from_str(&data).ok()?;
    let clients = config.get("clients")?.as_array()?;
    for c in clients {
        if c.get("id")?.as_str()? == client_id {
            return serde_json::from_value(c.clone()).ok();
        }
    }
    None
}


#[tauri::command]
async fn remote_client(client_id: String) -> Result<serde_json::Value, String> {
    print!("Remote client: {}", client_id);
    // 1. Get client info (implement get_client_by_id as needed)
    let client = get_client_by_id(&client_id)
        .ok_or_else(|| "Client not found".to_string())?;

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
    let result = child.wait_timeout(Duration::from_secs(5))
        .unwrap_or(None);

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

            let fallback_result = fallback_child.wait_timeout(Duration::from_secs(5))
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

// Helper: Wait with timeout for child process (add this trait)
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

#[tauri::command]
async fn control_service(service_key: String, req: ServiceControlRequest) -> Result<serde_json::Value, String> {
    // Define your service map (adjust as needed)
    let service_map: HashMap<&str, &str> = [
        ("iscsi", "target.service"),
        ("dhcp", "isc-dhcp-server.service"),
        ("tftp", "tftpd-hpa.service"),
    ].iter().cloned().collect();

    let Some(&service_name) = service_map.get(service_key.as_str()) else {
        return Err(format!("Unknown service key: {}", service_key));
    };

    println!("Received control action '{}' for service: {} ({})", req.action, service_key, service_name);

    match req.action.as_str() {
        "restart" => {
            let output = Command::new("sudo")
                .args(["systemctl", "restart", service_name])
                .output()
                .map_err(|e| format!("Failed to run systemctl: {e}"))?;

            if output.status.success() {
                Ok(serde_json::json!({ "message": format!("Service '{}' restart command issued successfully.", service_name) }))
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(format!("Failed to restart service '{}': {}", service_name, stderr))
            }
        }
        _ => Err(format!("Unsupported action '{}' for service '{}'", req.action, service_key)),
    }
}

#[tauri::command]
async fn get_service_config(service_key: String) -> Result<serde_json::Value, String> {
    // Map service keys to config file paths
    let config_file_map: HashMap<&str, &str> = [
        ("dhcp", "/etc/dhcp/dhcpd.conf"),
        ("tftp", "/etc/default/tftpd-hpa"),
        ("iscsi", "/etc/rtslib-fb-target/saveconfig.json"),
        // Add more as needed
    ].iter().cloned().collect();

    if service_key == "zfs" {
        // Get ZFS pool and dataset info
        let zpool_status = Command::new("sudo")
            .args(["zpool", "status"])
            .output()
            .map_err(|e| format!("Failed to run zpool status: {e}"))?;
        let zpool_status_str = String::from_utf8_lossy(&zpool_status.stdout);

        let zfs_list = Command::new("sudo")
            .args([
                "zfs", "list", "-H", "-t", "all",
                "-o", "name,type,used,avail,refer,mountpoint"
            ])
            .output()
            .map_err(|e| format!("Failed to run zfs list: {e}"))?;
        let zfs_list_str = String::from_utf8_lossy(&zfs_list.stdout);

        let content = format!(
            "=== ZFS Pool Status ===\n{}\n\n=== ZFS Datasets ===\n{}",
            zpool_status_str, zfs_list_str
        );
        Ok(serde_json::json!({ "text": content }))
    } else {
        // Look up config file path
        let config_path = config_file_map
            .get(service_key.as_str())
            .ok_or_else(|| format!("Unknown service key: {}", service_key))?;

        // Check file existence and type
        if !std::path::Path::new(config_path).exists() {
            return Err(format!("Configuration file not found: {}", config_path));
        }
        if !std::path::Path::new(config_path).is_file() {
            return Err(format!("Configuration path is not a file: {}", config_path));
        }

       if service_key == "iscsi" {
            let content = fs::read_to_string(config_path)
                .map_err(|e| format!("Error reading config file {}: {}", config_path, e))?;
            let json_val: serde_json::Value = serde_json::from_str(&content)
                .map_err(|e| format!("Error parsing JSON in {}: {}", config_path, e))?;
            Ok(json_val)
        } else {
            let content = fs::read_to_string(config_path)
                .map_err(|e| format!("Error reading config file {}: {}", config_path, e))?;
            Ok(serde_json::json!({ "text": content }))
        }
    }
}

#[tauri::command]
async fn get_zpool_stats() -> Result<serde_json::Value, String> {
    let output = std::process::Command::new("zpool")
        .args(["list", "-H", "-o", "name,size,alloc,free"])
        .output()
        .map_err(|e| format!("Failed to run zpool list: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    let line = stdout.lines().next().ok_or("No zpool found")?;
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 4 {
        return Err("Unexpected zpool output".to_string());
    }
    Ok(serde_json::json!({
        "name": parts[0],
        "size": parts[1],
        "used": parts[2],
        "available": parts[3],
    }))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
          get_clients, 
          get_services,
          get_masters,
          control_client,
          remote_client,
          control_service,
          get_zpool_stats,
          get_service_config
          ])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}