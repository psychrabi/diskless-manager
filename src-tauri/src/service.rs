use crate::cmd::{append_log, run_command_async, run_command_output, run_command_output_no_sudo};
use crate::config::{get_config, set_config, write_config};
use crate::error::AppError;
use crate::middleware::validate_auth_token_for_command;
use crate::types::service::SambaShare;
use crate::types::{DHCPConfig, HTTPConfig, PackageStatus, ServiceControlRequest, TFTPConfig};
use crate::{DHCP_CLIENTS_PATH, DHCP_CONFIG_PATH, TFTP_AUTOEXEC_PATH};
use async_process::Command as AsyncCommand;
use futures::io::AsyncWriteExt;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::sync::RwLock;

// Cache for system services information to reduce system calls
use once_cell::sync::Lazy;

static SERVICES_CACHE: Lazy<Arc<RwLock<ServicesCache>>> =
    Lazy::new(|| Arc::new(RwLock::new(ServicesCache::new())));

#[derive(Debug, Clone)]
struct ServicesCache {
    service_statuses: HashMap<String, Value>,
    package_statuses: Vec<PackageStatus>,
    last_updated: std::time::SystemTime,
    ttl: std::time::Duration,
}

impl ServicesCache {
    fn new() -> Self {
        ServicesCache {
            service_statuses: HashMap::new(),
            package_statuses: Vec::new(),
            last_updated: std::time::SystemTime::UNIX_EPOCH,
            ttl: std::time::Duration::from_secs(30), // 30 second cache TTL
        }
    }

    fn is_fresh(&self) -> bool {
        self.last_updated
            .elapsed()
            .map(|elapsed| elapsed < self.ttl)
            .unwrap_or(false)
    }

    fn needs_refresh(&self) -> bool {
        !self.is_fresh()
    }
}

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

// Helper: Restart service async
async fn restart_service_async(service: &str) -> Result<(), AppError> {
    match run_command_async(["systemctl", "restart", service]).await {
        Ok(_) => Ok(()),
        Err(e) => Err(AppError::Command(format!(
            "Failed to restart {}: {}",
            service, e
        ))),
    }
}

// Helper: Check package installed
async fn check_package_installed(package: &str) -> Result<bool, AppError> {
    let output = AsyncCommand::new("dpkg-query")
        .args(["-W", "-f=${Status}", package])
        .output()
        .await
        .map_err(|e| AppError::Command(format!("dpkg-query failed for {}: {}", package, e)))?;

    Ok(output.status.success()
        && String::from_utf8_lossy(&output.stdout).trim() == "install ok installed")
}

// Helper: Check service running
async fn check_service_running(service: &str) -> Result<bool, AppError> {
    let service_name = match service {
        "isc-dhcp-server" => "isc-dhcp-server.service".to_string(),
        "tftpd-hpa" => "tftpd-hpa.service".to_string(),
        "apache2" => "apache2.service".to_string(),
        "samba" => "smbd.service".to_string(),
        _ => format!("{}.service", service),
    };

    let output = AsyncCommand::new("systemctl")
        .args(["is-active", "--quiet", &service_name])
        .output()
        .await
        .map_err(|e| AppError::Command(format!("systemctl check failed for {}: {}", service, e)))?;

    Ok(output.status.success())
}

// Helper: Get package version
async fn get_package_version(package: &str) -> Result<Option<String>, AppError> {
    let output = AsyncCommand::new("dpkg-query")
        .args(["-W", "-f=${Version}", package])
        .output()
        .await
        .map_err(|e| {
            AppError::Command(format!("dpkg-query version failed for {}: {}", package, e))
        })?;

    if output.status.success() {
        Ok(String::from_utf8(output.stdout)
            .ok()
            .map(|s| s.trim().to_string()))
    } else {
        Ok(None)
    }
}

// Helper: Save config section
fn save_config_section(key: &str, value: Value) -> Result<(), AppError> {
    let mut cfg = get_config();
    if let Some(settings) = cfg.settings.as_object_mut() {
        settings.insert(key.to_string(), value);
    } else {
        cfg.settings = json!({ (key): value });
    }
    write_config(&cfg).map_err(AppError::Config)?;
    set_config(&cfg);
    Ok(())
}

