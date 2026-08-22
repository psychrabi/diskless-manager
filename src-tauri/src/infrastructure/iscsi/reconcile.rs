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

/// Check whether an iSCSI target currently has active initiator sessions.
///
/// The configfs path is absent when the target does not exist. That is
/// treated as no active sessions rather than as an infrastructure error.
pub fn target_has_active_sessions(target_iqn: &str) -> Result<bool> {
    if target_iqn.trim().is_empty() {
        return Ok(false);
    }

    match fs::read_to_string(dynamic_sessions_path(target_iqn)) {
        Ok(content) => Ok(has_active_sessions_content(&content)),
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
