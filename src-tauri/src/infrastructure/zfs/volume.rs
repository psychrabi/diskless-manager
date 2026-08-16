use anyhow::{bail, Context, Result};

use super::{provider::ZfsVolumeInfo, ZfsCommand};

#[derive(Debug, Clone)]
pub struct ZfsVolumeOperations {
    command: ZfsCommand,
}

impl ZfsVolumeOperations {
    pub const fn new(command: ZfsCommand) -> Self {
        Self { command }
    }

    pub fn exists(&self, volume: &str) -> bool {
        self.command.check(["zfs", "list", "-H", volume])
    }

    pub fn create(&self, volume: &str, size: &str, volblocksize: &str) -> Result<()> {
        if self.exists(volume) {
            bail!("ZFS volume already exists: {}", volume);
        }

        self.command
            .execute([
                "zfs",
                "create",
                "-s",
                "-V",
                size,
                "-o",
                &format!("volblocksize={}", volblocksize),
                volume,
            ])
            .with_context(|| format!("failed to create ZVOL '{}'", volume))
    }

    pub fn info(&self, volume: &str) -> Result<Option<ZfsVolumeInfo>> {
        if !self.exists(volume) {
            return Ok(None);
        }

        let volsize = self.command.get_property("volsize", volume)?;

        let volblocksize = self.command.get_property("volblocksize", volume)?;

        let used = self.command.get_property("used", volume)?;

        Ok(Some(ZfsVolumeInfo {
            name: volume.to_string(),

            volsize,

            volblocksize,

            used,
        }))
    }

    pub fn resize(&self, volume: &str, size: &str) -> Result<()> {
        self.command
            .execute(["zfs", "set", &format!("volsize={}", size), volume])
            .with_context(|| format!("failed to resize ZVOL '{}'", volume))
    }

    pub fn destroy(&self, volume: &str) -> Result<()> {
        self.command
            .execute(["zfs", "destroy", volume])
            .with_context(|| format!("failed to destroy ZVOL '{}'", volume))
    }

    pub fn set_property(&self, property: &str, value: &str, volume: &str) -> Result<()> {
        self.command.set_property(property, value, volume)
    }
}
