use serde::Serialize;
use std::{
    fs,
    io::Write,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::Command,
};

pub const NVMET_ROOT: &str = "/sys/kernel/config/nvmet";
pub const NVMET_PORT_ID: &str = "1";
pub const NVMET_TCP_PORT: u16 = 4420;
pub const NQN_PREFIX: &str = "nqn.2026-09.local.diskless:client.";

#[derive(Debug, Clone, Serialize)]
pub struct NvmeOfExportStatus {
    pub nqn: String,
    pub block_device: Option<String>,
    pub subsystem_present: bool,
    pub namespace_enabled: bool,
    pub port_attached: bool,
    pub tcp_port: u16,
    pub allow_any_host: bool,
    pub experimental: bool,
}

#[must_use]
pub fn nqn_for_client(client_name: &str) -> String {
    let suffix: String = client_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();

    format!("{NQN_PREFIX}{suffix}")
}

pub fn ensure_export(nqn: &str, block_device: &Path) -> Result<NvmeOfExportStatus, String> {
    validate_nqn(nqn)?;
    validate_block_device(block_device)?;
    ensure_kernel_support()?;
    ensure_nvmet_root()?;
    ensure_tcp_port()?;

    let subsystem = subsystem_path(nqn);
    if !subsystem.exists() {
        fs::create_dir(&subsystem)
            .map_err(|e| format!("failed to create NVMe subsystem {nqn}: {e}"))?;
    }

    // Initial Windows boot experiment: do not require host authentication yet.
    // This keeps firmware/Windows handoff testing separate from DH-HMAC-CHAP.
    write_attr(&subsystem.join("attr_allow_any_host"), "1")?;

    let namespace = subsystem.join("namespaces/1");
    if !namespace.exists() {
        fs::create_dir_all(&namespace)
            .map_err(|e| format!("failed to create namespace for {nqn}: {e}"))?;
    }

    let requested = block_device.to_string_lossy().into_owned();
    let enabled = read_attr(&namespace.join("enable")).as_deref() == Some("1");

    if enabled {
        let current = read_attr(&namespace.join("device_path"));
        if current.as_deref() != Some(requested.as_str()) {
            return Err(format!(
                "namespace {nqn} already points at {}, requested {requested}",
                current.unwrap_or_else(|| "an unknown block device".into())
            ));
        }
    } else {
        write_attr(&namespace.join("device_path"), &requested)?;
        write_attr(&namespace.join("enable"), "1")?;
    }

    let link = port_link(nqn);
    if link.symlink_metadata().is_err() {
        symlink(&subsystem, &link)
            .map_err(|e| format!("failed to attach {nqn} to NVMe/TCP port: {e}"))?;
    }

    inspect_export(nqn)
}

pub fn inspect_export(nqn: &str) -> Result<NvmeOfExportStatus, String> {
    validate_nqn(nqn)?;

    let subsystem = subsystem_path(nqn);
    let namespace = subsystem.join("namespaces/1");

    Ok(NvmeOfExportStatus {
        nqn: nqn.to_owned(),
        block_device: read_attr(&namespace.join("device_path")),
        subsystem_present: subsystem.exists(),
        namespace_enabled: read_attr(&namespace.join("enable")).as_deref() == Some("1"),
        port_attached: port_link(nqn).symlink_metadata().is_ok(),
        tcp_port: NVMET_TCP_PORT,
        allow_any_host: read_attr(&subsystem.join("attr_allow_any_host")).as_deref() == Some("1"),
        experimental: true,
    })
}

pub fn remove_export(nqn: &str) -> Result<(), String> {
    validate_nqn(nqn)?;

    let link = port_link(nqn);
    if link.symlink_metadata().is_ok() {
        fs::remove_file(&link)
            .map_err(|e| format!("failed to detach {nqn} from NVMe/TCP port: {e}"))?;
    }

    let subsystem = subsystem_path(nqn);
    let namespace = subsystem.join("namespaces/1");
    if namespace.exists() {
        if namespace.join("enable").exists() {
            write_attr(&namespace.join("enable"), "0")?;
        }
        fs::remove_dir(&namespace)
            .map_err(|e| format!("failed to remove namespace for {nqn}: {e}"))?;
    }

    let namespaces = subsystem.join("namespaces");
    if namespaces.exists() {
        let _ = fs::remove_dir(&namespaces);
    }

    if subsystem.exists() {
        fs::remove_dir(&subsystem)
            .map_err(|e| format!("failed to remove NVMe subsystem {nqn}: {e}"))?;
    }

    Ok(())
}

