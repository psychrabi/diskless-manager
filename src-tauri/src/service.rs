use crate::config::{get_config, set_config, write_config, Config};
use crate::utils::run_command;
use crate::DHCP_CONFIG_PATH;
use crate::DHCP_CLIENTS_PATH;
use crate::TFTP_AUTOEXEC_PATH;
use async_process::Command as AsyncCommand;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Command;

#[derive(Deserialize)]
pub struct ServiceControlRequest {
    pub action: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PackageStatus {
    name: String,
    service: String,
    installed: bool,
    configured: bool,
    running: bool,
    version: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InstallationProgress {
    package: String,
    status: String,
    progress: u8,
    message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DHCPLease {
    ip: String,
    mac: String,
    hostname: String,
    lease_time: String,
    status: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DHCPConfig {
    pub subnet_ip: String,
    pub start_ip: String,
    pub end_ip: String,
    pub subnet_mask: String,
    pub gateway_ip: String,
    pub dns_server1: String,
    pub dns_server2: String,
    pub broadcast_ip: String,
    pub next_server_ip: String,
    pub boot_server_ip: String,
    pub boot_script: String,
    pub boot_file_legacy: String,
    pub boot_file_uefi32: String,
    pub boot_file_uefi64: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TFTPConfig {
    pub tftp_root: String,
    pub tftp_server_ip: String,
    pub tftp_options: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HTTPConfig {
    pub http_root: String,
    pub http_server_ip: String,
    pub http_server_port: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SambaShare {
    name: String,
    path: String,
    read_only: bool,
    guest_ok: bool,
}

#[tauri::command]
pub async fn get_services(token: String, zfs_pool: String) -> Result<Value, String> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;
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
        let output = AsyncCommand::new("systemctl")
            .args(["is-active", service_name])
            .output()
            .await
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
                "name": key,
                "service": service_name.trim_end_matches(".service"),
                "status": status
            }),
        );
    }
    let zfs_status = match AsyncCommand::new("zpool")
        .args(["status", &zfs_pool])
        .output()
        .await
    {
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
            "service": "zfs",
            "status": zfs_status
        }),
    );

    // --- Update config.json with the new statuses using config cache ---
    let mut config: Config = get_config();
    config.services = serde_json::to_value(&statuses).unwrap_or(json!({}));
    if let Err(e) = write_config(&config) {
        println!("Error writing services status to config: {}", e);
        // Optionally: return an error here if you want to fail the command
    } else {
        set_config(&config); // update cache after write
    }

    Ok(serde_json::to_value(statuses).unwrap())
}

#[tauri::command]
pub async fn get_service_config(token: String, service_key: String) -> Result<serde_json::Value, String> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;
    // Map service keys to config file paths
    let config_file_map: HashMap<&str, &str> = [
        ("isc-dhcp-server", DHCP_CONFIG_PATH),
        // DHCP static client leases (separate include file)
        ("dhcp-clients", DHCP_CLIENTS_PATH),
        // TFTP autoexec (IPXE) editable file
        ("tftp-autoexec", TFTP_AUTOEXEC_PATH),
        ("tftpd-hpa", "/etc/default/tftpd-hpa"),
        ("apache2", "/etc/apache2/sites-available/000-default.conf"),
        ("smbd", "/etc/samba/smb.conf"),
        // Add more as needed
    ]
    .iter()
    .cloned()
    .collect();

    if service_key == "zfs" {
        // Get ZFS pool and dataset info
        let zpool_status = AsyncCommand::new("sudo")
            .args(["zpool", "status"])
            .output()
            .await
            .map_err(|e| format!("Failed to run zpool status: {e}"))?;
        let zpool_status_str = String::from_utf8_lossy(&zpool_status.stdout);

        let zfs_list = AsyncCommand::new("sudo")
            .args([
                "zfs",
                "list",
                "-t",
                "all",
                "-o",
                "name,type,used,avail,refer,mountpoint",
            ])
            .output()
            .await
            .map_err(|e| format!("Failed to run zfs list: {e}"))?;
        let zfs_list_str = String::from_utf8_lossy(&zfs_list.stdout);

        let content = format!(
            "=== ZFS Pool Status ===\n{}\n\n=== ZFS Datasets ===\n{}",
            zpool_status_str, zfs_list_str
        );
        Ok(serde_json::json!({ "text": content }))
    } else if service_key == "target" {
        // Only return the output of 'sudo targetcli ls'
        let iscsi_output = AsyncCommand::new("sudo")
            .args(["targetcli", "ls"])
            .output()
            .await
            .map_err(|e| format!("Failed to get targetcli ls: {e}"))?;

        let iscsi_config = String::from_utf8_lossy(&iscsi_output.stdout);
        let content = format!("=== TargetCLI Config ===\n\n\n{}", iscsi_config);
        Ok(serde_json::json!({ "text": content }))
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

        // Read using sudo cat to avoid direct Rust file I/O
        let output = AsyncCommand::new("sudo")
            .args(["cat", config_path])
            .output()
            .await
            .map_err(|e| format!("Failed to read config via sudo cat: {}", e))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }
        let content = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(serde_json::json!({ "text": content }))
    }
}

