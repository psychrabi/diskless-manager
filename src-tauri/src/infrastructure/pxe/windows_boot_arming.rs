//! Conservative offline Windows boot arming.
//!
//! This module deliberately keeps driver installation and boot arming separate.
//! Driver packages are installed through DISM; the armer only changes existing
//! registry services in an offline SYSTEM hive. It never fabricates NIC services,
//! writes CriticalDeviceDatabase entries, copies driver binaries, or applies
//! vendor-specific NDIS tuning.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsBootArmingOptions {
    /// Produce the plan/report without modifying the hive.
    #[serde(default)]
    pub dry_run: bool,
    /// Extend disk-class I/O timeout for transient network-storage stalls.
    #[serde(default = "default_disk_timeout")]
    pub disk_timeout_seconds: Option<u32>,
    /// Disable kernel crash dump creation. Opt-in because this affects diagnostics.
    #[serde(default)]
    pub disable_crash_dumps: bool,
    /// Clear PagingFiles/ExistingPageFiles on the boot image. Opt-in.
    #[serde(default)]
    pub clear_paging_files: bool,
    /// Set Tcpip\Parameters\DisableTaskOffload=1. Opt-in.
    #[serde(default)]
    pub disable_task_offload: bool,
}

fn default_disk_timeout() -> Option<u32> {
    Some(60)
}

