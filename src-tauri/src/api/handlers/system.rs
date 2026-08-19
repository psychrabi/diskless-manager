use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::core::service::ServiceManager;
use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct ClearCacheRequest {}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApplyNetworkSettingsRequest {}

pub async fn get_system_info(
    State(_state): State<AppState>,
) -> Result<Json<crate::commands::system::SystemInfo>, StatusCode> {
    // Call the existing Tauri command function directly - it doesn't need state
    match crate::commands::system::get_system_info().await {
        Ok(info) => Ok(Json(info)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn get_server_status(
    State(state): State<AppState>,
) -> Result<Json<crate::commands::system::ServerStatus>, StatusCode> {
    // Replicate the logic from the Tauri command
    let service_manager = ServiceManager::new();
    let services = service_manager.list_services();
    let services_running = services.iter().filter(|s| s.running).count() as u32;

    let clients_count: (i64,) = match sqlx::query_as("SELECT COUNT(*) FROM clients")
        .fetch_one(&state.db_pool)
        .await
    {
        Ok(count) => count,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let images_count: (i64,) = match sqlx::query_as("SELECT COUNT(*) FROM images")
        .fetch_one(&state.db_pool)
        .await
    {
        Ok(count) => count,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let status = crate::commands::system::ServerStatus {
        initialized: true,
        services_running,
        services_total: services.len() as u32,
        clients_count: clients_count.0 as u32,
        images_count: images_count.0 as u32,
    };

    Ok(Json(status))
}

pub async fn initialize_server(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let settings = state.settings.read().await;

    // Create directories
    if std::fs::create_dir_all(&settings.tftp.root_dir).is_err() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    if std::fs::create_dir_all(&settings.iscsi.targets_dir).is_err() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    if std::fs::create_dir_all(&settings.nfs.exports_dir).is_err() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    if std::fs::create_dir_all(&settings.samba.share_path).is_err() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    if std::fs::create_dir_all(&settings.storage.images_dir).is_err() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    if std::fs::create_dir_all(&settings.storage.snapshots_dir).is_err() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(Json(
        serde_json::json!({ "message": "Server initialized successfully" }),
    ))
}

pub async fn check_dependencies(
) -> Result<Json<Vec<crate::commands::system::DependencyStatus>>, StatusCode> {
    match crate::commands::system::check_dependencies().await {
        Ok(deps) => Ok(Json(deps)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn clear_cache() -> Result<Json<serde_json::Value>, StatusCode> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    // Flush filesystem buffers
    let _ = Command::new("sync").output();

    // Write 3 to drop_caches via sudo tee (requires root)
    let mut child = Command::new("sudo")
        .args(["-n", "tee", "/proc/sys/vm/drop_caches"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(b"3\n");
    }

    let status = child
        .wait()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if status.success() {
        Ok(Json(
            serde_json::json!({ "message": "Cache cleared successfully" }),
        ))
    } else {
        Err(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

pub async fn get_network_interfaces() -> Result<Json<Vec<String>>, StatusCode> {
    match crate::commands::system::get_network_interfaces().await {
        Ok(interfaces) => Ok(Json(interfaces)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn get_interface_ip(
    Path(name): Path<String>,
) -> Result<Json<Option<String>>, StatusCode> {
    match crate::commands::system::get_interface_ip(name).await {
        Ok(ip) => Ok(Json(ip)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn detect_server_network(
) -> Result<Json<crate::commands::system::NetworkDetection>, StatusCode> {
    match crate::commands::system::detect_server_network().await {
        Ok(detection) => Ok(Json(detection)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn apply_network_settings(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut settings = state.settings.read().await.clone();
    let server = &settings.server;

    if server.interface.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let interface = &server.interface[0];
    let ip = &server.ip_address;
    let mask = &server.netmask;
    let gateway = &server.gateway;
    let dns = &server.dns;

    // Convert dotted mask to prefix
    let prefix = mask_to_prefix(mask).unwrap_or(24);

    let dns_str = if dns.is_empty() {
        "8.8.8.8, 8.8.4.4".to_string()
    } else {
        dns.join(", ")
    };

    let netplan_content = format!(
        r#"network:
  version: 2
  renderer: networkd
  ethernets:
    {}:
      dhcp4: no
      addresses:
        - {}/{}
      gateway4: {}
      nameservers:
        addresses: [{}]
"#,
        interface, ip, prefix, gateway, dns_str
    );

    let path = "/etc/netplan/99-diskless-manager.yaml";
    if crate::services::write_with_sudo_tee(path, &netplan_content)
        .await
        .is_err()
    {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    // Apply netplan
    if crate::services::run_sudo_command(["netplan", "apply"])
        .await
        .is_err()
    {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    // Update related service configurations with the new static IP
    settings.tftp.server_ip = ip.clone();
    settings.http.server_ip = ip.clone();

    // Update DHCP settings
    settings.dhcp.next_server_ip = ip.clone();
    settings.dhcp.boot_server_ip = ip.clone();
    settings.dhcp.subnet_mask = mask.clone();
    settings.dhcp.gateway_ip = gateway.clone();

    // Calculate subnet and broadcast based on IP and Mask
    if let Ok(subnet) = crate::utils::network::calculate_network(ip, mask) {
        settings.dhcp.subnet_ip = subnet;
    }
    if let Ok(broadcast) = crate::utils::network::calculate_broadcast(ip, mask) {
        settings.dhcp.broadcast_ip = broadcast;
    }

    // Persist the updated settings
    {
        // 1. Update in-memory state
        let mut write_lock = state.settings.write().await;
        *write_lock = settings.clone();

        // 2. Save to TOML
        if write_lock.save(&state.config_path).is_err() {
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }

        // 3. Save to Database to ensure consistency on restart
        let current_config = crate::config::get_config();
        let mut new_config = current_config;

        let new_settings_value = match serde_json::to_value(&settings) {
            Ok(v) => v,
            Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        };

        if let (Some(obj), Some(new_obj)) = (
            new_config.settings.as_object_mut(),
            new_settings_value.as_object(),
        ) {
            for (k, v) in new_obj {
                obj.insert(k.clone(), v.clone());
            }
        } else {
            new_config.settings = new_settings_value;
        }

        if crate::config::write_config(&state.db_pool, &new_config)
            .await
            .is_err()
        {
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    // Regenerate and reload all services
    let service_manager = crate::services::ServiceManager::new(settings, state.db_pool.clone());
    if service_manager.generate_all_configs().await.is_err() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    if service_manager.restart_all().await.is_err() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(Json(
        serde_json::json!({ "message": "Network settings applied and services updated successfully" }),
    ))
}

pub async fn get_settings(
    State(state): State<AppState>,
) -> Result<Json<crate::core::config::Settings>, StatusCode> {
    let settings = state.settings.read().await;
    Ok(Json(settings.clone()))
}

pub async fn save_settings(
    State(state): State<AppState>,
    Json(settings): Json<crate::core::config::Settings>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Update the in-memory settings
    {
        let mut current = state.settings.write().await;
        *current = settings.clone();
    }

    // Update the settings in the database (merging with existing fields to avoid losing zpool_name etc)
    let current_config = crate::config::get_config();
    let mut new_config = current_config;

    let new_settings_value = match serde_json::to_value(&settings) {
        Ok(v) => v,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    if let (Some(obj), Some(new_obj)) = (
        new_config.settings.as_object_mut(),
        new_settings_value.as_object(),
    ) {
        for (k, v) in new_obj {
            obj.insert(k.clone(), v.clone());
        }
    } else {
        new_config.settings = new_settings_value;
    }

    if crate::config::write_config(&state.db_pool, &new_config)
        .await
        .is_err()
    {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    // Also persist to config.toml for redundancy and manual editing support
    let toml_path = state.config_path.with_extension("toml");
    if state.settings.read().await.save(&toml_path).is_err() {
        // Log but don't fail if TOML save fails
    }

    Ok(Json(
        serde_json::json!({ "message": "Settings saved successfully" }),
    ))
}

pub async fn setup_privileged_access() -> Result<Json<serde_json::Value>, StatusCode> {
    match crate::commands::system::setup_privileged_access().await {
        Ok(msg) => Ok(Json(serde_json::json!({ "message": msg }))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

fn mask_to_prefix(mask: &str) -> Option<u32> {
    let parts: Vec<u32> = mask.split('.').filter_map(|s| s.parse().ok()).collect();
    if parts.len() != 4 {
        return None;
    }
    let mut full_mask = 0u32;
    for part in parts {
        full_mask = (full_mask << 8) | part;
    }
    Some(full_mask.count_ones())
}

// ============================================================================
// Additional System Handlers
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct RamUsageResponse {
    pub total: u64,
    pub available: u64,
    pub used: u64,
    pub percent: f64,
}

pub async fn get_ram_usage(
    State(_state): State<AppState>,
) -> Result<Json<RamUsageResponse>, StatusCode> {
    use std::process::Command;

    // Try to get memory info from /proc/meminfo
    let output = Command::new("grep")
        .args(["MemTotal\\|MemAvailable", "/proc/meminfo"])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let content = String::from_utf8_lossy(&output.stdout);
            let mut total = 0u64;
            let mut available = 0u64;

            for line in content.lines() {
                if line.starts_with("MemTotal:") {
                    if let Some(val) = line.split_whitespace().nth(1) {
                        total = val.parse().unwrap_or(0) * 1024; // Convert KB to bytes
                    }
                } else if line.starts_with("MemAvailable:") {
                    if let Some(val) = line.split_whitespace().nth(1) {
                        available = val.parse().unwrap_or(0) * 1024; // Convert KB to bytes
                    }
                }
            }

            let used = total.saturating_sub(available);
            let percent = if total > 0 {
                (used as f64 / total as f64) * 100.0
            } else {
                0.0
            };

            Ok(Json(RamUsageResponse {
                total,
                available,
                used,
                percent,
            }))
        }
        _ => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ArcStatResponse {
    pub size: u64,
    pub max_size: u64,
    pub hit_ratio: f64,
}

pub async fn get_zfs_arcstat(
    State(_state): State<AppState>,
) -> Result<Json<ArcStatResponse>, StatusCode> {
    use std::fs;

    // Try to get ARC stats from /proc/spl/kstat/zfs/arcstats
    match fs::read_to_string("/proc/spl/kstat/zfs/arcstats") {
        Ok(content) => {
            let mut size = 0u64;
            let mut max_size = 0u64;
            let mut hits = 0u64;
            let mut misses = 0u64;

            for line in content.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    match parts[0] {
                        "size" => size = parts[2].parse().unwrap_or(0),
                        "c_max" => max_size = parts[2].parse().unwrap_or(0),
                        "hits" => hits = parts[2].parse().unwrap_or(0),
                        "misses" => misses = parts[2].parse().unwrap_or(0),
                        _ => {}
                    }
                }
            }

            let hit_ratio = if hits + misses > 0 {
                (hits as f64 / (hits + misses) as f64) * 100.0
            } else {
                0.0
            };

            Ok(Json(ArcStatResponse {
                size,
                max_size,
                hit_ratio,
            }))
        }
        Err(_) => {
            // Return default values if ARC stats not available
            Ok(Json(ArcStatResponse {
                size: 0,
                max_size: 0,
                hit_ratio: 0.0,
            }))
        }
    }
}
