use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

use super::{
    model::{IscsiTargetSpec, IscsiTargetState},
    IscsiProvisioner,
};

/// Return the configfs file that tracks dynamic iSCSI sessions for a target.
fn dynamic_sessions_path(target_iqn: &str) -> PathBuf {
    PathBuf::from(format!(
        "/sys/kernel/config/target/iscsi/{target_iqn}/tpgt_1/dynamic_sessions"
    ))
}

/// Interpret the contents of LIO's dynamic-session file.
///
/// An empty file means that the target has no active dynamic sessions.
/// Non-empty session entries mean that an initiator is connected.
fn has_active_sessions_content(content: &str) -> bool {
    content.lines().any(|line| !line.trim().is_empty())
}

/// Conservative session observation for automatic destructive operations.
/// Missing/unreadable files are unknown. Explicit ACL sessions are inspected
/// as well as dynamic sessions, since either can hold the client disk open.
pub fn confirmed_target_connected(target_iqn: &str) -> Result<bool> {
    anyhow::ensure!(
        !target_iqn.is_empty() && !target_iqn.contains('/') && !target_iqn.contains(".."),
        "invalid target identifier"
    );
    confirmed_sessions_at(&dynamic_sessions_path(target_iqn))
}

fn confirmed_sessions_at(path: &std::path::Path) -> Result<bool> {
    let content = fs::read_to_string(path).context("iSCSI session state unavailable")?;
    if has_active_sessions_content(&content) {
        return Ok(true);
    }
    let parent = path.parent().context("session path has no parent")?;
    for acl in fs::read_dir(parent.join("acls")).context("iSCSI ACL state unavailable")? {
        let info = fs::read_to_string(acl?.path().join("info"))
            .context("iSCSI ACL session state unavailable")?;
        if !info.trim_start().starts_with("No active iSCSI Session") {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Check whether an iSCSI target currently has active initiator sessions.
///
/// The configfs path is absent when the target does not exist. That is
/// treated as no active sessions rather than as an infrastructure error.
pub fn target_has_active_sessions(target_iqn: &str) -> Result<bool> {
    if target_iqn.trim().is_empty() {
        return Ok(false);
    }

    match fs::read_to_string(dynamic_sessions_path(target_iqn)) {
        Ok(content) if has_active_sessions_content(&content) => Ok(true),
        Ok(_) => confirmed_sessions_at(&dynamic_sessions_path(target_iqn)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).context("failed to inspect active iSCSI sessions"),
    }
}

/// Reconciles desired iSCSI state with the actual LIO configuration.
pub struct IscsiReconciler<P>
where
    P: IscsiProvisioner,
{
    provisioner: P,
}

impl<P> IscsiReconciler<P>
where
    P: IscsiProvisioner,
{
    pub fn new(provisioner: P) -> Self {
        Self { provisioner }
    }

    pub fn inspect(&self, spec: &IscsiTargetSpec) -> Result<IscsiTargetState> {
        self.provisioner.inspect_target(spec)
    }

    pub fn reconcile(&self, spec: &IscsiTargetSpec) -> Result<()> {
        self.provisioner.reconcile(spec)
    }

    pub fn remove(&self, target_iqn: &str) -> Result<()> {
        self.provisioner.remove_target(target_iqn)
    }
}

#[cfg(test)]
mod tests {
    use super::has_active_sessions_content;

    #[test]
    fn automatic_reset_requires_readable_session_and_acl_state() {
        let root = std::env::temp_dir().join(format!("diskless-session-{}", uuid::Uuid::new_v4()));
        let path = root.join("dynamic_sessions");
        assert!(super::confirmed_sessions_at(&path).is_err());
        std::fs::create_dir_all(root.join("acls/client")).unwrap();
        std::fs::write(&path, "").unwrap();
        assert!(super::confirmed_sessions_at(&path).is_err());
        std::fs::write(
            root.join("acls/client/info"),
            "No active iSCSI Session for Initiator Endpoint: client\n",
        )
        .unwrap();
        assert!(!super::confirmed_sessions_at(&path).unwrap());
        std::fs::write(
            root.join("acls/client/info"),
            "InitiatorName: client\nSession State: TARG_SESS_STATE_LOGGED_IN\n",
        )
        .unwrap();
        assert!(super::confirmed_sessions_at(&path).unwrap());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn empty_dynamic_session_content_means_no_active_sessions() {
        assert!(!has_active_sessions_content(""));
        assert!(!has_active_sessions_content("\n  \n"));
    }

    #[test]
    fn dynamic_session_content_detects_active_sessions() {
        let content = "iqn.1993-08.org.debian:01:client-a\n";

        assert!(has_active_sessions_content(content));
    }

    #[test]
    fn dynamic_session_content_ignores_blank_lines() {
        let content = "\n\t\n iqn.1993-08.org.debian:01:client-b \n";

        assert!(has_active_sessions_content(content));
    }
}
