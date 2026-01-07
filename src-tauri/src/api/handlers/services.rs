use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use crate::services::ServiceManager;
use crate::state::AppState;

pub async fn list_services(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::core::service::ServiceInfo>>, StatusCode> {
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

    Ok(Json(services))
}

pub async fn get_service_status(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<crate::core::service::ServiceStatus>, StatusCode> {
    let settings = state.settings.read().await;
    let service_manager = ServiceManager::new(settings.clone(), state.db_pool.clone());
    let status = service_manager
        .status(&name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Convert the new service status to the format expected by the frontend
    let converted_status = crate::core::service::ServiceStatus {
        name: name.to_string(),
        active: status.running,
        status: if status.running {
            "running".to_string()
        } else {
            "stopped".to_string()
        },
        pid: status.pid,
        memory: None, // Not available in new architecture
        uptime: None, // Not available in new architecture
    };

    Ok(Json(converted_status))
}

pub async fn start_service(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<String>, StatusCode> {
    let settings = state.settings.read().await;
    let service_manager = ServiceManager::new(settings.clone(), state.db_pool.clone());
    service_manager
        .start(&name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let response = format!("Service {} started", name);
    Ok(Json(response))
}

pub async fn stop_service(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<String>, StatusCode> {
    let settings = state.settings.read().await;
    let service_manager = ServiceManager::new(settings.clone(), state.db_pool.clone());
    service_manager
        .stop(&name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let response = format!("Service {} stopped", name);
    Ok(Json(response))
}

pub async fn restart_service(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<String>, StatusCode> {
    let settings = state.settings.read().await;
    let service_manager = ServiceManager::new(settings.clone(), state.db_pool.clone());
    service_manager
        .reload(&name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let response = format!("Service {} restarted", name);
    Ok(Json(response))
}
