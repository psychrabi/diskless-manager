// DHCP-related logic for config file management and helpers.

use regex::Regex;
use std::{fs, process::Command};
use async_process::Command as AsyncCommand;
use crate::{utils::{get_server_ip, append_log}, DHCP_CLIENTS_PATH};

pub async fn update_dhcp_config(client_id: &str, dhcp_entry: &str, is_new: bool) -> Result<(), String> {
    append_log("INFO", &format!("update_dhcp_config start: client_id={}, is_new={}", client_id, is_new));

    // Read DHCP config using sudo cat to avoid permission issues
    let output = AsyncCommand::new("sudo")
        .args(["cat", DHCP_CLIENTS_PATH])
        .output()
        .await
        .map_err(|e| format!("Failed to read DHCP config via sudo: {}", e))?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let msg = format!("Failed to read DHCP config via sudo: {}", stderr);
        append_log("ERROR", &msg);
        return Err(msg);
    }
    
    let content = String::from_utf8_lossy(&output.stdout).to_string();
    let backup_dir = "/srv/tftp/backups";
    let dhcp_backup_path = format!("{}/dhcp_clients_{}.bak", backup_dir, std::process::id());
    
    // Create backup directory if it doesn't exist
    if let Err(e) = std::fs::create_dir_all(backup_dir) {
        return Err(format!("Failed to create backup directory: {}", e));
    }
    
    fs::write(&dhcp_backup_path, &content)
        .map_err(|e| format!("Failed to backup DHCP config: {}", e))?;
    let mut new_content = content.clone();
    if !is_new {
        let formatted_name = format_client_name(client_id);
        // Use a more robust approach to remove the entire host block
        let host_start_pattern = format!(
            "host\\s+{}\\s*\\{{",
            regex::escape(&formatted_name)
        );
        let re_start = Regex::new(&host_start_pattern).map_err(|e| format!("Regex error: {}", e))?;
        
        // Find the start position of the host block
        if let Some(m) = re_start.find(&new_content) {
            let start_pos = m.start();
            let mut brace_count = 0;
            let mut end_pos = start_pos;
            let chars: Vec<char> = new_content.chars().collect();
            
            // Find the matching closing brace by counting braces
            for (i, &ch) in chars.iter().enumerate().skip(start_pos) {
                if ch == '{' {
                    brace_count += 1;
                } else if ch == '}' {
                    brace_count -= 1;
                    if brace_count == 0 {
                        end_pos = i + 1;
                        break;
                    }
                }
            }
            
            // Remove the entire host block
            if end_pos > start_pos {
                new_content = format!("{}{}", 
                    &new_content[..start_pos], 
                    &new_content[end_pos..]
                );
            }
        }
        
        // Clean up multiple blank lines
        let re_blank = Regex::new(r"\n\s*\n{2,}").unwrap();
        new_content = re_blank.replace_all(&new_content, "\n\n").to_string();
    }
    new_content = new_content.trim_end().to_string() + "\n\n" + dhcp_entry;
    let temp_path = format!("{}/dhcp_clients_{}.tmp", backup_dir, std::process::id());
    match fs::write(&temp_path, &new_content) {
        Ok(_) => {
            // Use sudo to move the temporary file to the actual config path
            let output = Command::new("sudo")
                .args(&["mv", &temp_path, DHCP_CLIENTS_PATH])
                .output()
                .map_err(|e| format!("Failed to move DHCP config with sudo: {}", e))?;

            if output.status.success() {
                append_log("INFO", &format!("DHCP config updated for client {}", client_id));
                let _ = fs::remove_file(&dhcp_backup_path);
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                let _ = fs::remove_file(&dhcp_backup_path);
                let msg = format!("Failed to write DHCP config (sudo mv failed): {}\n{}", stderr, stdout);
                append_log("ERROR", &msg);
                Err(msg)
            }
        }
        Err(e) => {
            let _ = fs::remove_file(&dhcp_backup_path);
            let msg = format!("Failed to write temporary DHCP config: {}", e);
            append_log("ERROR", &msg);
            Err(msg)
        }
    }
}

pub fn format_client_name(name: &str) -> String {
    if let Some(idx) = name.find('_') {
        if let Ok(num) = name[idx + 1..].parse::<u32>() {
            return format!("PC{:03}", num);
        }
    }
    name.to_uppercase()
}

pub fn create_dhcp_entry(name: &str, mac: &str, ip: &str, target_iqn: &str) -> String {
    let formatted_name = format_client_name(name);
    let entry = format!(
        r#"host {formatted_name} {{
    hardware ethernet {mac};
    fixed-address {ip};
    option host-name "{formatted_name}";
    option root-path "iscsi:{server_ip}::::{target_iqn}";
}}"#,
        formatted_name = formatted_name,
        mac = mac,
        ip = ip,
        target_iqn = target_iqn,
        server_ip = get_server_ip(),
    );
    append_log("DEBUG", &format!("create_dhcp_entry for {} -> {} bytes", name, entry.len()));
    entry
}
