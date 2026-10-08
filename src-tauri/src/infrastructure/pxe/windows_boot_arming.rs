//! Conservative offline Windows SYSTEM hive arming.
//!
//! This module separates driver installation from boot arming. Driver packages are
//! installed by DISM; this armer only changes registry values for services that already
//! exist in the target Windows installation.
//!
//! The mutation path uses Windows' own registry loader (reg.exe load/unload) instead
//! of rewriting REGF cells directly. It never creates vendor NIC services, never copies
//! driver binaries, never writes CriticalDeviceDatabase entries and never deletes
//! registry transaction logs.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const NET_CLASS_GUID: &str = "{4d36e972-e325-11ce-bfc1-08002be10318}";
const SERVICE_KERNEL_DRIVER: u32 = 0x0000_0001;
const SERVICE_FILE_SYSTEM_DRIVER: u32 = 0x0000_0002;
const CM_SERVICE_NETWORK_BOOT_LOAD: u32 = 0x0000_0001;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsBootArmConfig {
    #[serde(default)]
    pub allow_dirty_hive: bool,
    #[serde(default = "default_true")]
    pub increase_disk_timeout: bool,
    #[serde(default)]
    pub disable_task_offload: bool,
    #[serde(default)]
    pub disable_crash_dump: bool,
    #[serde(default)]
    pub clear_paging_files: bool,
}

