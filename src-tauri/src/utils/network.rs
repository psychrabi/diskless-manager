use crate::validation::{validate_ip_address, validate_mac_address};

use std::process::Command;

pub fn get_interface_ip(interface: &str) -> Option<String> {
    let output = Command::new("ip")
        .args(["addr", "show", interface])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines() {
        if line.contains("inet ") && !line.contains("inet6") {
            if let Some(ip_part) = line.split_whitespace().nth(1) {
                if let Some(ip) = ip_part.split('/').next() {
                    return Some(ip.to_string());
                }
            }
        }
    }

    None
}

pub fn list_interfaces() -> Vec<String> {
    let output = Command::new("ip").args(["link", "show"]).output().ok();

    match output {
        Some(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout
                .lines()
                .filter(|l| l.contains(": ") && !l.starts_with(' '))
                .filter_map(|l| {
                    l.split(": ")
                        .nth(1)
                        .map(|s| s.split('@').next().unwrap_or(s).to_string())
                })
                .filter(|name| name != "lo")
                .collect()
        }
        None => Vec::new(),
    }
}

pub fn is_valid_ip(ip: &str) -> bool {
    validate_ip_address(ip).is_ok()
}

pub fn is_valid_mac(mac: &str) -> bool {
    validate_mac_address(mac).is_ok()
}

// Synchronous client status function for backward compatibility
pub fn get_client_status_realtime(ip: String) -> String {
    // Consider ping reachability as Online
    let online = if ip.is_empty() || ip == "N/A" {
        false
    } else {
        match std::process::Command::new("ping")
            .args(["-c", "1", "-W", "1", &ip])
            .output()
        {
            Ok(out) => out.status.success(),
            Err(_) => false,
        }
    };

    if online {
        "Online".to_string()
    } else {
        "Offline".to_string()
    }
}

// Async ping function using spawn_blocking for better efficiency
pub async fn ping_host(ip: String) -> String {
    tokio::task::spawn_blocking(move || {
        match std::process::Command::new("ping")
            .args(["-c", "1", "-W", "2", &ip])
            .output()
        {
            Ok(out) => {
                if out.status.success() {
                    "Online".to_string()
                } else {
                    "Offline".to_string()
                }
            }
            Err(_) => "Offline".to_string(),
        }
    })
    .await
    .unwrap_or_else(|_| "Offline".to_string())
}
