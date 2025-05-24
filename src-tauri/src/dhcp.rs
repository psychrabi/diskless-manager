//! DHCP-related logic for config file management and helpers.

use regex::Regex;
use std::fs;

pub fn update_dhcp_config(client_id: &str, dhcp_entry: &str, is_new: bool) -> Result<(), String> {
    use crate::DHCP_CONFIG_PATH;
    let content = fs::read_to_string(DHCP_CONFIG_PATH)
        .map_err(|e| format!("Failed to read DHCP config: {}", e))?;
    let dhcp_backup_path = format!("{}.bak", DHCP_CONFIG_PATH);
    fs::write(&dhcp_backup_path, &content)
        .map_err(|e| format!("Failed to backup DHCP config: {}", e))?;
    let mut new_content = content.clone();
    if !is_new {
        let formatted_name = format_client_name(client_id);
        let host_pattern = format!(
            r"host\s+{}\s*\{{(?:[^\{{\}}]|(?:\{{[^\{{\}}]*\}}))*\}}\s*",
            regex::escape(&formatted_name)
        );
        let re = Regex::new(&host_pattern).map_err(|e| format!("Regex error: {}", e))?;
        new_content = re.replace(&new_content, "").to_string();
        let re_blank = Regex::new(r"\n\s*\n{2,}").unwrap();
        new_content = re_blank.replace_all(&new_content, "\n\n").to_string();
    }
    new_content = new_content.trim_end().to_string() + "\n\n" + dhcp_entry;
    match fs::write(DHCP_CONFIG_PATH, &new_content) {
        Ok(_) => {
            let _ = fs::remove_file(&dhcp_backup_path);
            Ok(())
        }
        Err(e) => {
            let _ = fs::write(DHCP_CONFIG_PATH, &content);
            let _ = fs::remove_file(&dhcp_backup_path);
            Err(format!("Failed to write DHCP config: {}", e))
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
    use crate::SERVER_IP;

    let formatted_name = format_client_name(name);
    format!(
        r#"host {formatted_name} {{
    hardware ethernet {mac};
    fixed-address {ip};
    option host-name "{formatted_name}";
    if substring (option vendor-class-identifier, 15, 5) = "00000" {{
        filename "ipxe.kpxe";
    }}
    elsif substring (option vendor-class-identifier, 15, 5) = "00006" {{
        filename "ipxe32.efi";
    }}
    else {{
        filename "ipxe.efi";
    }}
    option root-path "iscsi:{server_ip}::::{target_iqn}";
}}"#,
        formatted_name = formatted_name,
        mac = mac,
        ip = ip,
        target_iqn = target_iqn,
        server_ip = SERVER_IP.to_string(),
    )
}
