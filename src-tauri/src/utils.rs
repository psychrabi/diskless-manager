use std::process::Command;

use serde::Serialize;

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
