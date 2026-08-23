use crate::{cmd::run_command, DHCP_CLIENTS_PATH, DHCP_CONFIG_PATH};
use log::{error, info};
use regex::Regex;
use tokio::fs as async_fs;

pub async fn update_dhcp_config(
    client_id: &str,
    dhcp_entry: &str,
    _is_new: bool,
) -> Result<(), String> {
    info!("update_dhcp_config: client_id={}", client_id);

    let _lock = lock_dhcp_config()?;
    let content = std::fs::read_to_string(DHCP_CLIENTS_PATH).unwrap_or_default();
    let reconciled = reconcile_client_entry(
        &content,
        client_id,
        (!dhcp_entry.trim().is_empty()).then_some(dhcp_entry),
    );

    install_dhcp_config_file(&reconciled, DHCP_CLIENTS_PATH, "dhcp_clients").await
}

/// Atomically replace the manager-owned static client file, validate the
/// complete ISC DHCP configuration, and restore the previous file if the
/// staged configuration is invalid. Callers reload only after this succeeds.
pub async fn replace_dhcp_clients_config(content: &str) -> Result<(), String> {
    let _lock = lock_dhcp_config()?;
    install_dhcp_config_file(content, DHCP_CLIENTS_PATH, "dhcp_clients").await
}

/// Atomically replace the primary ISC DHCP configuration and restore it when
/// `dhcpd -t` rejects the staged content.
pub async fn replace_dhcp_config(content: &str) -> Result<(), String> {
    let _lock = lock_dhcp_config()?;
    install_dhcp_config_file(content, DHCP_CONFIG_PATH, "dhcpd_config").await
}

fn lock_dhcp_config() -> Result<std::fs::File, String> {
    let lock_path = std::env::temp_dir().join("diskless-manager-dhcp.lock");
    let lock_file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&lock_path)
        .map_err(|error| format!("Failed to open lock file: {error}"))?;

    use fs2::FileExt;
    lock_file
        .lock_exclusive()
        .map_err(|error| format!("Failed to acquire lock: {error}"))?;

    Ok(lock_file)
}

async fn install_dhcp_config_file(
    content: &str,
    destination: &str,
    backup_stem: &str,
) -> Result<(), String> {
    let original = std::fs::read_to_string(destination).unwrap_or_default();
    let backup_dir = "/srv/tftp/backups";
    async_fs::create_dir_all(backup_dir)
        .await
        .map_err(|error| format!("Failed to create backup dir: {error}"))?;

    let pid = std::process::id();
    let backup_path = format!("{backup_dir}/{backup_stem}_{pid}.bak");
    let temp_path = format!("{backup_dir}/{backup_stem}_{pid}.tmp");
    async_fs::write(&backup_path, &original)
        .await
        .map_err(|error| format!("Backup failed: {error}"))?;
    async_fs::write(&temp_path, content)
        .await
        .map_err(|error| format!("Temp write failed: {error}"))?;

    if let Err(error) = run_command(["mv", &temp_path, destination]) {
        let _ = async_fs::remove_file(&backup_path).await;
        return Err(format!("Failed to install DHCP configuration: {error}"));
    }

    if let Err(error) = run_command(["dhcpd", "-t", "-cf", crate::DHCP_CONFIG_PATH]) {
        error!("DHCP configuration validation failed; restoring {destination}: {error}");
        async_fs::write(&temp_path, &original)
            .await
            .map_err(|restore_error| {
                format!(
                    "DHCP validation failed ({error}) and restoring clients.conf failed: {restore_error}"
                )
            })?;
        run_command(["mv", &temp_path, destination]).map_err(|restore_error| {
            format!(
                "DHCP validation failed ({error}) and restoring clients.conf failed: {restore_error}"
            )
        })?;
        let _ = async_fs::remove_file(&backup_path).await;
        return Err(format!("DHCP configuration validation failed: {error}"));
    }

    let _ = async_fs::remove_file(&backup_path).await;
    info!("DHCP configuration {} validated and installed", destination);
    Ok(())
}

