use serde::Serialize;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
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
        sudo_command(["mkdir", path_str(&subsystem)?])?;
    }

    write_attr(&subsystem.join("attr_allow_any_host"), "1")?;

    let namespace = subsystem.join("namespaces/1");
    if !namespace.exists() {
        sudo_command(["mkdir", "-p", path_str(&namespace)?])?;
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
        sudo_command([
            "ln",
            "-s",
            path_str(&subsystem)?,
            path_str(&link)?,
        ])?;
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
        sudo_command(["rm", "-f", path_str(&link)?])?;
    }

    let subsystem = subsystem_path(nqn);
    let namespace = subsystem.join("namespaces/1");
    if namespace.exists() {
        if namespace.join("enable").exists() {
            write_attr(&namespace.join("enable"), "0")?;
        }
        sudo_command(["rmdir", path_str(&namespace)?])?;
    }

    let namespaces = subsystem.join("namespaces");
    if namespaces.exists() {
        let _ = sudo_command(["rmdir", path_str(&namespaces)?]);
    }

    if subsystem.exists() {
        sudo_command(["rmdir", path_str(&subsystem)?])?;
    }

    Ok(())
}

/// Remove every diskless-manager NVMe-oF export backed by `block_device`.
///
/// Storage reset/delete paths call this before destroying a ZVOL so an
/// enabled nvmet namespace cannot keep the block device busy. Only managed
/// NQNs are considered, and the namespace device path must match exactly.
pub fn remove_exports_for_block_device(block_device: &Path) -> Result<Vec<String>, String> {
    let requested = block_device.to_string_lossy().into_owned();
    if !requested.starts_with("/dev/zvol/") {
        return Ok(Vec::new());
    }

    let subsystems = Path::new(NVMET_ROOT).join("subsystems");
    if !subsystems.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(&subsystems).map_err(|error| {
        format!(
            "failed to inspect NVMe target subsystems in {}: {error}",
            subsystems.display()
        )
    })?;

    let mut matching_nqns = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "failed to inspect an NVMe target subsystem in {}: {error}",
                subsystems.display()
            )
        })?;

        let Some(nqn) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };

        if !nqn.starts_with(NQN_PREFIX) {
            continue;
        }

        let status = inspect_export(&nqn)?;
        if status.block_device.as_deref() == Some(requested.as_str()) {
            matching_nqns.push(nqn);
        }
    }

    for nqn in &matching_nqns {
        tracing::info!(
            nqn = %nqn,
            block_device = %requested,
            "removing NVMe-oF export before ZVOL destruction"
        );
        remove_export(nqn)?;
    }

    Ok(matching_nqns)
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
        sudo_command(["modprobe", module])?;
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
        sudo_command(["mkdir", path_str(&port)?])?;
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

fn path_str(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("path is not valid UTF-8: {}", path.display()))
}

fn sudo_command<const N: usize>(args: [&str; N]) -> Result<(), String> {
    let status = Command::new("sudo")
        .arg("-n")
        .args(args)
        .status()
        .map_err(|e| format!("failed to execute privileged NVMe target command: {e}"))?;

    if !status.success() {
        return Err(format!(
            "privileged NVMe target command failed with status {status}; ensure diskless-manager has passwordless sudo for nvmet setup commands"
        ));
    }

    Ok(())
}

fn write_attr(path: &Path, value: &str) -> Result<(), String> {
    let path = path_str(path)?;
    let mut child = Command::new("sudo")
        .arg("-n")
        .arg("tee")
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn privileged write for {path}: {e}"))?;

    {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| format!("failed to open stdin for privileged write to {path}"))?;
        stdin
            .write_all(value.as_bytes())
            .map_err(|e| format!("failed to write NVMe target attribute {path}: {e}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("failed waiting for privileged write to {path}: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "failed to write NVMe target attribute {path}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(())
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