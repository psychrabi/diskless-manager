//! Read-only NexBoot-style Boot NIC coverage for an existing Windows image.
//!
//! This probe checks existing driver-package coverage before requesting an
//! offline installation. It does not check or write boot-critical registry
//! identity and cannot establish SAN boot readiness. It clones the selected
//! snapshot, mounts its Windows volume read-only, and
//! accepts a driver only when all of these hold:
//!
//! - a network-class INF matches one of the observed Boot NIC hardware IDs;
//! - the same INF is published under `Windows/INF` (or is an inbox INF that
//!   lives there), so Windows can install it through PnP; and
//! - the driver's service binary exists in the image.
//!
//! It never writes to the source image. Any probe failure is returned to the
//! caller, which keeps the existing Windows servicing path; a missing driver is
//! not silently treated as covered.

use super::driver_selection::{normalize_architecture, pnp_id_matches};
use super::windows_inf::{
    boot_service_profiles_for_architecture, decode_inf_bytes, parse_inf, WindowsInfMetadata,
};
use crate::infrastructure::image::ImageBackend;
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

/// Limit filesystem inspection workload. Host commands still depend on host I/O.
const MAX_INF_FILES: usize = 16_384;
const MAX_INF_BYTES: u64 = 4 * 1024 * 1024;
const MAX_DIRECTORIES: usize = 32_768;
const DRIVER_STORE_DEPTH: usize = 4;

/// Cleanup failed and the inspection clone or mount needs operator attention.
#[derive(Debug, thiserror::Error)]
#[error("temporary Windows driver inspection cleanup failed")]
pub struct DriverInspectionCleanupFailure;

/// True when the selected snapshot has matching published Boot NIC driver files.
///
/// The snapshot is cloned into a temporary dataset so the probe never touches
/// the source image or a running client's clone. The caller owns the fallback
/// decision when this cannot be inspected.
pub fn snapshot_covers_boot_nic(
    backend: &dyn ImageBackend,
    source: &str,
    snapshot: &str,
    boot_nic_ids: &[String],
    architecture: &str,
) -> Result<bool> {
    if boot_nic_ids.is_empty() {
        return Ok(false);
    }
    let (parent, _) = source
        .rsplit_once('/')
        .context("Windows image name has no dataset parent")?;
    if snapshot.is_empty() || snapshot.contains('@') {
        bail!("invalid Windows snapshot name");
    }
    let clone = format!("{parent}/driver-check-{}", Uuid::new_v4().simple());
    let source_snapshot = format!("{source}@{snapshot}");
    backend
        .clone_image(&source_snapshot, &clone)
        .with_context(|| {
            format!("failed to clone Windows snapshot '{source_snapshot}' for driver inspection")
        })?;
    let result = backend
        .settle_device_changes()
        .context("failed to settle the temporary Windows inspection clone")
        .and_then(|()| inspect_clone(backend, &clone, boot_nic_ids, architecture));
    // A failed unmount must leave the clone intact, not destroy mounted storage.
    let cleanup = backend
        .validate_unmounted(&clone)
        .with_context(|| format!("temporary inspection clone '{clone}' is not safely unmounted"))
        .and_then(|()| backend.destroy(&clone))
        .with_context(|| format!("failed to remove temporary inspection clone '{clone}'"));
    match (result, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(cleanup)) => Err(cleanup.context(DriverInspectionCleanupFailure)),
        (Err(error), Err(cleanup)) => Err(cleanup
            .context(DriverInspectionCleanupFailure)
            .context(format!("driver inspection also failed: {error:#}"))),
    }
}