#[tauri::command]
pub async fn control_service(
    token: String,
    service_key: String,
    req: ServiceControlRequest,
) -> Result<Value, String> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;
    let service_map: HashMap<&str, &str> = [
        ("target", "target.service"),
        ("isc-dhcp-server", "isc-dhcp-server.service"),
        ("tftpd-hpa", "tftpd-hpa.service"),
        ("apache2", "apache2.service"),
        ("smbd", "smbd.service"),
    ]
    .iter()
    .cloned()
    .collect();
    let Some(&service_name) = service_map.get(service_key.as_str()) else {
        return Err(format!("Unknown service key: {}", service_key));
    };

    run_command(&["systemctl", &req.action, service_name])?;

    Ok(json!({
        "message": format!("Service '{}' {} command issued successfully.", service_name, &req.action)
    }))
}

#[tauri::command]
pub async fn check_services() -> Result<Value, String> {
    let required = vec![
        ("zfs", "zfsutils-linux"),
        ("targetcli", "targetcli-fb"),
        ("dhcpd", "isc-dhcp-server"),
        ("in.tftpd", "tftpd-hpa"),
        ("apache2", "apache2"),
        ("smbd", "samba"),
        ("wakeonlan", "wakeonlan"),
        // ("zfsutils-linux", "zfsutils-linux"),
    ];
    let mut statuses = HashMap::new();
    for (key, svc) in required {
        let installed = AsyncCommand::new("which")
            .arg(key)
            .output()
            .await
            .map(|s| s.status.success())
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
pub async fn install_service(service: String) -> Result<(), String> {
    let status = AsyncCommand::new("sudo")
        .args(&["apt-get", "install", "-y", &service])
        .status()
        .await
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("Failed to install {}", service))
    }
}

