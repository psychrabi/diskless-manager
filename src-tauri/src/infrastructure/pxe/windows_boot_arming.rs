//! Conservative offline Windows SYSTEM hive arming.
//!
//! This module separates driver installation from boot arming. Driver packages are
//! installed by DISM; this armer only changes registry values for services that already
//! exist in the target Windows installation or have a validated, DISM-staged INF profile.
//!
//! The mutation path uses Windows' own registry loader (reg.exe load/unload) instead
//! of rewriting REGF cells directly. Missing services require validated INF profiles; it never copies
//! driver binaries and never deletes registry transaction logs. CriticalDeviceDatabase
//! entries are written only when the caller supplies an explicit
//! `critical_device_boot_nic` entry for the machine's exact Boot NIC IDs.

// This file is included by the helper crate, whose feature set differs from the server crate.
#![allow(unexpected_cfgs)]

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const NET_CLASS_GUID: &str = "{4d36e972-e325-11ce-bfc1-08002be10318}";
/// Device setup class GUID for network adapters, required by
/// CriticalDeviceDatabase entries that start a boot NIC before PnP.
#[cfg_attr(feature = "windows-servicer", allow(dead_code))]
pub const NETWORK_ADAPTER_CLASS_GUID: &str = NET_CLASS_GUID;
const SERVICE_KERNEL_DRIVER: u32 = 0x0000_0001;
const SERVICE_FILE_SYSTEM_DRIVER: u32 = 0x0000_0002;
const CM_SERVICE_NETWORK_BOOT_LOAD: u32 = 0x0000_0001;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootNicCriticalDevice {
    pub service: String,
    #[serde(default)]
    pub class_guid: Option<String>,
    pub hardware_ids: Vec<String>,
}

/// Resolve one observed PCI adapter to the service supplied by its selected
/// complete driver package. Subsystem IDs are preferred; vendor/device IDs are
/// the fallback when firmware inventory did not expose a subsystem.
#[cfg(not(feature = "windows-servicer"))]
pub fn critical_device_for_boot_nic(
    catalog: &[super::NetworkDriverPackage],
    selected_ids: &[String],
    boot_nic_ids: &[String],
) -> Option<BootNicCriticalDevice> {
    let pci_ids: Vec<String> = boot_nic_ids
        .iter()
        .map(|id| id.trim().to_string())
        .map(|id| {
            id.rsplit_once('\\')
                .filter(|(hardware_id, _)| {
                    hardware_id.to_ascii_uppercase().starts_with("PCI\\VEN_")
                        && hardware_id.to_ascii_uppercase().contains("&DEV_")
                })
                .map(|(hardware_id, _)| hardware_id.to_string())
                .unwrap_or(id)
        })
        .filter(|id| {
            let id = id.to_ascii_uppercase();
            id.starts_with("PCI\\") && id.contains("VEN_") && id.contains("DEV_")
        })
        .collect();
    let subsystem_ids: Vec<String> = pci_ids
        .iter()
        .filter(|id| id.to_ascii_uppercase().contains("SUBSYS_"))
        .cloned()
        .collect();
    let hardware_ids = if subsystem_ids.is_empty() {
        pci_ids
    } else {
        subsystem_ids
    };
    if hardware_ids.is_empty() {
        return None;
    }
    let selected = selected_ids
        .iter()
        .filter_map(|id| catalog.iter().find(|package| &package.id == id));
    let package = selected
        .clone()
        .find(|package| {
            package.hardware_ids.iter().any(|candidate| {
                hardware_ids
                    .iter()
                    .any(|id| super::pnp_id_matches(candidate, id))
            })
        })
        .or_else(|| {
            selected.clone().find(|package| {
                package.hardware_ids.iter().any(|candidate| {
                    hardware_ids
                        .iter()
                        .any(|id| boot_package_id_matches(candidate, id))
                })
            })
        })
        .or_else(|| {
            selected.into_iter().find(|package| {
                package
                    .hardware_ids
                    .iter()
                    .chain(package.compatible_ids.iter())
                    .any(|candidate| {
                        hardware_ids
                            .iter()
                            .any(|id| boot_package_id_matches(candidate, id))
                    })
            })
        })?;
    let mut associated_services = package
        .associated_service_names
        .iter()
        .map(|service| service.trim())
        .filter(|service| !service.is_empty())
        .collect::<Vec<_>>();
    associated_services.sort_by_key(|service| service.to_ascii_lowercase());
    associated_services.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    let mut services = package
        .service_names
        .iter()
        .map(|service| service.trim())
        .filter(|service| !service.is_empty())
        .collect::<Vec<_>>();
    services.sort_by_key(|service| service.to_ascii_lowercase());
    services.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    let service = match associated_services.as_slice() {
        [service] => (*service).to_string(),
        [] if services.len() == 1 => services[0].to_string(),
        [] if services.is_empty() && package.inf_files.len() == 1 => package
            .service_name
            .as_deref()
            .map(str::trim)
            .filter(|service| !service.is_empty())?
            .to_string(),
        _ => return None,
    };
    Some(BootNicCriticalDevice {
        service,
        class_guid: Some(NETWORK_ADAPTER_CLASS_GUID.to_string()),
        hardware_ids,
    })
}

