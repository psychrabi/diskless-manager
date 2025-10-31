//! Service management commands - Tauri command handlers
//!
//! This module provides the Tauri command handlers for service management operations.

use crate::types::service::{ServiceControlRequest, PackageStatus};

#[tauri::command]
pub async fn get_services(token: String, zfs_pool: String) -> Result<serde_json::Value, String> {
    // Implementation would use ServiceCommands
    Ok(serde_json::json!({
        "iscsi": { "name": "iscsi", "service": "rtslib-fb-targetctl", "status": "active" },
        "dhcp": { "name": "dhcp", "service": "isc-dhcp-server", "status": "active" }
    }))
}

#[tauri::command]
pub async fn control_service(token: String, service_key: String, req: ServiceControlRequest) -> Result<serde_json::Value, String> {
    // Implementation would use ServiceCommands
    Ok(serde_json::json!({
        "message": format!("Service '{}' {} issued successfully.", service_key, req.action)
    }))
}

#[tauri::command]
pub async fn check_package_status(token: String) -> Result<serde_json::Value, String> {
    // Implementation would return PackageStatus array
    Ok(serde_json::json!([]))
}