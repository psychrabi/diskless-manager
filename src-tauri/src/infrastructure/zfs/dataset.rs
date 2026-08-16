use anyhow::{bail, Context, Result};

use super::{
    provider::{ZfsDatasetInfo, ZfsProvider},
    ZfsCommand,
};

#[derive(Debug, Clone)]
pub struct ZfsDatasetOperations {
    command: ZfsCommand,
}

impl ZfsDatasetOperations {
    pub const fn new(command: ZfsCommand) -> Self {
        Self { command }
    }

    fn parse_dataset_line(line: &str) -> Option<ZfsDatasetInfo> {
        let mut parts = line.split('\t');

        let name = parts.next()?.trim().to_string();
        let dataset_type = parts.next()?.trim().to_string();
        let used = parts.next().map(str::trim).map(str::to_string);
        let available = parts.next().map(str::trim).map(str::to_string);

        if name.is_empty() {
            return None;
        }

        Some(ZfsDatasetInfo {
            name,
            dataset_type,
            used,
            available,
        })
    }
}

impl ZfsProvider for ZfsDatasetOperations {
    fn exists(&self, name: &str) -> Result<bool> {
        Ok(self.command.check(["zfs", "list", "-H", name]))
    }

    fn dataset(&self, name: &str) -> Result<Option<ZfsDatasetInfo>> {
        if !self.exists(name)? {
            return Ok(None);
        }

        let output = self.command.execute_output([
            "zfs",
            "list",
            "-H",
            "-o",
            "name,type,used,available",
            name,
        ])?;

        Ok(output.lines().find_map(Self::parse_dataset_line))
    }

    fn list_datasets(&self, root: &str) -> Result<Vec<ZfsDatasetInfo>> {
        let output = self.command.execute_output([
            "zfs",
            "list",
            "-H",
            "-r",
            "-t",
            "filesystem,volume",
            "-o",
            "name,type,used,available",
            root,
        ])?;

        Ok(output
            .lines()
            .filter_map(Self::parse_dataset_line)
            .collect())
    }

    fn list_snapshots(&self, _root: &str) -> Result<Vec<super::provider::ZfsSnapshotInfo>> {
        /*
         * Snapshot operations are implemented by ZfsSnapshotOperations.
         *
         * This method intentionally isn't duplicated here.
         */
        bail!("snapshot listing must use ZfsSnapshotOperations")
    }

    fn create_dataset(&self, dataset: &str, properties: &[(&str, &str)]) -> Result<()> {
        if self.exists(dataset)? {
            return Ok(());
        }

        let mut args = vec!["zfs".to_string(), "create".to_string()];

        for (property, value) in properties {
            args.push("-o".to_string());
            args.push(format!("{property}={value}"));
        }

        args.push(dataset.to_string());

        self.command
            .execute(args)
            .with_context(|| format!("failed to create ZFS dataset '{dataset}'"))
    }

    fn create_volume(&self, volume: &str, size: &str, properties: &[(&str, &str)]) -> Result<()> {
        if self.exists(volume)? {
            bail!("ZFS volume already exists: {volume}");
        }

        let mut args = vec![
            "zfs".to_string(),
            "create".to_string(),
            "-s".to_string(),
            "-V".to_string(),
            size.to_string(),
        ];

        for (property, value) in properties {
            args.push("-o".to_string());
            args.push(format!("{property}={value}"));
        }

        args.push(volume.to_string());

        self.command
            .execute(args)
            .with_context(|| format!("failed to create ZFS volume '{volume}'"))
    }

    fn create_snapshot(&self, _dataset: &str, _snapshot: &str) -> Result<()> {
        bail!("snapshot creation must use ZfsSnapshotOperations")
    }

    fn clone_snapshot(&self, _snapshot: &str, _destination: &str) -> Result<()> {
        bail!("clone operations must use ZfsCloneOperations")
    }

    fn destroy(&self, name: &str) -> Result<()> {
        self.command
            .execute(["zfs", "destroy", name])
            .with_context(|| format!("failed to destroy ZFS object '{name}'"))
    }

    fn get_property(&self, property: &str, dataset: &str) -> Result<Option<String>> {
        self.command.get_property(property, dataset)
    }

    fn set_property(&self, property: &str, value: &str, dataset: &str) -> Result<()> {
        self.command.set_property(property, value, dataset)
    }
}
