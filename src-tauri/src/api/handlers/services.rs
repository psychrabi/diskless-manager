use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use serde_json::Value;

use crate::services::{write_with_sudo_tee, ServiceManager};
use crate::state::AppState;

fn service_config_files() -> Vec<(&'static str, String)> {
    let distro = crate::platform::detect();
    vec![
        ("dhcp", "/etc/dhcp/dhcpd.conf".to_string()),
        ("dhcp-clients", "/etc/dhcp/clients.conf".to_string()),
        ("tftp-autoexec", "/srv/tftp/autoexec.ipxe".to_string()),
        ("tftp", distro.tftp_defaults_path().to_string()),
        ("http", distro.http_config_path().to_string()),
        ("samba", "/etc/samba/smb.conf".to_string()),
    ]
}

fn requires_dhcp_validation(service_name: &str) -> bool {
    matches!(service_name, "dhcp" | "dhcp-clients")
}

pub async fn list_services(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::core::service::ServiceInfo>>, StatusCode> {
    log::info!("list_services endpoint called");
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
        // systemd boot persistence is orthogonal to app-settings
        // management; a query failure degrades to "unknown" (false)
        // rather than failing the whole list.
        let starts_on_boot = service_manager.boot_enabled(name).await.unwrap_or(false);
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
                    starts_on_boot,
                };

                services.push(service_info);
            }
            Err(e) => {
                log::warn!("Failed to get status for service {}: {:?}", name, e);
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
                    starts_on_boot,
                };

                services.push(service_info);
            }
        }
    }

    log::info!("Returning {} services", services.len());
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
    service_manager.start(&name).await.map_err(|error| {
        log::error!("Failed to start '{}': {:#}", name, error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let response = format!("Service {} started", name);
    Ok(Json(response))
}

pub async fn stop_service(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<String>, StatusCode> {
    let settings = state.settings.read().await;
    let service_manager = ServiceManager::new(settings.clone(), state.db_pool.clone());
    service_manager.stop(&name).await.map_err(|error| {
        log::error!("Failed to stop '{}': {:#}", name, error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let response = format!("Service {} stopped", name);
    Ok(Json(response))
}

pub async fn restart_service(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<String>, StatusCode> {
    let settings = state.settings.read().await;
    let service_manager = ServiceManager::new(settings.clone(), state.db_pool.clone());
    service_manager.reload(&name).await.map_err(|error| {
        log::error!("Failed to restart '{}': {:#}", name, error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let response = format!("Service {} restarted", name);
    Ok(Json(response))
}

/// Persist a service for start on boot without changing its running state.
pub async fn enable_service_boot(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<String>, StatusCode> {
    let settings = state.settings.read().await;
    let service_manager = ServiceManager::new(settings.clone(), state.db_pool.clone());
    service_manager.enable_boot(&name).await.map_err(|error| {
        log::error!("Failed to enable '{}' on boot: {:#}", name, error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(format!("Service {} will start on boot", name)))
}

/// Stop persisting a service for start on boot without changing its
/// running state. Use stop to halt it now.
pub async fn disable_service_boot(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<String>, StatusCode> {
    let settings = state.settings.read().await;
    let service_manager = ServiceManager::new(settings.clone(), state.db_pool.clone());
    service_manager.disable_boot(&name).await.map_err(|error| {
        log::error!("Failed to disable '{}' on boot: {:#}", name, error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(format!("Service {} will not start on boot", name)))
}

pub async fn start_all_services(State(state): State<AppState>) -> Result<Json<String>, StatusCode> {
    let settings = state.settings.read().await;
    let service_manager = ServiceManager::new(settings.clone(), state.db_pool.clone());
    service_manager
        .start_all()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json("All services started successfully".to_string()))
}

pub async fn stop_all_services(State(state): State<AppState>) -> Result<Json<String>, StatusCode> {
    let settings = state.settings.read().await;
    let service_manager = ServiceManager::new(settings.clone(), state.db_pool.clone());
    service_manager
        .stop_all()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json("All services stopped successfully".to_string()))
}

pub async fn restart_all_services(
    State(state): State<AppState>,
) -> Result<Json<String>, StatusCode> {
    let settings = state.settings.read().await;
    let service_manager = ServiceManager::new(settings.clone(), state.db_pool.clone());
    service_manager
        .restart_all()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json("All services restarted successfully".to_string()))
}

pub async fn get_service_config(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    use serde_json::json;
    use std::fs;

    let settings = state.settings.read().await;
    let service_manager = ServiceManager::new(settings.clone(), state.db_pool.clone());

    // Special handling for TFTP autoexec file
    if name == "tftp-autoexec" {
        match fs::read_to_string("/srv/tftp/autoexec.ipxe") {
            Ok(content) => {
                return Ok(Json(
                    json!({ "text": content, "path": "/srv/tftp/autoexec.ipxe" }),
                ));
            }
            Err(_) => {
                // File doesn't exist yet, return empty content
                return Ok(Json(
                    json!({ "text": "", "path": "/srv/tftp/autoexec.ipxe" }),
                ));
            }
        }
    }

    // Get config for regular services
    match service_manager.get_config(&name).await {
        Ok(config) => Ok(Json(json!({ "text": config }))),
        Err(_) => {
            // Return empty config if service config doesn't exist
            Ok(Json(json!({ "text": "" })))
        }
    }
}

pub async fn configure_service(
    State(state): State<AppState>,
    Extension(claims): Extension<crate::types::Claims>,
    Path(name): Path<String>,
    body: String,
) -> Result<Json<String>, StatusCode> {
    if claims.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }
    // If the body contains JSON with a "content" field, write raw content.
    // Otherwise, regenerate the config from the current settings.
    let raw_content = serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|v| v.get("content").and_then(|c| c.as_str()).map(String::from));

    if let Some(content) = raw_content {
        let config_files = service_config_files();
        let config_path = config_files
            .iter()
            .find_map(|(k, p)| {
                if *k == name.as_str() {
                    Some(p.as_str())
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                log::error!("Unknown service for config save: {}", name);
                StatusCode::NOT_FOUND
            })?;

        if name == "dhcp" {
            crate::infrastructure::dhcp::replace_dhcp_config(&content)
                .await
                .map_err(|e| {
                    log::error!("Failed to safely write config for {}: {}", name, e);
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
        } else if name == "dhcp-clients" {
            crate::infrastructure::dhcp::replace_dhcp_clients_config(&content)
                .await
                .map_err(|e| {
                    log::error!("Failed to safely write config for {}: {}", name, e);
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
        } else {
            write_with_sudo_tee(config_path, &content)
                .await
                .map_err(|e| {
                    log::error!("Failed to write config for {}: {}", name, e);
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
        }

        log::info!("Raw configuration saved for service: {}", name);
    } else {
        // Regenerate config from current settings (settings page flow)
        let settings = state.settings.read().await;
        let service_manager = ServiceManager::new(settings.clone(), state.db_pool.clone());
        service_manager
            .generate_service_config(&name)
            .await
            .map_err(|e| {
                log::error!("Failed to generate config for {}: {}", name, e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        log::info!(
            "Configuration regenerated from settings for service: {}",
            name
        );
    }

    // Reload the service to pick up new config
    let settings = state.settings.read().await;
    let service_manager = ServiceManager::new(settings.clone(), state.db_pool.clone());
    if requires_dhcp_validation(&name) {
        service_manager.dhcp.validate_config().await.map_err(|e| {
            log::error!(
                "DHCP configuration validation failed; service was not reloaded: {}",
                e
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        service_manager.reload(&name).await.map_err(|e| {
            log::error!("Failed to reload validated DHCP configuration: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    } else {
        let _ = service_manager.reload(&name).await;
    }

    Ok(Json(format!("Service {} configured successfully", name)))
}

pub async fn install_service(
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<String>, StatusCode> {
    let service = payload
        .get("service")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;

    log::info!(
        "Dependency install: install request received for '{}'",
        service
    );
    match crate::commands::system::install_package(service.to_string()).await {
        Ok(msg) => {
            log::info!(
                "Dependency install: request for '{}' completed successfully",
                service
            );
            Ok(Json(msg))
        }
        Err(e) => {
            log::error!("Failed to install service '{}': {}", service, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dhcp_configuration_changes_require_validation_before_reload() {
        assert!(requires_dhcp_validation("dhcp"));
        assert!(requires_dhcp_validation("dhcp-clients"));
        assert!(!requires_dhcp_validation("tftp"));
    }
}
