use regex::Regex;
use tokio::fs as async_fs;
use crate::{utils::{get_server_ip, append_log, run_command, run_command_output}, DHCP_CONFIG_PATH};

pub async fn update_dhcp_config(client_id: &str, dhcp_entry: &str, is_new: bool) -> Result<(), String> {
    append_log("INFO", &format!("update_dhcp_config: client_id={}, is_new={}", client_id, is_new));

    // Acquire lock to prevent race conditions
    let lock_path = std::env::temp_dir().join("diskless-manager-dhcp.lock");
    let lock_file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&lock_path)
        .map_err(|e| format!("Failed to open lock file: {}", e))?;

    use fs2::FileExt;
    lock_file.lock_exclusive().map_err(|e| format!("Failed to acquire lock: {}", e))?;

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
    
    // Lock is automatically released when lock_file goes out of scope
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_client_name() {
        assert_eq!(format_client_name("client_1"), "PC001");
        assert_eq!(format_client_name("client_10"), "PC010");
        assert_eq!(format_client_name("client_100"), "PC100");
        assert_eq!(format_client_name("my_pc"), "MY_PC");
        assert_eq!(format_client_name("test"), "TEST");
    }

    #[test]
    fn test_create_dhcp_entry() {
        let entry = create_dhcp_entry("client_1", "00:11:22:33:44:55", "192.168.1.100", "iqn.test");
        assert!(entry.contains("host PC001 {"));
        assert!(entry.contains("hardware ethernet 00:11:22:33:44:55;"));
        assert!(entry.contains("fixed-address 192.168.1.100;"));
        assert!(entry.contains("option host-name \"PC001\";"));
        // Note: server_ip might vary based on environment, so we just check structure
        assert!(entry.contains("option root-path \"iscsi:"));
        assert!(entry.contains("::::iqn.test\";"));
    }
}