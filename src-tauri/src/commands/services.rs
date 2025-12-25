use crate::services::ServiceManager;
use crate::state::AppState;
use tauri::State;

// Convert the new service status to the format expected by the frontend
fn convert_service_status(
    service_name: &str,
    new_status: crate::services::ServiceStatus,
) -> crate::core::service::ServiceStatus {
    crate::core::service::ServiceStatus {
        name: service_name.to_string(),
        active: new_status.running,
        status: if new_status.running {
            "running".to_string()
        } else {
            "stopped".to_string()
        },
        pid: new_status.pid,
        memory: None, // Not available in new architecture
        uptime: None, // Not available in new architecture
    }
}

#[tauri::command]
pub async fn list_services(
    state: State<'_, AppState>,
) -> Result<Vec<crate::core::service::ServiceInfo>, String> {
    let settings = state.settings.read().await;
    let service_manager = ServiceManager::new(settings.clone(), state.db_pool.clone());

    // Map service names to display names
    let service_display_names = [
        ("dhcp", "DHCP Server"),
        ("tftp", "TFTP Server"),
        ("iscsi", "iSCSI Target (LIO)"),
        ("nfs", "NFS Server"),
        ("http", "HTTP Server"),
        ("samba", "Samba Server"), // Add Samba if needed
    ];

    let mut services = Vec::new();

    for (name, display_name) in service_display_names {
        match service_manager.status(name).await {
            Ok(status) => {
                // Get the enabled status from settings
                let is_enabled = match name {
                    "dhcp" => service_manager.settings.dhcp.enabled,
                    "tftp" => service_manager.settings.tftp.enabled,
                    "iscsi" => service_manager.settings.iscsi.enabled,
                    "nfs" => service_manager.settings.nfs.enabled,
                    "http" => service_manager.settings.http.enabled,
                    "samba" => service_manager.settings.samba.enabled,
                    _ => true, // default to enabled
                };

                let service_info = crate::core::service::ServiceInfo {
                    name: name.to_string(),
                    display_name: display_name.to_string(),
                    running: status.running,
                    enabled: is_enabled,
                    pid: status.pid,
                };

                services.push(service_info);
            }
            Err(_) => {
                // If service status fails, add with default values
                let is_enabled = match name {
                    "dhcp" => service_manager.settings.dhcp.enabled,
                    "tftp" => service_manager.settings.tftp.enabled,
                    "iscsi" => service_manager.settings.iscsi.enabled,
                    "nfs" => service_manager.settings.nfs.enabled,
                    "http" => service_manager.settings.http.enabled,
                    "samba" => service_manager.settings.samba.enabled,
                    _ => false, // default to disabled if error
                };

                let service_info = crate::core::service::ServiceInfo {
                    name: name.to_string(),
                    display_name: display_name.to_string(),
                    running: false,
                    enabled: is_enabled,
                    pid: None,
                };

                services.push(service_info);
            }
        }
    }

    Ok(services)
}

#[tauri::command]
pub async fn get_service_status(
    state: State<'_, AppState>,
    name: String,
) -> Result<crate::core::service::ServiceStatus, String> {
    let settings = state.settings.read().await;
    let service_manager = ServiceManager::new(settings.clone(), state.db_pool.clone());
    let status = service_manager
        .status(&name)
        .await
        .map_err(|e| e.to_string())?;

    Ok(convert_service_status(&name, status))
}

#[tauri::command]
pub async fn start_service(state: State<'_, AppState>, name: String) -> Result<String, String> {
    let settings = state.settings.read().await;
    let service_manager = ServiceManager::new(settings.clone(), state.db_pool.clone());
    service_manager
        .start(&name)
        .await
        .map_err(|e| e.to_string())?;
    Ok(format!("Service {} started", name))
}

#[tauri::command]
pub async fn stop_service(state: State<'_, AppState>, name: String) -> Result<String, String> {
    let settings = state.settings.read().await;
    let service_manager = ServiceManager::new(settings.clone(), state.db_pool.clone());
    service_manager
        .stop(&name)
        .await
        .map_err(|e| e.to_string())?;
    Ok(format!("Service {} stopped", name))
}

#[tauri::command]
pub async fn restart_service(state: State<'_, AppState>, name: String) -> Result<String, String> {
    let settings = state.settings.read().await;
    let service_manager = ServiceManager::new(settings.clone(), state.db_pool.clone());
    service_manager
        .reload(&name)
        .await
        .map_err(|e| e.to_string())?;
    Ok(format!("Service {} restarted", name))
}

#[tauri::command]
pub async fn get_service_config(
    state: State<'_, AppState>,
    service_name: String,
) -> Result<serde_json::Value, String> {
    let settings = state.settings.read().await;
    let service_manager = ServiceManager::new(settings.clone(), state.db_pool.clone());
    let config = service_manager
        .get_config(&service_name)
        .await
        .map_err(|e| e.to_string())?;

    // Determine the config file path based on service name
    let config_path = match service_name.as_str() {
        "http" | "apache2" => "/etc/apache2/sites-available/diskless-server.conf",
        "samba" | "smbd" => "/etc/samba/smb.conf",
        "tftp" | "tftpd-hpa" => "/etc/default/tftpd-hpa",
        "dhcp" | "isc-dhcp-server" => "/etc/dhcp/dhcpd.conf",
        "nfs" | "nfs-kernel-server" => "/etc/exports",
        "iscsi" | "rtslib-fb-targetctl" => "/etc/target/saveconfig.json",
        _ => "/etc/default/config",
    };

    Ok(serde_json::json!({
        "text": config,
        "path": config_path
    }))
}

#[tauri::command]
pub async fn start_all_services(state: State<'_, AppState>) -> Result<String, String> {
    let settings = state.settings.read().await;
    let service_manager = ServiceManager::new(settings.clone(), state.db_pool.clone());
    service_manager
        .start_all()
        .await
        .map_err(|e| e.to_string())?;
    Ok("All services started successfully".to_string())
}

#[tauri::command]
pub async fn stop_all_services(state: State<'_, AppState>) -> Result<String, String> {
    let settings = state.settings.read().await;
    let service_manager = ServiceManager::new(settings.clone(), state.db_pool.clone());
    service_manager
        .stop_all()
        .await
        .map_err(|e| e.to_string())?;
    Ok("All services stopped successfully".to_string())
}
