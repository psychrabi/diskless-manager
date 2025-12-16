use crate::core::config::Settings;
use crate::core::service::ServiceManager;
use crate::state::AppState;
use serde::Serialize;
use std::process::Command;
use tauri::State;

#[derive(Debug, Clone, Serialize)]
pub struct SystemInfo {
    pub hostname: String,
    pub os: String,
    pub kernel: String,
    pub uptime: String,
    pub cpu_count: usize,
    pub memory_total: String,
    pub memory_available: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServerStatus {
    pub initialized: bool,
    pub services_running: u32,
    pub services_total: u32,
    pub clients_count: u32,
    pub images_count: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct DependencyStatus {
    pub name: String,
    pub installed: bool,
    pub version: Option<String>,
}

#[tauri::command]
pub async fn get_system_info() -> Result<SystemInfo, String> {
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let os = std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|content| {
            content
                .lines()
                .find(|l| l.starts_with("PRETTY_NAME="))
                .map(|l| {
                    l.trim_start_matches("PRETTY_NAME=")
                        .trim_matches('"')
                        .to_string()
                })
        })
        .unwrap_or_else(|| "Linux".to_string());

    let kernel = Command::new("uname")
        .arg("-r")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let uptime = Command::new("uptime")
        .arg("-p")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let cpu_count = num_cpus::get();

    let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let memory_total = parse_meminfo(&meminfo, "MemTotal:");
    let memory_available = parse_meminfo(&meminfo, "MemAvailable:");

    Ok(SystemInfo {
        hostname,
        os,
        kernel,
        uptime,
        cpu_count,
        memory_total,
        memory_available,
    })
}

fn parse_meminfo(content: &str, key: &str) -> String {
    content
        .lines()
        .find(|l| l.starts_with(key))
        .and_then(|l| l.split_whitespace().nth(1))
        .map(|kb| {
            let kb: u64 = kb.parse().unwrap_or(0);
            format_bytes(kb * 1024)
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn format_bytes(bytes: u64) -> String {
    const GB: u64 = 1024 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else {
        format!("{:.0} MB", bytes as f64 / MB as f64)
    }
}

#[tauri::command]
pub async fn get_server_status(state: State<'_, AppState>) -> Result<ServerStatus, String> {
    let service_manager = ServiceManager::new();
    let services = service_manager.list_services();
    let services_running = services.iter().filter(|s| s.running).count() as u32;

    let clients_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM clients")
        .fetch_one(&state.db_pool)
        .await
        .map_err(|e| e.to_string())?;

    let images_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM images")
        .fetch_one(&state.db_pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(ServerStatus {
        initialized: true,
        services_running,
        services_total: services.len() as u32,
        clients_count: clients_count.0 as u32,
        images_count: images_count.0 as u32,
    })
}

#[tauri::command]
pub async fn initialize_server(state: State<'_, AppState>) -> Result<String, String> {
    let settings = state.settings.read().await;

    // Create directories
    let dirs = [
        &settings.tftp.root_dir,
        &settings.iscsi.targets_dir,
        &settings.nfs.exports_dir,
        &settings.samba.share_path,
        &settings.storage.images_dir,
        &settings.storage.snapshots_dir,
    ];

    for dir in dirs {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("Failed to create {}: {}", dir.display(), e))?;
    }

    Ok("Server initialized successfully".to_string())
}

#[tauri::command]
pub async fn check_dependencies() -> Result<Vec<DependencyStatus>, String> {
    let dependencies = vec![
        ("qemu-img", "qemu-utils"),
        ("targetcli", "targetcli-fb"),
        ("dhcpd", "isc-dhcp-server"),
        ("in.tftpd", "tftpd-hpa"),
        ("exportfs", "nfs-kernel-server"),
        ("apache2", "apache2"),
        ("smbd", "samba"),
        ("wakeonlan", "wakeonlan"),
        ("zfs", "zfsutils-linux"),
        ("xfreerdp3", "freerdp3-x11"),
    ];

    let statuses: Vec<DependencyStatus> = dependencies
        .into_iter()
        .map(|(cmd, name)| {
            let output = Command::new("which").arg(cmd).output();
            let installed = output.map(|o| o.status.success()).unwrap_or(false);

            let version = Command::new("dpkg-query")
                .args(["--showformat=${Version}\n", "--show", name])
                .output()
                .ok()
                .and_then(|o| {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    let output = if stdout.is_empty() { stderr } else { stdout };
                    output.lines().next().map(|s| s.to_string())
                });

            DependencyStatus {
                name: name.to_string(),
                installed,
                version,
            }
        })
        .collect();

    Ok(statuses)
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    let settings = state.settings.read().await;
    Ok(settings.clone())
}

#[tauri::command]
pub async fn save_settings(state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
    let mut current = state.settings.write().await;
    *current = settings.clone();
    current
        .save(&state.config_path)
        .map_err(|e| e.to_string())?;
    tracing::info!("Settings saved to {}", state.config_path.display());
    Ok(())
}