impl Default for WindowsBootArmingOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            disk_timeout_seconds: default_disk_timeout(),
            disable_crash_dumps: false,
            clear_paging_files: false,
            disable_task_offload: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RegistryChange {
    pub key: String,
    pub value: String,
    pub registry_type: String,
    pub data: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WindowsBootArmPlan {
    pub hive_path: String,
    pub control_set: String,
    pub nic_services: Vec<String>,
    pub changes: Vec<RegistryChange>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WindowsBootArmReport {
    pub success: bool,
    pub dry_run: bool,
    pub hive_path: String,
    pub control_set: String,
    pub nic_services: Vec<String>,
    pub applied_changes: Vec<RegistryChange>,
    pub skipped_changes: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct WindowsBootArmer;

impl WindowsBootArmer {
    pub fn new() -> Self {
        Self
    }

    /// Build a mutation plan from NIC services already discovered from the target image.
    ///
    /// The service list must come from the image's own Net class bindings (or another
    /// trusted inventory source). Unknown services are not guessed from PCI vendor IDs.
    pub fn plan(
        &self,
        hive_path: &Path,
        control_set: impl Into<String>,
        nic_services: impl IntoIterator<Item = String>,
        options: &WindowsBootArmingOptions,
    ) -> Result<WindowsBootArmPlan> {
        if !hive_path.is_file() {
            bail!("offline SYSTEM hive does not exist: {}", hive_path.display());
        }

        let control_set = normalize_control_set(control_set.into())?;
        let mut services = nic_services
            .into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        services.sort_by_key(|value| value.to_ascii_lowercase());
        services.dedup_by(|a, b| a.eq_ignore_ascii_case(b));

        let mut changes = Vec::new();
        let mut warnings = Vec::new();

        if services.is_empty() {
            warnings.push(
                "no native NIC services were supplied; no NIC service will be boot-armed"
                    .to_string(),
            );
        }

        for service in &services {
            let key = format!(r"{}\Services\{}", control_set, service);
            changes.push(dword(
                &key,
                "Start",
                0,
                "existing native NIC miniport must be available at boot",
            ));
            changes.push(dword(
                &key,
                "BootFlags",
                1,
                "CM_SERVICE_NETWORK_BOOT_LOAD for network-boot critical driver",
            ));
        }

        // Kernel/data-path components only. MSiSCSI is a Win32 service and is
        // intentionally not promoted to SERVICE_BOOT_START here.
        for (service, boot_flags) in [("iScsiPrt", true), ("Tcpip", false), ("ndis", false)] {
            let key = format!(r"{}\Services\{}", control_set, service);
            changes.push(dword(
                &key,
                "Start",
                0,
                "existing Windows network/iSCSI boot component",
            ));
            if boot_flags {
                changes.push(dword(
                    &key,
                    "BootFlags",
                    1,
                    "CM_SERVICE_NETWORK_BOOT_LOAD for iSCSI port driver",
                ));
            }
        }

        if let Some(timeout) = options.disk_timeout_seconds {
            if timeout == 0 {
                bail!("disk timeout must be greater than zero");
            }
            changes.push(dword(
                &format!(r"{}\Services\disk", control_set),
                "TimeOutValue",
                timeout,
                "allow transient network-storage stalls without replacing driver state",
            ));
        }

        if options.disable_task_offload {
            changes.push(dword(
                &format!(r"{}\Services\Tcpip\Parameters", control_set),
                "DisableTaskOffload",
                1,
                "operator-enabled compatibility option for boot NIC offload issues",
            ));
        }

        if options.disable_crash_dumps {
            let key = format!(r"{}\Control\CrashControl", control_set);
            for (name, value) in [
                ("CrashDumpEnabled", 0),
                ("LogEvent", 0),
                ("SendAlert", 0),
                ("AutoReboot", 0),
            ] {
                changes.push(dword(
                    &key,
                    name,
                    value,
                    "operator-enabled diskless boot compatibility option",
                ));
            }
            changes.push(dword(
                &key,
                "DisplayParameters",
                1,
                "show bugcheck parameters while automatic reboot is disabled",
            ));
        }

        if options.clear_paging_files {
            let key = format!(
                r"{}\Control\Session Manager\Memory Management",
                control_set
            );
            changes.push(multi_sz(
                &key,
                "PagingFiles",
                "",
                "operator-enabled option to avoid paging over the boot iSCSI path",
            ));
            changes.push(multi_sz(
                &key,
                "ExistingPageFiles",
                "",
                "operator-enabled option to avoid stale boot-volume pagefile state",
            ));
        }

        Ok(WindowsBootArmPlan {
            hive_path: hive_path.display().to_string(),
            control_set,
            nic_services: services,
            changes,
            warnings,
        })
    }

    /// Resolve Select\Current and apply a plan using Windows' own registry hive loader.
    ///
    /// This backend is intentionally Windows-only. Linux support should use a vetted
    /// offline registry backend rather than a bespoke REGF writer.
    pub fn arm(
        &self,
        hive_path: &Path,
        nic_services: impl IntoIterator<Item = String>,
        options: &WindowsBootArmingOptions,
    ) -> Result<WindowsBootArmReport> {
        #[cfg(windows)]
        {
            return self.arm_windows(hive_path, nic_services, options);
        }

        #[cfg(not(windows))]
        {
            let _ = (hive_path, nic_services, options);
            bail!(
                "offline SYSTEM hive arming is not available on this host; use the Windows/WinPE arming helper or a future vetted Linux registry backend"
            )
        }
    }

    #[cfg(windows)]
    fn arm_windows(
        &self,
        hive_path: &Path,
        nic_services: impl IntoIterator<Item = String>,
        options: &WindowsBootArmingOptions,
    ) -> Result<WindowsBootArmReport> {
        let mount_name = format!(
            "DisklessManagerArm_{}_{}",
            std::process::id(),
            chrono::Utc::now().timestamp_millis().unsigned_abs()
        );
        let root = format!(r"HKLM\{}", mount_name);

        run_reg(["load", &root, &hive_path.display().to_string()])
            .context("failed to load offline SYSTEM hive")?;

        let result = (|| -> Result<WindowsBootArmReport> {
            let current = query_dword(&format!(r"{}\Select", root), "Current")?
                .ok_or_else(|| anyhow::anyhow!("offline SYSTEM hive has no Select\\Current"))?;
            if current == 0 || current > 999 {
                bail!("invalid Select\\Current value in offline SYSTEM hive: {current}");
            }

            let control_set = format!("ControlSet{current:03}");
            let plan = self.plan(hive_path, control_set.clone(), nic_services, options)?;

            let mut applied_changes = Vec::new();
            let mut skipped_changes = Vec::new();
            let mut warnings = plan.warnings.clone();

            for change in &plan.changes {
                let mounted_key = format!(r"{}\{}", root, change.key);

                // Never fabricate a service key. Non-service option keys may be
                // created because they are explicit operator-selected policy/settings.
                if change.key.contains(r"\Services\") {
                    let service_root = service_root_for_change(&mounted_key);
                    if let Some(service_root) = service_root {
                        if !reg_key_exists(&service_root)? {
                            skipped_changes.push(format!(
                                "{}\\{} skipped because the service key does not exist",
                                change.key, change.value
                            ));
                            continue;
                        }
                    }
                }

                if options.dry_run {
                    applied_changes.push(change.clone());
                    continue;
                }

                run_reg([
                    "add",
                    &mounted_key,
                    "/v",
                    &change.value,
                    "/t",
                    &change.registry_type,
                    "/d",
                    &change.data,
                    "/f",
                ])
                .with_context(|| {
                    format!("failed to set {}\\{}", change.key, change.value)
                })?;
                applied_changes.push(change.clone());
            }

            if options.dry_run {
                warnings.push("dry-run: no registry values were changed".to_string());
            }

            Ok(WindowsBootArmReport {
                success: true,
                dry_run: options.dry_run,
                hive_path: plan.hive_path,
                control_set,
                nic_services: plan.nic_services,
                applied_changes,
                skipped_changes,
                warnings,
            })
        })();

        // Always unload. If this fails after a successful arm, surface it because an
        // attached hive prevents clean image servicing/unmounting.
        let unload = run_reg(["unload", &root]).context("failed to unload offline SYSTEM hive");

        match (result, unload) {
            (Ok(report), Ok(())) => Ok(report),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
        }
    }
}

fn dword(key: &str, value: &str, data: u32, reason: &str) -> RegistryChange {
    RegistryChange {
        key: key.to_string(),
        value: value.to_string(),
        registry_type: "REG_DWORD".to_string(),
        data: data.to_string(),
        reason: reason.to_string(),
    }
}

fn multi_sz(key: &str, value: &str, data: &str, reason: &str) -> RegistryChange {
    RegistryChange {
        key: key.to_string(),
        value: value.to_string(),
        registry_type: "REG_MULTI_SZ".to_string(),
        data: data.to_string(),
        reason: reason.to_string(),
    }
}

fn normalize_control_set(value: String) -> Result<String> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("CurrentControlSet") {
        return Ok("CurrentControlSet".to_string());
    }

    let lower = value.to_ascii_lowercase();
    if let Some(number) = lower.strip_prefix("controlset") {
        if number.len() == 3 && number.chars().all(|ch| ch.is_ascii_digit()) {
            return Ok(format!("ControlSet{}", number));
        }
    }

    bail!("invalid Windows control set name: {value}")
}

#[cfg(windows)]
fn run_reg<const N: usize>(args: [&str; N]) -> Result<()> {
    let output = Command::new("reg.exe")
        .args(args)
        .output()
        .context("failed to execute reg.exe")?;
    if !output.status.success() {
        bail!(
            "reg.exe failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

#[cfg(windows)]
fn query_dword(key: &str, value: &str) -> Result<Option<u32>> {
    let output = Command::new("reg.exe")
        .args(["query", key, "/v", value])
        .output()
        .context("failed to execute reg.exe query")?;

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if !line.to_ascii_lowercase().contains(&value.to_ascii_lowercase()) {
            continue;
        }
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if let Some(raw) = fields.last() {
            if let Some(hex) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
                if let Ok(value) = u32::from_str_radix(hex, 16) {
                    return Ok(Some(value));
                }
            }
            if let Ok(value) = raw.parse::<u32>() {
                return Ok(Some(value));
            }
        }
    }
    Ok(None)
}

#[cfg(windows)]
fn reg_key_exists(key: &str) -> Result<bool> {
    let output = Command::new("reg.exe")
        .args(["query", key])
        .output()
        .context("failed to execute reg.exe query")?;
    Ok(output.status.success())
}

#[cfg(windows)]
fn service_root_for_change(key: &str) -> Option<String> {
    let marker = r"\Services\";
    let start = key.find(marker)? + marker.len();
    let remaining = &key[start..];
    let service = remaining.split('\\').next()?;
    let prefix = &key[..start];
    Some(format!("{prefix}{service}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_only_arms_supplied_nic_services_and_kernel_stack() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let armer = WindowsBootArmer::new();
        let plan = armer
            .plan(
                temp.path(),
                "ControlSet001",
                vec!["e2fexpress".to_string()],
                &WindowsBootArmingOptions::default(),
            )
            .unwrap();

        assert!(plan
            .changes
            .iter()
            .any(|c| c.key.ends_with(r"Services\e2fexpress") && c.value == "BootFlags"));
        assert!(!plan
            .changes
            .iter()
            .any(|c| c.key.contains("rt640x64") || c.key.contains("e1d68x64")));
        assert!(!plan
            .changes
            .iter()
            .any(|c| c.key.ends_with(r"Services\MSiSCSI") && c.value == "Start"));
        assert!(!plan
            .changes
            .iter()
            .any(|c| c.key.contains("CriticalDeviceDatabase")));
    }

    #[test]
    fn invasive_options_are_opt_in() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let armer = WindowsBootArmer::new();
        let conservative = armer
            .plan(
                temp.path(),
                "ControlSet001",
                Vec::<String>::new(),
                &WindowsBootArmingOptions::default(),
            )
            .unwrap();
        assert!(!conservative
            .changes
            .iter()
            .any(|c| c.key.contains("CrashControl") || c.value == "PagingFiles"));

        let options = WindowsBootArmingOptions {
            disable_crash_dumps: true,
            clear_paging_files: true,
            disable_task_offload: true,
            ..Default::default()
        };
        let expanded = armer
            .plan(
                temp.path(),
                "ControlSet001",
                Vec::<String>::new(),
                &options,
            )
            .unwrap();
        assert!(expanded
            .changes
            .iter()
            .any(|c| c.key.contains("CrashControl")));
        assert!(expanded
            .changes
            .iter()
            .any(|c| c.value == "PagingFiles"));
        assert!(expanded
            .changes
            .iter()
            .any(|c| c.value == "DisableTaskOffload"));
    }
}