impl Default for WindowsBootArmConfig {
    fn default() -> Self {
        Self {
            allow_dirty_hive: false,
            increase_disk_timeout: true,
            disable_task_offload: false,
            disable_crash_dump: false,
            clear_paging_files: false,
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum RegistryArmValue {
    Dword(u32),
    MultiString(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryArmChange {
    pub key: String,
    pub name: String,
    pub value: RegistryArmValue,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsBootInventory {
    pub active_control_set: String,
    pub native_nic_services: Vec<String>,
    pub existing_services: Vec<String>,
    pub driver_services: Vec<String>,
    pub hive_dirty: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsBootArmPlan {
    pub active_control_set: String,
    pub native_nic_services: Vec<String>,
    pub hive_dirty: bool,
    pub changes: Vec<RegistryArmChange>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsBootArmResult {
    pub system_hive: String,
    pub plan: WindowsBootArmPlan,
    pub applied: bool,
}

#[derive(Debug, Clone)]
pub struct WindowsBootArmer {
    reg: PathBuf,
}

impl WindowsBootArmer {
    pub fn new() -> Result<Self> {
        let reg = find_command("reg.exe")
            .or_else(|| find_command("reg"))
            .ok_or_else(|| anyhow::anyhow!(
                "Windows registry tool was not found; offline SYSTEM arming must run in a Windows servicing environment"
            ))?;
        Ok(Self { reg })
    }

    pub fn with_reg(path: impl Into<PathBuf>) -> Self {
        Self { reg: path.into() }
    }

    pub fn inspect(&self, system_hive: &Path) -> Result<WindowsBootInventory> {
        validate_system_hive(system_hive)?;
        let dirty = hive_is_dirty(system_hive)?;
        let mount = format!("HKLM\\DISKLESS_MANAGER_OFFLINE_{}", std::process::id());

        self.load_hive(&mount, system_hive)?;
        let inspected = self.inspect_loaded(&mount, dirty);
        let unload = self.unload_hive(&mount);

        match (inspected, unload) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(error), Ok(())) => Err(error),
            (Ok(_), Err(error)) => Err(error),
            (Err(primary), Err(cleanup)) => Err(primary.context(format!(
                "additionally failed to unload offline SYSTEM hive: {cleanup:#}"
            ))),
        }
    }

    pub fn plan(
        &self,
        system_hive: &Path,
        config: &WindowsBootArmConfig,
    ) -> Result<WindowsBootArmPlan> {
        validate_system_hive(system_hive)?;
        if hive_is_dirty(system_hive)? && !config.allow_dirty_hive {
            bail!(
                "SYSTEM hive is dirty (REGF primary/secondary sequence numbers differ); refusing to load it for planning without allow_dirty_hive"
            );
        }
        let inventory = self.inspect(system_hive)?;
        build_plan(&inventory, config)
    }

    pub fn arm(
        &self,
        system_hive: &Path,
        config: &WindowsBootArmConfig,
        apply: bool,
    ) -> Result<WindowsBootArmResult> {
        validate_system_hive(system_hive)?;
        let dirty = hive_is_dirty(system_hive)?;
        if dirty && !config.allow_dirty_hive {
            bail!(
                "SYSTEM hive is dirty (REGF primary/secondary sequence numbers differ); refusing offline mutation without allow_dirty_hive"
            );
        }

        let mount = format!("HKLM\\DISKLESS_MANAGER_OFFLINE_{}", std::process::id());
        self.load_hive(&mount, system_hive)?;

        let operation = (|| -> Result<WindowsBootArmPlan> {
            let inventory = self.inspect_loaded(&mount, dirty)?;
            let plan = build_plan(&inventory, config)?;
            if apply {
                for change in &plan.changes {
                    self.apply_change(&mount, change)?;
                }
            }
            Ok(plan)
        })();

        let unload = self.unload_hive(&mount);
        let plan = match (operation, unload) {
            (Ok(plan), Ok(())) => plan,
            (Err(error), Ok(())) => return Err(error),
            (Ok(_), Err(error)) => return Err(error),
            (Err(primary), Err(cleanup)) => {
                return Err(primary.context(format!(
                    "additionally failed to unload offline SYSTEM hive: {cleanup:#}"
                )))
            }
        };

        Ok(WindowsBootArmResult {
            system_hive: system_hive.display().to_string(),
            plan,
            applied: apply,
        })
    }

    fn inspect_loaded(&self, mount: &str, dirty: bool) -> Result<WindowsBootInventory> {
        let current = self
            .query_dword(&format!("{mount}\\Select"), "Current")?
            .ok_or_else(|| anyhow::anyhow!("offline SYSTEM hive has no Select\\Current DWORD"))?;
        if current == 0 || current > 999 {
            bail!("invalid Select\\Current value: {current}");
        }
        let control_set = format!("ControlSet{current:03}");

        let class_key = format!("{mount}\\{control_set}\\Control\\Class\\{NET_CLASS_GUID}");
        let output = self.query(&["query", &class_key, "/s", "/v", "Service"])?;
        let mut native_nic_services = BTreeSet::new();
        if output.status.success() {
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                if let Some(value) = parse_reg_value(line, "REG_SZ") {
                    let service = value.trim();
                    if !service.is_empty() {
                        native_nic_services.insert(service.to_string());
                    }
                }
            }
        }

        let mut candidates = native_nic_services.clone();
        for service in ["iScsiPrt", "Tcpip", "ndis", "MSiSCSI", "disk"] {
            candidates.insert(service.to_string());
        }

        let mut existing_services = BTreeSet::new();
        let mut driver_services = BTreeSet::new();
        for service in candidates {
            let key = format!("{mount}\\{control_set}\\Services\\{service}");
            if self.query(&["query", &key])?.status.success() {
                existing_services.insert(service.clone());
                if let Some(service_type) = self.query_dword(&key, "Type")? {
                    if service_type & (SERVICE_KERNEL_DRIVER | SERVICE_FILE_SYSTEM_DRIVER) != 0 {
                        driver_services.insert(service);
                    }
                }
            }
        }

        Ok(WindowsBootInventory {
            active_control_set: control_set,
            native_nic_services: native_nic_services.into_iter().collect(),
            existing_services: existing_services.into_iter().collect(),
            driver_services: driver_services.into_iter().collect(),
            hive_dirty: dirty,
        })
    }

    fn apply_change(&self, mount: &str, change: &RegistryArmChange) -> Result<()> {
        let key = format!("{mount}\\{}", change.key);
        let output = match &change.value {
            RegistryArmValue::Dword(value) => self.query(&[
                "add",
                &key,
                "/v",
                &change.name,
                "/t",
                "REG_DWORD",
                "/d",
                &value.to_string(),
                "/f",
            ])?,
            RegistryArmValue::MultiString(values) => {
                let data = values.join("\\0");
                self.query(&[
                    "add",
                    &key,
                    "/v",
                    &change.name,
                    "/t",
                    "REG_MULTI_SZ",
                    "/d",
                    &data,
                    "/f",
                ])?
            }
        };

        if !output.status.success() {
            bail!(
                "failed to apply {}\\{}: {}",
                change.key,
                change.name,
                command_error(&output)
            );
        }
        Ok(())
    }

    fn query_dword(&self, key: &str, value_name: &str) -> Result<Option<u32>> {
        let output = self.query(&["query", key, "/v", value_name])?;
        if !output.status.success() {
            return Ok(None);
        }
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let Some(value) = parse_reg_value(line, "REG_DWORD") else {
                continue;
            };
            let value = value.trim();
            if let Some(hex) = value.strip_prefix("0x").or_else(|| value.strip_prefix("0X")) {
                if let Ok(parsed) = u32::from_str_radix(hex, 16) {
                    return Ok(Some(parsed));
                }
            }
            if let Ok(parsed) = value.parse::<u32>() {
                return Ok(Some(parsed));
            }
        }
        Ok(None)
    }

    fn load_hive(&self, mount: &str, system_hive: &Path) -> Result<()> {
        let output = Command::new(&self.reg)
            .arg("load")
            .arg(mount)
            .arg(system_hive)
            .output()
            .with_context(|| format!("failed to execute {}", self.reg.display()))?;
        if !output.status.success() {
            bail!("failed to load offline SYSTEM hive: {}", command_error(&output));
        }
        Ok(())
    }

    fn unload_hive(&self, mount: &str) -> Result<()> {
        let output = self.query(&["unload", mount])?;
        if !output.status.success() {
            bail!("failed to unload offline SYSTEM hive: {}", command_error(&output));
        }
        Ok(())
    }

    fn query(&self, args: &[&str]) -> Result<Output> {
        Command::new(&self.reg)
            .args(args)
            .output()
            .with_context(|| format!("failed to execute {}", self.reg.display()))
    }
}

pub fn build_plan(
    inventory: &WindowsBootInventory,
    config: &WindowsBootArmConfig,
) -> Result<WindowsBootArmPlan> {
    if inventory.hive_dirty && !config.allow_dirty_hive {
        bail!(
            "SYSTEM hive is dirty; no arming plan will be produced unless allow_dirty_hive is enabled"
        );
    }

    let existing = inventory
        .existing_services
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let drivers = inventory
        .driver_services
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();

    let mut changes = Vec::new();
    let mut warnings = Vec::new();

    for service in &inventory.native_nic_services {
        if !existing.contains(&service.to_ascii_lowercase()) {
            warnings.push(format!(
                "Net-class binding references {service}, but Services\\{service} is absent; not fabricating a service key"
            ));
            continue;
        }
        push_dword(
            &mut changes,
            format!("{}\\Services\\{}", inventory.active_control_set, service),
            "Start",
            0,
            "native NIC service already bound by Windows; make it boot-start",
        );
        if drivers.contains(&service.to_ascii_lowercase()) {
            push_dword(
                &mut changes,
                format!("{}\\Services\\{}", inventory.active_control_set, service),
                "BootFlags",
                CM_SERVICE_NETWORK_BOOT_LOAD,
                "mark existing network driver for network boot",
            );
        } else {
            warnings.push(format!(
                "{service} is bound as a NIC service but is not typed as a kernel/filesystem driver; BootFlags not added"
            ));
        }
    }

    if existing.contains("iscsiprt") {
        push_dword(
            &mut changes,
            format!("{}\\Services\\iScsiPrt", inventory.active_control_set),
            "Start",
            0,
            "existing Microsoft iSCSI port driver is required before the network boot volume",
        );
        if drivers.contains("iscsiprt") {
            push_dword(
                &mut changes,
                format!("{}\\Services\\iScsiPrt", inventory.active_control_set),
                "BootFlags",
                CM_SERVICE_NETWORK_BOOT_LOAD,
                "mark existing iSCSI port driver for network boot",
            );
        }
    }

    for service in ["Tcpip", "ndis"] {
        if existing.contains(&service.to_ascii_lowercase()) {
            push_dword(
                &mut changes,
                format!("{}\\Services\\{service}", inventory.active_control_set),
                "Start",
                0,
                "existing core network stack component required for boot networking",
            );
        } else {
            warnings.push(format!(
                "core network service {service} was not found in {}",
                inventory.active_control_set
            ));
        }
    }

    if existing.contains("msiscsi") {
        warnings.push(
            "MSiSCSI service detected but left unchanged; boot-driver flags are only applied to driver services"
                .to_string(),
        );
    }

    if config.increase_disk_timeout && existing.contains("disk") {
        push_dword(
            &mut changes,
            format!("{}\\Services\\disk", inventory.active_control_set),
            "TimeOutValue",
            60,
            "allow transient network-storage stalls without immediately losing the boot LUN",
        );
    }

    if config.disable_task_offload && existing.contains("tcpip") {
        push_dword(
            &mut changes,
            format!("{}\\Services\\Tcpip\\Parameters", inventory.active_control_set),
            "DisableTaskOffload",
            1,
            "optional compatibility profile: disable TCP task offload",
        );
    }

    if config.disable_crash_dump {
        let key = format!("{}\\Control\\CrashControl", inventory.active_control_set);
        for (name, value) in [
            ("CrashDumpEnabled", 0),
            ("LogEvent", 0),
            ("SendAlert", 0),
            ("AutoReboot", 0),
            ("DisplayParameters", 1),
        ] {
            push_dword(
                &mut changes,
                key.clone(),
                name,
                value,
                "optional compatibility profile: avoid dump-stack use of the boot NIC",
            );
        }
    }

    if config.clear_paging_files {
        let key = format!(
            "{}\\Control\\Session Manager\\Memory Management",
            inventory.active_control_set
        );
        for name in ["PagingFiles", "ExistingPageFiles"] {
            changes.push(RegistryArmChange {
                key: key.clone(),
                name: name.to_string(),
                value: RegistryArmValue::MultiString(Vec::new()),
                reason:
                    "optional compatibility profile: keep paging traffic off the network boot volume"
                        .to_string(),
            });
        }
    }

    Ok(WindowsBootArmPlan {
        active_control_set: inventory.active_control_set.clone(),
        native_nic_services: inventory.native_nic_services.clone(),
        hive_dirty: inventory.hive_dirty,
        changes,
        warnings,
    })
}

fn push_dword(
    changes: &mut Vec<RegistryArmChange>,
    key: String,
    name: &str,
    value: u32,
    reason: &str,
) {
    changes.push(RegistryArmChange {
        key,
        name: name.to_string(),
        value: RegistryArmValue::Dword(value),
        reason: reason.to_string(),
    });
}

fn parse_reg_value<'a>(line: &'a str, reg_type: &str) -> Option<&'a str> {
    let marker = line.find(reg_type)?;
    Some(line[marker + reg_type.len()..].trim())
}

fn validate_system_hive(path: &Path) -> Result<()> {
    if !path.is_file() {
        bail!("SYSTEM hive does not exist: {}", path.display());
    }
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read SYSTEM hive {}", path.display()))?;
    if bytes.len() < 0x200 || &bytes[..4] != b"regf" {
        bail!("file is not a valid REGF hive: {}", path.display());
    }
    Ok(())
}

pub fn hive_is_dirty(path: &Path) -> Result<bool> {
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read SYSTEM hive {}", path.display()))?;
    if bytes.len() < 12 || &bytes[..4] != b"regf" {
        bail!("file is not a valid REGF hive: {}", path.display());
    }
    let primary = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    let secondary = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    Ok(primary != secondary)
}

fn command_error(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        return stderr;
    }
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn find_command(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inventory() -> WindowsBootInventory {
        WindowsBootInventory {
            active_control_set: "ControlSet001".to_string(),
            native_nic_services: vec!["e2fexpress".to_string(), "missingnic".to_string()],
            existing_services: vec![
                "e2fexpress".to_string(),
                "iScsiPrt".to_string(),
                "Tcpip".to_string(),
                "ndis".to_string(),
                "MSiSCSI".to_string(),
                "disk".to_string(),
            ],
            driver_services: vec![
                "e2fexpress".to_string(),
                "iScsiPrt".to_string(),
                "ndis".to_string(),
            ],
            hive_dirty: false,
        }
    }

    #[test]
    fn conservative_plan_only_arms_existing_services() {
        let plan = build_plan(&inventory(), &WindowsBootArmConfig::default()).unwrap();

        assert!(plan.changes.iter().any(|change| {
            change.key.ends_with("Services\\e2fexpress") && change.name == "BootFlags"
        }));
        assert!(!plan
            .changes
            .iter()
            .any(|change| change.key.contains("missingnic")));
        assert!(!plan
            .changes
            .iter()
            .any(|change| change.key.contains("CriticalDeviceDatabase")));
        assert!(!plan.changes.iter().any(|change| {
            change.key.ends_with("Services\\MSiSCSI") && change.name == "Start"
        }));
    }

    #[test]
    fn invasive_compatibility_changes_are_opt_in() {
        let base = build_plan(&inventory(), &WindowsBootArmConfig::default()).unwrap();
        assert!(!base
            .changes
            .iter()
            .any(|change| change.name == "DisableTaskOffload"));
        assert!(!base
            .changes
            .iter()
            .any(|change| change.name == "PagingFiles"));

        let config = WindowsBootArmConfig {
            disable_task_offload: true,
            disable_crash_dump: true,
            clear_paging_files: true,
            ..Default::default()
        };
        let extended = build_plan(&inventory(), &config).unwrap();
        assert!(extended
            .changes
            .iter()
            .any(|change| change.name == "DisableTaskOffload"));
        assert!(extended
            .changes
            .iter()
            .any(|change| change.name == "PagingFiles"));
    }

    #[test]
    fn dirty_hive_is_rejected_by_default() {
        let mut state = inventory();
        state.hive_dirty = true;
        assert!(build_plan(&state, &WindowsBootArmConfig::default()).is_err());
    }
}
