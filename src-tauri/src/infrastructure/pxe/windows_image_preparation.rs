//! Transactional Windows image preparation.
//!
//! One mount operation owns both driver installation and SYSTEM-hive boot arming.
//! Any failure discards the DISM mount so partial changes are not committed.

use super::{
    WindowsBootArmConfig, WindowsBootArmResult, WindowsBootArmer, WindowsDriverInjector,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn default_image_index() -> u32 {
    1
}

fn default_recursive() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsImagePreparationRequest {
    pub image_path: PathBuf,
    pub driver_root: PathBuf,
    #[serde(default)]
    pub mount_root: Option<PathBuf>,
    #[serde(default = "default_image_index")]
    pub image_index: u32,
    #[serde(default = "default_recursive")]
    pub recursive: bool,
    #[serde(default)]
    pub commit: bool,
    #[serde(default)]
    pub boot_arm: WindowsBootArmConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsImagePreparationResult {
    pub image_path: String,
    pub image_index: u32,
    pub driver_root: String,
    pub mount_path: String,
    pub system_hive_path: String,
    pub drivers_added: usize,
    pub boot_arm: WindowsBootArmResult,
    pub committed: bool,
}

#[derive(Debug, Clone)]
pub struct WindowsImagePreparer {
    driver_injector: WindowsDriverInjector,
    boot_armer: WindowsBootArmer,
}

impl WindowsImagePreparer {
    pub fn new() -> Result<Self> {
        Ok(Self {
            driver_injector: WindowsDriverInjector::new()?,
            boot_armer: WindowsBootArmer::new()?,
        })
    }

    pub fn prepare(
        &self,
        request: WindowsImagePreparationRequest,
    ) -> Result<WindowsImagePreparationResult> {
        validate_request(&request)?;

        let temporary_mount = if request.mount_root.is_none() {
            Some(TempDir::new().context("failed to create temporary DISM mount directory")?)
        } else {
            None
        };

        let mount_path = request
            .mount_root
            .clone()
            .or_else(|| temporary_mount.as_ref().map(|dir| dir.path().to_path_buf()))
            .expect("mount path must exist");

        fs::create_dir_all(&mount_path)
            .with_context(|| format!("failed to create mount path {}", mount_path.display()))?;

        self.driver_injector
            .mount_index(&request.image_path, &mount_path, request.image_index)?;

        let operation = (|| -> Result<(usize, PathBuf, WindowsBootArmResult)> {
            let drivers_added = self.driver_injector.add_drivers(
                &mount_path,
                &request.driver_root,
                request.recursive,
            )?;

            let system_hive = find_system_hive(&mount_path)?;
            let boot_arm = self
                .boot_armer
                .arm(&system_hive, &request.boot_arm, request.commit)
                .context("failed to prepare Windows SYSTEM hive for network boot")?;

            Ok((drivers_added, system_hive, boot_arm))
        })();

        let (drivers_added, system_hive, boot_arm) = match operation {
            Ok(value) => value,
            Err(error) => {
                let cleanup = self.driver_injector.discard_mount(&mount_path);
                return match cleanup {
                    Ok(()) => Err(error),
                    Err(cleanup_error) => Err(error.context(format!(
                        "additionally failed to discard mounted Windows image: {cleanup_error:#}"
                    ))),
                };
            }
        };

        if request.commit {
            if let Err(error) = self.driver_injector.commit_mount(&mount_path) {
                let _ = self.driver_injector.discard_mount(&mount_path);
                return Err(error).context("failed to commit prepared Windows image");
            }
        } else {
            self.driver_injector
                .discard_mount(&mount_path)
                .context("failed to discard Windows image after dry-run preparation")?;
        }

        Ok(WindowsImagePreparationResult {
            image_path: request.image_path.display().to_string(),
            image_index: request.image_index,
            driver_root: request.driver_root.display().to_string(),
            mount_path: mount_path.display().to_string(),
            system_hive_path: system_hive.display().to_string(),
            drivers_added,
            boot_arm,
            committed: request.commit,
        })
    }
}

pub fn servicing_available() -> bool {
    WindowsDriverInjector::new().is_ok() && WindowsBootArmer::new().is_ok()
}

fn validate_request(request: &WindowsImagePreparationRequest) -> Result<()> {
    if request.image_index == 0 {
        bail!("Windows image index must be greater than zero");
    }
    if !request.image_path.is_file() {
        bail!(
            "Windows image does not exist: {}",
            request.image_path.display()
        );
    }
    if !request.driver_root.is_dir() {
        bail!(
            "driver root does not exist: {}",
            request.driver_root.display()
        );
    }
    if request
        .mount_root
        .as_ref()
        .is_some_and(|path| path.exists() && !path.is_dir())
    {
        bail!(
            "mount path is not a directory: {}",
            request.mount_root.as_ref().unwrap().display()
        );
    }
    Ok(())
}

fn find_system_hive(mount_path: &Path) -> Result<PathBuf> {
    let canonical = mount_path.join("Windows").join("System32").join("config");
    if canonical.is_dir() {
        if let Some(path) = find_case_insensitive_file(&canonical, "SYSTEM")? {
            return Ok(path);
        }
    }

    bail!(
        "mounted image does not contain Windows/System32/config/SYSTEM under {}",
        mount_path.display()
    )
}

fn find_case_insensitive_file(directory: &Path, name: &str) -> Result<Option<PathBuf>> {
    for entry in fs::read_dir(directory)
        .with_context(|| format!("failed to read {}", directory.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_file()
            && entry
                .file_name()
                .to_str()
                .is_some_and(|value| value.eq_ignore_ascii_case(name))
        {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_request_is_dry_run_and_index_one() {
        let request: WindowsImagePreparationRequest = serde_json::from_str(
            r#"{
                "image_path":"image.wim",
                "driver_root":"drivers"
            }"#,
        )
        .unwrap();

        assert_eq!(request.image_index, 1);
        assert!(request.recursive);
        assert!(!request.commit);
    }

    #[test]
    fn finds_system_hive_case_insensitively() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("Windows").join("System32").join("config");
        fs::create_dir_all(&config).unwrap();
        let expected = config.join("system");
        fs::write(&expected, b"regf").unwrap();

        assert_eq!(find_system_hive(root.path()).unwrap(), expected);
    }

    #[test]
    fn rejects_zero_image_index_before_servicing() {
        let request = WindowsImagePreparationRequest {
            image_path: PathBuf::from("missing.wim"),
            driver_root: PathBuf::from("missing-drivers"),
            mount_root: None,
            image_index: 0,
            recursive: true,
            commit: false,
            boot_arm: WindowsBootArmConfig::default(),
        };

        assert!(validate_request(&request)
            .unwrap_err()
            .to_string()
            .contains("image index"));
    }
}
