use crate::config::{get_config, set_config, write_config, reload_config_from_disk, Config};
use crate::service;
use crate::utils::run_command;
use async_process::Command as AsyncCommand;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
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
struct InstallationProgress {
    package: String,
    status: String,
    progress: u8,
    message: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct DHCPLease {
    ip: String,
    mac: String,
    hostname: String,
    lease_time: String,
    status: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ServiceConfig {
    dhcp_range_start: String,
    dhcp_range_end: String,
    subnet: String,
    netmask: String,
    gateway: String,
    dns_servers: Vec<String>,
    tftp_root: String,
    http_root: String,
    samba_shares: Vec<SambaShare>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SambaShare {
    name: String,
    path: String,
    read_only: bool,
    guest_ok: bool,
}


#[tauri::command]
pub async fn get_services(zfs_pool: String) -> Result<Value, String> {
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
pub async fn get_service_config(service_key: String) -> Result<serde_json::Value, String> {
    // Map service keys to config file paths
    let config_file_map: HashMap<&str, &str> = [
        ("isc-dhcp-server", "/etc/dhcp/dhcpd.conf"),
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

        let content = fs::read_to_string(config_path)
            .map_err(|e| format!("Error reading config file {}: {}", config_path, e))?;

        Ok(serde_json::json!({ "text": content }))
    }
}

#[tauri::command]
pub async fn control_service(
    service_key: String,
    req: ServiceControlRequest,
) -> Result<Value, String> {
    let service_map: HashMap<&str, &str> = [
        ("iscsi", "target.service"),
        ("dhcp", "isc-dhcp-server.service"),
        ("tftp", "tftpd-hpa.service"),
        ("http", "apache2.service"),
        ("share", "smbd.service"),
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
pub async fn save_service_config(service_key: String, content: String) -> Result<(), String> {
    let config_file_map: HashMap<&str, &str> = [
        ("dhcp", "/etc/dhcp/dhcpd.conf"),
        ("tftp", "/etc/default/tftpd-hpa"),
        ("iscsi", "/etc/rtslib-fb-target/saveconfig.json"),
        ("http", "/etc/apache2/sites-available/000-default.conf"),
        ("share", "/etc/samba/smb.conf"),
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
        stdin
            .write_all(content.as_bytes())
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
                // ("wakeonlan", "wakeonlan"),
                // ("zfsutils-linux", "zfsutils-linux"),
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
async fn configure_dhcp_server(config: ServiceConfig) -> Result<String, String> {
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

option ipxe.no-pxedhcp 1;

# DHCP Server Configuration
default-lease-time 600;
max-lease-time 7200;
authoritative;
allow booting;
allow bootp;

# Define a class for PXE clients
class "pxeclients" {{
  match if substring(option vendor-class-identifier, 0, 9) = "PXEClient";
}}

# DHCP Configuration
subnet {} netmask {} {{
    # Only hand out dynamic leases to PXE clients
    pool {{
        allow members of "pxeclients";
        range {} {};
    }}    
    option routers {};
    option domain-name-servers {};
    option broadcast-address {};
    
    # PXE Boot Configuration
    next-server {};
      if substring (option vendor-class-identifier, 15, 5) = "00000" {{
        filename "ipxe.kpxe";
    }}
    elsif substring (option vendor-class-identifier, 15, 5) = "00006" {{
        filename "ipxe32.efi";
    }}
    else {{
        filename "snponly.efi";
    }}
}}

# Static leases will be added here
"#,
        config.subnet,
        config.netmask,
        config.dhcp_range_start,
        config.dhcp_range_end,
        config.gateway,
        config.dns_servers.join(", "),
        calculate_broadcast(&config.subnet, &config.netmask),
        get_server_ip().await.unwrap_or("192.168.1.1".to_string())
    );

    match fs::write("/etc/dhcp/dhcpd.conf", dhcp_config) {
        Ok(_) => {
            // Restart DHCP service
            restart_service("isc-dhcp-server").await?;
            Ok("DHCP server configured successfully".to_string())
        }
        Err(e) => Err(format!("Failed to write DHCP config: {}", e)),
    }
}

#[tauri::command]
async fn configure_tftp_server(tftp_root: String) -> Result<String, String> {
    let tftp_config = format!(
        r#"# Defaults for tftpd-hpa
TFTP_USERNAME="tftp"
TFTP_DIRECTORY="{}"
TFTP_ADDRESS="0.0.0.0:69"
TFTP_OPTIONS="--secure"
"#,
        tftp_root
    );

    // Create TFTP directory if it doesn't exist
    if let Err(e) = fs::create_dir_all(&tftp_root) {
        return Err(format!("Failed to create TFTP directory: {}", e));
    }

    match fs::write("/etc/default/tftpd-hpa", tftp_config) {
        Ok(_) => {
            restart_service("tftpd-hpa").await?;
            Ok("TFTP server configured successfully".to_string())
        }
        Err(e) => Err(format!("Failed to write TFTP config: {}", e)),
    }
}

#[tauri::command]
async fn configure_apache_server(http_root: String) -> Result<String, String> {
    let apache_config = format!(
        r#"<VirtualHost *:80>
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
        http_root, http_root, http_root
    );

    // Create HTTP directory if it doesn't exist
    if let Err(e) = fs::create_dir_all(&http_root) {
        return Err(format!("Failed to create HTTP directory: {}", e));
    }

    match fs::write(
        "/etc/apache2/sites-available/diskless-server.conf",
        apache_config,
    ) {
        Ok(_) => {
            // Enable the site and restart Apache
            let _ = AsyncCommand::new("sudo")
                .args(&["a2ensite", "diskless-server.conf"])
                .output()
                .await;

            restart_service("apache2").await?;
            Ok("Apache server configured successfully".to_string())
        }
        Err(e) => Err(format!("Failed to write Apache config: {}", e)),
    }
}

#[tauri::command]
async fn configure_samba_server(shares: Vec<SambaShare>) -> Result<String, String> {
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
        if let Err(e) = fs::create_dir_all(&share.path) {
            return Err(format!(
                "Failed to create share directory {}: {}",
                share.path, e
            ));
        }
    }

    match fs::write("/etc/samba/smb.conf", samba_config) {
        Ok(_) => {
            restart_service("smbd").await?;
            restart_service("nmbd").await?;
            Ok("Samba server configured successfully".to_string())
        }
        Err(e) => Err(format!("Failed to write Samba config: {}", e)),
    }
}
async fn get_server_ip() -> Result<String, String> {
    match AsyncCommand::new("hostname").args(&["-I"]).output().await {
        Ok(output) => {
            let ip_list = String::from_utf8_lossy(&output.stdout);
            let first_ip = ip_list.split_whitespace().next().unwrap_or("192.168.1.1");
            Ok(first_ip.to_string())
        }
        Err(_) => Ok("192.168.1.1".to_string()),
    }
}

fn calculate_broadcast(subnet: &str, _netmask: &str) -> String {
    // Simplified broadcast calculation - in production, use proper IP calculation
    if subnet.starts_with("192.168.1") {
        "192.168.1.255".to_string()
    } else {
        format!("{}.255", &subnet[..subnet.rfind('.').unwrap_or(0)])
    }
}