/// Replace or remove one manager-owned DHCP host block without disturbing
/// comments or host blocks owned by an operator. The result always has a
/// stable trailing newline so applying the same desired entry twice is a no-op.
pub fn reconcile_client_entry(
    content: &str,
    client_name: &str,
    desired_entry: Option<&str>,
) -> String {
    let formatted_name = format_client_name(client_name);
    let host_block_re = Regex::new(&format!(
        r#"(?ms)^[ \t]*host\s+{}\s*\{{.*?^[ \t]*\}}[ \t]*(?:\n|$)"#,
        regex::escape(&formatted_name)
    ))
    .expect("formatted DHCP host name must produce a valid regex");

    let remaining = host_block_re.replace_all(content, "");
    let remaining = remaining.trim_end();

    match desired_entry
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
    {
        Some(entry) if remaining.is_empty() => format!("{entry}\n"),
        Some(entry) => format!("{remaining}\n\n{entry}\n"),
        None if remaining.is_empty() => String::new(),
        None => format!("{remaining}\n"),
    }
}

pub fn format_client_name(name: &str) -> String {
    name.find('_')
        .and_then(|idx| name[idx + 1..].parse::<u32>().ok())
        .map(|num| format!("PC{:03}", num))
        .unwrap_or_else(|| name.to_uppercase())
}

/// Build a static host entry using the configured iSCSI portal rather than
/// discovering an unrelated host interface at reconciliation time.
pub fn create_dhcp_entry_for_server(
    name: &str,
    mac: &str,
    ip: &str,
    target_iqn: &str,
    server_ip: &str,
) -> String {
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
        server_ip = server_ip.trim(),
    );

    info!("DHCP entry for {}: {} bytes", name, entry.len());

    entry
}

/// Check whether a client's DHCP host block exactly matches the desired entry.
pub fn dhcp_entry_matches(content: &str, client_name: &str, desired_entry: &str) -> bool {
    let formatted_name = format_client_name(client_name);

    let host_block_re = Regex::new(&format!(
        r#"(?s)host\s+{}\s*\{{.*?\}}"#,
        regex::escape(&formatted_name)
    ));

    let Ok(regex) = host_block_re else {
        return false;
    };

    let desired = normalize_dhcp_block(desired_entry);

    let matches = regex
        .find_iter(content)
        .any(|matched| normalize_dhcp_block(matched.as_str()) == desired);

    matches
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
    fn configured_server_is_used_for_the_iscsi_root_path() {
        let entry = create_dhcp_entry_for_server(
            "client_1",
            "00:11:22:33:44:55",
            "192.168.1.100",
            "iqn.test",
            "192.168.1.250",
        );

        assert!(entry.contains("option root-path \"iscsi:192.168.1.250::::iqn.test\";"));
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

    #[test]
    fn reconcile_client_entry_replaces_only_the_managed_host_block() {
        let current = r#"# operator-maintained comment
host PC001 {
hardware ethernet 00:11:22:33:44:55;
fixed-address 192.168.1.101;
}

host PC002 {
hardware ethernet 00:11:22:33:44:66;
fixed-address 192.168.1.102;
}
"#;
        let desired = r#"host PC001 {
hardware ethernet 00:11:22:33:44:55;
fixed-address 192.168.1.100;
option host-name "PC001";
option root-path "iscsi:192.168.1.250::::iqn.test";
}"#;

        let reconciled = reconcile_client_entry(current, "client_1", Some(desired));

        assert!(reconciled.contains("# operator-maintained comment"));
        assert!(reconciled.contains(desired));
        assert!(reconciled.contains("host PC002 {"));
        assert!(!reconciled.contains("fixed-address 192.168.1.101;"));
    }

    #[test]
    fn reconcile_client_entry_is_idempotent() {
        let desired = r#"host PC001 {
hardware ethernet 00:11:22:33:44:55;
fixed-address 192.168.1.100;
option host-name "PC001";
option root-path "iscsi:192.168.1.250::::iqn.test";
}"#;

        let once = reconcile_client_entry("", "client_1", Some(desired));
        let twice = reconcile_client_entry(&once, "client_1", Some(desired));

        assert_eq!(twice, once);
        assert_eq!(twice.matches("host PC001 {").count(), 1);
    }

    #[test]
    fn reconcile_client_entry_removes_the_managed_host_and_preserves_other_content() {
        let current = r#"# operator-maintained comment
host PC001 {
hardware ethernet 00:11:22:33:44:55;
fixed-address 192.168.1.100;
}

host PC002 {
hardware ethernet 00:11:22:33:44:66;
fixed-address 192.168.1.102;
}
"#;

        let reconciled = reconcile_client_entry(current, "client_1", None);

        assert!(reconciled.contains("# operator-maintained comment"));
        assert!(reconciled.contains("host PC002 {"));
        assert!(!reconciled.contains("host PC001 {"));
    }
}
