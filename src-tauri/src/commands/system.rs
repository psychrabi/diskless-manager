use crate::ssh_executor::{SshConfig, SshExecutor};
use crate::utils::network::InterfaceInfo;
use serde::{Deserialize, Serialize};
use std::process::Command;

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

pub async fn check_dependencies() -> Result<Vec<DependencyStatus>, String> {
    let distro = crate::platform::detect();

    // Binary name -> package name, varying per distribution family.
    let dependencies: Vec<(&str, &str)> = match distro {
        crate::platform::Distro::Debian => vec![
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
        ],
        crate::platform::Distro::RedHat => vec![
            ("qemu-img", "qemu-img"),
            ("targetcli", "targetcli"),
            ("dhcpd", "dhcp-server"),
            ("in.tftpd", "tftp-server"),
            ("exportfs", "nfs-utils"),
            ("httpd", "httpd"),
            ("smbd", "samba"),
            ("wol", "wol"),
            ("zfs", "zfs"),
            ("xfreerdp", "freerdp"),
            ("iftop", "iftop"),
        ],
        crate::platform::Distro::Arch => vec![
            ("qemu-img", "qemu"),
            ("targetcli", "targetcli-fb"),
            ("dhcpd", "dhcp"),
            ("in.tftpd", "tftp-hpa"),
            ("exportfs", "nfs-utils"),
            ("httpd", "apache"),
            ("smbd", "samba"),
            ("wakeonlan", "wakeonlan"),
            ("zfs", "zfs"),
            ("xfreerdp", "freerdp"),
            ("iftop", "iftop"),
        ],
    };

    let mut handles = Vec::new();

    for (cmd, name) in dependencies {
        let cmd = cmd.to_string();
        let name = name.to_string();
        handles.push(tokio::spawn(async move {
            let output = Command::new("which").arg(&cmd).output();
            let binary_found = output.map(|o| o.status.success()).unwrap_or(false);
            let installed = crate::platform::is_package_installed(distro, &name) || binary_found;

            let version = crate::platform::package_version(distro, &name);

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
    let commands = crate::platform::detect().privileged_commands();

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

/// Whether privileged access has already been granted. The sudoers rule lives
/// in a root-only mode-0440 file that the app process (a regular user) cannot
/// read, so probe the grant indirectly: `sudo -n` with one of the exact
/// passwordless-command paths only succeeds when the diskless-manager rule
/// exists. `-n` guarantees the probe never prompts.
pub fn is_privileged_access_configured() -> bool {
    // Best-effort direct read, which only works when the process can read the
    // file (e.g. it is running as root).
    if let Ok(content) = std::fs::read_to_string("/etc/sudoers.d/diskless-manager") {
        if let Ok(user) = std::env::var("USER") {
            if content
                .lines()
                .any(|line| line.starts_with(&format!("{} ALL=", user)))
            {
                return true;
            }
        } else if !content.trim().is_empty() {
            return true;
        }
    }

    let Ok(output) = std::process::Command::new("sudo")
        .args(["-n", "/usr/bin/systemctl", "--version"])
        .output()
    else {
        return false;
    };
    let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
    output.status.success() && !stderr.contains("password is required")
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

/// Test SSH connectivity to a remote host
pub async fn test_ssh_connection(request: SshTestRequest) -> Result<SshTestResult, String> {
    let start_time = std::time::Instant::now();

    // Create SSH config for Windows
    let config = SshConfig {
        connection_timeout: 10,
        command_timeout: 30,
        username: request.username,
        password: request.password,
        disable_host_key_verification: false,
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
    password: Option<String>,
    command: String,
) -> Result<SshTestResult, String> {
    let start_time = std::time::Instant::now();

    // Create SSH config for Windows
    let config = SshConfig {
        connection_timeout: 10,
        command_timeout: 60, // Longer timeout for custom commands
        username,
        password,
        disable_host_key_verification: false,
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
    password: Option<String>,
) -> Result<WindowsSystemInfo, String> {
    let config = SshConfig {
        connection_timeout: 10,
        command_timeout: 30,
        username,
        password,
        disable_host_key_verification: false,
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
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SshTestResult {
    pub success: bool,
    pub message: String,
    pub duration_ms: u64,
    pub command_output: Option<String>,
}

pub async fn install_package(service: String) -> Result<String, String> {
    crate::platform::install_package(&service).await
}
