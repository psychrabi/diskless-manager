use std::process::Command;

use serde::Serialize;
use serde_json::json;

pub fn run_command(args: &[&str]) -> Result<(), String> {
    let status = Command::new("sudo")
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
    let output = Command::new("lsblk")
        .args(&["-dn", "-o", "NAME,SIZE,TYPE"])
        .output()
        .map_err(|e| e.to_string())?;
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

/// Parses human-readable size string (e.g., 50G, 1T) to bytes.
pub fn parse_size_to_bytes(size_str: &str) -> Result<u64, String> {
    let size_str = size_str.trim().to_uppercase();
    let units: std::collections::HashMap<char, u64> = [
        ('B', 1),
        ('K', 1024),
        ('M', 1024u64.pow(2)),
        ('G', 1024u64.pow(3)),
        ('T', 1024u64.pow(4)),
        ('P', 1024u64.pow(5)),
    ]
    .iter()
    .cloned()
    .collect();

    let re = regex::Regex::new(r"^(\d+(\.\d+)?)\s*([KMGTPE]?)B?$")
        .map_err(|e| format!("Failed to create regex: {}", e))?;
    
    let caps = re.captures(&size_str)
        .ok_or_else(|| format!("Invalid size format: {}", size_str))?;

    let value: f64 = caps[1].parse()
        .map_err(|e| format!("Failed to parse number: {}", e))?;
    
    let unit = caps.get(3).map_or('B', |m| m.as_str().chars().next().unwrap());

    if !units.contains_key(&unit) {
        return Err(format!("Invalid size unit: {}", unit));
    }

    Ok((value * units[&unit] as f64) as u64)
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
pub struct SwapStats {
    total: String,
    used: String,
    free: String,
}

#[derive(Serialize)]
pub struct RamUsage {
    memory: MemoryStats,
    swap: SwapStats,
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

    if lines.len() < 3 {
        return Err("Unexpected output from free command".to_string());
    }

    let mem_parts: Vec<&str> = lines[1].split_whitespace().collect();
    let swap_parts: Vec<&str> = lines[2].split_whitespace().collect();

    if mem_parts.len() < 7 || swap_parts.len() < 4 {
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

    let swap = SwapStats {
        total: swap_parts[1].to_string(),
        used: swap_parts[2].to_string(),
        free: swap_parts[3].to_string(),
    };

    Ok(RamUsage { memory, swap })
}

/// Clear RAM cache (sync and drop caches)
#[tauri::command]
pub fn clear_ram_cache() -> Result<(), String> {
    // First sync to ensure all data is written to disk
    Command::new("sync")
        .status()
        .map_err(|e| format!("Failed to sync: {}", e))?;

    // Drop caches (1=pagecache, 2=inodes/dentries, 3=all)
    Command::new("sudo")
        .args(["sh", "-c", "echo 3 > /proc/sys/vm/drop_caches"])
        .status()
        .map_err(|e| format!("Failed to drop caches: {}", e))?;

    Ok(())
}
