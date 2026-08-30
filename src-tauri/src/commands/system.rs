use crate::core::config::Settings;
use crate::core::service::ServiceManager;
use crate::ssh_executor::{SshConfig, SshExecutor};
use crate::state::AppState;
use crate::utils::network::InterfaceInfo;
use log::info;
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, Serialize)]
pub struct NetworkDetection {
    pub interfaces: Vec<InterfaceInfo>,
    pub primary_interface: Option<String>,
    pub primary_ip: Option<String>,
    pub primary_mask: Option<String>,
    pub gateway: Option<String>,
    pub dns: Vec<String>,
    pub hostname: String,
    pub domain: String,
}

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

#[expect(dead_code, reason = "Old Tauri command - handler implements its own")]
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

#[expect(dead_code, reason = "Old Tauri command - handler implements its own")]
pub async fn initialize_server(state: State<'_, AppState>) -> Result<String, String> {
    let settings = state.settings.read().await;

    // Create directories
    std::fs::create_dir_all(&settings.tftp.root_dir)
        .map_err(|e| format!("Failed to create {:?}: {}", settings.tftp.root_dir, e))?;
    std::fs::create_dir_all(&settings.iscsi.targets_dir)
        .map_err(|e| format!("Failed to create {:?}: {}", settings.iscsi.targets_dir, e))?;
    std::fs::create_dir_all(&settings.nfs.exports_dir)
        .map_err(|e| format!("Failed to create {:?}: {}", settings.nfs.exports_dir, e))?;
    std::fs::create_dir_all(&settings.samba.share_path)
        .map_err(|e| format!("Failed to create {:?}: {}", settings.samba.share_path, e))?;
    std::fs::create_dir_all(&settings.storage.images_dir)
        .map_err(|e| format!("Failed to create {:?}: {}", settings.storage.images_dir, e))?;
    std::fs::create_dir_all(&settings.storage.snapshots_dir).map_err(|e| {
        format!(
            "Failed to create {:?}: {}",
            settings.storage.snapshots_dir, e
        )
    })?;

    Ok("Server initialized successfully".to_string())
}

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
        ("iftop", "iftop"),
    ];

    let mut handles = Vec::new();

    for (cmd, name) in dependencies {
        let cmd = cmd.to_string();
        let name = name.to_string();
        handles.push(tokio::spawn(async move {
            let output = Command::new("which").arg(&cmd).output();
            let installed = output.map(|o| o.status.success()).unwrap_or(false);

            let version = Command::new("dpkg-query")
                .args(["--showformat=${Version}\n", "--show", &name])
                .output()
                .ok()
                .and_then(|o| {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    let output = if stdout.is_empty() { stderr } else { stdout };
                    output.lines().next().map(|s| s.to_string())
                });

            DependencyStatus {
                name,
                installed,
                version,
            }
        }));
    }

    let mut statuses = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(status) => statuses.push(status),
            Err(e) => return Err(format!("Task join error: {}", e)),
        }
    }

    Ok(statuses)
}

#[expect(dead_code, reason = "Old Tauri command - handler implements its own")]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    let settings = state.settings.read().await;
    Ok(settings.clone())
}

#[expect(dead_code, reason = "Old Tauri command - handler implements its own")]
pub async fn save_settings(state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
    // Update the in-memory settings
    let mut current = state.settings.write().await;
    *current = settings.clone();

    // Update the settings in the database (merging with existing fields to avoid losing zpool_name etc)
    let current_config = crate::config::get_config();
    let mut new_config = current_config;

    let new_settings_value = serde_json::to_value(&*current)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;

    if let (Some(obj), Some(new_obj)) = (
        new_config.settings.as_object_mut(),
        new_settings_value.as_object(),
    ) {
        for (k, v) in new_obj {
            obj.insert(k.clone(), v.clone());
        }
    } else {
        new_config.settings = new_settings_value;
    }

    crate::config::write_config(&state.db_pool, &new_config)
        .await
        .map_err(|e| format!("Failed to write config to database: {}", e))?;

    // Also persist to config.toml for redundancy and manual editing support
    let toml_path = state.config_path.with_extension("toml");
    if let Err(e) = current.save(&toml_path) {
        tracing::error!("Failed to save settings to {}: {}", toml_path.display(), e);
        // We don't necessarily want to return error here if DB save succeeded,
        // but it's good to log it. Actually, better to inform user if both fail.
    }

    info!("Settings saved to database and TOML");
    Ok(())
}

