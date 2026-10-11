//! Transactional Windows image preparation.
//!
//! One mount operation owns both driver installation and SYSTEM-hive boot arming.
//! Any failure discards the DISM mount so partial changes are not committed.

use super::windows_driver_injection::collect_inf_files_with_recursion;
use super::windows_inf::{boot_service_profiles, inspect_inf_file, InfBootService};
use super::{WindowsBootArmConfig, WindowsBootArmResult, WindowsBootArmer, WindowsDriverInjector};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
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
    #[serde(default)]
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
    #[serde(default)]
    pub iscsi: Option<super::windows_iscsi_servicing::WindowsIscsiConnection>,
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
        if let Some(connection) = &request.iscsi {
            return super::windows_iscsi_servicing::prepare(connection, &request);
        }
        validate_request(&request)?;
        let selected_services = selected_driver_services(&request.driver_root, request.recursive)?;

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
            let profiles = missing_service_profiles(
                &self.boot_armer,
                &system_hive,
                &mount_path,
                &request.driver_root,
                request.recursive,
                &selected_services,
                request.boot_arm.allow_dirty_hive,
            )?;
            let boot_arm = self
                .boot_armer
                .arm_with_profiles(
                    &system_hive,
                    &request.boot_arm,
                    request.commit,
                    &selected_services,
                    &profiles,
                )
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

    pub fn prepare_volume(
        &self,
        request: WindowsImagePreparationRequest,
    ) -> Result<WindowsImagePreparationResult> {
        if request.iscsi.is_some() || !request.image_path.is_dir() || !request.driver_root.is_dir()
        {
            bail!("offline Windows volume and driver directories are required");
        }
        let system_hive = find_system_hive(&request.image_path)?;
        let live_hive =
            PathBuf::from(std::env::var_os("SystemRoot").context("SystemRoot is not set")?)
                .join("System32/config/SYSTEM");
        if fs::canonicalize(&system_hive)? == fs::canonicalize(&live_hive)? {
            bail!("refusing to service the running Windows installation");
        }
        let selected = selected_driver_services(&request.driver_root, request.recursive)?;
        let drivers_added = self.driver_injector.add_drivers(
            &request.image_path,
            &request.driver_root,
            request.recursive,
        )?;
        let profiles = missing_service_profiles(
            &self.boot_armer,
            &system_hive,
            &request.image_path,
            &request.driver_root,
            request.recursive,
            &selected,
            request.boot_arm.allow_dirty_hive,
        )?;
        let boot_arm = self.boot_armer.arm_with_profiles(
            &system_hive,
            &request.boot_arm,
            request.commit,
            &selected,
            &profiles,
        )?;
        Ok(WindowsImagePreparationResult {
            image_path: request.image_path.display().to_string(),
            image_index: 0,
            driver_root: request.driver_root.display().to_string(),
            mount_path: request.image_path.display().to_string(),
            system_hive_path: system_hive.display().to_string(),
            drivers_added,
            boot_arm,
            committed: request.commit,
        })
    }
}

