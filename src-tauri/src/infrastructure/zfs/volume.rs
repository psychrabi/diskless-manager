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

        let output = self.command.execute_output([
            "zfs",
            "get",
            "-H",
            "-o",
            "value",
            "volsize,volblocksize,used",
            volume,
        ])?;
        let (volsize, volblocksize, used) = parse_volume_properties(&output);

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

/// Parse one `zfs get -H -o value volsize,volblocksize,used` output.
/// `-` (e.g. snapshots) and absent lines map to `None`, matching
/// `ZfsCommand::get_property`.
fn parse_volume_properties(output: &str) -> (Option<String>, Option<String>, Option<String>) {
    let mut lines = output.lines().map(str::trim).map(|value| {
        if value.is_empty() || value == "-" {
            None
        } else {
            Some(value.to_string())
        }
    });
    (
        lines.next().flatten(),
        lines.next().flatten(),
        lines.next().flatten(),
    )
}

#[cfg(test)]
mod tests {
    use super::parse_volume_properties;

    #[test]
    fn volume_properties_parse_combined_get_output() {
        let (size, block, used) = parse_volume_properties("50G\n16K\n12.5G\n");
        assert_eq!(size, Some("50G".to_string()));
        assert_eq!(block, Some("16K".to_string()));
        assert_eq!(used, Some("12.5G".to_string()));
    }

    #[test]
    fn volume_properties_treat_dash_as_missing() {
        let (size, block, used) = parse_volume_properties("-\n16K\n");
        assert_eq!(size, None);
        assert_eq!(block, Some("16K".to_string()));
        assert_eq!(used, None);
    }
}
