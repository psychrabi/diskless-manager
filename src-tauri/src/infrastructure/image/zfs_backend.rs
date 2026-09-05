use anyhow::{bail, Context, Result};

use std::path::Path;

use super::backend::{ImageBackend, ImageBackendInfo};

use crate::{
    config::get_zpool_name,
    core::image::ImageFormat,
    infrastructure::{
        nvmeof::remove_exports_for_block_device,
        zfs::{
            ZfsCloneOperations, ZfsCommand, ZfsDatasetOperations, ZfsProvider,
            ZfsSnapshotOperations, ZfsVolumeOperations,
        },
    },
};

#[derive(Clone)]
pub struct ZfsImageBackend {
    command: ZfsCommand,
    datasets: ZfsDatasetOperations,
    volumes: ZfsVolumeOperations,
    snapshots: ZfsSnapshotOperations,
    clones: ZfsCloneOperations,
}

impl ZfsImageBackend {
    pub fn new() -> Self {
        let command = ZfsCommand::new();

        Self {
            command,

            datasets: ZfsDatasetOperations::new(command),

            volumes: ZfsVolumeOperations::new(command),

            snapshots: ZfsSnapshotOperations::new(command),

            clones: ZfsCloneOperations::new(command),
        }
    }

    fn parse_size(value: &str) -> Result<u64> {
        let value = value.trim();

        if value.is_empty() || value == "-" {
            return Ok(0);
        }

        if let Ok(bytes) = value.parse::<u64>() {
            return Ok(bytes);
        }

        let mut number = String::new();

        let mut suffix = String::new();

        for character in value.chars() {
            if character.is_ascii_digit() || character == '.' {
                number.push(character);
            } else if !character.is_whitespace() {
                suffix.push(character);
            }
        }

        let number: f64 = number.parse()?;

        let multiplier = match suffix.to_ascii_lowercase().as_str() {
            "k" | "kb" => 1024_f64,

            "m" | "mb" => 1024_f64.powi(2),

            "g" | "gb" => 1024_f64.powi(3),

            "t" | "tb" => 1024_f64.powi(4),

            "p" | "pb" => 1024_f64.powi(5),

            _ => {
                bail!("unsupported ZFS size '{}'", value)
            }
        };

        Ok((number * multiplier) as u64)
    }
}

impl Default for ZfsImageBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl ImageBackend for ZfsImageBackend {
    fn settle_device_changes(&self) -> Result<()> {
        crate::infrastructure::command::run_command_output_no_sudo([
            "udevadm",
            "settle",
            "--timeout=10",
        ])
        .map(|_| ())
        .map_err(anyhow::Error::from)
        .context("failed to settle ZVOL device changes")
    }
    fn clone_origin(&self, name: &str) -> Result<Option<String>> {
        self.command.get_property("origin", name)
    }
    fn exists(&self, name: &str) -> Result<bool> {
        Ok(self.datasets.exists(name)? || self.volumes.exists(name))
    }

    fn create_volume(&self, name: &str, size_gb: u64) -> Result<()> {
        self.volumes
            .create(name, &format!("{}G", size_gb), "128K")
            .context("failed to create image ZVOL")
    }

    fn destroy(&self, name: &str) -> Result<()> {
        let block_device = format!("/dev/zvol/{name}");
        let block_device = Path::new(&block_device);

        if block_device.exists() {
            let removed = remove_exports_for_block_device(block_device)
                .map_err(anyhow::Error::msg)
                .with_context(|| {
                    format!(
                        "failed to detach NVMe-oF export before destroying ZFS volume '{}'",
                        name
                    )
                })?;

            if !removed.is_empty() {
                tracing::info!(
                    dataset = %name,
                    block_device = %block_device.display(),
                    nqns = ?removed,
                    "detached NVMe-oF exports before ZFS destruction"
                );
            }
        }

        self.volumes
            .destroy(name)
            .or_else(|_| self.datasets.destroy(name))
            .context("failed to destroy ZFS image")
    }

    fn rename(&self, old_name: &str, new_name: &str) -> Result<()> {
        self.command
            .execute(["zfs", "rename", old_name, new_name])
            .context("failed to rename ZFS image")
    }

