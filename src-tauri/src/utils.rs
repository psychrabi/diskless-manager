use std::process::Command;

use serde::Serialize;

pub fn run_command(args: &[&str]) -> Result<(), String> {
    // Print the command being executed
    println!("Executing command: sudo {}", args.join(" "));
    
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
    memory: MemoryStats
}

/// Get current RAM usage statistics
#[tauri::command]
pub fn get_ram_usage() -> Result<RamUsage, String> {
    // Use run_command to check if "free" is available (for error handling consistency)
    run_command(&["free", "-g"])?;

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
pub async fn get_zfs_arcstat() -> Result<serde_json::Value, String> {
    use std::fs;
    let arcstat_path = "/proc/spl/kstat/zfs/arcstats";
    let content = fs::read_to_string(arcstat_path).map_err(|e| e.to_string())?;
    let mut hits = 0u64;
    let mut misses = 0u64;
    let mut size = 0u64;
    for line in content.lines() {
        if line.starts_with("hits ") {
            hits = line.split_whitespace().nth(2).and_then(|v| v.parse().ok()).unwrap_or(0);
        }
        if line.starts_with("misses ") {
            misses = line.split_whitespace().nth(2).and_then(|v| v.parse().ok()).unwrap_or(0);
        }
        if line.starts_with("size ") {
            size = line.split_whitespace().nth(2).and_then(|v| v.parse().ok()).unwrap_or(0);
        }
    }
    let hit_percent = if hits + misses > 0 {
        (hits as f64 / (hits + misses) as f64) * 100.0
    } else {
        0.0
    };
    Ok(serde_json::json!({
        "size": size,
        "hit_percent": hit_percent
    }))
}
