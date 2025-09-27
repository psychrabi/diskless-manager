use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use dirs;
use serde::Serialize;

pub fn run_command(args: &[&str]) -> Result<(), String> {
    // Print the command being executed
    println!("Executing command: sudo {}", args.join(" "));
    
    let status = Command::new("sudo")
        .arg("-n")
        .args(args)
        .status()
        .map_err(|e| format!("Failed to run command: {}: {}", args.join(" "), e))?;
    if !status.success() {
        return Err(format!("Command failed: {}", args.join(" ")));
    }
    Ok(())
}

pub fn run_command_check(args: &[&str]) -> i32 {
    Command::new("sudo")
        .arg("-n")
        .args(args)
        .status()
        .map(|s| s.code().unwrap_or(-1))
        .unwrap_or(-1)
}

// add a function to run command with output
pub fn run_command_output(args: &[&str]) -> Result<String, String> {
    let output = Command::new("sudo")
        .arg("-n")
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run command: {}: {}", args.join(" "), e))?;
    if !output.status.success() {
        return Err(format!("Command failed: {}", args.join(" ")));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn get_server_ip() -> String {
    // Prefer the IP used for default route, then fallback to enumerating interfaces
    if let Ok(output) = Command::new("ip").args(["route", "get", "1"]).output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(ip) = parse_src_ip_from_ip_route(&stdout) {
                return ip;
            }
        }
    }

    // Fallback: parse `ip -4 addr show` and pick first private IPv4
    if let Ok(output) = Command::new("ip").args(["-4", "addr", "show"]).output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for token in stdout.split_whitespace() {
                if token.contains('/') && token.chars().any(|c| c.is_ascii_digit()) {
                    let ip = token.split('/').next().unwrap_or("");
                    if is_private_ipv4(ip) {
                        return ip.to_string();
                    }
                }
            }
        }
    }

    // Final fallback
    "192.168.1.200".to_string()
}

fn parse_src_ip_from_ip_route(output: &str) -> Option<String> {
    for line in output.lines() {
        if let Some(idx) = line.find("src ") {
            let ip = line[idx + 4..].split_whitespace().next().unwrap_or("");
            if is_private_ipv4(ip) {
                return Some(ip.to_string());
            } else if ip.parse::<std::net::Ipv4Addr>().is_ok() {
                // If not private but valid, still use it as last-resort for this parser
                return Some(ip.to_string());
            }
        }
    }
    None
}

fn is_private_ipv4(ip: &str) -> bool {
    // 10.0.0.0/8
    if ip.starts_with("10.") { return true; }
    // 172.16.0.0/12 -> 172.16.0.0 - 172.31.255.255
    if let Some(rest) = ip.strip_prefix("172.") {
        if let Some(second) = rest.split('.').next() {
            if let Ok(n) = second.parse::<u8>() {
                if (16..=31).contains(&n) { return true; }
            }
        }
    }
    // 192.168.0.0/16
    if ip.starts_with("192.168.") { return true; }
    false
}

#[derive(Serialize)]
pub struct Disk {
    name: String,
    size: String,
}

#[tauri::command]
pub fn list_disks() -> Result<Vec<Disk>, String> {
    // Use lsblk to list disks (Linux only)
    let output = match Command::new("lsblk").args(["-dn", "-o", "NAME,SIZE,TYPE"]).output() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Warning: 'lsblk' not available: {}", e);
            return Ok(vec![]);
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let disks = stdout
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() == 3 && parts[2] == "disk" {
                Some(Disk {
                    name: parts[0].to_string(),
                    size: parts[1].to_string(),
                })
            } else {
                None
            }
        })
        .collect();
    Ok(disks)
}

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
    memory: MemoryStats
}

/// Get current RAM usage statistics
#[tauri::command]
pub fn get_ram_usage() -> Result<RamUsage, String> {
    // Use run_command to check if "free" is available (for error handling consistency)
    let output = Command::new("free")
        .arg("-h")
        .output()
        .map_err(|e| format!("Failed to run free command: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    if lines.len() < 3 {
        return Err("Unexpected output from free command".to_string());
    }

    let mem_parts: Vec<&str> = lines[1].split_whitespace().collect();

    if mem_parts.len() < 7 {
        return Err("Invalid memory information format".to_string());
    }

    let memory = MemoryStats {
        total: mem_parts[1].to_string(),
        used: mem_parts[2].to_string(),
        free: mem_parts[3].to_string(),
        shared: mem_parts[4].to_string(),
        buff_cache: mem_parts[5].to_string(),
        available: mem_parts[6].to_string(),
    };

   

    Ok(RamUsage { memory })
}

/// Clear RAM cache (sync and drop caches)
#[tauri::command]
pub fn clear_ram_cache() -> Result<serde_json::Value, String> {
    // Run the full command with sudo: sync; echo 3 > /proc/sys/vm/drop_caches
    run_command(&["sh", "-c", "sync; echo 3 > /proc/sys/vm/drop_caches"])?;

    Ok(serde_json::json!({ "message": "Ram Cleared successfully" }))
}

#[tauri::command]
pub fn get_service_logs(unit: String, lines: Option<u32>) -> Result<String, String> {
  let num = lines.unwrap_or(200).to_string();
  match Command::new("sudo").arg("-n").args(["journalctl","-u", &unit, "-n", &num, "--no-pager"]).output() {
    Ok(out) => {
      if out.status.success() { Ok(String::from_utf8_lossy(&out.stdout).to_string()) }
      else { Err(String::from_utf8_lossy(&out.stderr).to_string()) }
    }
    Err(e) => Err(e.to_string()),
  }
}


pub fn log_file_path() -> PathBuf {
    // Use config dir (e.g. ~/.config/com.diskless.local/diskless-manager.log)
    let mut base = dirs::config_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")));
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
    let path = log_file_path();
    fs::read_to_string(&path).unwrap_or_default()
}

/// Clear the log file (best-effort)
pub fn clear_logs() -> Result<(), String> {
    let path = log_file_path();
    OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(&path)
        .map(|_| ())
        .map_err(|e| format!("Failed to clear log file: {}", e))
}