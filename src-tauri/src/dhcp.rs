use crate::{
    cmd::{get_server_ip, run_command},
    DHCP_CLIENTS_PATH,
};
use log::{error, info};
use regex::Regex;
use tokio::fs as async_fs;

pub async fn update_dhcp_config(
    client_id: &str,
    dhcp_entry: &str,
    is_new: bool,
) -> Result<(), String> {
    info!(
        "update_dhcp_config: client_id={}, is_new={}",
        client_id, is_new
    );

    let lock_path = std::env::temp_dir().join("diskless-manager-dhcp.lock");

    let lock_file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&lock_path)
        .map_err(|error| format!("Failed to open lock file: {}", error))?;

    use fs2::FileExt;

    lock_file
        .lock_exclusive()
        .map_err(|error| format!("Failed to acquire lock: {}", error))?;

    let mut content = std::fs::read_to_string(DHCP_CLIENTS_PATH).unwrap_or_default();

    let backup_dir = "/srv/tftp/backups";

    async_fs::create_dir_all(backup_dir)
        .await
        .map_err(|error| format!("Failed to create backup dir: {}", error))?;

    let pid = std::process::id();

    let dhcp_backup_path = format!("{}/dhcp_clients_{}.bak", backup_dir, pid);

    async_fs::write(&dhcp_backup_path, &content)
        .await
        .map_err(|error| format!("Backup failed: {}", error))?;

    if !is_new {
        let formatted_name = format_client_name(client_id);

        let host_block_re = Regex::new(&format!(
            concat!(
                r#"host\s+{}\s*\{{\s*"#,
                r#"hardware\s+ethernet\s+[^;]+\s*;\s+"#,
                r#"fixed-address\s+[^;]+\s*;\s+"#,
                r#"option\s+host-name\s+"[^"]*"\s*;\s+"#,
                r#"option\s+root-path\s+"[^"]*"\s*;\s*\}}"#
            ),
            regex::escape(&formatted_name)
        ))
        .map_err(|error| format!("Regex error: {}", error))?;

        content = host_block_re.replace_all(&content, "").to_string();

        let blank_re = Regex::new(r"\n\s*\n{2,}")
            .map_err(|error| format!("Failed to compile blank line regex: {}", error))?;

        content = blank_re.replace_all(&content, "\n\n").to_string();
    }

    if !is_new && dhcp_entry.trim().is_empty() {
        let temp_path = format!("{}/dhcp_clients_{}.tmp", backup_dir, pid);

        async_fs::write(&temp_path, content.trim_end())
            .await
            .map_err(|error| format!("Temp write failed: {}", error))?;

        if let Err(error) = run_command(["mv", &temp_path, DHCP_CLIENTS_PATH]) {
            let message = format!("Sudo mv failed: {}", error);

            error!("{}", message);

            let _ = async_fs::remove_file(&dhcp_backup_path).await;
            let _ = async_fs::remove_file(&temp_path).await;

            return Err(message);
        }

        info!("DHCP entry removed for {}", client_id);

        let _ = async_fs::remove_file(&dhcp_backup_path).await;

        return Ok(());
    }

    content = if content.trim().is_empty() {
        dhcp_entry.to_string()
    } else {
        format!("{}\n\n{}", content.trim_end(), dhcp_entry)
    };

    let temp_path = format!("{}/dhcp_clients_{}.tmp", backup_dir, pid);

    async_fs::write(&temp_path, &content)
        .await
        .map_err(|error| format!("Temp write failed: {}", error))?;

    if let Err(error) = run_command(["mv", &temp_path, DHCP_CLIENTS_PATH]) {
        let message = format!("Sudo mv failed: {}", error);

        error!("{}", message);

        let _ = async_fs::remove_file(&dhcp_backup_path).await;
        let _ = async_fs::remove_file(&temp_path).await;

        return Err(message);
    }

    info!("DHCP updated for {}", client_id);

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

    info!("DHCP entry for {}: {} bytes", name, entry.len());

    entry
}

/// Check whether a client's DHCP host block exactly matches the desired entry.
pub fn dhcp_entry_matches(content: &str, client_name: &str, desired_entry: &str) -> bool {
    let formatted_name = format_client_name(client_name);
    let host_block_re = Regex::new(&format!(
        concat!(
            r#"(?ms)^\s*host\s+{}\s*\{{.*?^\s*\}}\s*$"#
        ),
        regex::escape(&formatted_name)
    ));

    let Ok(regex) = host_block_re else {
        return false;
    };

    regex
        .find_iter(content)
        .any(|matched| normalize_dhcp_block(matched.as_str()) == normalize_dhcp_block(desired_entry))
}

fn normalize_dhcp_block(value: &str) -> String {
    value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
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
        assert!(entry.contains("option root-path \"iscsi:"));
        assert!(entry.contains("::::iqn.test\";"));
    }

    #[test]
    fn test_dhcp_entry_matches_exact_block() {
        let desired = r#"host PC001 {
hardware ethernet 00:11:22:33:44:55;
fixed-address 192.168.1.100;
option host-name "PC001";
option root-path "iscsi:192.168.1.1::::iqn.test";
}"#;

        let content = format!("# header\n\n{}\n\n# footer\n", desired);

        assert!(dhcp_entry_matches(&content, "client_1", desired));
    }

    #[test]
    fn test_dhcp_entry_matches_detects_drift() {
        let desired = r#"host PC001 {
hardware ethernet 00:11:22:33:44:55;
fixed-address 192.168.1.100;
option host-name "PC001";
option root-path "iscsi:192.168.1.1::::iqn.test";
}"#;

        let drifted = desired.replace("192.168.1.100", "192.168.1.101");

        assert!(!dhcp_entry_matches(&drifted, "client_1", desired));
    }
}
