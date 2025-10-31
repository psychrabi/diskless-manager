use dirs;
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

#[derive(Serialize)]
pub struct MemoryStats {
    total: String,
    used: String,
    free: String,
    shared: String,
    buff_cache: String,
    available: String,
}

#[derive(Serialize)]
pub struct RamUsage {
    memory: MemoryStats,
}


#[derive(thiserror::Error, Debug)]
pub enum CommandError {
    #[error("Failed to execute command {cmd}: {source}")]
    Execution { cmd: String, source: std::io::Error },
    #[error("Command {cmd} failed with status {status}")]
    Failure { cmd: String, status: i32 },    
}

// Generic command runner with sudo -n
fn exec_sudo_cmd<II>(args: II) -> Result<Output, CommandError>
where
    II: IntoIterator,
    II::Item: AsRef<std::ffi::OsStr>,
{
    let args_vec: Vec<_> = args.into_iter().collect();
    let cmd_str = args_vec
        .iter()
        .map(|a| a.as_ref().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ");
    let output = Command::new("sudo")
        .arg("-n")
        .args(args_vec.iter())
        .stdin(Stdio::null()) // Avoid hanging on input
        .output()
        .map_err(|e| CommandError::Execution {
            cmd: cmd_str.clone(),
            source: e,
        })?;
    let status = output.status.code().unwrap_or(-1);
    if !output.status.success() {
        return Err(CommandError::Failure {
            cmd: cmd_str,
            status,
        });
    }
    Ok(output)
}

pub fn run_command<II>(args: II) -> Result<(), String>
where
    II: IntoIterator,
    II::Item: AsRef<std::ffi::OsStr>,
{
    let args_vec: Vec<_> = args.into_iter().collect();
    let cmd_str = args_vec
        .iter()
        .map(|a| a.as_ref().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ");
    println!("Executing command: sudo {}", cmd_str);
    exec_sudo_cmd(args_vec.iter()).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn run_command_check<II>(args: II) -> i32
where
    II: IntoIterator,
    II::Item: AsRef<std::ffi::OsStr>,
{
    let args_vec: Vec<_> = args.into_iter().collect();
    let cmd_str = args_vec
        .iter()
        .map(|a| a.as_ref().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ");
    println!("Executing command: sudo {}", cmd_str);
    match exec_sudo_cmd(args_vec.iter()) {
        Ok(o) => o.status.code().unwrap_or(-1),
        Err(_) => -1,
    }
}

pub fn run_command_output<II>(args: II) -> Result<String, String>
where
    II: IntoIterator,
    II::Item: AsRef<std::ffi::OsStr>,
{
    let args_vec: Vec<_> = args.into_iter().collect();
    let cmd_str = args_vec
        .iter()
        .map(|a| a.as_ref().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ");
    println!("Executing command: sudo {}", cmd_str);
    let output = exec_sudo_cmd(args_vec.iter()).map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn run_command_output_no_sudo<II>(args: II) -> Result<String, String>
where
    II: IntoIterator,
    II::Item: AsRef<std::ffi::OsStr>,
{
    let args_vec: Vec<_> = args.into_iter().collect();
    let cmd_str = args_vec
        .iter()
        .map(|a| a.as_ref().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ");
    println!("Executing command: {}", cmd_str);
    
    let mut cmd_iter = args_vec.iter();
    let program = cmd_iter.next().ok_or("No command provided")?;
    
    let output = std::process::Command::new(program)
        .args(cmd_iter)
        .output()
        .map_err(|e| e.to_string())?;
    
    if !output.status.success() {
        return Err(format!("Command failed: {}", String::from_utf8_lossy(&output.stderr)));
    }
    
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn get_server_ip() -> String {
    // More robust parsing using regex for IP extraction
    let output = match Command::new("ip").args(["route", "get", "1"]).output() {
        Ok(o) if o.status.success() => o,
        Ok(o) => {
            eprintln!(
                "Warning: Failed to get server IP: {}",
                String::from_utf8_lossy(&o.stderr)
            );
            return "192.168.1.200".to_string();
        }
        Err(e) => {
            eprintln!("Warning: Failed to detect server IP: {}", e);
            return "192.168.1.200".to_string();
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let re = regex::Regex::new(r"src\s+(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})").unwrap();
    if let Some(caps) = re.captures(&stdout) {
        let ip = &caps[1];
        if ip.starts_with("192.168.") || ip.starts_with("10.") || ip.starts_with("172.16.") || ip.starts_with("172.17.") || ip.starts_with("172.18.") || ip.starts_with("172.19.") || ip.starts_with("172.20.") || ip.starts_with("172.21.") || ip.starts_with("172.22.") || ip.starts_with("172.23.") || ip.starts_with("172.24.") || ip.starts_with("172.25.") || ip.starts_with("172.26.") || ip.starts_with("172.27.") || ip.starts_with("172.28.") || ip.starts_with("172.29.") || ip.starts_with("172.30.") || ip.starts_with("172.31.") {
            return ip.to_string();
        }
    }

    eprintln!("Warning: Could not find valid server IP address in output");
    "192.168.1.200".to_string()
}

#[derive(Serialize)]
pub struct Disk {
    name: String,
    size: String,
}

#[tauri::command]
pub fn list_disks() -> Result<Vec<Disk>, String> {
    let output = Command::new("lsblk")
        .args(["-dn", "-o", "NAME,SIZE,TYPE"])
        .output()
        .map_err(|e| format!("'lsblk' not available: {}", e))?;
    if !output.status.success() {
        return Ok(vec![]);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut disks = Vec::new();
    for line in stdout.lines().skip(1) { // Skip header
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() == 3 && parts[2] == "disk" {
            disks.push(Disk {
                name: parts[0].to_string(),
                size: parts[1].to_string(),
            });
        }
    }
    Ok(disks)
}

/// Get current RAM usage statistics
#[tauri::command]
pub fn get_ram_usage() -> Result<RamUsage, String> {
    let output = Command::new("free")
        .arg("-h")
        .output()
        .map_err(|e| format!("Failed to run free command: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    if lines.len() < 2 {
        return Err("Unexpected output from free command".to_string());
    }

    // Parse Mem line (index 1, after header)
    let mem_line = lines[1];
    let parts: Vec<&str> = mem_line.split_whitespace().collect();
    if parts.len() < 7 {
        return Err("Invalid memory information format".to_string());
    }

    let memory = MemoryStats {
        total: parts[1].to_string(),
        used: parts[2].to_string(),
        free: parts[3].to_string(),
        shared: parts[4].to_string(),
        buff_cache: parts[5].to_string(),
        available: parts[6].to_string(),
    };

    Ok(RamUsage { memory })
}

/// Clear RAM cache (sync and drop caches)
#[tauri::command]
pub fn clear_ram_cache() -> Result<serde_json::Value, String> {
    run_command(vec!["sh", "-c", "sync && echo 3 > /proc/sys/vm/drop_caches"])?;

    Ok(serde_json::json!({ "message": "RAM cache cleared successfully" }))
}

#[tauri::command]
pub fn get_service_logs(unit: String, lines: Option<u32>) -> Result<String, String> {
    let num = lines.unwrap_or(200).to_string();
    let args_vec: Vec<_> = vec!["journalctl", "-u", &unit, "-n", &num, "--no-pager"];
    let output = run_command_output_no_sudo(args_vec.iter()).map_err(|e| e.to_string())?;
    Ok(output)
}

pub fn log_file_path() -> PathBuf {
    let mut base = dirs::config_dir()
        .unwrap_or_else(|| dirs::home_dir().expect("No home dir"));
    base.push("com.diskless.local");
    let _ = std::fs::create_dir_all(&base);
    base.push("diskless-manager.log");
    base
}

/// Append a single line with level and timestamp to the log file.
/// This is best-effort and should not panic.
pub fn append_log(level: &str, msg: &str) {
    let path = log_file_path();
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let _ = writeln!(f, "[{}] {}: {}", ts, level, msg);
    }
}

/// Read the whole log file as a string (returns empty string on error)
pub fn read_logs() -> String {
    fs::read_to_string(log_file_path()).unwrap_or_default()
}

/// Clear the log file (best-effort)
pub fn clear_logs() -> Result<(), String> {
    let path = log_file_path();
    OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(path)
        .map(drop)
        .map_err(|e| format!("Failed to clear log file: {}", e))
}