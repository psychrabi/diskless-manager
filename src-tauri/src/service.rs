use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::process::Command;

#[derive(Deserialize)]
pub struct ServiceControlRequest {
    pub action: String,
}

#[tauri::command]
pub async fn get_services(zfs_pool: String) -> Result<Value, String> {
    let mut statuses = HashMap::new();
    print!("Getting services status... \n");
    let service_map = vec![
        ("iscsi", "target.service"),
        ("dhcp", "isc-dhcp-server.service"),
        ("tftp", "tftpd-hpa.service"),
        ("http", "apache2.service"),
        ("share", "smbd.service"),
    ];
    for (key, service_name) in service_map {
        let output = Command::new("systemctl")
            .args(["is-active", service_name])
            .output()
            .map_err(|e| e.to_string())?;
        let status = if output.status.success() {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        } else {
            "inactive".to_string()
        };
        print!("Service {} status: {} \n", service_name, status);
        statuses.insert(
            key,
            json!({
                "name": service_name.trim_end_matches(".service"),
                "status": status
            }),
        );
    }
    let zfs_status = match Command::new("zpool").args(["status", &zfs_pool]).output() {
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
            let status = if pool_state == "ONLINE" {
                "active"
            } else {
                "degraded"
            };
            status.to_string()
        }
        Ok(_) => "error".to_string(),
        Err(_) => "error".to_string(),
    };
    statuses.insert(
        "zfs",
        json!({
            "name": format!("ZFS Pool ({})", zfs_pool),
            "status": zfs_status
        }),
    );
    Ok(serde_json::to_value(statuses).unwrap())
}

#[tauri::command]
pub async fn get_service_config(service_key: String) -> Result<serde_json::Value, String> {
    // Map service keys to config file paths
    let config_file_map: HashMap<&str, &str> = [
        ("dhcp", "/etc/dhcp/dhcpd.conf"),
        ("tftp", "/etc/default/tftpd-hpa"),
        ("iscsi", "/etc/rtslib-fb-target/saveconfig.json"),
        ("http", "/etc/apache2/sites-available/000-default.conf"),
        ("share", "/etc/samba/smb.conf"),
        // Add more as needed
    ]
    .iter()
    .cloned()
    .collect();

    if service_key == "zfs" {
        // Get ZFS pool and dataset info
        let zpool_status = Command::new("sudo")
            .args(["zpool", "status"])
            .output()
            .map_err(|e| format!("Failed to run zpool status: {e}"))?;
        let zpool_status_str = String::from_utf8_lossy(&zpool_status.stdout);

        let zfs_list = Command::new("sudo")
            .args([
                "zfs",
                "list",
                "-t",
                "all",
                "-o",
                "name,type,used,avail,refer,mountpoint",
            ])
            .output()
            .map_err(|e| format!("Failed to run zfs list: {e}"))?;
        let zfs_list_str = String::from_utf8_lossy(&zfs_list.stdout);

        let content = format!(
            "=== ZFS Pool Status ===\n{}\n\n=== ZFS Datasets ===\n{}",
            zpool_status_str, zfs_list_str
        );
        Ok(serde_json::json!({ "text": content }))
    } else if service_key == "iscsi" {
        let config_path = config_file_map
            .get(service_key.as_str())
            .ok_or_else(|| format!("Unknown service key: {}", service_key))?;

        let output = Command::new("sudo")
            .arg("cat")
            .arg(config_path)
            .output()
            .map_err(|e| format!("Failed to execute sudo cat: {e}"))?;

        if !output.status.success() {
            return Err(format!(
                "Failed to read file: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let content = String::from_utf8_lossy(&output.stdout);

        let json_val: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| format!("Error parsing JSON in {}: {}", config_path, e))?;

        Ok(json_val)
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

        let content = fs::read_to_string(config_path)
            .map_err(|e| format!("Error reading config file {}: {}", config_path, e))?;

        Ok(serde_json::json!({ "text": content }))
    }
}

#[tauri::command]
pub async fn control_service(
    service_key: String,
    req: ServiceControlRequest,
) -> Result<Value, String> {
    let service_map: HashMap<&str, &str> = [
        ("iscsi", "target.service"),
        ("dhcp", "isc-dhcp-server.service"),
        ("tftp", "tftpd-hpa.service"),
        ("http", "apache2.service"),
        ("share", "smbd.service"),
    ]
    .iter()
    .cloned()
    .collect();
    let Some(&service_name) = service_map.get(service_key.as_str()) else {
        return Err(format!("Unknown service key: {}", service_key));
    };
    println!(
        "Received control action '{}' for service: {} ({})",
        req.action, service_key, service_name
    );
    match req.action.as_str() {
        "restart" => {
            let output = Command::new("sudo")
                .args(["systemctl", "restart", service_name])
                .output()
                .map_err(|e| format!("Failed to run systemctl: {e}"))?;
            if output.status.success() {
                Ok(
                    json!({ "message": format!("Service '{}' restart command issued successfully.", service_name) }),
                )
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(format!(
                    "Failed to restart service '{}': {}",
                    service_name, stderr
                ))
            }
        }
        _ => Err(format!(
            "Unsupported action '{}' for service '{}'",
            req.action, service_key
        )),
    }
}

#[tauri::command]
pub fn check_services() -> Result<Value, String> {
    let required = vec![
        ("zfs", "zfsutils-linux"),
        ("targetcli", "targetcli-fb"),
        ("dhcpd", "isc-dhcp-server"),
        ("tftp", "tftpd-hpa"),
        ("apache2", "apache2"),
        ("smbd", "samba"),
    ];
    let mut statuses = HashMap::new();
    for (key, svc) in required {
        let installed = Command::new("which")
            .arg(key)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        statuses.insert(
            key,
            json!({
                "name": svc,
                "installed": installed
            }),
        );
    }
    Ok(serde_json::to_value(statuses).unwrap())
}

#[tauri::command]
pub fn install_service(service: String) -> Result<(), String> {
    let status = Command::new("sudo")
        .args(&["apt-get", "install", "-y", &service])
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("Failed to install {}", service))
    }
}