#[cfg(not(feature = "windows-servicer"))]
fn boot_package_id_matches(package_id: &str, hardware_id: &str) -> bool {
    if super::pnp_id_matches(package_id, hardware_id) {
        return true;
    }
    let package_id = package_id.trim().to_ascii_lowercase().replace('/', "\\");
    let hardware_id = hardware_id.trim().to_ascii_lowercase().replace('/', "\\");
    hardware_id
        .strip_prefix(&package_id)
        .is_some_and(|suffix| suffix.starts_with('&'))
}

/// Build the same strict boot contract for a single WinPE-adapted client or a
/// shared image covering several inventoried clients.
#[cfg(not(feature = "windows-servicer"))]
pub fn boot_arm_config_for_nics(
    catalog: &[super::NetworkDriverPackage],
    selected_ids: &[String],
    boot_nic_groups: &[Vec<String>],
) -> Result<WindowsBootArmConfig> {
    if boot_nic_groups.is_empty() || boot_nic_groups.iter().any(Vec::is_empty) {
        bail!("at least one non-empty Boot NIC hardware-ID group is required");
    }
    let mut config = WindowsBootArmConfig {
        require_boot_nic_binding: true,
        disable_task_offload: true,
        stabilize_nic_driver_defaults: true,
        ..Default::default()
    };
    let mut groups = BTreeSet::new();
    let mut devices = BTreeSet::new();
    let mut services_by_id = BTreeMap::<String, String>::new();
    for group in boot_nic_groups {
        let ids = super::normalize_hardware_ids(group)
            .map_err(|error| anyhow::anyhow!("invalid Boot NIC hardware ID: {error}"))?;
        if ids.is_empty() {
            bail!("Boot NIC hardware-ID group is empty");
        }
        let critical = critical_device_for_boot_nic(catalog, selected_ids, &ids);
        let expected = critical
            .as_ref()
            .map(|entry| entry.hardware_ids.clone())
            .unwrap_or(ids);
        let mut group_key = expected
            .iter()
            .map(|id| id.to_ascii_lowercase())
            .collect::<Vec<_>>();
        group_key.sort();
        if !groups.insert(group_key.join(",")) {
            continue;
        }
        config
            .expected_boot_nic_ids
            .extend(expected.iter().cloned());
        config.expected_boot_nic_groups.push(expected.clone());
        if let Some(critical) = critical {
            let mut mapping_ids = critical
                .hardware_ids
                .iter()
                .map(|id| id.to_ascii_lowercase())
                .collect::<Vec<_>>();
            mapping_ids.sort();
            let mapping_key = format!(
                "{}:{}",
                critical.service.to_ascii_lowercase(),
                mapping_ids.join(",")
            );
            if !devices.insert(mapping_key) {
                continue;
            }
            for id in &critical.hardware_ids {
                let id_key = id.to_ascii_lowercase();
                if let Some(service) =
                    services_by_id.insert(id_key.clone(), critical.service.clone())
                {
                    if !service.eq_ignore_ascii_case(&critical.service) {
                        bail!("conflicting boot NIC driver services for {id_key}");
                    }
                }
            }
            config.critical_device_boot_nics.push(critical);
        }
    }
    config
        .expected_boot_nic_ids
        .sort_by_key(|id| id.to_ascii_lowercase());
    config
        .expected_boot_nic_ids
        .dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    Ok(config)
}

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
    /// Disable the Windows Filtering Platform lightweight filter, which is not
    /// boot-start or paging-I/O compatible and breaks iSCSI boot when it is
    /// bound to the boot NIC (KB976042).
    #[serde(default = "default_true")]
    pub disable_wfp_lwf: bool,
    /// Expected boot NIC hardware IDs. When non-empty, the plan checks that
    /// the image can start this NIC without disk-based PnP (existing Enum
    /// device instance or CriticalDeviceDatabase entry) and warns otherwise.
    #[serde(default)]
    pub expected_boot_nic_ids: Vec<String>,
    /// Alternative IDs for each known Boot NIC. A shared image must have a
    /// boot binding for every client adapter, while any one ID can identify it.
    #[serde(default)]
    pub expected_boot_nic_groups: Vec<Vec<String>>,
    /// Turn the boot-NIC binding check above into a hard error instead of a
    /// warning.
    #[serde(default)]
    pub require_boot_nic_binding: bool,
    /// Opt-in: create CriticalDeviceDatabase entries for the boot NIC so the
    /// kernel can start it before the iSCSI boot volume is available.
    #[serde(default)]
    pub critical_device_boot_nic: Option<BootNicCriticalDevice>,
    /// Additional adapter bindings for images shared by clients with different NICs.
    #[serde(default)]
    pub critical_device_boot_nics: Vec<BootNicCriticalDevice>,
    /// Write conservative Realtek 8125 stability defaults into the selected
    /// NIC service's `Parameters` key. A CDDB-started NIC has no adapter
    /// instance, so the driver falls back to service-level parameters; the
    /// values mirror a known-good locally-bound instance (EEE, Green
    /// Ethernet, interrupt moderation, latest-generation offloads, and
    /// selective suspend disabled) that does not drop iSCSI boot sessions.
    #[serde(default)]
    pub stabilize_nic_driver_defaults: bool,
}

