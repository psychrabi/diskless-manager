use super::{Operation, Reply, Request, Response, EXECUTABLE, SUBCOMMAND, VERSION};
use crate::infrastructure::iscsi::{
    ChapCredentials, IscsiLunState, IscsiProvisionResult, IscsiProvisioner, IscsiTargetSpec,
    IscsiTargetState,
};
use anyhow::{bail, ensure, Context, Result};
use std::{
    io::Write,
    process::{Command, Stdio},
};

/// Native LIO backend. Mutations run inside the installed application's
/// private `internal-iscsi` command via sudo. Never retry a failed native
/// mutation through targetcli.
#[derive(Debug, Clone, Copy, Default)]
pub struct ConfigfsProvisioner;
impl ConfigfsProvisioner {
    pub const fn new() -> Self {
        Self
    }
    pub(crate) fn probe() -> bool {
        matches!(Self::request(Operation::Probe), Ok(Reply::Unit))
    }
    /// Persist LIO state for boot. Native when available, otherwise the
    /// legacy `targetcli saveconfig` (old installations only).
    pub(crate) fn save_config() -> Result<()> {
        if Self::probe() {
            return Self::unit(Operation::Save);
        }
        crate::infrastructure::command::run_command(["targetcli", "saveconfig"])
            .map_err(anyhow::Error::from)
            .context("targetcli saveconfig failed")
    }
    fn request(operation: Operation) -> Result<Reply> {
        let input = serde_json::to_vec(&Request {
            version: VERSION,
            operation,
        })?;
        ensure!(
            input.len() as u64 <= super::MAX_REQUEST,
            "iSCSI request too large"
        );
        let mut child = Command::new("sudo")
            .args(["-n", "-H", EXECUTABLE, SUBCOMMAND])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("launch native iSCSI command")?;
        let written = child
            .stdin
            .take()
            .context("native iSCSI stdin unavailable")?
            .write_all(&input);
        if let Err(error) = written {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error).context("send native iSCSI request");
        }
        let output = child
            .wait_with_output()
            .context("wait for native iSCSI command")?;
        let reply: Response = serde_json::from_slice(&output.stdout)
            .map_err(|_| anyhow::anyhow!("native iSCSI command returned no valid response; check installation and sudo permission (status {})", output.status))?;
        ensure!(reply.version == VERSION, "native iSCSI protocol mismatch");
        let result = reply.result.map_err(anyhow::Error::msg)?;
        ensure!(
            output.status.success(),
            "native iSCSI process failed ({})",
            output.status
        );
        Ok(result)
    }
    fn unit(operation: Operation) -> Result<()> {
        ensure!(
            matches!(Self::request(operation)?, Reply::Unit),
            "unexpected native iSCSI response"
        );
        Ok(())
    }
}

impl IscsiProvisioner for ConfigfsProvisioner {
    fn create_target(&self, spec: &IscsiTargetSpec) -> Result<()> {
        self.create_target_transaction(spec).map(|_| ())
    }
    fn create_target_transaction(&self, spec: &IscsiTargetSpec) -> Result<IscsiProvisionResult> {
        match Self::request(Operation::Create(spec.clone()))? {
            Reply::Created(created) => Ok(created),
            _ => bail!("unexpected native iSCSI response"),
        }
    }
    fn remove_target(&self, target: &str) -> Result<()> {
        Self::unit(Operation::RemoveTarget {
            target: target.into(),
        })
    }
    fn remove_target_with_backstores(&self, target: &str, backstores: &[String]) -> Result<()> {
        Self::unit(Operation::RemoveOwned {
            target: target.into(),
            backstores: backstores.to_vec(),
        })
    }
    fn target_exists(&self, target: &str) -> Result<bool> {
        match Self::request(Operation::Exists {
            target: target.into(),
        })? {
            Reply::Exists(value) => Ok(value),
            _ => bail!("unexpected native iSCSI response"),
        }
    }
    fn set_chap_auth(&self, target: &str, chap: Option<&ChapCredentials>) -> Result<()> {
        Self::unit(Operation::SetChap {
            target: target.into(),
            chap: chap.cloned(),
        })
    }
    fn list_target_luns(&self, target: &str) -> Result<Vec<IscsiLunState>> {
        match Self::request(Operation::List {
            target: target.into(),
        })? {
            Reply::Luns(luns) => Ok(luns),
            _ => bail!("unexpected native iSCSI response"),
        }
    }
    fn list_target_backstores(&self, target: &str) -> Result<Vec<String>> {
        Ok(self
            .list_target_luns(target)?
            .into_iter()
            .map(|lun| lun.backstore)
            .collect())
    }
    fn inspect_target(&self, spec: &IscsiTargetSpec) -> Result<IscsiTargetState> {
        match Self::request(Operation::Inspect(spec.clone()))? {
            Reply::State(state) => Ok(state),
            _ => bail!("unexpected native iSCSI response"),
        }
    }
    fn reconcile(&self, spec: &IscsiTargetSpec) -> Result<()> {
        if !self.inspect_target(spec)?.is_ready() {
            self.create_target(spec)?;
        }
        Ok(())
    }
}
