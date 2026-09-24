//! Native LIO operations, dispatched through the installed application's private CLI.
mod engine;
mod fs;
#[cfg(test)]
mod tests;
mod transport;

use super::{
    ChapCredentials, IscsiLunState, IscsiProvisionResult, IscsiTargetSpec, IscsiTargetState,
};
use anyhow::{bail, ensure, Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    path::Path,
    process::Command,
};
pub use transport::ConfigfsProvisioner;

pub const EXECUTABLE: &str = "/usr/bin/diskless-manager";
pub const SUBCOMMAND: &str = "internal-iscsi";
const ROOT: &str = "/sys/kernel/config/target";
const VERSION: u32 = 1;
const MAX_REQUEST: u64 = 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u32,
    #[serde(flatten)]
    operation: Operation,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "operation", content = "arguments", rename_all = "snake_case")]
enum Operation {
    Probe,
    Save,
    Create(IscsiTargetSpec),
    RemoveOwned {
        target: String,
        backstores: Vec<String>,
    },
    RemoveTarget {
        target: String,
    },
    Exists {
        target: String,
    },
    List {
        target: String,
    },
    Inspect(IscsiTargetSpec),
    SetChap {
        target: String,
        chap: Option<ChapCredentials>,
    },
}

#[derive(Serialize, Deserialize)]
struct Response {
    version: u32,
    result: std::result::Result<Reply, String>,
}
#[derive(Serialize, Deserialize)]
enum Reply {
    Unit,
    Exists(bool),
    Created(IscsiProvisionResult),
    Luns(Vec<IscsiLunState>),
    State(IscsiTargetState),
}

fn identifier(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 223
            && value != "."
            && value != ".."
            && value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._:-".contains(&c)),
        "invalid iSCSI identifier"
    );
    Ok(())
}

fn validate_spec(spec: &IscsiTargetSpec) -> Result<()> {
    identifier(&spec.target_iqn)?;
    ensure!(spec.target_iqn.starts_with("iqn."), "invalid target IQN");
    spec.portal_address
        .parse::<std::net::IpAddr>()
        .context("invalid portal address")?;
    ensure!(spec.portal_port != 0, "invalid portal port");
    ensure!(
        !spec.luns.is_empty() && spec.luns.len() <= 256,
        "invalid LUN count"
    );
    let mut numbers = std::collections::HashSet::new();
    let mut names = std::collections::HashSet::new();
    for lun in &spec.luns {
        identifier(&lun.backstore)?;
        ensure!(
            lun.lun <= 65535 && numbers.insert(lun.lun) && names.insert(&lun.backstore),
            "invalid or duplicate LUN/backstore"
        );
        let path = lun.block_device.to_str().context("invalid device path")?;
        ensure!(
            path.starts_with("/dev/zvol/")
                && !path.split('/').any(|p| p == "." || p == "..")
                && path
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"/_-.:".contains(&c)),
            "device must be a ZFS block volume"
        );
    }
    validate_chap(spec.chap.as_ref())
}

fn validate_chap(chap: Option<&ChapCredentials>) -> Result<()> {
    if let Some(chap) = chap {
        identifier(&chap.username)?;
        ensure!(
            (12..=16).contains(&chap.password.len())
                && chap.password.bytes().all(|b| b.is_ascii_alphanumeric()),
            "invalid CHAP credentials"
        );
    }
    Ok(())
}

fn lock(path: &str) -> Result<std::fs::File> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    file.lock_exclusive()?;
    Ok(file)
}

fn save() -> Result<()> {
    let output = Command::new("/usr/bin/targetcli")
        .arg("saveconfig")
        .output()
        .context("start targetcli persistence")?;
    // Do not include subprocess output; it can contain authentication settings.
    ensure!(
        output.status.success(),
        "targetcli saveconfig failed ({})",
        output.status
    );
    Ok(())
}

/// Private command entrypoint. No application state, database, or GUI startup.
pub fn run_command() -> Result<()> {
    let result = (|| -> Result<Reply> {
        let status = std::fs::read_to_string("/proc/self/status")?;
        let root = status
            .lines()
            .find(|s| s.starts_with("Uid:"))
            .and_then(|s| s.split_whitespace().nth(2))
            == Some("0");
        ensure!(root, "internal iSCSI command requires root");
        let mut input = Vec::new();
        std::io::stdin()
            .take(MAX_REQUEST + 1)
            .read_to_end(&mut input)?;
        ensure!(input.len() as u64 <= MAX_REQUEST, "iSCSI request too large");
        // Never echo malformed input: it may contain CHAP secrets.
        let request: Request =
            serde_json::from_slice(&input).map_err(|_| anyhow::anyhow!("invalid iSCSI request"))?;
        ensure!(request.version == VERSION, "unsupported iSCSI protocol");
        if matches!(request.operation, Operation::Probe) {
            ensure!(
                Path::new(ROOT).join("iscsi").is_dir(),
                "LIO configfs is not initialized"
            );
            return Ok(Reply::Unit);
        }
        let _transaction = lock("/run/diskless-manager-iscsi.lock")?;
        let mut kernel_lock = Some(lock("/run/targetcli.lock")?);
        let engine = engine::Engine::new(fs::KernelFs, ROOT.into());
        // targetcli takes this same lock. Release it for persistence, and
        // reacquire it before rollback if persistence failed.
        let mut persist = || -> Result<()> {
            drop(kernel_lock.take());
            let result = save();
            kernel_lock = Some(lock("/run/targetcli.lock")?);
            result
        };
        engine.execute(request.operation, &mut persist)
    })();
    let failed = result.is_err();
    let response = Response {
        version: VERSION,
        result: result.map_err(|e| format!("{e:#}")),
    };
    serde_json::to_writer(std::io::stdout().lock(), &response)?;
    std::io::stdout().flush()?;
    if failed {
        bail!("native iSCSI operation failed");
    }
    Ok(())
}
