use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};

use crate::state::AppState;

/// Snapshot of cumulative ARC counters used to derive per-interval hit rates.
struct ArcCounterSample {
    hits: u64,
    misses: u64,
    demand_data_hits: u64,
    demand_data_misses: u64,
}

static LAST_ARC_SAMPLE: OnceLock<Mutex<Option<ArcCounterSample>>> = OnceLock::new();

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
    let settings = state.settings.read().await.clone();
    let service_manager = crate::services::ServiceManager::new(settings, state.db_pool.clone());
    let service_names = ["dhcp", "tftp", "iscsi", "nfs", "samba", "http"];
    let mut services_running = 0;
    for name in service_names {
        if service_manager
            .status(name)
            .await
            .is_ok_and(|status| status.running)
        {
            services_running += 1;
        }
    }

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
        services_total: service_names.len() as u32,
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

    if crate::platform::apply_static_network_config(interface, ip, prefix, gateway, dns)
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
    let _client_guard = state.client_mutations.lock().await;

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

    // Persist settings only. Rewriting cached client rows here could resurrect
    // deleted clients or overwrite their persistence choice.
    persist_settings_snapshot(&state, &new_config, &settings).await?;
    *state.settings.write().await = settings.clone();

    Ok(Json(
        serde_json::json!({ "message": "Settings saved successfully" }),
    ))
}