pub async fn setup_privileged_access() -> Result<String, String> {
    let user = std::env::var("USER").unwrap_or_else(|_| {
        Command::new("id")
            .args(["-un"])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "root".to_string())
    });

    // We list the exactly required commands with their paths found in the system
    let commands = [
        "/usr/bin/apt-get",
        "/usr/bin/systemctl",
        "/usr/sbin/zfs",
        "/usr/sbin/zpool",
        "/usr/bin/targetcli",
        "/usr/bin/tee",
        "/usr/bin/mkdir",
        "/usr/bin/sync",
        "/usr/sbin/exportfs",
        "/usr/sbin/a2ensite",
        "/usr/sbin/a2enmod",
        "/usr/bin/journalctl",
        "/usr/bin/rm",
        "/usr/bin/mv",
        "/usr/bin/cp",
        "/usr/sbin/netplan",
        "/usr/sbin/dhcpd",
    ];

    let commands_str = commands.join(", ");
    let sudoers_content = format!("{} ALL=(ALL) NOPASSWD: {}\n", user, commands_str);

    // Use pkexec to create the sudoers file
    let script = format!(
        "echo '{}' > /etc/sudoers.d/diskless-manager && chmod 0440 /etc/sudoers.d/diskless-manager",
        sudoers_content
    );

    let output = Command::new("pkexec")
        .args(["sh", "-c", &script])
        .output()
        .map_err(|e| format!("Failed to spawn pkexec: {}", e))?;

    if output.status.success() {
        Ok("Privileged access configured successfully. Administrative tasks will no longer require password prompts.".to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "Authorization failed or error occurred: {}",
            stderr
        ))
    }
}

pub async fn get_network_interfaces() -> Result<Vec<String>, String> {
    Ok(crate::utils::network::list_interfaces())
}

pub async fn get_interface_ip(interface: String) -> Result<Option<String>, String> {
    Ok(crate::utils::network::get_interface_ip(&interface))
}

pub async fn detect_server_network() -> Result<NetworkDetection, String> {
    let interfaces_names = crate::utils::network::list_interfaces();
    let mut interfaces = Vec::new();
    let mut primary_interface = None;
    let mut primary_ip = None;
    let mut primary_mask = None;

    for name in interfaces_names {
        let ip = crate::utils::network::get_interface_ip(&name);
        let mask = crate::utils::network::get_interface_mask(&name);

        if primary_interface.is_none() && ip.is_some() {
            primary_interface = Some(name.clone());
            primary_ip = ip.clone();
            primary_mask = mask.clone();
        }
        interfaces.push(InterfaceInfo {
            name: name.clone(),
            ip,
            mask,
        });
    }

    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let domain = crate::utils::network::get_domain();
    let gateway = crate::utils::network::get_gateway();
    let dns = crate::utils::network::get_dns();

    Ok(NetworkDetection {
        interfaces,
        primary_interface,
        primary_ip,
        primary_mask,
        gateway,
        dns,
        hostname,
        domain,
    })
}