    fn clone_image(&self, source: &str, destination: &str) -> Result<()> {
        if !source.contains('@') {
            bail!("ZFS image clone source must be a snapshot: '{}'", source);
        }

        if destination.contains('@') {
            bail!(
                "ZFS clone destination cannot be a snapshot: '{}'",
                destination
            );
        }

        if self.exists(destination)? {
            bail!("ZFS clone destination already exists: '{}'", destination);
        }

        self.clones.clone(source, destination).with_context(|| {
            format!(
                "failed to clone ZFS snapshot '{}' to '{}'",
                source, destination
            )
        })
    }

    fn create_snapshot(&self, dataset: &str, snapshot: &str) -> Result<()> {
        self.snapshots.create(dataset, snapshot)
    }

    fn destroy_snapshot(&self, dataset: &str, snapshot: &str) -> Result<()> {
        self.snapshots.destroy(dataset, snapshot)
    }

    fn rollback_snapshot(&self, dataset: &str, snapshot: &str) -> Result<()> {
        self.snapshots.rollback(dataset, snapshot)
    }

    fn resize(&self, name: &str, size_gb: u64) -> Result<()> {
        self.volumes.resize(name, &format!("{}G", size_gb))
    }

    fn import_raw(&self, source: &Path, destination: &str, size_bytes: u64) -> Result<()> {
        let size_gb = size_bytes.div_ceil(1024 * 1024 * 1024);

        self.create_volume(destination, size_gb.max(1))?;

        let source = source
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("source path is not valid UTF-8"))?;

        let device = format!("/dev/zvol/{}", destination);

        let output = std::process::Command::new("dd")
            .args([
                &format!("if={}", source),
                &format!("of={}", device),
                "bs=16M",
                "conv=fdatasync",
                "status=progress",
            ])
            .output()
            .context("failed to execute dd")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);

            let _ = self.destroy(destination);

            bail!("failed to import raw image: {}", stderr.trim());
        }

        Ok(())
    }

    fn verify(&self, name: &str) -> Result<bool> {
        if !self.exists(name)? {
            return Ok(false);
        }

        let info = self.volumes.info(name)?;

        Ok(info.is_some())
    }

    fn info(&self, name: &str) -> Result<ImageBackendInfo> {
        let volume = self
            .volumes
            .info(name)?
            .ok_or_else(|| anyhow::anyhow!("ZFS volume not found: {}", name))?;

        let virtual_size = volume
            .volsize
            .as_deref()
            .map(Self::parse_size)
            .transpose()?
            .unwrap_or(0);

        let actual_size = volume
            .used
            .as_deref()
            .map(Self::parse_size)
            .transpose()?
            .unwrap_or(0);

        let snapshots = self
            .snapshots
            .list(name)?
            .into_iter()
            .map(|snapshot| snapshot.snapshot)
            .collect();

        Ok(ImageBackendInfo {
            virtual_size,
            actual_size,
            format: ImageFormat::Raw,
            backing_file: None,
            snapshots,
        })
    }

    fn set_os_type(&self, name: &str, os_type: &str) -> Result<()> {
        self.command.set_property("org.diskless:os", os_type, name)
    }

    fn image_parent(&self) -> Result<String> {
        let zpool = get_zpool_name();

        let root = format!("{}/image-disk", zpool);

        if !self.datasets.exists(&root)? {
            self.datasets
                .create_dataset(&root, &[("org.diskless:type", "image")])?;
        } else {
            let image_type = self.datasets.get_property("org.diskless:type", &root)?;

            if image_type.as_deref() != Some("image") {
                self.datasets
                    .set_property("org.diskless:type", "image", &root)?;
            }
        }

        Ok(root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_has_default_constructor() {
        let _backend = ZfsImageBackend::new();
    }

    #[test]
    fn parse_size_bytes() {
        assert_eq!(
            ZfsImageBackend::parse_size("1073741824").unwrap(),
            1_073_741_824
        );
    }

    #[test]
    fn parse_size_gigabytes() {
        assert_eq!(ZfsImageBackend::parse_size("1G").unwrap(), 1_073_741_824);
    }

    #[test]
    fn parse_size_megabytes() {
        assert_eq!(
            ZfsImageBackend::parse_size("512M").unwrap(),
            512 * 1024 * 1024
        );
    }

    #[test]
    fn parse_empty_size_as_zero() {
        assert_eq!(ZfsImageBackend::parse_size("-").unwrap(), 0);
    }
}