fn inspect_clone(
    backend: &dyn ImageBackend,
    clone: &str,
    boot_nic_ids: &[String],
    architecture: &str,
) -> Result<bool> {
    let probe = backend
        .probe_boot(clone)
        .with_context(|| format!("failed to inspect partitions of '{clone}'"))?;
    let mut partitions = probe
        .partitions
        .iter()
        .filter(|partition| {
            partition
                .filesystem
                .as_deref()
                .is_some_and(|filesystem| filesystem.eq_ignore_ascii_case("ntfs"))
        })
        .collect::<Vec<_>>();
    // Windows normally owns the largest NTFS volume; WinRE is tried as a
    // fallback only when the first candidate is not a Windows installation.
    partitions.sort_by_key(|partition| std::cmp::Reverse(partition.size));

    let mut mount_error: Option<anyhow::Error> = None;
    for partition in partitions {
        let mount = tempfile::TempDir::new().context("failed to create inspection mount point")?;
        let device = if partition.name.starts_with('/') {
            PathBuf::from(&partition.name)
        } else {
            Path::new("/dev").join(&partition.name)
        };
        if let Err(error) = mount_read_only(&device, mount.path()) {
            log::debug!(
                "could not mount Windows inspection partition '{}' read-only: {error:#}",
                device.display()
            );
            mount_error.get_or_insert(error);
            continue;
        }
        let inspection = if mount.path().join("Windows/System32").is_dir() {
            driver_store_covers_boot_nic(mount.path(), boot_nic_ids, architecture)
        } else {
            Ok(false)
        };
        let unmount = unmount_read_only(mount.path());
        let unmount = if let Err(error) = unmount {
            // Do not let TempDir recursively clean a still-mounted volume.
            // Retain the path alongside the clone for manual recovery.
            let retained_mount = mount.keep();
            Err(error.context(format!(
                "inspection mount retained at '{}' for recovery",
                retained_mount.display()
            )))
        } else {
            Ok(())
        };
        match (inspection, unmount) {
            (Ok(true), Ok(())) => return Ok(true),
            (Ok(false), Ok(())) => continue,
            (Err(error), Ok(())) => return Err(error),
            (Ok(_), Err(error)) => return Err(error),
            (Err(error), Err(unmount_error)) => {
                return Err(error.context(format!(
                    "additionally failed to unmount the inspection volume: {unmount_error:#}"
                )))
            }
        }
    }
    match mount_error {
        Some(error) => Err(error
            .context("no NTFS volume of the selected Windows snapshot could be mounted read-only")),
        None => Ok(false),
    }
}

