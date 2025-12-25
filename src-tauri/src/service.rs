use crate::cmd::run_command_async;
use crate::config::{get_config, set_config, write_config};
use crate::error::AppError;
use crate::middleware::validate_auth_token_for_command;
use crate::services::ServiceManager;
use crate::state::AppState;
use crate::types::service::SambaShare;
use crate::types::{DHCPConfig, HTTPConfig, PackageStatus, TFTPConfig};
use crate::{DHCP_CLIENTS_PATH, DHCP_CONFIG_PATH, TFTP_AUTOEXEC_PATH};
use async_process::Command as AsyncCommand;
use futures::io::AsyncWriteExt;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;

// Cache for system services information to reduce system calls
use once_cell::sync::Lazy;

// Common auth validator
fn validate_token(token: &str) -> Result<(), AppError> {
    match validate_auth_token_for_command(token) {
        Ok(_) => Ok(()),
        Err(e) => Err(AppError::Auth(format!(
            "Authentication failed: {}",
            e.message
        ))),
    }
}

// Helper: Write content to path using sudo tee (async)
async fn write_with_sudo_tee(path: &str, content: &str) -> Result<(), AppError> {
    let mut child = AsyncCommand::new("sudo")
        .arg("-n")
        .arg("tee")
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::Command(format!("Failed to spawn sudo tee for {}: {}", path, e)))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(content.as_bytes()).await.map_err(|e| {
            AppError::Io(std::io::Error::other(format!(
                "Failed to write to stdin for {}: {}",
                path, e
            )))
        })?;
    }

    let output = child
        .output()
        .await
        .map_err(|e| AppError::Command(format!("Failed to wait for tee on {}: {}", path, e)))?;

    if !output.status.success() {
        Err(AppError::Command(format!(
            "Failed to write {}: {}",
            path,
            String::from_utf8_lossy(&output.stderr)
        )))
    } else {
        Ok(())
    }
}

#[tauri::command]
pub async fn install_service(service: String, token: String) -> Result<(), AppError> {
    validate_token(&token)?;

    match Command::new("pkexec")
        .args(["apt-get", "install", "-y", &service])
        .output()
    {
        Ok(_) => Ok(()),
        Err(e) => Err(AppError::Command(format!(
            "Failed to install {}: {}",
            service, e
        ))),
    }
}

#[tauri::command]
pub async fn save_service_config(
    token: String,
    service_key: String,
    content: String,
) -> Result<(), AppError> {
    validate_token(&token)?;

    let config_file_map = [
        ("isc-dhcp-server", DHCP_CONFIG_PATH),
        ("dhcp-clients", DHCP_CLIENTS_PATH),
        ("tftp-autoexec", TFTP_AUTOEXEC_PATH),
        ("tftpd-hpa", "/etc/default/tftpd-hpa"),
        (
            "apache2",
            "/etc/apache2/sites-available/diskless-server.conf",
        ),
        ("smbd", "/etc/samba/smb.conf"),
    ];

    let path = config_file_map
        .iter()
        .find_map(|&(k, v)| {
            if k == service_key.as_str() {
                Some(v)
            } else {
                None
            }
        })
        .ok_or_else(|| AppError::NotFound(format!("Unknown service: {}", service_key)))?;

    write_with_sudo_tee(path, &content).await
}

#[tauri::command]
pub async fn configure_dhcp_server(
    state: State<'_, AppState>,
    token: String,
    config: DHCPConfig,
) -> Result<String, AppError> {
    validate_token(&token)?;

    // Update the settings in the app state
    {
        let mut settings = state.settings.write().await;
        settings.dhcp.range_start = config.start_ip.clone();
        settings.dhcp.range_end = config.end_ip.clone();
        settings.dhcp.subnet_mask = config.subnet_mask.clone();
        settings.dhcp.gateway = config.gateway_ip.clone();
        settings.dhcp.dns_servers = vec![config.dns_server1.clone(), config.dns_server2.clone()];
        // Enable DHCP service in settings
        settings.dhcp.enabled = true;
    }

    // Use the new services architecture to generate the configuration
    let settings = state.settings.read().await;
    let service_manager = ServiceManager::new(settings.clone(), state.db_pool.clone());
    service_manager
        .dhcp
        .generate_config()
        .await
        .map_err(|e| AppError::Command(format!("Failed to generate DHCP config: {}", e)))?;

    // Restart the DHCP service using the new architecture
    service_manager
        .regenerate_dhcp_config()
        .await
        .map_err(|e| AppError::Command(format!("Failed to restart DHCP service: {}", e)))?;

    Ok("DHCP server configured successfully".to_string())
}

