use std::net::Ipv4Addr;
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
    ip.parse::<Ipv4Addr>().is_ok()
}

pub fn is_valid_mac(mac: &str) -> bool {
    let cleaned: String = mac.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    cleaned.len() == 12
}
