use crate::config::{get_config, write_config};
use crate::core::config::{DhcpConfig, HttpConfig, SambaConfig, TftpConfig};
use crate::error::AppError;

use crate::services::ServiceManager;
use crate::state::AppState;
use crate::{DHCP_CLIENTS_PATH, DHCP_CONFIG_PATH, TFTP_AUTOEXEC_PATH};
use async_process::Command as AsyncCommand;
use futures::io::AsyncWriteExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tauri::State;

use crate::middleware::validate_auth;

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

#[expect(dead_code, reason = "Old Tauri command replaced by new services architecture")]
pub async fn install_service(service: String, token: String) -> Result<(), AppError> {
    validate_auth(&token)?;

    match Command::new("sudo")
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

#[expect(dead_code, reason = "Old Tauri command replaced by new services architecture")]
pub async fn save_service_config(
    token: String,
    service_key: String,
    content: String,
) -> Result<(), AppError> {
    validate_auth(&token)?;

    let config_file_map = [
        ("dhcp", DHCP_CONFIG_PATH),
        ("dhcp-clients", DHCP_CLIENTS_PATH),
        ("tftp-autoexec", TFTP_AUTOEXEC_PATH),
        ("tftp", "/etc/default/tftpd-hpa"),
        ("http", "/etc/apache2/sites-available/diskless-server.conf"),
        ("samba", "/etc/samba/smb.conf"),
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

#[expect(dead_code, reason = "Old Tauri command replaced by new services architecture")]
pub async fn configure_dhcp_server(
    state: State<'_, AppState>,
    token: String,
    config: DhcpConfig,
) -> Result<String, AppError> {
    validate_auth(&token)?;

    let dhcp_config = format!(
        r#"# Global Config
option space ipxe;
option ipxe-encap-opts code 175 = encapsulate ipxe;
option ipxe.priority code 1 = signed integer 8;
option ipxe.keep-san code 8 = unsigned integer 8;
option ipxe.skip-san-boot code 9 = unsigned integer 8;
option ipxe.syslogs code 85 = string;
option ipxe.cert code 91 = string;
option ipxe.privkey code 92 = string;
option ipxe.crosscert code 93 = string;
option ipxe.no-pxedhcp code 176 = unsigned integer 8;
option ipxe.bus-id code 177 = string;
option ipxe.san-filename code 188 = string;
option ipxe.bios-drive code 189 = unsigned integer 8;
option ipxe.username code 190 = string;
option ipxe.password code 191 = string;
option ipxe.reverse-username code 192 = string;
option ipxe.reverse-password code 193 = string;
option ipxe.version code 235 = string;
option iscsi-initiator-iqn code 203 = string;
# Feature indicators
option ipxe.pxeext code 16 = unsigned integer 8;
option ipxe.iscsi code 17 = unsigned integer 8;
option ipxe.aoe code 18 = unsigned integer 8;
option ipxe.http code 19 = unsigned integer 8;
option ipxe.https code 20 = unsigned integer 8;
option ipxe.tftp code 21 = unsigned integer 8;
option ipxe.ftp code 22 = unsigned integer 8;
option ipxe.dns code 23 = unsigned integer 8;
option ipxe.bzimage code 24 = unsigned integer 8;
option ipxe.multiboot code 25 = unsigned integer 8;
option ipxe.slam code 26 = unsigned integer 8;
option ipxe.srp code 27 = unsigned integer 8;
option ipxe.nbi code 32 = unsigned integer 8;
option ipxe.pxe code 33 = unsigned integer 8;
option ipxe.elf code 34 = unsigned integer 8;
option ipxe.comboot code 35 = unsigned integer 8;
option ipxe.efi code 36 = unsigned integer 8;
option ipxe.fcoe code 37 = unsigned integer 8;
option ipxe.vlan code 38 = unsigned integer 8;
option ipxe.menu code 39 = unsigned integer 8;
option ipxe.sdi code 40 = unsigned integer 8;
option ipxe.nfs code 41 = unsigned integer 8;
option client-architecture code 93 = unsigned integer 16;
option ipxe.no-pxedhcp 1;

# DHCP Server Configuration
default-lease-time 86400;
max-lease-time 86400;
authoritative;
allow booting;
allow bootp;
one-lease-per-client true;

# Define a class for PXE clients
class "pxeclients" {{
  match if substring(option vendor-class-identifier, 0, 9) = "PXEClient";
}}

#on commit {{
#  set clip = binary-to-ascii(10, 8, ".", leased-address);
#  set clmac = concat(
#    suffix(concat("0", binary-to-ascii(16, 8, "", substring(hardware, 1, 1))), 2), ":",
#    suffix(concat("0", binary-to-ascii(16, 8, "", substring(hardware, 2, 1))), 2), ":",
#    suffix(concat("0", binary-to-ascii(16, 8, "", substring(hardware, 3, 1))), 2), ":",
#    suffix(concat("0", binary-to-ascii(16, 8, "", substring(hardware, 4, 1))), 2), ":",
#    suffix(concat("0", binary-to-ascii(16, 8, "", substring(hardware, 5, 1))), 2), ":",
#    suffix(concat("0", binary-to-ascii(16, 8, "", substring(hardware, 6, 1))), 2)
#  );
#  execute("/usr/bin/diskless-manager", "auto-add-client", "--mac", clmac, "--ip", clip);
#}}

# DHCP Configuration
subnet {} netmask {} {{
    # Only hand out dynamic leases to PXE clients
    pool {{
        allow members of "pxeclients";
        range {} {};
    }}
    option routers {};
    option domain-name-servers {},{};
    option broadcast-address {};

    # PXE Boot Configuration
    next-server {};
    if exists user-class and option user-class = "iPXE" {{
        filename "http://{}/{}";
    }} elsif option client-architecture = 00:00 {{
        filename "{}";
    }} elsif option client-architecture = 00:06 {{
        filename "{}";
    }} elsif option client-architecture = 00:07 {{
        filename "{}";
    }}
}}

# Static leases will be added here
include "/etc/dhcp/clients.conf";
"#,
        config.subnet_ip,
        config.subnet_mask,
        config.start_ip,
        config.end_ip,
        config.gateway_ip,
        config.dns_server1,
        config.dns_server2,
        config.broadcast_ip,
        config.next_server_ip,
        config.boot_server_ip,
        config.boot_script,
        config.boot_file_legacy,
        config.boot_file_uefi32,
        config.boot_file_uefi64
    );

    write_with_sudo_tee(DHCP_CONFIG_PATH, &dhcp_config).await?;

    // Update the settings in the app state
    {
        let mut settings = state.settings.write().await;
        settings.dhcp.start_ip = config.start_ip.clone();
        settings.dhcp.end_ip = config.end_ip.clone();
        settings.dhcp.subnet_mask = config.subnet_mask.clone();
        settings.dhcp.gateway_ip = config.gateway_ip.clone();
        settings.dhcp.dns_server1 = config.dns_server1.clone();
        settings.dhcp.dns_server2 = config.dns_server2.clone();
        // Enable DHCP service in settings
        settings.dhcp.enabled = config.enabled;
    }

    // Update the settings in the database
    let current_config = get_config();
    let mut new_config = current_config;
    new_config.settings = serde_json::to_value(&*state.settings.read().await)
        .map_err(|e| AppError::Config(format!("Failed to serialize settings: {}", e)))?;

    write_config(&state.db_pool, &new_config)
        .await
        .map_err(|e| AppError::Config(format!("Failed to write config to database: {}", e)))?;

    Ok("DHCP server configured successfully".to_string())
}

#[expect(dead_code, reason = "Old Tauri command replaced by new services architecture")]
pub async fn configure_tftp_server(
    state: State<'_, AppState>,
    token: String,
    tftp_config: TftpConfig,
) -> Result<String, AppError> {
    validate_auth(&token)?;

    // Update the settings in the app state
    {
        let mut settings = state.settings.write().await;
        settings.tftp.root_dir = PathBuf::from(&tftp_config.root_dir).display().to_string();
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

    // Update the settings in the database
    let current_config = get_config();
    let mut new_config = current_config;
    new_config.settings = serde_json::to_value(&*state.settings.read().await)
        .map_err(|e| AppError::Config(format!("Failed to serialize settings: {}", e)))?;

    write_config(&state.db_pool, &new_config)
        .await
        .map_err(|e| AppError::Config(format!("Failed to write config to database: {}", e)))?;

    Ok("TFTP server configured successfully".to_string())
}

#[expect(dead_code, reason = "Old Tauri command replaced by new services architecture")]
pub async fn configure_apache_server(
    state: State<'_, AppState>,
    token: String,
    http_config: HttpConfig,
) -> Result<String, AppError> {
    validate_auth(&token)?;

    // Update the settings in the app state
    {
        let mut settings = state.settings.write().await;
        // Update the HTTP service settings
        settings.http.root_dir = PathBuf::from(&http_config.root_dir).display().to_string();
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

    // Update the settings in the database
    let current_config = get_config();
    let mut new_config = current_config;
    new_config.settings = serde_json::to_value(&*state.settings.read().await)
        .map_err(|e| AppError::Config(format!("Failed to serialize settings: {}", e)))?;

    write_config(&state.db_pool, &new_config)
        .await
        .map_err(|e| AppError::Config(format!("Failed to write config to database: {}", e)))?;

    Ok("Apache server configured successfully".to_string())
}

#[expect(dead_code, reason = "Old Tauri command replaced by new services architecture")]
pub async fn configure_samba_server(
    state: State<'_, AppState>,
    token: String,
    shares: Vec<SambaConfig>,
) -> Result<String, AppError> {
    validate_auth(&token)?;

    // Update the settings in the app state
    // Note: Samba shares are not directly stored in Settings as individual shares
    // Instead, we'll update the general Samba configuration
    {
        let mut settings = state.settings.write().await;
        // For now, we'll just enable Samba
        settings.samba.enabled = true;
        // If there are shares, we could update the first one as an example
        if let Some(first_share) = shares.first() {
            settings.samba.share_name = first_share.share_name.clone();
            settings.samba.share_path = PathBuf::from(&first_share.share_path);
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

    // Update the settings in the database
    let current_config = get_config();
    let mut new_config = current_config;
    new_config.settings = serde_json::to_value(&*state.settings.read().await)
        .map_err(|e| AppError::Config(format!("Failed to serialize settings: {}", e)))?;

    write_config(&state.db_pool, &new_config)
        .await
        .map_err(|e| AppError::Config(format!("Failed to write config to database: {}", e)))?;

    Ok("Samba server configured successfully".to_string())
}

#[expect(dead_code, reason = "Old Tauri command replaced by new services architecture")]
pub async fn configure_nfs_server(
    state: State<'_, AppState>,
    token: String,
    exports_dir: String,
) -> Result<String, AppError> {
    validate_auth(&token)?;

    // Update the settings in the app state
    {
        let mut settings = state.settings.write().await;
        settings.nfs.exports_dir = PathBuf::from(&exports_dir);
        settings.nfs.enabled = true;
    }

    // Use the new services architecture to generate the configuration
    let settings = state.settings.read().await;
    let service_manager = ServiceManager::new(settings.clone(), state.db_pool.clone());

    // Generate the NFS configuration
    service_manager
        .nfs
        .generate_config()
        .await
        .map_err(|e| AppError::Command(format!("Failed to generate NFS config: {}", e)))?;

    // Restart the NFS service using the new architecture
    service_manager
        .nfs
        .reload()
        .await
        .map_err(|e| AppError::Command(format!("Failed to restart NFS service: {}", e)))?;

    // Update the settings in the database
    let current_config = get_config();
    let mut new_config = current_config;
    new_config.settings = serde_json::to_value(&*state.settings.read().await)
        .map_err(|e| AppError::Config(format!("Failed to serialize settings: {}", e)))?;

    write_config(&state.db_pool, &new_config)
        .await
        .map_err(|e| AppError::Config(format!("Failed to write config to database: {}", e)))?;

    Ok("NFS server configured successfully".to_string())
}