#[tauri::command]
pub async fn configure_tftp_server(
    state: State<'_, AppState>,
    token: String,
    tftp_config: TFTPConfig,
) -> Result<String, AppError> {
    validate_token(&token)?;

    // Update the settings in the app state
    {
        let mut settings = state.settings.write().await;
        settings.tftp.root_dir = PathBuf::from(&tftp_config.tftp_root);
        // Enable TFTP service in settings
        settings.tftp.enabled = true;
    }

    // Use the new services architecture to generate the configuration
    let settings = state.settings.read().await;
    let service_manager = ServiceManager::new(settings.clone(), state.db_pool.clone());
    service_manager
        .tftp
        .generate_config()
        .await
        .map_err(|e| AppError::Command(format!("Failed to generate TFTP config: {}", e)))?;

    // Restart the TFTP service using the new architecture
    service_manager
        .tftp
        .reload()
        .await
        .map_err(|e| AppError::Command(format!("Failed to restart TFTP service: {}", e)))?;

    Ok("TFTP server configured successfully".to_string())
}

#[tauri::command]
pub async fn configure_apache_server(
    state: State<'_, AppState>,
    token: String,
    http_config: HTTPConfig,
) -> Result<String, AppError> {
    validate_token(&token)?;

    // Update the settings in the app state
    {
        let mut settings = state.settings.write().await;
        // Update the HTTP service settings
        settings.http.root_dir = PathBuf::from(&http_config.http_root);
        // Enable HTTP service in settings
        settings.http.enabled = true;
    }

    // Use the new services architecture to generate the configuration
    let settings = state.settings.read().await;
    let service_manager = ServiceManager::new(settings.clone(), state.db_pool.clone());
    service_manager
        .http
        .generate_config()
        .await
        .map_err(|e| AppError::Command(format!("Failed to generate HTTP config: {}", e)))?;

    // Restart the HTTP service using the new architecture
    service_manager
        .http
        .reload()
        .await
        .map_err(|e| AppError::Command(format!("Failed to restart HTTP service: {}", e)))?;

    Ok("Apache server configured successfully".to_string())
}

#[tauri::command]
pub async fn configure_samba_server(
    state: State<'_, AppState>,
    token: String,
    shares: Vec<SambaShare>,
) -> Result<String, AppError> {
    validate_token(&token)?;

    // Update the settings in the app state
    // Note: Samba shares are not directly stored in Settings as individual shares
    // Instead, we'll update the general Samba configuration
    {
        let mut settings = state.settings.write().await;
        // For now, we'll just enable Samba
        settings.samba.enabled = true;
        // If there are shares, we could update the first one as an example
        if let Some(first_share) = shares.first() {
            settings.samba.share_name = first_share.name.clone();
            settings.samba.share_path = PathBuf::from(&first_share.path);
            settings.samba.read_only = first_share.read_only;
            settings.samba.guest_ok = first_share.guest_ok;
        }
    }

    // Use the new services architecture to generate the configuration
    let settings = state.settings.read().await;
    let service_manager = ServiceManager::new(settings.clone(), state.db_pool.clone());

    // Restart the Samba service using the new architecture
    // The generate_config is called internally by reload()
    service_manager
        .samba
        .reload()
        .await
        .map_err(|e| AppError::Command(format!("Failed to restart Samba service: {}", e)))?;

    Ok("Samba server configured successfully".to_string())
}