#[tauri::command]
pub async fn save_service_config(token: String, service_key: String, content: String) -> Result<(), String> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;
    let config_file_map: HashMap<&str, &str> = [
        ("isc-dhcp-server", DHCP_CONFIG_PATH),
        ("dhcp-clients", DHCP_CLIENTS_PATH),
        ("tftp-autoexec", TFTP_AUTOEXEC_PATH),
        ("tftpd-hpa", "/etc/default/tftpd-hpa"),
        ("target", "/etc/rtslib-fb-target/saveconfig.json"),
        ("apache2", "/etc/apache2/sites-available/000-default.conf"),
        ("smbd", "/etc/samba/smb.conf"),
        // Add more as needed
    ]
    .iter()
    .cloned()
    .collect();

    let config_path = config_file_map
        .get(service_key.as_str())
        .ok_or_else(|| format!("Unknown service key: {}", service_key))?;

    // Write using sudo tee for protected files
    let mut child = Command::new("sudo")
        .arg("tee")
        .arg(config_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn sudo tee: {}", e))?;

    if let Some(stdin) = child.stdin.as_mut() {
        use std::io::Write;
        stdin
            .write(content.as_bytes())
            .map_err(|e| format!("Failed to write to stdin: {}", e))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait for tee: {}", e))?;
    if !output.status.success() {
        return Err(format!(
            "Failed to write config: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

#[tauri::command]
pub async fn check_package_status() -> Result<Vec<PackageStatus>, String> {
    let packages = vec![
        ("isc-dhcp-server", "isc-dhcp-server"),
        ("tftpd-hpa", "tftpd-hpa"),
        ("target", "targetcli-fb"),
        ("apache2", "apache2"),
        ("smbd", "samba"),
        ("wakeonlan", "wakeonlan"),
        ("zfsutils-linux", "zfsutils-linux"),
    ];

    let mut status_list = Vec::new();

    for (service, package) in packages {
        let installed = check_package_installed(package).await;
        let running = if installed {
            check_service_running(service).await
        } else {
            false
        };
        let version = if installed {
            get_package_version(package).await
        } else {
            None
        };

        status_list.push(PackageStatus {
            name: package.to_string(),
            service: service.to_string(),
            installed,
            configured: running, // Simplified - running implies configured
            running,
            version,
        });
    }

    Ok(status_list)
}

async fn check_package_installed(package: &str) -> bool {
    match AsyncCommand::new("dpkg")
        .args(&["-l", package])
        .output()
        .await
    {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

async fn check_service_running(service: &str) -> bool {
    let service_name = match service {
        "isc-dhcp-server" => "isc-dhcp-server",
        "tftpd-hpa" => "tftpd-hpa",
        "apache2" => "apache2",
        "samba" => "smbd",

        _ => service,
    };

    match AsyncCommand::new("systemctl")
        .args(&["is-active", "--quiet", service_name])
        .output()
        .await
    {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

async fn get_package_version(package: &str) -> Option<String> {
    match AsyncCommand::new("dpkg-query")
        .args(&["-W", "-f=${Version}", package])
        .output()
        .await
    {
        Ok(output) => {
            if output.status.success() {
                String::from_utf8(output.stdout).ok()
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

#[tauri::command]
pub async fn install_packages() -> Result<String, String> {
    // Update package list first
    let update_result = AsyncCommand::new("sudo")
        .args(&["apt", "update"])
        .output()
        .await;

    if update_result.is_err() {
        return Err("Failed to update package list".to_string());
    }

    // Install all required packages
    let packages = vec![
        "isc-dhcp-server",
        "tftpd-hpa",
        "targetcli-fb",
        "apache2",
        "samba",
        "samba-common-bin",
    ];

    let mut install_cmd = AsyncCommand::new("sudo");
    install_cmd.args(&["apt", "install", "-y"]);
    for package in &packages {
        install_cmd.arg(package);
    }

    match install_cmd.output().await {
        Ok(output) => {
            if output.status.success() {
                Ok("All packages installed successfully".to_string())
            } else {
                let error = String::from_utf8_lossy(&output.stderr);
                Err(format!("Installation failed: {}", error))
            }
        }
        Err(e) => Err(format!("Failed to run installation: {}", e)),
    }
}

#[tauri::command]
pub async fn restart_service(service: &str) -> Result<(), String> {
    match AsyncCommand::new("sudo")
        .args(&["systemctl", "restart", service])
        .output()
        .await
    {
        Ok(output) => {
            if output.status.success() {
                Ok(())
            } else {
                let error = String::from_utf8_lossy(&output.stderr);
                Err(format!("Failed to restart {}: {}", service, error))
            }
        }
        Err(e) => Err(format!("Failed to restart {}: {}", service, e)),
    }
}

#[tauri::command]
pub async fn configure_dhcp_server(token: String, config: DHCPConfig) -> Result<String, String> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;

    // Save to config
    let mut cfg = crate::config::read_config();
    let mut settings = cfg.settings.as_object().cloned().unwrap_or_default();
    settings.insert("dhcp".to_string(), serde_json::to_value(&config).map_err(|e| e.to_string())?);
    cfg.settings = serde_json::Value::Object(settings);
    crate::config::write_config(&cfg).map_err(|e| format!("Failed to save DHCP config: {}", e))?;

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

on commit {{
  set clip = binary-to-ascii(10, 8, ".", leased-address);
  set clmac = concat(
    suffix(concat("0", binary-to-ascii(16, 8, "", substring(hardware, 1, 1))), 2), ":",
    suffix(concat("0", binary-to-ascii(16, 8, "", substring(hardware, 2, 1))), 2), ":",
    suffix(concat("0", binary-to-ascii(16, 8, "", substring(hardware, 3, 1))), 2), ":",
    suffix(concat("0", binary-to-ascii(16, 8, "", substring(hardware, 4, 1))), 2), ":",
    suffix(concat("0", binary-to-ascii(16, 8, "", substring(hardware, 5, 1))), 2), ":",
    suffix(concat("0", binary-to-ascii(16, 8, "", substring(hardware, 6, 1))), 2)
  );
  execute("/usr/local/bin/provision_client.sh", clmac);
}}

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
        config.boot_file_uefi64,
    );

    // Write with sudo tee instead of fs::write
    let mut child = Command::new("sudo")
        .arg("tee")
        .arg(DHCP_CONFIG_PATH)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn sudo tee: {}", e))?;
    if let Some(stdin) = child.stdin.as_mut() {
        use std::io::Write;
        stdin
            .write(dhcp_config.as_bytes())
            .map_err(|e| format!("Failed to write to stdin: {}", e))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait for tee: {}", e))?;
    if !output.status.success() {
        return Err(format!(
            "Failed to write DHCP config: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    // Restart DHCP service
    restart_service("isc-dhcp-server").await?;
    Ok("DHCP server configured successfully".to_string())
}

#[tauri::command]
pub async fn configure_tftp_server(token: String, tftp_config: TFTPConfig) -> Result<String, String> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;

    // Save to config
    let mut cfg = crate::config::read_config();
    let mut settings = cfg.settings.as_object().cloned().unwrap_or_default();
    settings.insert("tftp".to_string(), serde_json::to_value(&tftp_config).map_err(|e| e.to_string())?);
    cfg.settings = serde_json::Value::Object(settings);
    crate::config::write_config(&cfg).map_err(|e| format!("Failed to save TFTP config: {}", e))?;

    let tftp_content = format!(
        r#"# Defaults for tftpd-hpa
TFTP_USERNAME="tftp"
TFTP_DIRECTORY="{}"
TFTP_ADDRESS="{}:69"
TFTP_OPTIONS="{}"
"#,
        tftp_config.tftp_root, tftp_config.tftp_server_ip, tftp_config.tftp_options
    );

    // Create TFTP directory if it doesn't exist
    if let Err(e) = std::fs::create_dir_all(&tftp_config.tftp_root) {
        return Err(format!("Failed to create TFTP directory: {}", e));
    }

    // Write with sudo tee instead of fs::write
    let mut child = Command::new("sudo")
        .arg("tee")
        .arg("/etc/default/tftpd-hpa")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn sudo tee: {}", e))?;
    if let Some(stdin) = child.stdin.as_mut() {
        use std::io::Write;
        stdin
            .write(tftp_content.as_bytes())
            .map_err(|e| format!("Failed to write to stdin: {}", e))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait for tee: {}", e))?;
    if !output.status.success() {
        return Err(format!(
            "Failed to write TFTP config: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    restart_service("tftpd-hpa").await?;
    Ok("TFTP server configured successfully".to_string())
}

#[tauri::command]
pub async fn configure_apache_server(token: String, http_config: HTTPConfig) -> Result<String, String> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;

    // Save to config
    let mut cfg = crate::config::read_config();
    let mut settings = cfg.settings.as_object().cloned().unwrap_or_default();
    settings.insert("http".to_string(), serde_json::to_value(&http_config).map_err(|e| e.to_string())?);
    cfg.settings = serde_json::Value::Object(settings);
    crate::config::write_config(&cfg).map_err(|e| format!("Failed to save HTTP config: {}", e))?;

    let apache_config = format!(
        r#"<VirtualHost {}:{}>
    DocumentRoot {}
    ServerName diskless-server
    
    <Directory {}>
        Options Indexes FollowSymLinks
        AllowOverride None
        Require all granted
    </Directory>
    
    # Enable directory browsing for boot files
    <Directory {}/boot>
        Options +Indexes
        IndexOptions +FancyIndexing
    </Directory>
</VirtualHost>
"#,
        http_config.http_server_ip, http_config.http_server_port, http_config.http_root, http_config.http_root, http_config.http_root
    );

    // Create HTTP directory if it doesn't exist
    if let Err(e) = std::fs::create_dir_all(&http_config.http_root) {
        return Err(format!("Failed to create HTTP directory: {}", e));
    }

    // Write with sudo tee instead of fs::write
    let mut child = Command::new("sudo")
        .arg("tee")
        .arg("/etc/apache2/sites-available/diskless-server.conf")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn sudo tee: {}", e))?;
    if let Some(stdin) = child.stdin.as_mut() {
        use std::io::Write;
        stdin
            .write(apache_config.as_bytes())
            .map_err(|e| format!("Failed to write to stdin: {}", e))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait for tee: {}", e))?;
    if !output.status.success() {
        return Err(format!(
            "Failed to write Apache config: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    // Enable the site and restart Apache
    let _ = AsyncCommand::new("sudo")
        .args(&["a2ensite", "diskless-server.conf"]) 
        .output()
        .await;

    restart_service("apache2").await?;
    Ok("Apache server configured successfully".to_string())
}

#[tauri::command]
pub async fn configure_samba_server(token: String, shares: Vec<SambaShare>) -> Result<String, String> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;
    let mut samba_config = String::from(
        r#"[global]
   workgroup = WORKGROUP
   server string = Diskless Boot Server
   netbios name = DISKLESS-SERVER
   security = user
   map to guest = bad user
   dns proxy = no

"#,
    );

    for share in shares {
        samba_config.push_str(&format!(
            r#"[{}]
   path = {}
   browseable = yes
   read only = {}
   guest ok = {}
   create mask = 0644
   directory mask = 0755

"#,
            share.name,
            share.path,
            if share.read_only { "yes" } else { "no" },
            if share.guest_ok { "yes" } else { "no" }
        ));

        // Create share directory if it doesn't exist
        if let Err(e) = std::fs::create_dir_all(&share.path) {
            return Err(format!(
                "Failed to create share directory {}: {}",
                share.path, e
            ));
        }
    }

    // Write with sudo tee instead of fs::write
    let mut child = Command::new("sudo")
        .arg("tee")
        .arg("/etc/samba/smb.conf")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn sudo tee: {}", e))?;
    if let Some(stdin) = child.stdin.as_mut() {
        use std::io::Write;
        stdin
            .write(samba_config.as_bytes())
            .map_err(|e| format!("Failed to write to stdin: {}", e))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait for tee: {}", e))?;
    if !output.status.success() {
        return Err(format!(
            "Failed to write Samba config: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    restart_service("smbd").await?;
    restart_service("nmbd").await?;
    Ok("Samba server configured successfully".to_string())
}