fn mount_read_only(device: &Path, mountpoint: &Path) -> Result<()> {
    let output = Command::new("ntfs-3g")
        .args(["-o", "ro,noatime,nodev,nosuid,noexec"])
        .arg(device)
        .arg(mountpoint)
        .output()
        .context("failed to execute ntfs-3g")?;
    if !output.status.success() {
        bail!(
            "ntfs-3g could not mount '{}': {}",
            device.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn unmount_read_only(mountpoint: &Path) -> Result<()> {
    let output = Command::new("umount")
        .arg(mountpoint)
        .output()
        .context("failed to execute umount")?;
    if output.status.success() {
        return Ok(());
    }
    bail!(
        "could not unmount '{}': {}",
        mountpoint.display(),
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

/// Check a mounted Windows volume for a published, network-class driver that
/// matches any observed Boot NIC hardware ID. Pure filesystem inspection.
pub fn driver_store_covers_boot_nic(
    windows_root: &Path,
    boot_nic_ids: &[String],
    architecture: &str,
) -> Result<bool> {
    if boot_nic_ids.is_empty() {
        return Ok(false);
    }
    let published_dir = windows_root.join("Windows/INF");
    let published = published_inf_hashes(&published_dir)?;
    let store = windows_root.join("Windows/System32/DriverStore/FileRepository");
    let mut budget = MAX_INF_FILES;

    for inf in collect_inf_files(&store, DRIVER_STORE_DEPTH, &mut budget)? {
        let Some((bytes, metadata)) = read_inf(&inf) else {
            continue;
        };
        if !driver_matches(&metadata, boot_nic_ids, architecture) {
            continue;
        }
        let digest = Sha256::digest(&bytes);
        if !published.contains_key(digest.as_slice()) {
            continue;
        }
        if service_binary_present(windows_root, &inf, &metadata, architecture) {
            return Ok(true);
        }
    }

    // Inbox network drivers live directly in Windows/INF and are not necessarily
    // staged in DriverStore. They are already published by definition.
    for inf in collect_inf_files(&published_dir, 0, &mut budget)? {
        let Some((_bytes, metadata)) = read_inf(&inf) else {
            continue;
        };
        if driver_matches(&metadata, boot_nic_ids, architecture)
            && service_binary_present(windows_root, &inf, &metadata, architecture)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn driver_matches(
    metadata: &WindowsInfMetadata,
    boot_nic_ids: &[String],
    architecture: &str,
) -> bool {
    metadata.is_network_class
        && supports_architecture(metadata, architecture)
        && metadata
            .hardware_ids
            .iter()
            .chain(metadata.compatible_ids.iter())
            .any(|candidate| {
                boot_nic_ids
                    .iter()
                    .any(|observed| boot_nic_matches(candidate, observed))
            })
}

/// Match an INF hardware ID against the IDs reported for a Boot NIC.
///
/// Windows reports hardware IDs most-specific first and may append
/// `SUBSYS`, `REV`, or `CC` qualifiers. A generic INF ID (`VEN` + `DEV`)
/// covers every such variant; a subsystem-specific INF only covers its own
/// variant. The device-instance suffix form (`...\\4&...`) is also accepted.
fn boot_nic_matches(inf_id: &str, observed_id: &str) -> bool {
    if pnp_id_matches(inf_id, observed_id) {
        return true;
    }
    let inf = normalize_pnp(inf_id);
    if inf.is_empty() {
        return false;
    }
    let normalized = normalize_pnp(observed_id);
    let observed = hardware_portion(&normalized);
    let observed_without_revision = strip_qualifiers(observed, &["&rev_", "&cc_"]);
    if observed_without_revision == inf {
        return true;
    }
    let specific = inf.contains("&subsys_") || inf.contains("&rev_") || inf.contains("&cc_");
    !specific
        && (strip_qualifiers(observed, &["&subsys_", "&rev_", "&cc_"]) == inf
            || strip_qualifiers(observed, &["&subsys_", "&rev_", "&cc_"])
                .starts_with(&format!("{inf}&")))
}

fn strip_qualifiers(value: &str, qualifiers: &[&str]) -> String {
    let cut = qualifiers
        .iter()
        .filter_map(|qualifier| value.find(qualifier))
        .min()
        .unwrap_or(value.len());
    value[..cut].to_string()
}

/// Drop the instance-specific suffix from a device instance ID while leaving
/// plain hardware IDs such as `PCI\\VEN_8086&DEV_15F3` intact.
fn hardware_portion(value: &str) -> &str {
    match value.rsplit_once('\\') {
        Some((hardware, _)) if hardware.contains('\\') => hardware,
        _ => value,
    }
}

fn normalize_pnp(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .to_ascii_lowercase()
        .replace('/', "\\")
}

fn supports_architecture(metadata: &WindowsInfMetadata, architecture: &str) -> bool {
    let target = normalize_architecture(architecture);
    matches!(target.as_str(), "x64" | "x86" | "arm64")
        && (metadata.architectures.is_empty()
            || metadata
                .architectures
                .iter()
                .any(|candidate| normalize_architecture(candidate) == target))
}

fn read_inf(path: &Path) -> Option<(Vec<u8>, WindowsInfMetadata)> {
    let file = fs::metadata(path).ok()?;
    if !file.is_file() || file.len() > MAX_INF_BYTES {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    let metadata = parse_inf(&decode_inf_bytes(&bytes));
    Some((bytes, metadata))
}

/// SHA-256 of every INF published under `Windows/INF`, mirroring NexBoot's
/// rule that a DriverStore package is not considered installed unless Windows
/// also published its INF for PnP.
fn published_inf_hashes(directory: &Path) -> Result<BTreeMap<Vec<u8>, BTreeSet<String>>> {
    let mut published: BTreeMap<Vec<u8>, BTreeSet<String>> = BTreeMap::new();
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(published),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", directory.display()))
        }
    };
    for (index, entry) in entries.enumerate() {
        if index >= MAX_INF_FILES {
            bail!("published Windows INF directory exceeds the inspection entry limit");
        }
        let entry = entry.context("failed to enumerate published Windows INF files")?;
        let path = entry.path();
        if !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("inf"))
        {
            continue;
        }
        let Some((bytes, _)) = read_inf(&path) else {
            continue;
        };
        published
            .entry(Sha256::digest(&bytes).to_vec())
            .or_default()
            .insert(
                path.file_name()
                    .map(|name| name.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default(),
            );
    }
    Ok(published)
}

fn collect_inf_files(root: &Path, max_depth: usize, budget: &mut usize) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut directories = 0usize;
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((directory, depth)) = stack.pop() {
        if *budget == 0 || directories >= MAX_DIRECTORIES {
            break;
        }
        directories += 1;
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to read {}", directory.display()))
            }
        };
        for entry in entries.flatten() {
            if *budget == 0 {
                break;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            let path = entry.path();
            if file_type.is_dir() && depth < max_depth {
                stack.push((path, depth + 1));
            } else if file_type.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("inf"))
            {
                files.push(path);
                *budget -= 1;
            }
        }
    }
    Ok(files)
}

fn service_binary_present(
    windows_root: &Path,
    inf_path: &Path,
    metadata: &WindowsInfMetadata,
    architecture: &str,
) -> bool {
    let binaries = boot_service_profiles_for_architecture(inf_path, architecture)
        .map(|profiles| {
            profiles
                .into_iter()
                .map(|profile| profile.binary)
                .collect::<Vec<_>>()
        })
        .ok()
        .filter(|binaries| !binaries.is_empty())
        .unwrap_or_else(|| metadata.service_binaries.clone());
    let drivers = windows_root.join("Windows/System32/drivers");
    let package = inf_path.parent().unwrap_or(windows_root);
    binaries.iter().any(|binary| {
        resolve_service_binary(binary, package, &drivers)
            .is_some_and(|path| case_insensitive_file(&path))
    })
}

/// Windows paths are case-insensitive; the mounted probe runs on Linux.
fn case_insensitive_file(path: &Path) -> bool {
    if path.is_file() {
        return true;
    }
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return false;
    };
    let name = name.to_string_lossy();
    fs::read_dir(parent).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(&name)
                && entry.path().is_file()
        })
    })
}