#[expect(dead_code, reason = "Old Tauri command - handler implements its own")]
pub async fn apply_network_settings(state: State<'_, AppState>) -> Result<String, String> {
    let mut settings = state.settings.read().await.clone();
    let server = &settings.server;

    if server.interface.is_empty() {
        return Err("No interface selected".to_string());
    }

    let interface = &server.interface[0];
    let ip = &server.ip_address;
    let mask = &server.netmask;
    let gateway = &server.gateway;
    let dns = &server.dns;

    // Convert dotted mask to prefix
    let prefix = mask_to_prefix(mask).unwrap_or(24);

    let dns_str = if dns.is_empty() {
        "8.8.8.8, 8.8.4.4".to_string()
    } else {
        dns.join(", ")
    };

    let netplan_content = format!(
        r#"network:
  version: 2
  renderer: networkd
  ethernets:
    {}:
      dhcp4: no
      addresses:
        - {}/{}
      gateway4: {}
      nameservers:
        addresses: [{}]
"#,
        interface, ip, prefix, gateway, dns_str
    );

    let path = "/etc/netplan/99-diskless-manager.yaml";
    crate::services::write_with_sudo_tee(path, &netplan_content)
        .await
        .map_err(|e| format!("Failed to write netplan config: {}", e))?;

    // Apply netplan
    crate::services::run_sudo_command(["netplan", "apply"])
        .await
        .map_err(|e| format!("Failed to apply netplan: {}", e))?;

    // Update related service configurations with the new static IP
    settings.tftp.server_ip = ip.clone();
    settings.http.server_ip = ip.clone();

    // Update DHCP settings
    settings.dhcp.next_server_ip = ip.clone();
    settings.dhcp.boot_server_ip = ip.clone();
    settings.dhcp.subnet_mask = mask.clone();
    settings.dhcp.gateway_ip = gateway.clone();

    // Calculate subnet and broadcast based on IP and Mask
    if let Ok(subnet) = crate::utils::network::calculate_network(ip, mask) {
        settings.dhcp.subnet_ip = subnet;
    }
    if let Ok(broadcast) = crate::utils::network::calculate_broadcast(ip, mask) {
        settings.dhcp.broadcast_ip = broadcast;
    }

    // Persist the updated settings
    {
        // 1. Update in-memory state
        let mut write_lock = state.settings.write().await;
        *write_lock = settings.clone();

        // 2. Save to TOML
        write_lock
            .save(&state.config_path)
            .map_err(|e| format!("Failed to save settings to file: {}", e))?;

        // 3. Save to Database to ensure consistency on restart
        let current_config = crate::config::get_config();
        let mut new_config = current_config;

        let new_settings_value = serde_json::to_value(&settings)
            .map_err(|e| format!("Failed to serialize settings: {}", e))?;

        if let (Some(obj), Some(new_obj)) = (
            new_config.settings.as_object_mut(),
            new_settings_value.as_object(),
        ) {
            for (k, v) in new_obj {
                obj.insert(k.clone(), v.clone());
            }
        } else {
            new_config.settings = new_settings_value;
        }

        crate::config::write_config(&state.db_pool, &new_config)
            .await
            .map_err(|e| format!("Failed to write config to database: {}", e))?;
    }

    // Regenerate and reload all services
    let service_manager = crate::services::ServiceManager::new(settings, state.db_pool.clone());
    service_manager
        .generate_all_configs()
        .await
        .map_err(|e| format!("Failed to regenerate service configs: {}", e))?;
    service_manager
        .restart_all()
        .await
        .map_err(|e| format!("Failed to restart services: {}", e))?;

    Ok("Network settings applied and services updated successfully.".to_string())
}

/// Test SSH connectivity to a remote host
pub async fn test_ssh_connection(request: SshTestRequest) -> Result<SshTestResult, String> {
    let start_time = std::time::Instant::now();

    // Create SSH config for Windows
    let config = SshConfig {
        connection_timeout: 10,
        command_timeout: 30,
        username: request.username,
        disable_host_key_verification: true,
        max_retries: 1,
    };

    let executor = SshExecutor::with_config(config);

    // Test basic connectivity first
    match executor.check_connectivity(&request.host).await {
        Ok(true) => {
            // Try a simple command to verify it works
            match executor
                .execute_command(&request.host, "echo 'SSH connection successful'")
                .await
            {
                Ok(result) => {
                    let duration_ms = start_time.elapsed().as_millis() as u64;
                    Ok(SshTestResult {
                        success: true,
                        message: "SSH connection and command execution successful".to_string(),
                        duration_ms,
                        command_output: Some(result.stdout),
                    })
                }
                Err(e) => {
                    let duration_ms = start_time.elapsed().as_millis() as u64;
                    Ok(SshTestResult {
                        success: false,
                        message: format!("SSH connected but command failed: {}", e),
                        duration_ms,
                        command_output: None,
                    })
                }
            }
        }
        Ok(false) => {
            let duration_ms = start_time.elapsed().as_millis() as u64;
            Ok(SshTestResult {
                success: false,
                message: "SSH connection failed - check host, port, and credentials".to_string(),
                duration_ms,
                command_output: None,
            })
        }
        Err(e) => {
            let duration_ms = start_time.elapsed().as_millis() as u64;
            Ok(SshTestResult {
                success: false,
                message: format!("SSH connection error: {}", e),
                duration_ms,
                command_output: None,
            })
        }
    }
}

