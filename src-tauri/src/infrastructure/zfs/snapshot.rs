use anyhow::{Context, Result};

use super::{
    provider::{ZfsProvider, ZfsSnapshotInfo},
    ZfsCommand,
};

#[derive(Debug, Clone)]
pub struct ZfsSnapshotOperations {
    command: ZfsCommand,
}

impl ZfsSnapshotOperations {
    pub const fn new(command: ZfsCommand) -> Self {
        Self { command }
    }

    pub fn rollback(&self, dataset: &str, snapshot: &str) -> Result<()> {
        let name = format!("{}@{}", dataset, snapshot);

        self.command
            .execute(["zfs", "rollback", "-r", &name])
            .with_context(|| format!("failed to rollback ZFS snapshot '{}'", name))
    }

    fn parse_line(line: &str) -> Option<ZfsSnapshotInfo> {
        let mut parts = line.split('\t');

        let name = parts.next()?.trim().to_string();

        let used = parts.next().map(str::trim).map(str::to_string);

        let (dataset, snapshot) = name.split_once('@')?;

        if dataset.is_empty() || snapshot.is_empty() {
            return None;
        }
        let dataset = dataset.to_string();

        let snapshot = snapshot.to_string();

        Some(ZfsSnapshotInfo {
            name,
            dataset,
            snapshot,
            used,
        })
    }

    pub fn list(&self, root: &str) -> Result<Vec<ZfsSnapshotInfo>> {
        let output = self.command.execute_output([
            "zfs",
            "list",
            "-H",
            "-t",
            "snapshot",
            "-o",
            "name,used",
            "-r",
            root,
        ])?;

        Ok(output.lines().filter_map(Self::parse_line).collect())
    }

    pub fn exists(&self, dataset: &str, snapshot: &str) -> Result<bool> {
        let name = format!("{dataset}@{snapshot}");

        Ok(self
            .command
            .check(["zfs", "list", "-H", "-t", "snapshot", &name]))
    }

    pub fn create(&self, dataset: &str, snapshot: &str) -> Result<()> {
        let name = format!("{dataset}@{snapshot}");

        if self.exists(dataset, snapshot)? {
            return Ok(());
        }

        self.command
            .execute(["zfs", "snapshot", &name])
            .with_context(|| format!("failed to create ZFS snapshot '{name}'"))
    }

    pub fn destroy(&self, dataset: &str, snapshot: &str) -> Result<()> {
        let name = format!("{}@{}", dataset, snapshot);

        self.command
            .execute(["zfs", "destroy", &name])
            .with_context(|| format!("failed to destroy ZFS snapshot '{}'", name))
    }
}

impl ZfsProvider for ZfsSnapshotOperations {
    fn exists(&self, name: &str) -> Result<bool> {
        Ok(self.command.check(["zfs", "list", "-H", name]))
    }

    fn dataset(&self, _name: &str) -> Result<Option<super::provider::ZfsDatasetInfo>> {
        Ok(None)
    }

    fn list_datasets(&self, _root: &str) -> Result<Vec<super::provider::ZfsDatasetInfo>> {
        Ok(Vec::new())
    }

    fn list_snapshots(&self, root: &str) -> Result<Vec<ZfsSnapshotInfo>> {
        self.list(root)
    }

    fn create_dataset(&self, _dataset: &str, _properties: &[(&str, &str)]) -> Result<()> {
        anyhow::bail!("dataset creation must use ZfsDatasetOperations")
    }

    fn create_volume(
        &self,
        _volume: &str,
        _size: &str,
        _properties: &[(&str, &str)],
    ) -> Result<()> {
        anyhow::bail!("volume creation must use ZfsVolumeOperations")
    }

    fn create_snapshot(&self, dataset: &str, snapshot: &str) -> Result<()> {
        self.create(dataset, snapshot)
    }

    fn clone_snapshot(&self, _snapshot: &str, _destination: &str) -> Result<()> {
        anyhow::bail!("clone operations must use ZfsCloneOperations")
    }

    fn destroy(&self, name: &str) -> Result<()> {
        self.command.execute(["zfs", "destroy", name])
    }

    fn get_property(&self, property: &str, dataset: &str) -> Result<Option<String>> {
        self.command.get_property(property, dataset)
    }

    fn set_property(&self, property: &str, value: &str, dataset: &str) -> Result<()> {
        self.command.set_property(property, value, dataset)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_snapshot_line() {
        let result = ZfsSnapshotOperations::parse_line("pool/images/windows@s1\t128K").unwrap();

        assert_eq!(result.name, "pool/images/windows@s1");

        assert_eq!(result.dataset, "pool/images/windows");

        assert_eq!(result.snapshot, "s1");

        assert_eq!(result.used.as_deref(), Some("128K"));
    }

    #[test]
    fn reject_dataset_without_snapshot() {
        assert!(ZfsSnapshotOperations::parse_line("pool/images/windows\t128K").is_none());
    }

    #[test]
    fn reject_empty_snapshot_name() {
        assert!(ZfsSnapshotOperations::parse_line("pool/images/windows@\t128K").is_none());
    }
}
