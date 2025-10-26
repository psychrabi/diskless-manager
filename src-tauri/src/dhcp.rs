use regex::Regex;
use tokio::fs as async_fs;
use crate::{utils::{get_server_ip, append_log, run_command, run_command_output}, DHCP_CONFIG_PATH};

pub async fn update_dhcp_config(client_id: &str, dhcp_entry: &str, is_new: bool) -> Result<(), String> {
    append_log("INFO", &format!("update_dhcp_config: client_id={}, is_new={}", client_id, is_new));

    // Read current config async
    let mut content = match run_command_output(["cat", DHCP_CONFIG_PATH]) {
        Ok(output) => output,
        Err(e) => {
            let msg = format!("DHCP read failed: {}", e);
            append_log("ERROR", &msg);
            return Err(msg);
        }
    };

    // Backup
    let backup_dir = "/srv/tftp/backups";
    async_fs::create_dir_all(backup_dir).await
        .map_err(|e| format!("Failed to create backup dir: {}", e))?;
    let pid = std::process::id();
    let dhcp_backup_path = format!("{}/dhcp_clients_{}.bak", backup_dir, pid);
    async_fs::write(&dhcp_backup_path, &content).await
        .map_err(|e| format!("Backup failed: {}", e))?;

    // Remove old entry if not new
    if !is_new {
        let formatted_name = format_client_name(client_id);
        let host_block_re = Regex::new(&format!(
            concat!(
                r#"host\s+{}\s*\{{ \s*hardware\s+ethernet\s+[^;]+\s*;\s+fixed-address\s+[^;]+\s*;\s+option\s+host-name\s+"[^"]*"\s*;"#,
                r#"\s+option\s+root-path\s+"[^"]*"\s*;\s*\}}"#
            ),
            regex::escape(&formatted_name)
        )).map_err(|e| format!("Regex error: {}", e))?;

        content = host_block_re.replace_all(&content, "").to_string();

        // Normalize whitespace
        let blank_re = Regex::new(r"\n\s*\n{2,}").unwrap();
        content = blank_re.replace_all(&content, "\n\n").to_string();
    }

    // Append new entry
    content = format!("{}\n\n{}", content.trim_end(), dhcp_entry);

    // Write via temp file + sudo mv
    let temp_path = format!("{}/dhcp_clients_{}.tmp", backup_dir, pid);
    async_fs::write(&temp_path, &content).await
        .map_err(|e| format!("Temp write failed: {}", e))?;

    if let Err(e) = run_command(["mv", &temp_path, DHCP_CONFIG_PATH]) {
        let msg = format!("Sudo mv failed: {}", e);
        append_log("ERROR", &msg);
        let _ = async_fs::remove_file(&dhcp_backup_path).await;
        let _ = async_fs::remove_file(&temp_path).await;
        return Err(msg);
    }
    
    append_log("INFO", &format!("DHCP updated for {}", client_id));
    let _ = async_fs::remove_file(&dhcp_backup_path).await;
    Ok(())
}

pub fn format_client_name(name: &str) -> String {
    name.find('_')
        .and_then(|idx| name[idx + 1..].parse::<u32>().ok())
        .map(|num| format!("PC{:03}", num))
        .unwrap_or_else(|| name.to_uppercase())
}

pub fn create_dhcp_entry(name: &str, mac: &str, ip: &str, target_iqn: &str) -> String {
    let formatted_name = format_client_name(name);
    let server_ip = get_server_ip();
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
        server_ip = server_ip,
    );
    append_log("DEBUG", &format!("DHCP entry for {}: {} bytes", name, entry.len()));
    entry
}