#[tauri::command]
pub async fn get_services(token: String, zfs_pool: String) -> Result<Value, AppError> {
    validate_token(&token)?;
    append_log("INFO", "get_services called");

    // Try to get cached data first
    {
        let cache = SERVICES_CACHE.read().await;

        if !cache.needs_refresh() {
            // Return cached data
            return serde_json::to_value(cache.service_statuses.clone())
                .map_err(|e| AppError::Internal(e.to_string()));
        }
    } // Drop read lock

    let mut cache = SERVICES_CACHE.write().await;

    // Double-check after acquiring write lock
    if !cache.needs_refresh() {
        // Another thread may have updated the cache while we were waiting
        return serde_json::to_value(cache.service_statuses.clone())
            .map_err(|e| AppError::Internal(e.to_string()));
    }

    let service_map = [
        ("iscsi", "rtslib-fb-targetctl.service"),
        ("dhcp", "isc-dhcp-server.service"),
        ("tftp", "tftpd-hpa.service"),
        ("http", "apache2.service"),
        ("share", "smbd.service"),
    ];

    let mut statuses = HashMap::new();
    for (key, service_name) in service_map {
        let output = AsyncCommand::new("systemctl")
            .args(["is-active", service_name])
            .output()
            .await
            .map_err(|e| {
                AppError::Command(format!(
                    "systemctl is-active failed for {}: {}",
                    service_name, e
                ))
            })?;

        let status = if output.status.success() {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        } else {
            "inactive".to_string()
        };

        statuses.insert(
            key.to_string(),
            json!({
                "name": key,
                "service": service_name.trim_end_matches(".service"),
                "status": status
            }),
        );
    }

    // ZFS status
    let zfs_status = match run_command_output(["zpool", "status", &zfs_pool]) {
        Ok(stdout) => {
            let pool_state = stdout
                .lines()
                .find_map(|line| {
                    if line.trim_start().starts_with("state:") {
                        line.split(':').nth(1).map(|s| s.trim().to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or("unknown".to_string());
            (if pool_state == "ONLINE" {
                "active"
            } else {
                "degraded"
            })
            .to_string()
        }
        _ => "error".to_string(),
    };

    statuses.insert(
        "zfs".to_string(),
        json!({
            "name": format!("ZFS Pool ({})", zfs_pool),
            "service": "zfs",
            "status": zfs_status
        }),
    );

    // Update cache
    cache.service_statuses = statuses.clone();
    cache.last_updated = std::time::SystemTime::now();

    save_config_section(
        "services",
        serde_json::to_value(&statuses).map_err(|e| AppError::Internal(e.to_string()))?,
    )?;

    serde_json::to_value(statuses).map_err(|e| AppError::Internal(e.to_string()))
}

#[tauri::command]
pub async fn get_service_config(token: String, service_key: String) -> Result<Value, AppError> {
    validate_token(&token)?;
    append_log("INFO", &format!("get_service_config: {}", service_key));

    match service_key.as_str() {
        "zfsutils-linux" => {
            let zpool = run_command_output(["zpool", "status"])
                .map_err(|e| AppError::Command(format!("zpool status failed: {}", e)))?;
            let zfs_list = run_command_output_no_sudo([
                "zfs",
                "list",
                "-t",
                "all",
                "-o",
                "name,type,used,avail,referservice,mountpoint",
            ])
            .map_err(|e| AppError::Command(format!("zfs list failed: {}", e)))?;

            let content = format!(
                "=== ZFS Pool Status ===\n{}\n\n=== ZFS Datasets ===\n{}",
                zpool, zfs_list
            );
            Ok(
                json!({ "text": content, "path": "zpool status && zfs list -t all -o name,type,used,avail,refer,mountpoint" }),
            )
        }
        "rtslib-fb-targetctl" => {
            let output = run_command_output(["targetcli", "ls"])
                .map_err(|e| AppError::Command(format!("targetcli ls failed: {}", e)))?;
            let content = format!("=== TargetCLI Config ===\n\n{}", output);
            Ok(json!({ "text": content, "path": "targetcli ls" }))
        }
        _ => {
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
                ("exportfs", "/etc/exports"),
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

            if !std::path::Path::new(path).exists() {
                return Err(AppError::NotFound(format!("Config not found: {}", path)));
            }

            match run_command_output(["cat", path]) {
                Ok(content) => {
                    append_log(
                        "INFO",
                        &format!("Read {}: {} bytes", service_key, content.len()),
                    );
                    Ok(json!({ "text": content, "path": path }))
                }
                Err(e) => Err(AppError::Io(std::io::Error::other(format!(
                    "Failed to read {}: {}",
                    path, e
                )))),
            }
        }
    }
}

#[tauri::command]
pub async fn control_service(
    token: String,
    service_key: String,
    req: ServiceControlRequest,
) -> Result<Value, AppError> {
    validate_token(&token)?;
    append_log(
        "INFO",
        &format!("control_service: {} {}", service_key, req.action),
    );

    let service_map = [
        ("rtslib-fb-targetctl", "rtslib-fb-targetctl.service"),
        ("isc-dhcp-server", "isc-dhcp-server.service"),
        ("tftpd-hpa", "tftpd-hpa.service"),
        ("apache2", "apache2.service"),
        ("smbd", "smbd.service"),
    ];

    let service_name = service_map
        .iter()
        .find_map(|&(k, v)| {
            if k == service_key.as_str() {
                Some(v)
            } else {
                None
            }
        })
        .ok_or_else(|| AppError::NotFound(format!("Unknown service: {}", service_key)))?;

    crate::cmd::run_command_async(&["systemctl", &req.action, service_name]).await?;

    // Invalidate cache since we've modified service state
    {
        let mut cache = SERVICES_CACHE.write().await;
        cache.last_updated = std::time::SystemTime::UNIX_EPOCH; // Force refresh next time
    }

    Ok(json!({
        "message": format!("'{}' service {} successfully.", service_key, &req.action)
    }))
}

#[tauri::command]
pub async fn install_service(service: String, token: String) -> Result<(), AppError> {
    validate_token(&token)?;

    match run_command_async(["apt-get", "install", "-y", &service]).await {
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
pub async fn check_package_status() -> Result<Value, AppError> {
    // Try to get cached data first
    {
        let cache = SERVICES_CACHE.read().await;

        if !cache.needs_refresh() {
            // Return cached data
            return serde_json::to_value(cache.package_statuses.clone())
                .map_err(|e| AppError::Internal(format!("Serialization failed: {}", e)));
        }
    } // Drop read lock

    let mut cache = SERVICES_CACHE.write().await;

    // Double-check after acquiring write lock
    if !cache.needs_refresh() {
        // Another thread may have updated the cache while we were waiting
        return serde_json::to_value(cache.package_statuses.clone())
            .map_err(|e| AppError::Internal(format!("Serialization failed: {}", e)));
    }

    let packages = [
        ("isc-dhcp-server", "isc-dhcp-server"),
        ("tftpd-hpa", "tftpd-hpa"),
        ("rtslib-fb-targetctl", "targetcli-fb"),
        ("apache2", "apache2"),
        ("smbd", "samba"),
        ("wakeonlan", "wakeonlan"),
        ("zfsutils-linux", "zfsutils-linux"),
    ];

    let mut status_list = Vec::new();
    for (service, package) in packages {
        let installed = check_package_installed(package).await?;
        let running = if installed {
            check_service_running(service).await?
        } else {
            false
        };
        let version = if installed {
            get_package_version(package).await?
        } else {
            None
        };

        status_list.push(PackageStatus {
            name: package.to_string(),
            service: service.to_string(),
            installed,
            configured: running,
            running,
            version,
        });
    }

    // Update cache
    cache.package_statuses = status_list.clone();
    cache.last_updated = std::time::SystemTime::now();

    let services_value = serde_json::to_value(&status_list)
        .map_err(|e| AppError::Internal(format!("Serialization failed: {}", e)))?;

    if let Err(e) = save_config_section("services", services_value.clone()) {
        append_log("WARN", &format!("Persist services failed: {}", e));
    } else {
        append_log("INFO", "Persisted services state");
    }

    Ok(services_value)
}

#[tauri::command]
pub async fn install_packages() -> Result<String, AppError> {
    // Update
    let _ = run_command_async(["apt", "update"]).await;

    let packages = [
        "isc-dhcp-server",
        "tftpd-hpa",
        "targetcli-fb",
        "apache2",
        "samba",
        "samba-common-bin",
    ];

    // Prepare args: ["apt", "install", "-y", pkg1, pkg2, ...]
    let mut args = vec!["apt", "install", "-y"];
    args.extend(packages);

    match run_command_async(&args).await {
        Ok(_) => Ok("Packages installed successfully".to_string()),
        Err(e) => Err(AppError::Command(format!("Installation failed: {}", e))),
    }
}

// #[tauri::command]
// pub async fn restart_service(service: &str) -> Result<(), AppError> {
//     let result = restart_service_async(service).await;

//     // Invalidate cache since we've modified service state
//     if result.is_ok() {
//         let mut cache = SERVICES_CACHE.write().await;
//         cache.last_updated = std::time::SystemTime::UNIX_EPOCH; // Force refresh next time
//     }

//     result
// }

#[tauri::command]
pub async fn configure_dhcp_server(token: String, config: DHCPConfig) -> Result<String, AppError> {
    validate_token(&token)?;

    save_config_section(
        "dhcp",
        serde_json::to_value(&config).map_err(|e| AppError::Internal(e.to_string()))?,
    )?;

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
  execute("/usr/bin/diskless-manager", "auto-add-client", "--mac", clmac, "--ip", clip);
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
        config.boot_file_uefi64
    );

    write_with_sudo_tee(DHCP_CONFIG_PATH, &dhcp_config).await?;
    restart_service_async("isc-dhcp-server.service").await?;

    Ok("DHCP server configured successfully".to_string())
}

#[tauri::command]
pub async fn configure_tftp_server(
    token: String,
    tftp_config: TFTPConfig,
) -> Result<String, AppError> {
    validate_token(&token)?;

    save_config_section(
        "tftp",
        serde_json::to_value(&tftp_config).map_err(|e| AppError::Internal(e.to_string()))?,
    )?;

    let tftp_content = format!(
        r#"# Defaults for tftpd-hpa
TFTP_USERNAME="tftp"
TFTP_DIRECTORY="{}"
TFTP_ADDRESS="{}:69"
TFTP_OPTIONS="{}"
"#,
        tftp_config.tftp_root, tftp_config.tftp_server_ip, tftp_config.tftp_options
    );

    std::fs::create_dir_all(&tftp_config.tftp_root).map_err(|e| {
        AppError::Io(std::io::Error::other(format!(
            "Create TFTP dir failed: {}",
            e
        )))
    })?;

    write_with_sudo_tee("/etc/default/tftpd-hpa", &tftp_content).await?;
    restart_service_async("tftpd-hpa.service").await?;

    Ok("TFTP server configured successfully".to_string())
}

#[tauri::command]
pub async fn configure_apache_server(
    token: String,
    http_config: HTTPConfig,
) -> Result<String, AppError> {
    validate_token(&token)?;

    save_config_section(
        "http",
        serde_json::to_value(&http_config).map_err(|e| AppError::Internal(e.to_string()))?,
    )?;

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
    <Directory {}>
        Options +Indexes
        IndexOptions +FancyIndexing
    </Directory>
</VirtualHost>
"#,
        http_config.http_server_ip,
        http_config.http_server_port,
        http_config.http_root,
        http_config.http_root,
        http_config.http_root
    );

    std::fs::create_dir_all(&http_config.http_root).map_err(|e| {
        AppError::Io(std::io::Error::other(format!(
            "Create HTTP dir failed: {}",
            e
        )))
    })?;

    write_with_sudo_tee(
        "/etc/apache2/sites-available/diskless-server.conf",
        &apache_config,
    )
    .await?;

    let _ = run_command_async(["a2ensite", "diskless-server.conf"]).await;
    restart_service_async("apache2.service").await?;

    Ok("Apache server configured successfully".to_string())
}

#[tauri::command]
pub async fn configure_samba_server(
    token: String,
    shares: Vec<SambaShare>,
) -> Result<String, AppError> {
    validate_token(&token)?;

    let mut samba_config = r#"[global]
   workgroup = WORKGROUP
   server string = Diskless Boot Server
   netbios name = DISKLESS-SERVER
   security = user
   map to guest = bad user
   dns proxy = no

"#
    .to_string();

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

        std::fs::create_dir_all(&share.path).map_err(|e| {
            AppError::Io(std::io::Error::other(format!(
                "Create share dir {} failed: {}",
                share.path, e
            )))
        })?;
    }

    write_with_sudo_tee("/etc/samba/smb.conf", &samba_config).await?;
    restart_service_async("smbd.service").await?;
    restart_service_async("nmbd.service").await?;

    Ok("Samba server configured successfully".to_string())
}

pub fn save_services_state(services: &Value) -> Result<(), AppError> {
    let mut cfg = get_config();
    if let Some(settings) = cfg.settings.as_object_mut() {
        settings.insert("services".to_string(), services.clone());
    } else {
        cfg.settings = json!({ "services": services });
    }
    write_config(&cfg).map_err(AppError::Config)?;
    set_config(&cfg);
    Ok(())
}