impl Default for WindowsBootArmConfig {
    fn default() -> Self {
        Self {
            allow_dirty_hive: false,
            increase_disk_timeout: true,
            disable_task_offload: false,
            disable_crash_dump: false,
            clear_paging_files: false,
            disable_wfp_lwf: true,
            expected_boot_nic_ids: Vec::new(),
            expected_boot_nic_groups: Vec::new(),
            require_boot_nic_binding: false,
            critical_device_boot_nic: None,
            critical_device_boot_nics: Vec::new(),
            stabilize_nic_driver_defaults: false,
        }
    }
}

/// Canonical summary of the arming decisions that change registry outcomes.
///
/// Driver revision identity must cover the arm policy: a revision serviced
/// without DisableTaskOffload is not the same boot contract as one serviced
/// with it, so the two must never be reused for each other. Only fields
/// that alter the plan are summarized; NIC and package identity are already
/// part of the revision hash separately.
#[cfg_attr(feature = "windows-servicer", allow(dead_code))]
pub fn boot_arm_policy_digest(config: &WindowsBootArmConfig) -> String {
    let flag = |value: bool| if value { "1" } else { "0" };
    let mut cddb_entries = config
        .critical_device_boot_nic
        .iter()
        .chain(&config.critical_device_boot_nics)
        .map(|entry| {
            let mut ids = entry
                .hardware_ids
                .iter()
                .map(|id| id.to_ascii_lowercase())
                .collect::<Vec<_>>();
            ids.sort();
            format!(
                "{}:{}:{}",
                entry.service.to_ascii_lowercase(),
                entry.class_guid.as_deref().unwrap_or_default(),
                ids.join(",")
            )
        })
        .collect::<Vec<_>>();
    cddb_entries.sort();
    let mut nic_groups = config
        .expected_boot_nic_groups
        .iter()
        .map(|group| {
            let mut ids = group
                .iter()
                .map(|id| id.to_ascii_lowercase())
                .collect::<Vec<_>>();
            ids.sort();
            ids.join(",")
        })
        .collect::<Vec<_>>();
    nic_groups.sort();
    format!(
        "disk_timeout={};task_offload={};crash_dump={};paging_files={};wfp_lwf={};cddb={};nic_groups={};nic_stability={}",
        flag(config.increase_disk_timeout),
        flag(config.disable_task_offload),
        flag(config.disable_crash_dump),
        flag(config.clear_paging_files),
        flag(config.disable_wfp_lwf),
        cddb_entries.join("|"),
        nic_groups.join("|"),
        flag(config.stabilize_nic_driver_defaults),
    )
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
    #[serde(default)]
    pub msiscsi_image_path: Option<String>,
    #[serde(default)]
    pub wfp_lwf_start: Option<u32>,
    #[serde(default)]
    pub enum_pci_devices: Vec<String>,
    #[serde(default)]
    pub critical_device_database: Vec<ExistingCriticalDevice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExistingCriticalDevice {
    pub key: String,
    #[serde(default)]
    pub service: Option<String>,
    #[serde(default)]
    pub class_guid: Option<String>,
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

    /// Inspect the running OS without loading or modifying its SYSTEM hive.
    /// Only the Windows client helper uses this entry point.
    pub fn inspect_running(&self, services: &[String]) -> Result<WindowsBootInventory> {
        anyhow::ensure!(cfg!(windows), "local boot preparation requires Windows");
        for service in services {
            validate_service_name(service)?;
        }
        self.inspect_loaded("HKLM\\SYSTEM", false, services)
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

        let msiscsi_image_path = self
            .query(&[
                "query",
                &format!("{mount}\\{control_set}\\Services\\MSiSCSI"),
                "/v",
                "ImagePath",
            ])
            .ok()
            .and_then(|output| {
                if !output.status.success() {
                    return None;
                }
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .find_map(|line| {
                        parse_reg_value(line, "REG_EXPAND_SZ")
                            .or_else(|| parse_reg_value(line, "REG_SZ"))
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(str::to_string)
                    })
            });
        let wfp_lwf_start = self
            .query_dword(
                &format!("{mount}\\{control_set}\\Services\\WFPLWFS"),
                "Start",
            )
            .ok()
            .flatten();
        let enum_pci_key = format!("{mount}\\{control_set}\\Enum\\PCI");
        let enum_pci_devices = self
            .query(&["query", &enum_pci_key])
            .ok()
            .and_then(|output| {
                if !output.status.success() {
                    return None;
                }
                let mut names = Vec::new();
                for line in String::from_utf8_lossy(&output.stdout).lines() {
                    if let Some(device) = enum_pci_device_key(line) {
                        names.push(device);
                    }
                }
                Some(names)
            })
            .unwrap_or_default();
        let cddb_key = format!("{mount}\\{control_set}\\Control\\CriticalDeviceDatabase");
        let critical_device_keys = self
            .query(&["query", &cddb_key])
            .ok()
            .and_then(|output| {
                if !output.status.success() {
                    return None;
                }
                let mut names = BTreeSet::new();
                for line in String::from_utf8_lossy(&output.stdout).lines() {
                    let line = line.trim();
                    if let Some(pos) = line.rfind('\\') {
                        let name = line[pos + 1..].trim();
                        if !name.is_empty()
                            && !name.starts_with("HKEY_LOCAL_MACHINE")
                            && !name.contains('\\')
                        {
                            names.insert(name.to_ascii_lowercase());
                        }
                    }
                }
                Some(names)
            })
            .unwrap_or_default();
        let mut critical_device_database = Vec::new();
        for key in critical_device_keys {
            let registry_key = format!("{cddb_key}\\{key}");
            let Ok(output) = self.query(&["query", &registry_key]) else {
                continue;
            };
            if !output.status.success() {
                continue;
            }
            let mut mapping = ExistingCriticalDevice {
                key,
                service: None,
                class_guid: None,
            };
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                if let Some(value) = parse_reg_sz_value(line, "Service") {
                    mapping.service = Some(value);
                } else if let Some(value) = parse_reg_sz_value(line, "ClassGUID") {
                    mapping.class_guid = Some(value);
                }
            }
            critical_device_database.push(mapping);
        }

        Ok(WindowsBootInventory {
            active_control_set: control_set,
            native_nic_services: native_nic_services.into_iter().collect(),
            selected_nic_services: selected_services.to_vec(),
            existing_services: existing_services.into_iter().collect(),
            driver_services: driver_services.into_iter().collect(),
            driver_boot_flags,
            hive_dirty: dirty,
            msiscsi_image_path,
            wfp_lwf_start,
            enum_pci_devices,
            critical_device_database,
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
    // Boot-NIC binding gate: the image must be able to start the NIC before
    // disk-based PnP runs, otherwise iSCSI boot hangs at the firmware handoff.
    // A CriticalDeviceDatabase entry configured for this run also satisfies the
    // gate because the same plan creates it before the image boots.
    let mut planned_cddb = BTreeMap::<String, String>::new();
    for entry in config
        .critical_device_boot_nic
        .iter()
        .chain(&config.critical_device_boot_nics)
    {
        validate_service_name(&entry.service)?;
        if entry
            .class_guid
            .as_deref()
            .is_some_and(|guid| !guid.eq_ignore_ascii_case(NET_CLASS_GUID))
        {
            bail!("CriticalDeviceDatabase NIC class GUID must be {NET_CLASS_GUID}");
        }
        let ids = normalize_boot_nic_ids(&entry.hardware_ids)
            .map_err(|error| anyhow::anyhow!("invalid CriticalDeviceDatabase NIC ID: {error}"))?;
        if ids.is_empty() {
            bail!("CriticalDeviceDatabase NIC entry has no hardware IDs");
        }
        if !inventory
            .selected_nic_services
            .iter()
            .chain(&inventory.native_nic_services)
            .any(|service| service.eq_ignore_ascii_case(&entry.service))
        {
            bail!(
                "CriticalDeviceDatabase service {} is not a selected NIC driver",
                entry.service
            );
        }
        if !existing.contains(&entry.service.to_ascii_lowercase())
            || !drivers.contains(&entry.service.to_ascii_lowercase())
        {
            bail!(
                "CriticalDeviceDatabase service {} is not a registered kernel NIC driver",
                entry.service
            );
        }
        for id in ids {
            let key = cddb_hardware_id_key(&id);
            if let Some(existing) = planned_cddb.insert(key.clone(), entry.service.clone()) {
                if !existing.eq_ignore_ascii_case(&entry.service) {
                    bail!("conflicting CriticalDeviceDatabase services for {key}");
                }
            }
        }
    }
    if !config.expected_boot_nic_ids.is_empty() || !config.expected_boot_nic_groups.is_empty() {
        let legacy_group = std::slice::from_ref(&config.expected_boot_nic_ids);
        let groups = if config.expected_boot_nic_groups.is_empty() {
            legacy_group
        } else {
            &config.expected_boot_nic_groups
        };
        for group in groups {
            let has_mapping = group.iter().any(|id| {
                let enum_key = id
                    .trim()
                    .trim_start_matches("PCI\\")
                    .trim_start_matches("pci\\");
                let cddb_key = cddb_hardware_id_key(id);
                inventory
                    .enum_pci_devices
                    .iter()
                    .any(|value| value.eq_ignore_ascii_case(enum_key))
                    || inventory.critical_device_database.iter().any(|mapping| {
                        mapping.key.eq_ignore_ascii_case(&cddb_key)
                            && mapping
                                .class_guid
                                .as_deref()
                                .is_some_and(|guid| guid.eq_ignore_ascii_case(NET_CLASS_GUID))
                            && mapping.service.as_deref().is_some_and(|service| {
                                let service = service.to_ascii_lowercase();
                                existing.contains(&service)
                                    && drivers.contains(&service)
                                    && nic_services_contains(inventory, &service)
                            })
                    })
                    || planned_cddb.contains_key(&cddb_key)
            });
            if has_mapping {
                continue;
            }
            let message = format!(
                "no Enum/CriticalDeviceDatabase mapping exists for the expected boot NIC IDs ({:?}); the image may not start its boot NIC before the iSCSI root",
                group
            );
            if config.require_boot_nic_binding {
                bail!("{message}");
            }
            warnings.push(message);
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
        let is_realtek_8125_boot_nic = config
            .critical_device_boot_nic
            .iter()
            .chain(&config.critical_device_boot_nics)
            .any(|entry| {
                entry.service.eq_ignore_ascii_case(service)
                    && entry.hardware_ids.iter().any(|id| {
                        let id = id.to_ascii_uppercase();
                        id.starts_with("PCI\\VEN_10EC&DEV_8125")
                    })
            });
        if config.stabilize_nic_driver_defaults && is_realtek_8125_boot_nic {
            // A CDDB-started NIC has no adapter instance, so tunable
            // Realtek parameters fall back to the service Parameters key.
            // Mirror the values from a known-good locally-bound instance:
            // keep power saving, Ethernet, interrupt moderation, selective
            // suspend, and the latest-generation offloads off so the boot
            // NIC does not drop iSCSI sessions mid-boot. The link is pinned
            // to 1 Gbps (`*SpeedDuplex=6`) because the 8125's receive path
            // demonstrably wedges under sustained 2.5 Gbps read bursts in
            // early boot (freezes TCP acknowledgements while TX continues).
            for (name, value) in [
                ("*EEE", "0"),
                ("AdvancedEEE", "0"),
                ("EnableGreenEthernet", "0"),
                ("PowerSavingMode", "0"),
                ("*InterruptModeration", "0"),
                ("*SelectiveSuspend", "0"),
                ("*LsoV2IPv4", "0"),
                ("*LsoV2IPv6", "0"),
                ("*TCPChecksumOffloadIPv4", "0"),
                ("*TCPChecksumOffloadIPv6", "0"),
                ("*SpeedDuplex", "6"),
            ] {
                push_string(
                    &mut changes,
                    format!(
                        "{}\\Services\\{}\\Parameters",
                        inventory.active_control_set, service,
                    ),
                    name,
                    value.to_string(),
                    "conservative Realtek 8125 stability default for CDDB-started NICs without an adapter instance",
                );
            }
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
        // MSiSCSI's ImagePath is sometimes persisted in the malformed
        // `\SystemRoot\C:\WINDOWS\...` form produced by cloning an image
        // captured on a different machine. That value prevents the service from
        // starting and breaks the client reporter and session management.
        if let Some(path) = &inventory.msiscsi_image_path {
            if let Some(fixed) = path.strip_prefix("\\SystemRoot\\").filter(|rest| {
                rest.len() >= 3
                    && rest.as_bytes()[1] == b':'
                    && rest.as_bytes()[2] == b'\\'
                    && rest.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            }) {
                push_expand_string(
                    &mut changes,
                    format!("{}\\Services\\MSiSCSI", inventory.active_control_set),
                    "ImagePath",
                    fixed.to_string(),
                    "normalize malformed MSiSCSI ImagePath inherited from a cloned image",
                );
            }
        }
        warnings.push(
            "MSiSCSI service detected but left unchanged otherwise; boot-driver flags are only applied to driver services"
                .to_string(),
        );
    }
    if config.disable_wfp_lwf {
        if let Some(start) = inventory.wfp_lwf_start {
            if start != 4 {
                push_dword(
                    &mut changes,
                    format!("{}\\Services\\WFPLWFS", inventory.active_control_set),
                    "Start",
                    4,
                    "disable the WFP lightweight filter, which is not boot-start or paging-I/O compatible (KB976042)",
                );
                push_dword(
                    &mut changes,
                    format!("{}\\Services\\WFPLWFS", inventory.active_control_set),
                    "StartOverride",
                    4,
                    "disable the WFP lightweight filter, which is not boot-start or paging-I/O compatible (KB976042)",
                );
            }
        }
    }

    for cd in config
        .critical_device_boot_nic
        .iter()
        .chain(&config.critical_device_boot_nics)
    {
        for id in &cd.hardware_ids {
            let key = format!(
                "{}\\Control\\CriticalDeviceDatabase\\{}",
                inventory.active_control_set,
                cddb_hardware_id_key(id)
            );
            push_string(
                &mut changes,
                key.clone(),
                "Service",
                cd.service.clone(),
                "opt-in CriticalDeviceDatabase entry so the boot NIC can start before the iSCSI root",
            );
            if let Some(class_guid) = &cd.class_guid {
                push_string(
                    &mut changes,
                    key,
                    "ClassGUID",
                    class_guid.clone(),
                    "opt-in CriticalDeviceDatabase entry so the boot NIC can start before the iSCSI root",
                );
            }
        }
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

fn cddb_hardware_id_key(id: &str) -> String {
    id.trim().replace('\\', "#").to_ascii_lowercase()
}

fn normalize_boot_nic_ids(ids: &[String]) -> Result<Vec<String>, String> {
    let mut normalized = Vec::new();
    for id in ids {
        let clean = id.trim().trim_matches('"').replace('/', "\\");
        if clean.is_empty()
            || clean.len() > 128
            || !clean.chars().all(|c| {
                c.is_ascii_alphanumeric()
                    || matches!(
                        c,
                        '\\' | '&' | '_' | '-' | '.' | '+' | '(' | ')' | '{' | '}' | ','
                    )
            })
        {
            return Err(format!("invalid hardware ID: '{id}'"));
        }
        if !normalized
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(&clean))
        {
            normalized.push(clean);
        }
    }
    Ok(normalized)
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

fn enum_pci_device_key(line: &str) -> Option<String> {
    let marker = "\\enum\\pci\\";
    let lower = line.to_ascii_lowercase();
    let start = lower.find(marker)? + marker.len();
    let device = line[start..].split('\\').next()?.trim();
    (!device.is_empty() && !device.starts_with("Value")).then(|| device.to_string())
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

fn push_string(
    changes: &mut Vec<RegistryArmChange>,
    key: String,
    name: &str,
    value: String,
    reason: &str,
) {
    changes.push(RegistryArmChange {
        key,
        name: name.to_string(),
        value: RegistryArmValue::String(value),
        reason: reason.to_string(),
    });
}

fn push_expand_string(
    changes: &mut Vec<RegistryArmChange>,
    key: String,
    name: &str,
    value: String,
    reason: &str,
) {
    changes.push(RegistryArmChange {
        key,
        name: name.to_string(),
        value: RegistryArmValue::ExpandString(value),
        reason: reason.to_string(),
    });
}

fn parse_reg_value<'a>(line: &'a str, reg_type: &str) -> Option<&'a str> {
    let marker = line.find(reg_type)?;
    Some(line[marker + reg_type.len()..].trim())
}

fn parse_reg_sz_value(line: &str, name: &str) -> Option<String> {
    let mut fields = line.split_whitespace();
    if !fields.next()?.eq_ignore_ascii_case(name) || !fields.next()?.eq_ignore_ascii_case("REG_SZ")
    {
        return None;
    }
    let value = fields.collect::<Vec<_>>().join(" ");
    (!value.is_empty()).then_some(value)
}

fn nic_services_contains(inventory: &WindowsBootInventory, service: &str) -> bool {
    inventory
        .native_nic_services
        .iter()
        .chain(&inventory.selected_nic_services)
        .any(|candidate| candidate.eq_ignore_ascii_case(service))
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
            msiscsi_image_path: None,
            wfp_lwf_start: None,
            enum_pci_devices: Vec::new(),
            critical_device_database: Vec::new(),
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
    fn nic_stability_defaults_write_service_parameters() {
        let base = build_plan(&inventory(), &WindowsBootArmConfig::default()).unwrap();
        assert!(!base
            .changes
            .iter()
            .any(|change| change.key.ends_with("Services\\e2fexpress\\Parameters")));

        let config = WindowsBootArmConfig {
            stabilize_nic_driver_defaults: true,
            critical_device_boot_nic: Some(BootNicCriticalDevice {
                service: "e2fexpress".into(),
                class_guid: Some(NET_CLASS_GUID.into()),
                hardware_ids: vec!["PCI\\VEN_10EC&DEV_8125".into()],
            }),
            ..Default::default()
        };
        let plan = build_plan(&inventory(), &config).unwrap();
        let key = "ControlSet001\\Services\\e2fexpress\\Parameters";
        for name in [
            "*EEE",
            "AdvancedEEE",
            "EnableGreenEthernet",
            "PowerSavingMode",
            "*InterruptModeration",
            "*SelectiveSuspend",
            "*LsoV2IPv4",
            "*LsoV2IPv6",
            "*TCPChecksumOffloadIPv4",
            "*TCPChecksumOffloadIPv6",
            "*SpeedDuplex",
        ] {
            assert!(
                plan.changes
                    .iter()
                    .any(|change| change.key == key && change.name == name),
                "stability value {name} must be planned"
            );
        }
        assert!(boot_arm_policy_digest(&config).contains("nic_stability=1"));

        let intel = WindowsBootArmConfig {
            stabilize_nic_driver_defaults: true,
            critical_device_boot_nic: Some(BootNicCriticalDevice {
                service: "e2fexpress".into(),
                class_guid: Some(NET_CLASS_GUID.into()),
                hardware_ids: vec!["PCI\\VEN_8086&DEV_15F3".into()],
            }),
            ..Default::default()
        };
        let plan = build_plan(&inventory(), &intel).unwrap();
        assert!(!plan
            .changes
            .iter()
            .any(|change| change.key.ends_with("Services\\e2fexpress\\Parameters")));
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
    fn normalizes_malformed_msiscsi_image_path() {
        let mut state = inventory();
        state.msiscsi_image_path =
            Some("\\SystemRoot\\C:\\Windows\\System32\\svchost.exe -k netsvcs -p".into());
        let plan = build_plan(&state, &WindowsBootArmConfig::default()).unwrap();
        assert!(plan.changes.iter().any(|change| {
            change.key.ends_with("Services\\MSiSCSI")
                && change.name == "ImagePath"
                && matches!(&change.value, RegistryArmValue::ExpandString(v) if v == "C:\\Windows\\System32\\svchost.exe -k netsvcs -p")
        }));
    }

    #[test]
    fn leaves_correct_msiscsi_image_path_unchanged() {
        let mut state = inventory();
        state.msiscsi_image_path = Some("C:\\Windows\\System32\\svchost.exe -k netsvcs -p".into());
        let plan = build_plan(&state, &WindowsBootArmConfig::default()).unwrap();
        assert!(!plan
            .changes
            .iter()
            .any(|change| change.key.ends_with("Services\\MSiSCSI") && change.name == "ImagePath"));
    }

    #[test]
    fn disables_wfp_lightweight_filter_when_enabled() {
        let mut state = inventory();
        state.wfp_lwf_start = Some(0);
        let plan = build_plan(&state, &WindowsBootArmConfig::default()).unwrap();
        assert!(plan
            .changes
            .iter()
            .any(|change| change.key.ends_with("Services\\WFPLWFS")
                && change.name == "Start"
                && matches!(change.value, RegistryArmValue::Dword(4))));
    }

    #[test]
    fn leaves_already_disabled_wfp_filter_alone() {
        let mut state = inventory();
        state.wfp_lwf_start = Some(4);
        let plan = build_plan(&state, &WindowsBootArmConfig::default()).unwrap();
        assert!(!plan
            .changes
            .iter()
            .any(|change| change.key.ends_with("Services\\WFPLWFS")));
    }

    #[test]
    fn warns_when_boot_nic_has_no_offline_mapping() {
        let mut state = inventory();
        state.enum_pci_devices.clear();
        state.critical_device_database.clear();
        let config = WindowsBootArmConfig {
            expected_boot_nic_ids: vec!["PCI\\VEN_10EC&DEV_8125&SUBSYS_B0021F4C&REV_05".into()],
            ..Default::default()
        };
        let plan = build_plan(&state, &config).unwrap();
        assert!(plan
            .warnings
            .iter()
            .any(|w| w.contains("no Enum/CriticalDeviceDatabase mapping")));
    }

    #[test]
    fn no_warning_when_boot_nic_is_mapped() {
        let mut state = inventory();
        state
            .enum_pci_devices
            .push("VEN_10EC&DEV_8125&SUBSYS_B0021F4C&REV_05".into());
        let config = WindowsBootArmConfig {
            expected_boot_nic_ids: vec!["PCI\\VEN_10EC&DEV_8125&SUBSYS_B0021F4C&REV_05".into()],
            ..Default::default()
        };
        let plan = build_plan(&state, &config).unwrap();
        assert!(!plan
            .warnings
            .iter()
            .any(|w| w.contains("no Enum/CriticalDeviceDatabase mapping")));
    }

    #[test]
    fn existing_cddb_binding_satisfies_boot_nic_gate() {
        let mut state = inventory();
        state.critical_device_database.push(ExistingCriticalDevice {
            key: "pci#ven_10ec&dev_8125&subsys_b0021f4c&rev_05".into(),
            service: Some("e2fexpress".into()),
            class_guid: Some(NET_CLASS_GUID.into()),
        });
        let config = WindowsBootArmConfig {
            expected_boot_nic_ids: vec!["PCI\\VEN_10EC&DEV_8125&SUBSYS_B0021F4C&REV_05".into()],
            require_boot_nic_binding: true,
            ..Default::default()
        };
        let plan = build_plan(&state, &config).unwrap();
        assert!(!plan
            .warnings
            .iter()
            .any(|warning| warning.contains("no Enum/CriticalDeviceDatabase mapping")));
    }

    #[test]
    fn stale_cddb_entry_without_valid_driver_and_network_class_does_not_satisfy_binding() {
        let mut state = inventory();
        state.enum_pci_devices.clear();
        let key = "pci#ven_10ec&dev_8125&subsys_b0021f4c&rev_05";
        let config = WindowsBootArmConfig {
            expected_boot_nic_ids: vec!["PCI\\VEN_10EC&DEV_8125&SUBSYS_B0021F4C&REV_05".into()],
            require_boot_nic_binding: true,
            ..Default::default()
        };
        for (service, class_guid) in [
            (None, Some(NET_CLASS_GUID)),
            (Some("missingnic"), Some(NET_CLASS_GUID)),
            (Some("e2fexpress"), Some("{invalid}")),
        ] {
            state.critical_device_database = vec![ExistingCriticalDevice {
                key: key.into(),
                service: service.map(str::to_owned),
                class_guid: class_guid.map(str::to_owned),
            }];
            assert!(build_plan(&state, &config)
                .unwrap_err()
                .to_string()
                .contains("no Enum/CriticalDeviceDatabase mapping"));
        }
    }

    #[test]
    fn reads_service_and_class_values_from_registry_output() {
        assert_eq!(
            parse_reg_sz_value("    Service    REG_SZ    e2fexpress", "Service").as_deref(),
            Some("e2fexpress")
        );
        assert_eq!(
            parse_reg_sz_value(
                "    ClassGUID    REG_SZ    {4d36e972-e325-11ce-bfc1-08002be10318}",
                "ClassGUID"
            )
            .as_deref(),
            Some(NET_CLASS_GUID)
        );
        assert!(parse_reg_sz_value("    Service    REG_DWORD    1", "Service").is_none());
    }

    #[test]
    fn enum_pci_inspection_keeps_the_device_id_without_the_instance_suffix() {
        let line = r"HKEY_LOCAL_MACHINE\DISKLESS_MANAGER_OFFLINE\ControlSet001\Enum\PCI\VEN_8086&DEV_15F3&SUBSYS_00018086\3&11583659&0&10";
        assert_eq!(
            enum_pci_device_key(line).as_deref(),
            Some("VEN_8086&DEV_15F3&SUBSYS_00018086")
        );
        assert!(enum_pci_device_key(
            r"HKEY_LOCAL_MACHINE\DISKLESS_MANAGER_OFFLINE\ControlSet001\Enum\PCI"
        )
        .is_none());
    }

    #[test]
    fn require_boot_nic_binding_turns_warning_into_error() {
        let mut state = inventory();
        state.enum_pci_devices.clear();
        state.critical_device_database.clear();
        let config = WindowsBootArmConfig {
            expected_boot_nic_ids: vec!["PCI\\VEN_10EC&DEV_8125&SUBSYS_B0021F4C&REV_05".into()],
            require_boot_nic_binding: true,
            ..Default::default()
        };
        assert!(build_plan(&state, &config).is_err());
    }

    #[test]
    fn planned_critical_device_database_satisfies_boot_nic_gate() {
        let mut state = inventory();
        state.selected_nic_services = vec!["rt640x64".into()];
        state.existing_services.push("rt640x64".into());
        state.driver_services.push("rt640x64".into());
        state.enum_pci_devices.clear();
        state.critical_device_database.clear();
        let config = WindowsBootArmConfig {
            expected_boot_nic_ids: vec!["PCI\\VEN_10EC&DEV_8125&SUBSYS_B0021F4C&REV_05".into()],
            require_boot_nic_binding: true,
            critical_device_boot_nic: Some(BootNicCriticalDevice {
                service: "rt640x64".into(),
                class_guid: Some(NET_CLASS_GUID.into()),
                hardware_ids: vec!["PCI\\VEN_10EC&DEV_8125&SUBSYS_B0021F4C&REV_05".into()],
            }),
            ..Default::default()
        };
        let plan = build_plan(&state, &config).unwrap();
        assert!(plan.changes.iter().any(|change| {
            change
                .key
                .contains("CriticalDeviceDatabase\\pci#ven_10ec&dev_8125&subsys_b0021f4c&rev_05")
        }));
    }

    #[test]
    fn shared_image_requires_and_arms_each_client_boot_nic() {
        let mut state = inventory();
        state.selected_nic_services = vec!["e2fexpress".into(), "NetKVM".into()];
        state.existing_services.push("NetKVM".into());
        state.driver_services.push("NetKVM".into());
        let first_id = "PCI\\VEN_10EC&DEV_8125&SUBSYS_B0021F4C&REV_05";
        let second_id = "PCI\\VEN_1AF4&DEV_1000";
        let config = WindowsBootArmConfig {
            expected_boot_nic_groups: vec![vec![first_id.into()], vec![second_id.into()]],
            require_boot_nic_binding: true,
            critical_device_boot_nics: vec![
                BootNicCriticalDevice {
                    service: "e2fexpress".into(),
                    class_guid: Some(NET_CLASS_GUID.into()),
                    hardware_ids: vec![first_id.into()],
                },
                BootNicCriticalDevice {
                    service: "NetKVM".into(),
                    class_guid: Some(NET_CLASS_GUID.into()),
                    hardware_ids: vec![second_id.into()],
                },
            ],
            ..Default::default()
        };

        let plan = build_plan(&state, &config).unwrap();
        for id in [first_id, second_id] {
            let key = id.replace('\\', "#").to_ascii_lowercase();
            assert!(plan.changes.iter().any(|change| {
                change
                    .key
                    .ends_with(&format!("CriticalDeviceDatabase\\{key}"))
                    && change.name == "Service"
            }));
        }

        let mut incomplete = config.clone();
        incomplete.critical_device_boot_nics.pop();
        assert!(build_plan(&state, &incomplete)
            .unwrap_err()
            .to_string()
            .contains("no Enum/CriticalDeviceDatabase mapping"));

        state.selected_nic_services = vec!["e2fexpress".into()];
        assert!(build_plan(&state, &config)
            .unwrap_err()
            .to_string()
            .contains("not a selected NIC driver"));
    }

    #[test]
    fn critical_device_database_entries_are_opt_in() {
        let mut state = inventory();
        state.selected_nic_services = vec!["rt640x64".into()];
        state.existing_services.push("rt640x64".into());
        state.driver_services.push("rt640x64".into());
        let config = WindowsBootArmConfig {
            critical_device_boot_nic: Some(BootNicCriticalDevice {
                service: "rt640x64".into(),
                class_guid: Some("{4d36e972-e325-11ce-bfc1-08002be10318}".into()),
                hardware_ids: vec!["PCI\\VEN_10EC&DEV_8125&SUBSYS_B0021F4C&REV_05".into()],
            }),
            ..Default::default()
        };
        let plan = build_plan(&state, &config).unwrap();
        assert!(plan.changes.iter().any(|change| {
            change
                .key
                .contains("CriticalDeviceDatabase\\pci#ven_10ec&dev_8125&subsys_b0021f4c&rev_05")
        }));
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