fn validate_nqn(nqn: &str) -> Result<(), String> {
    if !nqn.starts_with(NQN_PREFIX) || nqn.len() <= NQN_PREFIX.len() {
        return Err(format!("refusing unmanaged NQN; expected prefix {NQN_PREFIX}"));
    }
    if nqn.contains('/') || nqn.contains("..") {
        return Err("invalid NQN path characters".into());
    }
    Ok(())
}

fn validate_block_device(path: &Path) -> Result<(), String> {
    let value = path.to_string_lossy();
    if !value.starts_with("/dev/zvol/") {
        return Err(format!(
            "experimental NVMe-oF exports are restricted to /dev/zvol devices (got {value})"
        ));
    }
    if !path.exists() {
        return Err(format!("ZVOL block device does not exist: {value}"));
    }
    Ok(())
}

fn ensure_kernel_support() -> Result<(), String> {
    for module in ["nvmet", "nvmet-tcp"] {
        let status = Command::new("modprobe")
            .arg(module)
            .status()
            .map_err(|e| format!("failed to execute modprobe {module}: {e}"))?;
        if !status.success() {
            return Err(format!("modprobe {module} failed with status {status}"));
        }
    }
    Ok(())
}

fn ensure_nvmet_root() -> Result<(), String> {
    let root = Path::new(NVMET_ROOT);
    for required in [root.to_path_buf(), root.join("subsystems"), root.join("ports")] {
        if !required.exists() {
            return Err(format!("required nvmet configfs path is missing: {}", required.display()));
        }
    }
    Ok(())
}

fn ensure_tcp_port() -> Result<(), String> {
    let port = Path::new(NVMET_ROOT).join("ports").join(NVMET_PORT_ID);
    if !port.exists() {
        fs::create_dir(&port)
            .map_err(|e| format!("failed to create nvmet port {NVMET_PORT_ID}: {e}"))?;
    }

    let current_type = read_attr(&port.join("addr_trtype")).unwrap_or_default();
    if current_type.is_empty() {
        write_attr(&port.join("addr_trsvcid"), &NVMET_TCP_PORT.to_string())?;
        write_attr(&port.join("addr_traddr"), "0.0.0.0")?;
        write_attr(&port.join("addr_adrfam"), "ipv4")?;
        write_attr(&port.join("addr_trtype"), "tcp")?;
    } else {
        let service = read_attr(&port.join("addr_trsvcid")).unwrap_or_default();
        if current_type != "tcp" || service != NVMET_TCP_PORT.to_string() {
            return Err(format!(
                "nvmet port {NVMET_PORT_ID} is already configured as {current_type}/{service}; expected tcp/{NVMET_TCP_PORT}"
            ));
        }
    }

    if !port.join("subsystems").exists() {
        return Err(format!("nvmet port {} has no subsystems directory", port.display()));
    }

    Ok(())
}

fn subsystem_path(nqn: &str) -> PathBuf {
    Path::new(NVMET_ROOT).join("subsystems").join(nqn)
}

fn port_link(nqn: &str) -> PathBuf {
    Path::new(NVMET_ROOT)
        .join("ports")
        .join(NVMET_PORT_ID)
        .join("subsystems")
        .join(nqn)
}

fn write_attr(path: &Path, value: &str) -> Result<(), String> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|e| format!("failed to open {}: {e}", path.display()))?;
    file.write_all(value.as_bytes())
        .map_err(|e| format!("failed to write {}: {e}", path.display()))
}

fn read_attr(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|value| value.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_client_nqn() {
        assert_eq!(
            nqn_for_client("PC001"),
            "nqn.2026-09.local.diskless:client.pc001"
        );
        assert_eq!(
            nqn_for_client("Lab PC/02"),
            "nqn.2026-09.local.diskless:client.lab_pc_02"
        );
    }

    #[test]
    fn rejects_unmanaged_nqn() {
        assert!(validate_nqn("nqn.example:other").is_err());
        assert!(validate_nqn("nqn.2026-09.local.diskless:client.pc001").is_ok());
    }
}