/// Write settings to the database, the global config, and the TOML mirror.
///
/// Shared by full settings saves and surgical mutations (e.g. enrollment
/// window open/close) so every path persists identically.
async fn persist_settings_snapshot(
    state: &AppState,
    new_config: &crate::types::AppConfig,
    settings: &crate::core::config::Settings,
) -> Result<(), StatusCode> {
    let mut transaction = state
        .db_pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Some(values) = new_config.settings.as_object() {
        for (key, value) in values {
            sqlx::query("INSERT INTO app_config (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
                .bind(key).bind(value.to_string()).execute(&mut *transaction).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }
    transaction
        .commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    crate::config::set_config(new_config);

    // Also persist to config.toml for redundancy and manual editing support
    let toml_path = state.config_path.with_extension("toml");
    if settings.save(&toml_path).is_err() {
        // Log but don't fail if TOML save fails
    }

    Ok(())
}

/// Request body for opening the enrollment window.
#[derive(Debug, serde::Deserialize)]
pub struct OpenEnrollmentRequest {
    /// Window length in minutes. Defaults to the configured
    /// `enrollment.window_minutes`. Clamped to 1..=60.
    pub minutes: Option<u32>,
}

/// Open the PXE enrollment window for self-registering unknown machines.
///
/// Admitted machines land disabled (pool) until enabled and provisioned.
/// There is no permanent auto-register: the window always expires.
pub async fn open_enrollment(
    State(state): State<AppState>,
    Json(request): Json<OpenEnrollmentRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let _client_guard = state.client_mutations.lock().await;

    let default_minutes = state.settings.read().await.enrollment.window_minutes;
    let minutes = request.minutes.unwrap_or(default_minutes).clamp(1, 60);
    let open_until = chrono::Utc::now().timestamp() + i64::from(minutes) * 60;

    let settings = {
        let mut guard = state.settings.write().await;
        guard.enrollment.open_until = Some(open_until);
        guard.clone()
    };

    let current_config = crate::config::get_config();
    let mut new_config = current_config;
    new_config.settings = serde_json::to_value(&settings)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    persist_settings_snapshot(&state, &new_config, &settings).await?;
    *state.settings.write().await = settings.clone();

    tracing::info!(
        open_until = open_until,
        minutes = minutes,
        "enrollment window opened"
    );
    Ok(Json(serde_json::json!({
        "message": format!("Enrollment opened for {} minutes", minutes),
        "open_until": open_until,
        "window_minutes": minutes,
    })))
}

/// Close the PXE enrollment window immediately.
///
/// Unknown machines are denied from this point on; already-registered
/// (even pending) records are unaffected.
pub async fn close_enrollment(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let _client_guard = state.client_mutations.lock().await;

    let settings = {
        let mut guard = state.settings.write().await;
        guard.enrollment.open_until = None;
        guard.clone()
    };

    let current_config = crate::config::get_config();
    let mut new_config = current_config;
    new_config.settings = serde_json::to_value(&settings)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    persist_settings_snapshot(&state, &new_config, &settings).await?;
    *state.settings.write().await = settings.clone();

    tracing::info!("enrollment window closed");
    Ok(Json(serde_json::json!({
        "message": "Enrollment closed",
        "open_until": serde_json::Value::Null,
    })))
}

pub async fn setup_privileged_access() -> Result<Json<serde_json::Value>, StatusCode> {
    match crate::commands::system::setup_privileged_access().await {
        Ok(msg) => Ok(Json(serde_json::json!({ "message": msg }))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn check_privileged_access() -> Result<Json<serde_json::Value>, StatusCode> {
    let authorized = crate::commands::system::is_privileged_access_configured();
    let message = if authorized {
        "Privileged access is already configured"
    } else {
        "Privileged access has not been granted yet"
    };
    Ok(Json(
        serde_json::json!({ "authorized": authorized, "message": message }),
    ))
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
    pub used_percent: f64,
    pub hits: u64,
    pub misses: u64,
    pub hit_ratio: f64,
    pub demand_data_hit_ratio: f64,
    pub l2_size: u64,
    pub l2_hits: u64,
    pub l2_misses: u64,
    pub l2_hit_ratio: f64,
    pub interval_hit_ratio: f64,
    pub interval_demand_hit_ratio: f64,
    pub interval_warming_up: bool,
}

pub async fn get_zfs_arcstat(
    State(_state): State<AppState>,
) -> Result<Json<ArcStatResponse>, StatusCode> {
    use std::fs;

    let empty = || ArcStatResponse {
        size: 0,
        max_size: 0,
        used_percent: 0.0,
        hits: 0,
        misses: 0,
        hit_ratio: 0.0,
        demand_data_hit_ratio: 0.0,
        l2_size: 0,
        l2_hits: 0,
        l2_misses: 0,
        l2_hit_ratio: 0.0,
        interval_hit_ratio: 0.0,
        interval_demand_hit_ratio: 0.0,
        interval_warming_up: true,
    };

    // Try to get ARC stats from /proc/spl/kstat/zfs/arcstats
    match fs::read_to_string("/proc/spl/kstat/zfs/arcstats") {
        Ok(content) => {
            let mut size = 0u64;
            let mut max_size = 0u64;
            let mut hits = 0u64;
            let mut misses = 0u64;
            let mut demand_data_hits = 0u64;
            let mut demand_data_misses = 0u64;
            let mut l2_size = 0u64;
            let mut l2_hits = 0u64;
            let mut l2_misses = 0u64;

            for line in content.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    match parts[0] {
                        "size" => size = parts[2].parse().unwrap_or(0),
                        "c_max" => max_size = parts[2].parse().unwrap_or(0),
                        "hits" => hits = parts[2].parse().unwrap_or(0),
                        "misses" => misses = parts[2].parse().unwrap_or(0),
                        "demand_data_hits" => demand_data_hits = parts[2].parse().unwrap_or(0),
                        "demand_data_misses" => demand_data_misses = parts[2].parse().unwrap_or(0),
                        "l2_size" => l2_size = parts[2].parse().unwrap_or(0),
                        "l2_hits" => l2_hits = parts[2].parse().unwrap_or(0),
                        "l2_misses" => l2_misses = parts[2].parse().unwrap_or(0),
                        _ => {}
                    }
                }
            }

            let ratio = |hits: u64, misses: u64| -> f64 {
                if hits + misses > 0 {
                    (hits as f64 / (hits + misses) as f64) * 100.0
                } else {
                    0.0
                }
            };

            let used_percent = if max_size > 0 {
                (size as f64 / max_size as f64) * 100.0
            } else {
                0.0
            };

            let interval = {
                let lock = LAST_ARC_SAMPLE.get_or_init(|| Mutex::new(None));
                let mut sample = lock.lock().unwrap_or_else(|e| e.into_inner());
                let current = ArcCounterSample {
                    hits,
                    misses,
                    demand_data_hits,
                    demand_data_misses,
                };
                let interval = sample.as_ref().map(|prev| {
                    (
                        ratio(
                            hits.saturating_sub(prev.hits),
                            misses.saturating_sub(prev.misses),
                        ),
                        ratio(
                            demand_data_hits.saturating_sub(prev.demand_data_hits),
                            demand_data_misses.saturating_sub(prev.demand_data_misses),
                        ),
                    )
                });
                *sample = Some(current);
                interval
            };
            let (interval_hit_ratio, interval_demand_hit_ratio) = interval.unwrap_or((0.0, 0.0));
            let interval_warming_up = interval.is_none();

            Ok(Json(ArcStatResponse {
                size,
                max_size,
                used_percent,
                hits,
                misses,
                hit_ratio: ratio(hits, misses),
                demand_data_hit_ratio: ratio(demand_data_hits, demand_data_misses),
                l2_size,
                l2_hits,
                l2_misses,
                l2_hit_ratio: ratio(l2_hits, l2_misses),
                interval_hit_ratio,
                interval_demand_hit_ratio,
                interval_warming_up,
            }))
        }
        Err(_) => {
            // Return default values if ARC stats not available
            Ok(Json(empty()))
        }
    }
}
