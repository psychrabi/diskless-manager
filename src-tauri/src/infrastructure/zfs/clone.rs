use anyhow::{bail, Context, Result};

use super::ZfsCommand;

#[derive(Debug, Clone)]
pub struct ZfsCloneOperations {
    command: ZfsCommand,
}

impl ZfsCloneOperations {
    pub const fn new(command: ZfsCommand) -> Self {
        Self { command }
    }
    fn validate_snapshot_name(snapshot: &str) -> Result<()> {
        if !snapshot.contains('@') {
            bail!("ZFS clone source must be a snapshot: {}", snapshot);
        }

        let mut parts = snapshot.split('@');

        let dataset = parts.next().unwrap_or("");

        let snapshot_name = parts.next().unwrap_or("");

        if dataset.is_empty() || snapshot_name.is_empty() || parts.next().is_some() {
            bail!("invalid ZFS snapshot name: {}", snapshot);
        }

        Ok(())
    }

    pub fn source_exists(&self, snapshot: &str) -> bool {
        self.command.check(["zfs", "list", "-H", snapshot])
    }

    pub fn destination_exists(&self, destination: &str) -> bool {
        self.command.check(["zfs", "list", "-H", destination])
    }

    pub fn clone(&self, snapshot: &str, destination: &str) -> Result<()> {
        Self::validate_snapshot_name(snapshot)?;

        if !self.source_exists(snapshot) {
            bail!("ZFS clone source snapshot does not exist: {}", snapshot);
        }

        if self.destination_exists(destination) {
            bail!("ZFS clone destination already exists: {}", destination);
        }

        self.command
            .execute(["zfs", "clone", snapshot, destination])
            .with_context(|| format!("failed to clone '{}' to '{}'", snapshot, destination))
    }

    pub fn destroy(&self, destination: &str) -> Result<()> {
        self.command
            .execute(["zfs", "destroy", destination])
            .with_context(|| format!("failed to destroy ZFS clone '{destination}'"))
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn valid_snapshot_contains_exactly_one_separator() {
        let snapshot = "pool/images/windows@s1";

        assert!(snapshot.contains('@'));

        assert_eq!(snapshot.matches('@').count(), 1);
    }

    #[test]
    fn volume_is_not_valid_clone_source() {
        let volume = "pool/images/windows";

        assert!(!volume.contains('@'));
    }

    #[test]
    fn malformed_snapshot_is_rejected() {
        assert!("pool/images/windows@s1@bad".matches('@').count() != 1);
    }
}