/// Execute a custom SSH command on a remote host
pub async fn execute_ssh_command(
    host: String,
    username: String,
    command: String,
) -> Result<SshTestResult, String> {
    let start_time = std::time::Instant::now();

    // Create SSH config for Windows
    let config = SshConfig {
        connection_timeout: 10,
        command_timeout: 60, // Longer timeout for custom commands
        username,
        disable_host_key_verification: true,
        max_retries: 1,
    };

    let executor = SshExecutor::with_config(config);

    match executor.execute_command(&host, &command).await {
        Ok(result) => {
            let duration_ms = start_time.elapsed().as_millis() as u64;
            Ok(SshTestResult {
                success: result.exit_code == 0,
                message: if result.exit_code == 0 {
                    "Command executed successfully".to_string()
                } else {
                    format!("Command failed with exit code {}", result.exit_code)
                },
                duration_ms,
                command_output: Some(format!(
                    "STDOUT:\n{}\nSTDERR:\n{}",
                    result.stdout, result.stderr
                )),
            })
        }
        Err(e) => {
            let duration_ms = start_time.elapsed().as_millis() as u64;
            Ok(SshTestResult {
                success: false,
                message: format!("SSH execution error: {}", e),
                duration_ms,
                command_output: None,
            })
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct WindowsSystemInfo {
    pub computer_name: String,
    pub os_version: String,
    pub architecture: String,
    pub total_memory: String,
    pub available_memory: String,
    pub cpu_info: String,
}

/// Get system information from a Windows machine via SSH
pub async fn get_windows_system_info(
    host: String,
    username: String,
) -> Result<WindowsSystemInfo, String> {
    let config = SshConfig {
        connection_timeout: 10,
        command_timeout: 30,
        username,
        disable_host_key_verification: true,
        max_retries: 1,
    };

    let executor = SshExecutor::with_config(config);

    // PowerShell command to get system info
    let ps_command = r#"
        $info = Get-ComputerInfo
        Write-Output "COMPUTER_NAME:$($env:COMPUTERNAME)"
        Write-Output "OS_VERSION:$($info.WindowsProductName) $($info.WindowsVersion)"
        Write-Output "ARCHITECTURE:$($info.CsProcessors[0].Architecture)"
        Write-Output "TOTAL_MEMORY:$([math]::Round($info.TotalPhysicalMemory/1GB, 2)) GB"
        Write-Output "AVAILABLE_MEMORY:$([math]::Round($info.AvailablePhysicalMemory/1GB, 2)) GB"
        Write-Output "CPU_INFO:$($info.CsProcessors[0].Name)"
    "#;

    let command = format!("powershell.exe -Command \"{}\"", ps_command);

    match executor.execute_command(&host, &command).await {
        Ok(result) if result.exit_code == 0 => {
            let mut info = WindowsSystemInfo {
                computer_name: "Unknown".to_string(),
                os_version: "Unknown".to_string(),
                architecture: "Unknown".to_string(),
                total_memory: "Unknown".to_string(),
                available_memory: "Unknown".to_string(),
                cpu_info: "Unknown".to_string(),
            };

            // Parse the output
            for line in result.stdout.lines() {
                if let Some((key, value)) = line.split_once(':') {
                    match key {
                        "COMPUTER_NAME" => info.computer_name = value.to_string(),
                        "OS_VERSION" => info.os_version = value.to_string(),
                        "ARCHITECTURE" => info.architecture = value.to_string(),
                        "TOTAL_MEMORY" => info.total_memory = value.to_string(),
                        "AVAILABLE_MEMORY" => info.available_memory = value.to_string(),
                        "CPU_INFO" => info.cpu_info = value.to_string(),
                        _ => {}
                    }
                }
            }

            Ok(info)
        }
        Ok(result) => Err(format!(
            "Command failed with exit code {}: {}",
            result.exit_code, result.stderr
        )),
        Err(e) => Err(format!("SSH execution failed: {}", e)),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshTestRequest {
    pub host: String,
    pub username: String,
    pub port: Option<u16>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SshTestResult {
    pub success: bool,
    pub message: String,
    pub duration_ms: u64,
    pub command_output: Option<String>,
}

fn mask_to_prefix(mask: &str) -> Option<u32> {
    let parts: Vec<u32> = mask.split('.').filter_map(|s| s.parse().ok()).collect();
    if parts.len() != 4 {
        return None;
    }
    let mut full_mask = 0u32;
    for part in parts {
        full_mask = (full_mask << 8) | part;
    }
    Some(full_mask.count_ones())
}

pub async fn install_package(service: String) -> Result<String, String> {
    let output = Command::new("sudo")
        .args(["apt-get", "install", "-y", &service])
        .output()
        .map_err(|e| format!("Failed to spawn apt-get: {}", e))?;

    if output.status.success() {
        Ok(format!("Package {} installed successfully", service))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("Failed to install {}: {}", service, stderr))
    }
}
