//! Conservative offline Windows SYSTEM hive arming.
//!
//! This module separates driver installation from boot arming. Driver packages are
//! installed by DISM; this armer only changes registry values for services that already
//! exist in the target Windows installation or have a validated, DISM-staged INF profile.
//!
//! The mutation path uses Windows' own registry loader (reg.exe load/unload) instead
//! of rewriting REGF cells directly. Missing services require validated INF profiles; it never copies
//! driver binaries, never writes CriticalDeviceDatabase entries and never deletes
//! registry transaction logs.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
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
    String(String),
    ExpandString(String),
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
    #[serde(default)]
    pub selected_nic_services: Vec<String>,
    pub existing_services: Vec<String>,
    pub driver_services: Vec<String>,
    #[serde(default)]
    pub driver_boot_flags: BTreeMap<String, u32>,
    pub hive_dirty: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsBootArmPlan {
    pub active_control_set: String,
    pub native_nic_services: Vec<String>,
    #[serde(default)]
    pub selected_nic_services: Vec<String>,
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
        self.inspect_with_services(system_hive, &[])
    }

    pub fn inspect_with_services(
        &self,
        system_hive: &Path,
        services: &[String],
    ) -> Result<WindowsBootInventory> {
        self.inspect_with_policy(system_hive, services, false)
    }

    pub fn inspect_with_policy(
        &self,
        system_hive: &Path,
        services: &[String],
        allow_dirty_hive: bool,
    ) -> Result<WindowsBootInventory> {
        for service in services {
            validate_service_name(service)?;
        }
        validate_system_hive(system_hive)?;
        let dirty = hive_is_dirty(system_hive)?;
        if dirty && !allow_dirty_hive {
            bail!("SYSTEM hive is dirty; refusing to load it without allow_dirty_hive");
        }
        let mount = format!("HKLM\\DISKLESS_MANAGER_OFFLINE_{}", std::process::id());

        self.load_hive(&mount, system_hive)?;
        let inspected = self.inspect_loaded(&mount, dirty, services);
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
        let inventory = self.inspect_with_policy(system_hive, &[], config.allow_dirty_hive)?;
        build_plan(&inventory, config)
    }

    pub fn arm(
        &self,
        system_hive: &Path,
        config: &WindowsBootArmConfig,
        apply: bool,
    ) -> Result<WindowsBootArmResult> {
        self.arm_with_services(system_hive, config, apply, &[])
    }

    pub fn arm_with_services(
        &self,
        system_hive: &Path,
        config: &WindowsBootArmConfig,
        apply: bool,
        selected_services: &[String],
    ) -> Result<WindowsBootArmResult> {
        self.arm_with_profiles(system_hive, config, apply, selected_services, &[])
    }

    pub fn arm_with_profiles(
        &self,
        system_hive: &Path,
        config: &WindowsBootArmConfig,
        apply: bool,
        selected_services: &[String],
        profiles: &[super::windows_inf::InfBootService],
    ) -> Result<WindowsBootArmResult> {
        for service in selected_services {
            validate_service_name(service)?;
        }
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
            let mut inventory = self.inspect_loaded(&mount, dirty, selected_services)?;
            let mut reflected = Vec::new();
            for profile in profiles {
                validate_service_name(&profile.name)?;
                if inventory
                    .existing_services
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case(&profile.name))
                {
                    continue;
                }
                if !selected_services
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case(&profile.name))
                {
                    bail!("service reflection must match a selected NIC");
                }
                if !profile.binary.starts_with("\\SystemRoot\\System32\\")
                    || profile.binary.contains("..")
                    || profile.binary.contains('%')
                {
                    bail!(
                        "service binary must be verified inside the offline Windows installation"
                    );
                }
                let key = format!(
                    "{}\\Services\\{}",
                    inventory.active_control_set, profile.name
                );
                let reason = "reflect missing kernel service from the DISM-staged driver INF";
                push_dword(&mut reflected, key.clone(), "Type", 1, reason);
                push_dword(
                    &mut reflected,
                    key.clone(),
                    "ErrorControl",
                    profile.error_control,
                    reason,
                );
                reflected.push(RegistryArmChange {
                    key: key.clone(),
                    name: "ImagePath".into(),
                    value: RegistryArmValue::String(profile.binary.clone()),
                    reason: reason.into(),
                });
                for (name, value) in [
                    ("DisplayName", &profile.display_name),
                    ("Description", &profile.description),
                ] {
                    if let Some(value) = value {
                        reflected.push(RegistryArmChange {
                            key: key.clone(),
                            name: name.into(),
                            value: RegistryArmValue::String(value.clone()),
                            reason: reason.into(),
                        });
                    }
                }
                if let Some(group) = &profile.group {
                    reflected.push(RegistryArmChange {
                        key: key.clone(),
                        name: "Group".into(),
                        value: RegistryArmValue::String(group.clone()),
                        reason: reason.into(),
                    });
                }
                if !profile.dependencies.is_empty() {
                    reflected.push(RegistryArmChange {
                        key: key.clone(),
                        name: "DependOnService".into(),
                        value: RegistryArmValue::MultiString(profile.dependencies.clone()),
                        reason: reason.into(),
                    });
                }
                for (name, value) in &profile.event_log {
                    let value = match value {
                        super::windows_inf::InfServiceValue::ExpandString(v) => {
                            RegistryArmValue::ExpandString(v.clone())
                        }
                        super::windows_inf::InfServiceValue::Dword(v) => {
                            RegistryArmValue::Dword(*v)
                        }
                        _ => bail!("unsupported event-log value type"),
                    };
                    reflected.push(RegistryArmChange {
                        key: format!(
                            "{}\\Services\\EventLog\\System\\{}",
                            inventory.active_control_set, profile.name
                        ),
                        name: name.clone(),
                        value,
                        reason: reason.into(),
                    });
                }
                for (subkey, name, value) in &profile.parameters {
                    use super::windows_inf::InfServiceValue;
                    let value = match value {
                        InfServiceValue::String(v) => RegistryArmValue::String(v.clone()),
                        InfServiceValue::ExpandString(_) => {
                            bail!("expandable strings are only supported for event logging")
                        }
                        InfServiceValue::Dword(v) => RegistryArmValue::Dword(*v),
                        InfServiceValue::MultiString(v) => RegistryArmValue::MultiString(v.clone()),
                    };
                    reflected.push(RegistryArmChange {
                        key: if subkey.is_empty() {
                            key.clone()
                        } else {
                            format!("{key}\\{subkey}")
                        },
                        name: name.clone(),
                        value,
                        reason: reason.into(),
                    });
                }
                inventory.existing_services.push(profile.name.clone());
                inventory.driver_services.push(profile.name.clone());
                inventory
                    .driver_boot_flags
                    .insert(profile.name.to_ascii_lowercase(), profile.boot_flags);
            }
            let mut plan = build_plan(&inventory, config)?;
            reflected.append(&mut plan.changes);
            plan.changes = reflected;
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

    fn inspect_loaded(
        &self,
        mount: &str,
        dirty: bool,
        selected_services: &[String],
    ) -> Result<WindowsBootInventory> {
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
        candidates.extend(selected_services.iter().cloned());
        for service in ["iScsiPrt", "Tcpip", "ndis", "MSiSCSI", "disk"] {
            candidates.insert(service.to_string());
        }

        let mut existing_services = BTreeSet::new();
        let mut driver_services = BTreeSet::new();
        let mut driver_boot_flags = BTreeMap::new();
        for service in candidates {
            let key = format!("{mount}\\{control_set}\\Services\\{service}");
            if self.query(&["query", &key])?.status.success() {
                existing_services.insert(service.clone());
                if let Some(service_type) = self.query_dword(&key, "Type")? {
                    if service_type & (SERVICE_KERNEL_DRIVER | SERVICE_FILE_SYSTEM_DRIVER) != 0 {
                        driver_boot_flags.insert(
                            service.to_ascii_lowercase(),
                            self.query_dword(&key, "BootFlags")?.unwrap_or(0),
                        );
                        driver_services.insert(service);
                    }
                }
            }
        }

        Ok(WindowsBootInventory {
            active_control_set: control_set,
            native_nic_services: native_nic_services.into_iter().collect(),
            selected_nic_services: selected_services.to_vec(),
            existing_services: existing_services.into_iter().collect(),
            driver_services: driver_services.into_iter().collect(),
            driver_boot_flags,
            hive_dirty: dirty,
        })
    }

    fn apply_change(&self, mount: &str, change: &RegistryArmChange) -> Result<()> {
        let key = format!("{mount}\\{}", change.key);
        let output = match &change.value {
            RegistryArmValue::ExpandString(value) => self.query(&[
                "add",
                &key,
                "/v",
                &change.name,
                "/t",
                "REG_EXPAND_SZ",
                "/d",
                value,
                "/f",
            ])?,
            RegistryArmValue::String(value) => self.query(&[
                "add",
                &key,
                "/v",
                &change.name,
                "/t",
                "REG_SZ",
                "/d",
                value,
                "/f",
            ])?,
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
            if let Some(hex) = value
                .strip_prefix("0x")
                .or_else(|| value.strip_prefix("0X"))
            {
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
            bail!(
                "failed to load offline SYSTEM hive: {}",
                command_error(&output)
            );
        }
        Ok(())
    }

    fn unload_hive(&self, mount: &str) -> Result<()> {
        let output = self.query(&["unload", mount])?;
        if !output.status.success() {
            bail!(
                "failed to unload offline SYSTEM hive: {}",
                command_error(&output)
            );
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

    for service in &inventory.selected_nic_services {
        validate_service_name(service)?;
        if !existing.contains(&service.to_ascii_lowercase()) {
            bail!("selected NIC service {service} is not registered in the target SYSTEM hive after driver installation; the image is not ready for this NIC");
        }
        if !drivers.contains(&service.to_ascii_lowercase()) {
            bail!("selected NIC service {service} is not a kernel/filesystem driver; refusing boot-start arming");
        }
        if !inventory
            .native_nic_services
            .iter()
            .any(|native| native.eq_ignore_ascii_case(service))
        {
            warnings.push(format!("{service} has no native Windows adapter binding in the offline inventory. Package staging and boot-start arming do not prove target adapter initialization; validate SAN boot on the target hardware before deployment."));
        }
    }
    let nic_services = inventory
        .native_nic_services
        .iter()
        .chain(&inventory.selected_nic_services)
        .map(|service| (service.to_ascii_lowercase(), service))
        .collect::<BTreeMap<_, _>>();
    for service in nic_services.values() {
        if !existing.contains(&service.to_ascii_lowercase()) {
            warnings.push(format!(
                "Net-class binding references {service}, but Services\\{service} is absent; not fabricating a service key"
            ));
            continue;
        }
        if !drivers.contains(&service.to_ascii_lowercase()) {
            warnings.push(format!("{service} is bound as a NIC service but is not typed as a kernel/filesystem driver; boot-start arming skipped"));
            continue;
        }
        push_dword(
            &mut changes,
            format!("{}\\Services\\{}", inventory.active_control_set, service),
            "Start",
            0,
            "registered NIC driver required for network boot",
        );
        push_dword(
            &mut changes,
            format!("{}\\Services\\{}", inventory.active_control_set, service),
            "BootFlags",
            inventory
                .driver_boot_flags
                .get(&service.to_ascii_lowercase())
                .copied()
                .unwrap_or(0)
                | CM_SERVICE_NETWORK_BOOT_LOAD,
            "mark existing network driver for network boot",
        );
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
                inventory
                    .driver_boot_flags
                    .get("iscsiprt")
                    .copied()
                    .unwrap_or(0)
                    | CM_SERVICE_NETWORK_BOOT_LOAD,
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
            format!(
                "{}\\Services\\Tcpip\\Parameters",
                inventory.active_control_set
            ),
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
        selected_nic_services: inventory.selected_nic_services.clone(),
        hive_dirty: inventory.hive_dirty,
        changes,
        warnings,
    })
}

fn validate_service_name(service: &str) -> Result<()> {
    if service.is_empty()
        || service.len() > 256
        || !service
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || b"_.-".contains(&ch))
    {
        bail!("invalid NIC driver service name: {service:?}");
    }
    Ok(())
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
    let bytes =
        fs::read(path).with_context(|| format!("failed to read SYSTEM hive {}", path.display()))?;
    if bytes.len() < 0x200 || &bytes[..4] != b"regf" {
        bail!("file is not a valid REGF hive: {}", path.display());
    }
    Ok(())
}