fn missing_service_profiles(
    armer: &WindowsBootArmer,
    hive: &Path,
    root: &Path,
    drivers: &Path,
    recursive: bool,
    selected: &[String],
    allow_dirty_hive: bool,
) -> Result<Vec<InfBootService>> {
    let inventory = armer.inspect_with_policy(hive, selected, allow_dirty_hive)?;
    let missing: BTreeSet<_> = selected
        .iter()
        .filter(|s| {
            !inventory
                .existing_services
                .iter()
                .any(|e| e.eq_ignore_ascii_case(s))
        })
        .map(|s| s.to_ascii_lowercase())
        .collect();
    let store = root.join("Windows/System32/DriverStore/FileRepository");
    let mut profiles = std::collections::BTreeMap::new();
    for inf in collect_inf_files_with_recursion(drivers, recursive)? {
        let metadata = inspect_inf_file(&inf)?;
        if !metadata.is_network_class
            || !metadata
                .associated_service_names
                .iter()
                .any(|s| missing.contains(&s.to_ascii_lowercase()))
        {
            continue;
        }
        for mut profile in boot_service_profiles(&inf)? {
            if !missing.contains(&profile.name.to_ascii_lowercase()) {
                continue;
            }
            let (directory, filename) = profile
                .binary
                .split_once('\\')
                .context("unsupported service binary path")?;
            if !["%12%", "%13%"].contains(&directory)
                || filename.contains(['\\', '/', '%', ':'])
                || filename == ".."
                || filename.is_empty()
            {
                bail!(
                    "unsupported service binary {}; install this NIC in the image first",
                    profile.binary
                );
            }
            // Identify the signed package DISM actually staged, not an arbitrary SYS from the upload.
            let bytes = fs::read(&inf)?;
            let mut matches = Vec::new();
            for entry in fs::read_dir(&store)? {
                let folder = entry?.path();
                if !folder.is_dir() {
                    continue;
                }
                for file in fs::read_dir(&folder)? {
                    let file = file?.path();
                    if file
                        .extension()
                        .is_some_and(|v| v.eq_ignore_ascii_case("inf"))
                        && fs::read(&file)? == bytes
                    {
                        matches.push(folder.clone());
                    }
                }
            }
            if matches.len() != 1 {
                bail!(
                    "DISM-staged package must be uniquely identifiable for {}",
                    profile.name
                );
            }
            let package = &matches[0];
            let binary = package.join(filename);
            let canonical = fs::canonicalize(&binary)?;
            if !canonical.starts_with(fs::canonicalize(package)?) || !binary.is_file() {
                bail!("service binary escaped its staged package");
            }
            let relative = if directory == "%13%" {
                binary
                    .strip_prefix(root)?
                    .to_string_lossy()
                    .replace('/', "\\")
            } else {
                let destination = root.join("Windows/System32/drivers").join(filename);
                if destination.exists() && fs::read(&destination)? != fs::read(&binary)? {
                    bail!("refusing to replace a different existing boot driver binary");
                }
                if !destination.exists() {
                    fs::copy(&binary, &destination)?;
                }
                format!("Windows\\System32\\drivers\\{filename}")
            };
            profile.binary = format!(
                "\\SystemRoot\\{}",
                relative
                    .strip_prefix("Windows\\")
                    .context("invalid staged driver path")?
            );
            let key = profile.name.to_ascii_lowercase();
            if let Some(old) = profiles.insert(key, profile.clone()) {
                if old != profile {
                    bail!("selected packages have conflicting service declarations");
                }
            }
        }
    }
    Ok(profiles.into_values().collect())
}

fn selected_driver_services(driver_root: &Path, recursive: bool) -> Result<Vec<String>> {
    let mut services = BTreeSet::new();
    for inf in collect_inf_files_with_recursion(driver_root, recursive)? {
        let metadata = inspect_inf_file(&inf)?;
        if metadata.is_network_class {
            if metadata.associated_service_names.is_empty() {
                bail!("cannot resolve the associated NIC service in {}; Include/Needs or unresolved INF flags require Windows installation before boot arming", inf.display());
            }
            services.extend(metadata.associated_service_names);
        }
    }
    Ok(services.into_iter().collect())
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
            request
                .mount_root
                .as_ref()
                .expect("mount root was validated as present")
                .display()
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
    fn dirty_hive_refused_before_service_inspection_can_load_it() {
        let root = tempfile::tempdir().unwrap();
        let hive = root.path().join("SYSTEM");
        let mut bytes = vec![0; 0x200];
        bytes[..4].copy_from_slice(b"regf");
        bytes[4..8].copy_from_slice(&1u32.to_le_bytes());
        fs::write(&hive, bytes).unwrap();
        let armer = WindowsBootArmer::with_reg(root.path().join("must-not-execute"));
        let error =
            missing_service_profiles(&armer, &hive, root.path(), root.path(), true, &[], false)
                .unwrap_err();
        assert!(error.to_string().contains("hive is dirty"), "{error:#}");
        let error =
            missing_service_profiles(&armer, &hive, root.path(), root.path(), true, &[], true)
                .unwrap_err();
        assert!(!error.to_string().contains("hive is dirty"), "{error:#}");
    }

    #[test]
    fn selects_only_network_function_drivers_and_rejects_unresolved_services() {
        let root = tempfile::tempdir().unwrap();
        let inf = root.path().join("nic.inf");
        fs::write(&inf, "[Version]\nClass=Net\n[Install.NT.Services]\nAddService=Filter,0,FilterInstall\nAddService=NetKVM,2,NicInstall\n").unwrap();
        fs::write(root.path().join("storage.inf"), "[Version]\nClass=SCSIAdapter\n[Install.NT.Services]\nAddService=StorageDriver,2,Install\n").unwrap();
        assert_eq!(
            selected_driver_services(root.path(), true).unwrap(),
            vec!["NetKVM"]
        );
        fs::write(&inf, "[Version]\nClass=Net\n[Install.NT.Services]\nInclude=other.inf\nNeeds=Other.Services\n").unwrap();
        assert!(selected_driver_services(root.path(), true)
            .unwrap_err()
            .to_string()
            .contains("associated NIC service"));
    }

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
            iscsi: None,
        };

        assert!(validate_request(&request)
            .unwrap_err()
            .to_string()
            .contains("image index"));
    }
}