/// Resolve the service binary paths INF files use in practice. A path outside
/// the image or a package directory is rejected rather than trusted.
fn resolve_service_binary(binary: &str, package: &Path, drivers: &Path) -> Option<PathBuf> {
    let normalized = binary
        .trim()
        .trim_matches('"')
        .replace('/', "\\")
        .to_ascii_lowercase();
    if normalized.is_empty() || normalized.contains("..") {
        return None;
    }
    let relative = normalized
        .strip_prefix("%12%\\")
        .or_else(|| normalized.strip_prefix("\\systemroot\\system32\\drivers\\"))
        .or_else(|| normalized.strip_prefix("%systemroot%\\system32\\drivers\\"))
        .or_else(|| {
            normalized
                .split_once("system32\\drivers\\")
                .map(|(_, rest)| rest)
        });
    if let Some(relative) = relative {
        return Some(drivers.join(relative));
    }
    if let Some(relative) = normalized.strip_prefix("%13%\\") {
        return Some(package.join(relative));
    }
    if !normalized.contains('\\') && !normalized.contains('%') {
        return Some(drivers.join(normalized));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::image::BootProbe;
    use crate::infrastructure::image::ImageBackendInfo;
    use std::sync::Mutex;

    #[derive(Default)]
    struct CleanupBackend {
        settle_fails: bool,
        probe_fails: bool,
        destroy_fails: bool,
        mounted: bool,
        calls: Mutex<Vec<(&'static str, String)>>,
    }

    impl CleanupBackend {
        fn record(&self, operation: &'static str, name: &str) {
            self.calls.lock().unwrap().push((operation, name.into()));
        }

        fn inspect(&self) -> Result<bool> {
            snapshot_covers_boot_nic(
                self,
                "pool/images/windows",
                "ready",
                &["PCI\\VEN_1AF4&DEV_1000".into()],
                "amd64",
            )
        }

        fn assert_calls(&self, expected: &[&str]) {
            let calls = self.calls.lock().unwrap();
            assert_eq!(
                calls
                    .iter()
                    .map(|(operation, _)| *operation)
                    .collect::<Vec<_>>(),
                expected
            );
            let clone = &calls[0].1;
            assert!(clone.starts_with("pool/images/driver-check-"));
            for (operation, name) in calls.iter() {
                if *operation != "settle" {
                    assert_eq!(name, clone, "operation must use only the temporary clone");
                }
            }
        }
    }

    impl ImageBackend for CleanupBackend {
        fn clone_image(&self, source: &str, destination: &str) -> Result<()> {
            assert_eq!(source, "pool/images/windows@ready");
            self.record("clone", destination);
            Ok(())
        }
        fn settle_device_changes(&self) -> Result<()> {
            self.record("settle", "");
            if self.settle_fails {
                bail!("fake settle failure");
            }
            Ok(())
        }
        fn probe_boot(&self, name: &str) -> Result<BootProbe> {
            self.record("probe", name);
            if self.probe_fails {
                bail!("fake probe failure");
            }
            // No partitions: tests cannot invoke mount commands or touch storage.
            Ok(BootProbe::default())
        }
        fn validate_unmounted(&self, name: &str) -> Result<()> {
            self.record("validate", name);
            if self.mounted {
                bail!("fake volume remains mounted");
            }
            Ok(())
        }
        fn destroy(&self, name: &str) -> Result<()> {
            self.record("destroy", name);
            assert!(!self.mounted, "must never destroy mounted storage");
            if self.destroy_fails {
                bail!("fake destroy failure");
            }
            Ok(())
        }
        fn exists(&self, _: &str) -> Result<bool> {
            unreachable!()
        }
        fn create_volume(&self, _: &str, _: u64) -> Result<()> {
            unreachable!()
        }
        fn rename(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn create_snapshot(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn destroy_snapshot(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn rollback_snapshot(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn resize(&self, _: &str, _: u64) -> Result<()> {
            unreachable!()
        }
        fn import_raw(&self, _: &Path, _: &str, _: u64) -> Result<()> {
            unreachable!()
        }
        fn verify(&self, _: &str) -> Result<bool> {
            unreachable!()
        }
        fn info(&self, _: &str) -> Result<ImageBackendInfo> {
            unreachable!()
        }
        fn set_os_type(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn image_parent(&self) -> Result<String> {
            unreachable!()
        }
    }

    #[test]
    fn settle_failure_still_cleans_up_clone() {
        let backend = CleanupBackend {
            settle_fails: true,
            ..Default::default()
        };
        assert!(format!("{:#}", backend.inspect().unwrap_err()).contains("fake settle failure"));
        backend.assert_calls(&["clone", "settle", "validate", "destroy"]);
    }

    #[test]
    fn probe_failure_still_cleans_up_clone() {
        let backend = CleanupBackend {
            probe_fails: true,
            ..Default::default()
        };
        let error = backend.inspect().unwrap_err();
        assert!(format!("{error:#}").contains("fake probe failure"));
        assert!(error
            .downcast_ref::<DriverInspectionCleanupFailure>()
            .is_none());
        backend.assert_calls(&["clone", "settle", "probe", "validate", "destroy"]);
    }

    #[test]
    fn cleanup_failure_is_propagated_after_successful_probe() {
        let backend = CleanupBackend {
            destroy_fails: true,
            ..Default::default()
        };
        let error = backend.inspect().unwrap_err();
        assert!(format!("{error:#}").contains("fake destroy failure"));
        assert!(error
            .downcast_ref::<DriverInspectionCleanupFailure>()
            .is_some());
        backend.assert_calls(&["clone", "settle", "probe", "validate", "destroy"]);
    }

    #[test]
    fn cleanup_failure_preserves_probe_failure() {
        let backend = CleanupBackend {
            probe_fails: true,
            destroy_fails: true,
            ..Default::default()
        };
        let error = backend.inspect().unwrap_err();
        assert!(format!("{error:#}").contains("fake probe failure"));
        assert!(error
            .downcast_ref::<DriverInspectionCleanupFailure>()
            .is_some());
        assert!(error
            .chain()
            .any(|cause| cause.to_string() == "fake destroy failure"));
        backend.assert_calls(&["clone", "settle", "probe", "validate", "destroy"]);
    }

    #[test]
    fn mounted_clone_is_never_destroyed() {
        let backend = CleanupBackend {
            mounted: true,
            ..Default::default()
        };
        assert!(
            format!("{:#}", backend.inspect().unwrap_err()).contains("fake volume remains mounted")
        );
        backend.assert_calls(&["clone", "settle", "probe", "validate"]);
    }

    const NIC_INF: &str = r#"[Version]
Signature="$Windows NT$"
Class=Net
ClassGuid={4d36e972-e325-11ce-bfc1-08002be10318}
Provider=%VENDOR%
DriverVer=01/01/2026,1.0.0.0

[Manufacturer]
%VENDOR%=Vendor,NTamd64

[Vendor.NTamd64]
%DEVICE%=Install, PCI\VEN_1AF4&DEV_1000

[Install.NTamd64]
CopyFiles=DriverFiles

[Install.NTamd64.Services]
AddService=netkvm,0x00000002,Service

[Install.NTamd64.HW]
AddReg=Hardware

[Hardware]
HKR,,*IfType,,6

[DriverFiles]
netkvm.sys

[Service]
ServiceBinary=%12%\netkvm.sys

[Strings]
VENDOR="Test Vendor"
DEVICE="Test NIC"
"#;

    fn windows_root() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn write(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn published_driver_matching_the_boot_nic_is_covered() {
        let root = windows_root();
        write(
            &root
                .path()
                .join("Windows/System32/DriverStore/FileRepository/vendor/nic.inf"),
            NIC_INF,
        );
        write(&root.path().join("Windows/INF/oem12.inf"), NIC_INF);
        write(
            &root.path().join("Windows/System32/drivers/netkvm.sys"),
            "binary",
        );

        assert!(driver_store_covers_boot_nic(
            root.path(),
            &["PCI\\VEN_1AF4&DEV_1000&SUBSYS_00000000".to_string()],
            "x64",
        )
        .unwrap());
    }

    #[test]
    fn unpublished_driver_store_package_is_not_covered() {
        let root = windows_root();
        write(
            &root
                .path()
                .join("Windows/System32/DriverStore/FileRepository/vendor/nic.inf"),
            NIC_INF,
        );
        write(
            &root.path().join("Windows/System32/drivers/netkvm.sys"),
            "binary",
        );

        assert!(!driver_store_covers_boot_nic(
            root.path(),
            &["PCI\\VEN_1AF4&DEV_1000".to_string()],
            "x64",
        )
        .unwrap());
    }

    #[test]
    fn published_driver_without_its_service_binary_is_not_covered() {
        let root = windows_root();
        write(
            &root
                .path()
                .join("Windows/System32/DriverStore/FileRepository/vendor/nic.inf"),
            NIC_INF,
        );
        write(&root.path().join("Windows/INF/oem12.inf"), NIC_INF);

        assert!(!driver_store_covers_boot_nic(
            root.path(),
            &["PCI\\VEN_1AF4&DEV_1000".to_string()],
            "x64",
        )
        .unwrap());
    }

    #[test]
    fn inbox_driver_is_covered_without_a_driver_store_package() {
        let root = windows_root();
        write(&root.path().join("Windows/INF/netkvm.inf"), NIC_INF);
        write(
            &root.path().join("Windows/System32/drivers/NETKVM.SYS"),
            "binary",
        );

        assert!(driver_store_covers_boot_nic(
            root.path(),
            &["PCI\\VEN_1AF4&DEV_1000".to_string()],
            "amd64",
        )
        .unwrap());
    }

    #[test]
    fn boot_nic_matching_accepts_subsystem_variants_of_a_generic_id() {
        assert!(boot_nic_matches(
            "PCI\\VEN_1AF4&DEV_1000",
            "PCI\\VEN_1AF4&DEV_1000&SUBSYS_00000000&REV_01",
        ));
        assert!(boot_nic_matches(
            "PCI\\VEN_1AF4&DEV_1000",
            "PCI\\VEN_1AF4&DEV_1000\\4&1A2B3C4D&0&00E0",
        ));
        assert!(boot_nic_matches(
            "PCI\\VEN_1AF4&DEV_1000&SUBSYS_00000000",
            "PCI\\VEN_1AF4&DEV_1000&SUBSYS_00000000&REV_01",
        ));
        assert!(!boot_nic_matches(
            "PCI\\VEN_1AF4&DEV_1000&SUBSYS_11111111",
            "PCI\\VEN_1AF4&DEV_1000&SUBSYS_00000000",
        ));
        assert!(!boot_nic_matches(
            "PCI\\VEN_1AF4&DEV_1001",
            "PCI\\VEN_1AF4&DEV_1000&SUBSYS_00000000",
        ));
    }

    #[test]
    fn architecture_and_hardware_id_mismatches_are_not_covered() {
        let root = windows_root();
        write(
            &root
                .path()
                .join("Windows/System32/DriverStore/FileRepository/vendor/nic.inf"),
            NIC_INF,
        );
        write(&root.path().join("Windows/INF/oem12.inf"), NIC_INF);
        write(
            &root.path().join("Windows/System32/drivers/netkvm.sys"),
            "binary",
        );

        assert!(!driver_store_covers_boot_nic(
            root.path(),
            &["PCI\\VEN_1AF4&DEV_1000".to_string()],
            "arm64",
        )
        .unwrap());
        assert!(!driver_store_covers_boot_nic(
            root.path(),
            &["PCI\\VEN_1AF4&DEV_1000".to_string()],
            "",
        )
        .unwrap());
        assert!(!driver_store_covers_boot_nic(
            root.path(),
            &["PCI\\VEN_8086&DEV_15F3".to_string()],
            "x64",
        )
        .unwrap());
    }
}