pub fn hive_is_dirty(path: &Path) -> Result<bool> {
    let bytes =
        fs::read(path).with_context(|| format!("failed to read SYSTEM hive {}", path.display()))?;
    if bytes.len() < 12 || &bytes[..4] != b"regf" {
        bail!("file is not a valid REGF hive: {}", path.display());
    }
    let primary = u32::from_le_bytes(
        bytes[4..8]
            .try_into()
            .expect("hive header length validated"),
    );
    let secondary = u32::from_le_bytes(
        bytes[8..12]
            .try_into()
            .expect("hive header length validated"),
    );
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
            selected_nic_services: Vec::new(),
            driver_boot_flags: BTreeMap::new(),
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
        assert!(!plan
            .changes
            .iter()
            .any(|change| { change.key.ends_with("Services\\MSiSCSI") && change.name == "Start" }));
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

    #[test]
    fn arms_selected_unbound_driver_and_preserves_other_boot_flags() {
        let mut state = inventory();
        state.selected_nic_services = vec!["NetKVM".into()];
        state.existing_services.push("NetKVM".into());
        state.driver_services.push("NetKVM".into());
        state.driver_boot_flags.insert("netkvm".into(), 4);
        let plan = build_plan(&state, &WindowsBootArmConfig::default()).unwrap();
        assert!(plan
            .changes
            .iter()
            .any(|change| change.key.ends_with("Services\\NetKVM")
                && change.name == "Start"
                && matches!(change.value, RegistryArmValue::Dword(0))));
        assert!(plan
            .changes
            .iter()
            .any(|change| change.key.ends_with("Services\\NetKVM")
                && change.name == "BootFlags"
                && matches!(change.value, RegistryArmValue::Dword(5))));
        assert!(plan
            .warnings
            .iter()
            .any(|warning| warning.contains("NetKVM")
                && warning.contains("no native Windows adapter binding")));
        state.native_nic_services.push("netkvm".into());
        let bound = build_plan(&state, &WindowsBootArmConfig::default()).unwrap();
        assert!(!bound
            .warnings
            .iter()
            .any(|warning| warning.contains("no native Windows adapter binding")));
    }

    #[test]
    fn selected_driver_must_be_registered_as_a_driver() {
        let mut state = inventory();
        state.selected_nic_services = vec!["missingnic".into()];
        assert!(build_plan(&state, &WindowsBootArmConfig::default())
            .unwrap_err()
            .to_string()
            .contains("not registered"));
        state.selected_nic_services = vec!["MSiSCSI".into()];
        assert!(build_plan(&state, &WindowsBootArmConfig::default())
            .unwrap_err()
            .to_string()
            .contains("not a kernel"));
    }

    #[cfg(windows)]
    #[test]
    fn native_registry_arms_an_unbound_service_in_a_disposable_hive() {
        let armer = WindowsBootArmer::new().unwrap();
        let root = tempfile::tempdir().unwrap();
        let hive = root.path().join("SYSTEM");
        let fixture = format!(
            "HKCU\\Software\\DisklessManagerBootArmTest{}",
            std::process::id()
        );
        let setup = (|| -> Result<()> {
            for (key, name, value) in [
                (format!("{fixture}\\Select"), "Current", "1"),
                (
                    format!("{fixture}\\ControlSet001\\Services\\TestNic"),
                    "Type",
                    "1",
                ),
                (
                    format!("{fixture}\\ControlSet001\\Services\\TestNic"),
                    "Start",
                    "3",
                ),
                (
                    format!("{fixture}\\ControlSet001\\Services\\TestNic"),
                    "BootFlags",
                    "4",
                ),
            ] {
                let output = armer.query(&[
                    "add",
                    &key,
                    "/v",
                    name,
                    "/t",
                    "REG_DWORD",
                    "/d",
                    value,
                    "/f",
                ])?;
                anyhow::ensure!(output.status.success(), "{}", command_error(&output));
            }
            let output = armer.query(&["save", &fixture, hive.to_str().unwrap(), "/y"])?;
            anyhow::ensure!(output.status.success(), "{}", command_error(&output));
            Ok(())
        })();
        let cleanup = armer.query(&["delete", &fixture, "/f"]).unwrap();
        setup.unwrap();
        assert!(cleanup.status.success());

        let result = armer
            .arm_with_services(
                &hive,
                &WindowsBootArmConfig::default(),
                true,
                &["TestNic".into()],
            )
            .unwrap();
        assert!(result.plan.native_nic_services.is_empty());
        assert_eq!(result.plan.selected_nic_services, vec!["TestNic"]);
        let mount = format!("HKLM\\DISKLESS_VERIFY_{}", std::process::id());
        armer.load_hive(&mount, &hive).unwrap();
        let key = format!("{mount}\\ControlSet001\\Services\\TestNic");
        let start = armer.query_dword(&key, "Start");
        let flags = armer.query_dword(&key, "BootFlags");
        armer.unload_hive(&mount).unwrap();
        assert_eq!(start.unwrap(), Some(0));
        assert_eq!(flags.unwrap(), Some(5));
        let profile = super::super::windows_inf::InfBootService {
            name: "NewNic".into(),
            binary: r"\SystemRoot\System32\DriverStore\FileRepository\nic\newnic.sys".into(),
            group: Some("NDIS".into()),
            display_name: Some("Test adapter".into()),
            description: None,
            event_log: vec![
                (
                    "EventMessageFile".into(),
                    super::super::windows_inf::InfServiceValue::ExpandString(
                        r"%SystemRoot%\System32\netevent.dll".into(),
                    ),
                ),
                (
                    "TypesSupported".into(),
                    super::super::windows_inf::InfServiceValue::Dword(7),
                ),
            ],
            error_control: 1,
            boot_flags: 4,
            dependencies: Vec::new(),
            parameters: vec![(
                "Parameters".into(),
                "Option".into(),
                super::super::windows_inf::InfServiceValue::Dword(3),
            )],
        };
        let result = armer
            .arm_with_profiles(
                &hive,
                &WindowsBootArmConfig::default(),
                true,
                &["NewNic".into()],
                &[profile],
            )
            .unwrap();
        assert!(result.plan.changes.iter().any(|c| c.name == "ImagePath"));
        armer.load_hive(&mount, &hive).unwrap();
        let key = format!("{mount}\\ControlSet001\\Services\\NewNic");
        let start = armer.query_dword(&key, "Start");
        let flags = armer.query_dword(&key, "BootFlags");
        let option = armer.query_dword(&format!("{key}\\Parameters"), "Option");
        let event_key = format!("{mount}\\ControlSet001\\Services\\EventLog\\System\\NewNic");
        let event_types = armer.query_dword(&event_key, "TypesSupported");
        let event_message = armer
            .query(&["query", &event_key, "/v", "EventMessageFile"])
            .unwrap();
        armer.unload_hive(&mount).unwrap();
        assert_eq!(start.unwrap(), Some(0));
        assert_eq!(flags.unwrap(), Some(5));
        assert_eq!(option.unwrap(), Some(3));
        assert_eq!(event_types.unwrap(), Some(7));
        assert!(String::from_utf8_lossy(&event_message.stdout).contains("REG_EXPAND_SZ"));
        assert!(String::from_utf8_lossy(&event_message.stdout)
            .contains(r"%SystemRoot%\System32\netevent.dll"));
    }
}
