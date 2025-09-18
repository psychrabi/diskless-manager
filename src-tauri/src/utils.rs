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

pub fn get_server_ip() -> String {
    // Try to get the server's IP address using `ip route get 1`
    match Command::new("ip").args(&["route", "get", "1"]).output() {
        Ok(output) => {
            if !output.status.success() {
                eprintln!(
                    "Warning: Failed to get server IP: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                return "192.168.1.200".to_string();
            }
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Some(idx) = line.find("src") {
                    let ip_part = &line[idx + 3..].trim();
                    let ip = ip_part.split_whitespace().next().unwrap_or("");
                    if ip.starts_with("192.168.") || ip.starts_with("10.") {
                        return ip.to_string();
                    }
                }
            }
            eprintln!("Warning: Could not find valid server IP address in output");
            "192.168.1.200".to_string()
        }
        Err(e) => {
            eprintln!("Warning: Failed to detect server IP: {}", e);
            "192.168.1.200".to_string()
        }
    }
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