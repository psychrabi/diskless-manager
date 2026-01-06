use crate::validation::{validate_ip_address, validate_mac_address};
use serde::Serialize;

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

pub fn get_interface_mask(interface: &str) -> Option<String> {
    let output = Command::new("ip")
        .args(["addr", "show", interface])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines() {
        if line.contains("inet ") && !line.contains("inet6") {
            if let Some(ip_part) = line.split_whitespace().nth(1) {
                if let Some(mask_part) = ip_part.split('/').nth(1) {
                    // Convert CIDR to dotted decimal
                    if let Ok(prefix) = mask_part.parse::<u32>() {
                        return Some(cidr_to_netmask(prefix));
                    }
                }
            }
        }
    }

    None
}

fn cidr_to_netmask(prefix: u32) -> String {
    let mask: u32 = if prefix == 0 {
        0
    } else {
        0xFFFFFFFFu32 << (32 - prefix)
    };
    format!(
        "{}.{}.{}.{}",
        (mask >> 24) & 0xFF,
        (mask >> 16) & 0xFF,
        (mask >> 8) & 0xFF,
        mask & 0xFF
    )
}

pub fn get_gateway() -> Option<String> {
    let output = Command::new("ip")
        .args(["route", "show", "default"])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().next().and_then(|line| {
        line.split_whitespace()
            .position(|s| s == "via")
            .and_then(|pos| line.split_whitespace().nth(pos + 1))
            .map(|s| s.to_string())
    })
}

pub fn get_dns() -> Vec<String> {
    // Try resolvectl first (modern systemd based systems)
    if let Ok(output) = Command::new("resolvectl").arg("status").output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut dns_servers = Vec::new();
        for line in stdout.lines() {
            if line.trim().starts_with("DNS Servers:")
                || line.trim().starts_with("Current DNS Server:")
            {
                let parts: Vec<&str> = line
                    .split_whitespace()
                    .skip_while(|s| !s.contains('.'))
                    .collect();
                for part in parts {
                    if is_valid_ip(part) {
                        dns_servers.push(part.to_string());
                    }
                }
            }
        }
        if !dns_servers.is_empty() {
            // Deduplicate preserving order
            let mut unique_dns = Vec::new();
            for dns in dns_servers {
                if !unique_dns.contains(&dns) {
                    unique_dns.push(dns);
                }
            }
            return unique_dns;
        }
    }

    // Fallback to reading /etc/resolv.conf
    let content = std::fs::read_to_string("/etc/resolv.conf").unwrap_or_default();
    content
        .lines()
        .filter(|l| l.starts_with("nameserver "))
        .filter_map(|l| l.split_whitespace().nth(1))
        .filter(|ip| *ip != "127.0.0.53") // Ignore systemd-resolved stub
        .map(|s| s.to_string())
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct InterfaceInfo {
    pub name: String,
    pub ip: Option<String>,
    pub mask: Option<String>,
}

pub fn get_domain() -> String {
    Command::new("hostname")
        .arg("-d")
        .output()
        .ok()
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        })
        .unwrap_or_else(|| "local".to_string())
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

pub fn calculate_network(ip: &str, netmask: &str) -> anyhow::Result<String> {
    let ip_parts: Vec<u8> = ip
        .split('.')
        .map(|s| s.parse())
        .collect::<Result<Vec<_>, _>>()?;
    let mask_parts: Vec<u8> = netmask
        .split('.')
        .map(|s| s.parse())
        .collect::<Result<Vec<_>, _>>()?;

    if ip_parts.len() != 4 || mask_parts.len() != 4 {
        return Err(anyhow::anyhow!("Invalid IP or Netmask format"));
    }

    let network: Vec<String> = ip_parts
        .iter()
        .zip(mask_parts.iter())
        .map(|(ip, mask)| (ip & mask).to_string())
        .collect();

    Ok(network.join("."))
}

pub fn calculate_broadcast(ip: &str, netmask: &str) -> anyhow::Result<String> {
    let ip_parts: Vec<u8> = ip
        .split('.')
        .map(|s| s.parse())
        .collect::<Result<Vec<_>, _>>()?;
    let mask_parts: Vec<u8> = netmask
        .split('.')
        .map(|s| s.parse())
        .collect::<Result<Vec<_>, _>>()?;

    if ip_parts.len() != 4 || mask_parts.len() != 4 {
        return Err(anyhow::anyhow!("Invalid IP or Netmask format"));
    }

    let broadcast: Vec<String> = ip_parts
        .iter()
        .zip(mask_parts.iter())
        .map(|(ip, mask)| (ip | (!mask)).to_string())
        .collect();

    Ok(broadcast.join("."))
}
