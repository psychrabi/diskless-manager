//! Offline identity, network and drive-letter personalization for job-owned
//! Windows clones.
//!
//! A base image inherits whatever identity, static network configuration and
//! drive-letter reservations it was captured with. Booting many clients from
//! one image therefore produces duplicate hostnames, duplicate static
//! addresses, and drive letters reserved for disks the client does not have.
//!
//! Everything here is opt-in and runs on an offline SYSTEM hive owned by a
//! servicing job: the source image is never edited, the live installation is
//! refused, and a dry run returns the exact diff without writing. Computer-name
//! changes follow the documented registry contract (`Control\ComputerName` and
//! the TCP/IP hostname), not an ad-hoc key tree, and drive-letter cleanup never
//! touches `C:` or a volume identity.
//!
//! `ActiveComputerName` is deliberately not written: Windows derives it at boot
//! from `ComputerName`, and a stale value there has no effect on the next boot.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};

/// Requested personalization for one clone.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WindowsPersonalizationPlan {
    /// Desired computer name. `None` leaves the inherited name untouched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub computer_name: Option<String>,
    /// Convert interfaces that carry static IPv4 settings to DHCP. Interfaces
    /// that are already DHCP, or have no static IPv4 address, are untouched.
    #[serde(default)]
    pub convert_static_ipv4_to_dhcp: bool,
    /// Remove non-`C:` drive-letter reservations from `MountedDevices`.
    #[serde(default)]
    pub clear_stale_drive_letters: bool,
}

impl WindowsPersonalizationPlan {
    pub fn is_empty(&self) -> bool {
        self.computer_name.is_none()
            && !self.convert_static_ipv4_to_dhcp
            && !self.clear_stale_drive_letters
    }

    pub fn validate(&self) -> Result<()> {
        if let Some(name) = &self.computer_name {
            normalize_computer_name(name)?;
        }
        if self.is_empty() {
            bail!("personalization plan requests no change");
        }
        Ok(())
    }

    /// The name as it will be written, or `None` when the name is untouched.
    pub fn normalized_computer_name(&self) -> Result<Option<String>> {
        self.computer_name
            .as_deref()
            .map(normalize_computer_name)
            .transpose()
    }
}

/// Windows computer names are NetBIOS names: 1-15 characters of letters,
/// digits and hyphens, not all digits, and not a reserved device name.
pub fn normalize_computer_name(raw: &str) -> Result<String> {
    let name = raw.trim();
    if name.is_empty() {
        bail!("computer name is empty");
    }
    if name.len() > 15 {
        bail!("computer name must be at most 15 characters");
    }
    if !name
        .bytes()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == b'-')
    {
        bail!("computer name may only contain letters, digits and hyphens");
    }
    if name.starts_with('-') || name.ends_with('-') {
        bail!("computer name must not start or end with a hyphen");
    }
    if name.bytes().all(|ch| ch.is_ascii_digit()) {
        bail!("computer name must not be all digits");
    }
    let upper = name.to_ascii_uppercase();
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if RESERVED.contains(&upper.as_str()) {
        bail!("computer name '{upper}' is reserved by Windows");
    }
    Ok(upper)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComputerNameChange {
    pub current: Option<String>,
    pub desired: String,
}

/// What happened to one TCP/IP interface.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InterfaceAction {
    /// Static IPv4 settings were (or would be) replaced by DHCP.
    Convert,
    /// Already DHCP; nothing to do.
    AlreadyDhcp,
    /// No static IPv4 address; nothing to do.
    NoStaticIpv4,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InterfaceChange {
    pub id: String,
    pub action: InterfaceAction,
    /// Static IPv4 addresses found in the hive, for the operator to review.
    pub static_ipv4: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MountedDeviceChanges {
    /// `\DosDevices\X:` values that are (or would be) removed.
    pub removed: Vec<String>,
    /// `\DosDevices\X:` values kept because they are `C:`.
    pub retained: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WindowsPersonalizationDiff {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub computer_name: Option<ComputerNameChange>,
    #[serde(default)]
    pub interfaces: Vec<InterfaceChange>,
    #[serde(default)]
    pub mounted_devices: MountedDeviceChanges,
}

impl WindowsPersonalizationDiff {
    pub fn is_empty(&self) -> bool {
        self.computer_name.is_none()
            && !self
                .interfaces
                .iter()
                .any(|change| change.action == InterfaceAction::Convert)
            && self.mounted_devices.removed.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WindowsPersonalizationOutcome {
    /// False for a dry run; the diff still describes the planned changes.
    pub applied: bool,
    pub diff: WindowsPersonalizationDiff,
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// Apply the plan to the offline hive. A non-committing call is a dry run: it
/// inspects, reports the diff and writes nothing. The diff is produced from the
/// same hive load that the writes use, so a plan is never diffed against one
/// state and written to another.
pub fn apply(
    system_hive: &Path,
    plan: &WindowsPersonalizationPlan,
    allow_dirty_hive: bool,
    commit: bool,
) -> Result<WindowsPersonalizationOutcome> {
    plan.validate()?;
    refuse_live_installation(system_hive)?;
    let registry = Registry::new();
    with_loaded_hive(system_hive, allow_dirty_hive, |mount| {
        let diff = inspect_loaded(&registry, mount, plan)?;
        if !commit || diff.is_empty() {
            return Ok(WindowsPersonalizationOutcome {
                applied: false,
                diff,
                warnings: Vec::new(),
            });
        }
        let control_set = registry.current_control_set(mount)?;
        let mut warnings = Vec::new();
        if let Some(change) = &diff.computer_name {
            registry.write_computer_name(mount, control_set, &change.desired)?;
        }
        for interface in diff
            .interfaces
            .iter()
            .filter(|change| change.action == InterfaceAction::Convert)
        {
            registry.convert_interface_to_dhcp(mount, control_set, &interface.id)?;
        }
        for value in &diff.mounted_devices.removed {
            // One failed reservation must not abandon the rest of a verified
            // dry run, but it must be reported rather than discarded.
            if let Err(error) = registry.delete_value(
                &format!("{mount}\\ControlSet{control_set:03}\\MountedDevices"),
                value,
            ) {
                warnings.push(format!("could not remove '{value}': {error:#}"));
            }
        }
        Ok(WindowsPersonalizationOutcome {
            applied: true,
            diff,
            warnings,
        })
    })
}

fn inspect_loaded(
    registry: &Registry,
    mount: &str,
    plan: &WindowsPersonalizationPlan,
) -> Result<WindowsPersonalizationDiff> {
    let control_set = registry.current_control_set(mount)?;
    Ok(WindowsPersonalizationDiff {
        computer_name: plan
            .normalized_computer_name()?
            .map(|desired| -> Result<ComputerNameChange> {
                let key = format!(
                    "{mount}\\ControlSet{control_set:03}\\Control\\ComputerName\\ComputerName"
                );
                Ok(ComputerNameChange {
                    current: registry
                        .query_value(&key, "ComputerName")?
                        .map(|value| value.data)
                        .filter(|value| !value.is_empty()),
                    desired,
                })
            })
            .transpose()?,
        interfaces: if plan.convert_static_ipv4_to_dhcp {
            registry.inspect_interfaces(mount, control_set)?
        } else {
            Vec::new()
        },
        mounted_devices: if plan.clear_stale_drive_letters {
            registry.inspect_mounted_devices(mount, control_set)?
        } else {
            MountedDeviceChanges::default()
        },
    })
}

fn refuse_live_installation(system_hive: &Path) -> Result<()> {
    let Some(system_root) = std::env::var_os("SystemRoot") else {
        return Ok(());
    };
    let live = PathBuf::from(system_root).join("System32/config/SYSTEM");
    if live.exists() && fs_canonical(&live)? == fs_canonical(system_hive)? {
        bail!("refusing to personalize the running Windows installation");
    }
    Ok(())
}

fn fs_canonical(path: &Path) -> Result<PathBuf> {
    std::fs::canonicalize(path).with_context(|| format!("failed to resolve {}", path.display()))
}

/// Load the offline hive under a job-unique key, refusing a dirty hive unless
/// the caller opted in, and always unload it again.
fn with_loaded_hive<T>(
    system_hive: &Path,
    allow_dirty_hive: bool,
    operation: impl FnOnce(&str) -> Result<T>,
) -> Result<T> {
    validate_hive_path(system_hive)?;
    if !system_hive.is_file() {
        bail!(
            "offline SYSTEM hive does not exist: {}",
            system_hive.display()
        );
    }
    if super::windows_boot_arming::hive_is_dirty(system_hive)? && !allow_dirty_hive {
        bail!("SYSTEM hive is dirty; refusing offline personalization without allow_dirty_hive");
    }
    let registry = Registry::new();
    let mount = format!(
        "HKLM\\DISKLESS_MANAGER_PERSONALIZE_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default()
    );
    registry.load(&mount, system_hive)?;
    let result = operation(&mount);
    let unloaded = registry.unload(&mount);
    match (result, unloaded) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(unload_error)) => Err(error.context(format!(
            "additionally failed to unload '{mount}': {unload_error:#}"
        ))),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RegValue {
    name: String,
    kind: String,
    data: String,
}

/// Thin `reg.exe` wrapper. Paths are built from a validated control-set number
/// and hive keys only, so no user input reaches the command line unquoted.
struct Registry {
    executable: PathBuf,
}

impl Registry {
    fn new() -> Self {
        Self {
            executable: PathBuf::from("reg.exe"),
        }
    }

    fn run(&self, args: &[&str]) -> Result<Output> {
        let output = Command::new(&self.executable)
            .args(args)
            .output()
            .context("failed to execute reg.exe")?;
        Ok(output)
    }

    fn run_checked(&self, args: &[&str]) -> Result<Output> {
        let output = self.run(args)?;
        if !output.status.success() {
            bail!(
                "reg.exe {} failed: {}",
                args.first().copied().unwrap_or_default(),
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(output)
    }

    fn load(&self, mount: &str, hive: &Path) -> Result<()> {
        self.run_checked(&["load", mount, &hive.display().to_string()])
            .map(|_| ())
            .with_context(|| format!("failed to load offline hive {}", hive.display()))
    }

    fn unload(&self, mount: &str) -> Result<()> {
        self.run_checked(&["unload", mount]).map(|_| ())
    }

    fn current_control_set(&self, mount: &str) -> Result<u32> {
        let output = self.run_checked(&["query", &format!("{mount}\\Select"), "/v", "Current"])?;
        let values = parse_reg_values(&String::from_utf8_lossy(&output.stdout));
        let current = values
            .iter()
            .find(|value| {
                value.name.eq_ignore_ascii_case("Current")
                    && value.kind.eq_ignore_ascii_case("REG_DWORD")
            })
            .and_then(|value| parse_dword(&value.data))
            .context("offline SYSTEM hive has no valid Select\\Current DWORD")?;
        if !(1..=999).contains(&current) {
            bail!("offline SYSTEM Select\\Current {current} is not a control set");
        }
        Ok(current)
    }

    fn query_value(&self, key: &str, name: &str) -> Result<Option<RegValue>> {
        let output = self.run(&["query", key, "/v", name])?;
        if !output.status.success() {
            return Ok(None);
        }
        Ok(parse_reg_values(&String::from_utf8_lossy(&output.stdout))
            .into_iter()
            .find(|value| value.name.eq_ignore_ascii_case(name)))
    }

    fn subkeys(&self, key: &str) -> Result<Vec<String>> {
        let output = self.run(&["query", key])?;
        if !output.status.success() {
            return Ok(Vec::new());
        }
        Ok(parse_reg_subkeys(&String::from_utf8_lossy(&output.stdout)))
    }

    fn inspect_interfaces(&self, mount: &str, control_set: u32) -> Result<Vec<InterfaceChange>> {
        let root =
            format!("{mount}\\ControlSet{control_set:03}\\Services\\Tcpip\\Parameters\\Interfaces");
        let mut changes = Vec::new();
        for id in self.subkeys(&root)? {
            let guid = match id.rsplit('\\').next() {
                Some(guid) => guid.to_owned(),
                None => continue,
            };
            let key = format!("{root}\\{guid}");
            let enable_dhcp = self
                .query_value(&key, "EnableDHCP")?
                .filter(|value| value.kind.eq_ignore_ascii_case("REG_DWORD"))
                .and_then(|value| parse_dword(&value.data))
                .unwrap_or(0);
            let static_ipv4 = self
                .query_value(&key, "IPAddress")?
                .map(|value| static_ipv4_addresses(&value.data))
                .unwrap_or_default();
            let action = if enable_dhcp == 1 {
                InterfaceAction::AlreadyDhcp
            } else if static_ipv4.is_empty() {
                InterfaceAction::NoStaticIpv4
            } else {
                InterfaceAction::Convert
            };
            changes.push(InterfaceChange {
                id: guid,
                action,
                static_ipv4,
            });
        }
        Ok(changes)
    }

    fn inspect_mounted_devices(
        &self,
        mount: &str,
        control_set: u32,
    ) -> Result<MountedDeviceChanges> {
        let output = self.run(&[
            "query",
            &format!("{mount}\\ControlSet{control_set:03}\\MountedDevices"),
        ])?;
        let mut changes = MountedDeviceChanges::default();
        for value in parse_reg_values(&String::from_utf8_lossy(&output.stdout)) {
            match drive_letter_decision(&value.name) {
                DriveLetterDecision::Remove => changes.removed.push(value.name),
                DriveLetterDecision::Keep => changes.retained.push(value.name),
                DriveLetterDecision::Other => {}
            }
        }
        changes.removed.sort();
        changes.retained.sort();
        Ok(changes)
    }

    fn write_computer_name(&self, mount: &str, control_set: u32, name: &str) -> Result<()> {
        let name_key =
            format!("{mount}\\ControlSet{control_set:03}\\Control\\ComputerName\\ComputerName");
        let tcp_key = format!("{mount}\\ControlSet{control_set:03}\\Services\\Tcpip\\Parameters");
        self.run_checked(&registry_add_args(
            &name_key,
            "ComputerName",
            "REG_SZ",
            name,
        ))?;
        // Windows reads both hostname values; they must agree with the
        // computer name or the client boots with a stale DNS name.
        for value in ["Hostname", "NV Hostname"] {
            self.run_checked(&registry_add_args(&tcp_key, value, "REG_SZ", name))?;
        }
        Ok(())
    }

    fn convert_interface_to_dhcp(&self, mount: &str, control_set: u32, id: &str) -> Result<()> {
        let key = format!(
            "{mount}\\ControlSet{control_set:03}\\Services\\Tcpip\\Parameters\\Interfaces\\{id}"
        );
        self.run_checked(&registry_add_args(&key, "EnableDHCP", "REG_DWORD", "1"))?;
        for value in ["IPAddress", "SubnetMask", "DefaultGateway"] {
            self.run_checked(&registry_add_args(&key, value, "REG_MULTI_SZ", ""))?;
        }
        Ok(())
    }

    fn delete_value(&self, key: &str, name: &str) -> Result<()> {
        self.run_checked(&["delete", key, "/v", name, "/f"])
            .map(|_| ())
    }
}

fn registry_add_args<'a>(
    key: &'a str,
    name: &'a str,
    kind: &'a str,
    value: &'a str,
) -> Vec<&'a str> {
    vec!["add", key, "/v", name, "/t", kind, "/d", value, "/f"]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DriveLetterDecision {
    Remove,
    Keep,
    Other,
}

/// Only `\DosDevices\<letter>:` reservations are considered, and `C:` is always
/// kept because it is the usual boot volume letter.
fn drive_letter_decision(value_name: &str) -> DriveLetterDecision {
    let Some(letter) = value_name.strip_prefix(r"\DosDevices\") else {
        return DriveLetterDecision::Other;
    };
    let bytes = letter.as_bytes();
    if bytes.len() != 2 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' {
        return DriveLetterDecision::Other;
    }
    if bytes[0].eq_ignore_ascii_case(&b'C') {
        DriveLetterDecision::Keep
    } else {
        DriveLetterDecision::Remove
    }
}

fn static_ipv4_addresses(data: &str) -> Vec<String> {
    parse_multi_sz(data)
        .into_iter()
        .filter(|address| address.parse::<std::net::Ipv4Addr>().is_ok() && address != "0.0.0.0")
        .collect()
}

/// `reg query` prints `    Name    REG_TYPE    data`. Multi-string data is
/// printed with literal `\0` separators.
fn parse_reg_values(output: &str) -> Vec<RegValue> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let (name, rest) = split_once_whitespace(line)?;
            let (kind, data) = match split_once_whitespace(rest) {
                Some((kind, data)) => (kind, data),
                // A value with no data still prints its type.
                None => (rest, ""),
            };
            if !kind.to_ascii_uppercase().starts_with("REG_") {
                return None;
            }
            Some(RegValue {
                name: name.to_owned(),
                kind: kind.to_owned(),
                data: data.to_owned(),
            })
        })
        .collect()
}

/// Split off the first whitespace-delimited token.
fn split_once_whitespace(line: &str) -> Option<(&str, &str)> {
    let end = line.find(char::is_whitespace)?;
    Some((&line[..end], line[end..].trim_start()))
}

/// Subkeys are printed as fully qualified `HKEY_LOCAL_MACHINE\...` lines.
fn parse_reg_subkeys(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("HKEY_") || trimmed.starts_with("HKLM\\") {
                Some(trimmed.to_owned())
            } else {
                None
            }
        })
        .collect()
}

fn parse_dword(data: &str) -> Option<u32> {
    let value = data.trim();
    match value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        Some(hex) => u32::from_str_radix(hex, 16).ok(),
        None => value.parse::<u32>().ok(),
    }
}

fn parse_multi_sz(data: &str) -> Vec<String> {
    data.split("\\0")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Reject hive paths that try to escape into a caller-chosen key tree.
pub fn validate_hive_path(path: &Path) -> Result<()> {
    if !path.is_absolute() {
        bail!("SYSTEM hive path must be absolute");
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        bail!("SYSTEM hive path must not contain '..'");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computer_names_follow_netbios_rules() {
        assert_eq!(normalize_computer_name(" pc-lab-01 ").unwrap(), "PC-LAB-01");
        assert!(normalize_computer_name("").is_err());
        assert!(normalize_computer_name("a".repeat(16).as_str()).is_err());
        assert!(normalize_computer_name("pc_lab").is_err());
        assert!(normalize_computer_name("-pc").is_err());
        assert!(normalize_computer_name("12345").is_err());
        assert!(normalize_computer_name("con").is_err());
        assert!(normalize_computer_name("a".repeat(15).as_str()).is_ok());
    }

    #[test]
    fn empty_plans_are_rejected_and_unknown_fields_are_ignored() {
        let plan = WindowsPersonalizationPlan::default();
        assert!(plan.is_empty());
        assert!(plan.validate().is_err());

        let plan: WindowsPersonalizationPlan = serde_json::from_value(serde_json::json!({
            "computer_name": "pc-01",
            "convert_static_ipv4_to_dhcp": true,
            "clear_stale_drive_letters": false
        }))
        .unwrap();
        assert_eq!(
            plan.normalized_computer_name().unwrap().as_deref(),
            Some("PC-01")
        );
        assert!(plan.validate().is_ok());

        // Both switches default to off so an older caller never opts in.
        let plan: WindowsPersonalizationPlan =
            serde_json::from_value(serde_json::json!({"computer_name": null})).unwrap();
        assert!(!plan.convert_static_ipv4_to_dhcp);
        assert!(!plan.clear_stale_drive_letters);
        assert!(plan.is_empty());
    }

    #[test]
    fn registry_output_is_parsed_by_type() {
        let output = "\r\nHKEY_LOCAL_MACHINE\\MOUNT\\Select\r\n    Current    REG_DWORD    0x1\r\n    Default    REG_DWORD    0x2\r\n";
        let values = parse_reg_values(output);
        assert_eq!(values.len(), 2);
        assert_eq!(values[0].kind, "REG_DWORD");
        assert_eq!(parse_dword(&values[0].data), Some(1));
        assert_eq!(values[1].data, "0x2");
        assert_eq!(parse_dword("0x2"), Some(2));
        assert_eq!(parse_dword("17"), Some(17));
        assert_eq!(parse_dword("garbage"), None);

        let multi = "    IPAddress    REG_MULTI_SZ    192.0.2.10\\0192.0.2.11\r\n";
        let values = parse_reg_values(multi);
        assert_eq!(
            parse_multi_sz(&values[0].data),
            vec!["192.0.2.10".to_owned(), "192.0.2.11".to_owned()]
        );

        let subkeys = parse_reg_subkeys(
            "\r\nHKEY_LOCAL_MACHINE\\MOUNT\\Interfaces\r\nHKEY_LOCAL_MACHINE\\MOUNT\\Interfaces\\{A}\r\nHKEY_LOCAL_MACHINE\\MOUNT\\Interfaces\\{B}\r\n",
        );
        assert_eq!(subkeys.len(), 3);
    }

    #[test]
    fn only_ipv4_static_addresses_are_candidates() {
        let mut addresses = static_ipv4_addresses("192.0.2.10\\0192.0.2.11");
        addresses.sort();
        assert_eq!(addresses, vec!["192.0.2.10", "192.0.2.11"]);
        assert!(static_ipv4_addresses("0.0.0.0").is_empty());
        assert!(static_ipv4_addresses("fe80::1").is_empty());
        assert!(static_ipv4_addresses("").is_empty());
    }

    #[test]
    fn drive_letter_cleanup_keeps_the_boot_letter_and_volume_identities() {
        assert_eq!(
            drive_letter_decision(r"\DosDevices\C:"),
            DriveLetterDecision::Keep
        );
        assert_eq!(
            drive_letter_decision(r"\DosDevices\c:"),
            DriveLetterDecision::Keep
        );
        assert_eq!(
            drive_letter_decision(r"\DosDevices\D:"),
            DriveLetterDecision::Remove
        );
        assert_eq!(
            drive_letter_decision(r"\DosDevices\Z:"),
            DriveLetterDecision::Remove
        );
        // Volume identities, EFI entries and anything unexpected are untouched.
        assert_eq!(
            drive_letter_decision(r"\??\Volume{8f0c2f1e-0000-0000-0000-100000000000}"),
            DriveLetterDecision::Other
        );
        assert_eq!(
            drive_letter_decision(r"\DosDevices\Volume{8f0c}"),
            DriveLetterDecision::Other
        );
        assert_eq!(
            drive_letter_decision(r"\DosDevices\EFI"),
            DriveLetterDecision::Other
        );
        assert_eq!(drive_letter_decision("C:"), DriveLetterDecision::Other);
    }

    #[test]
    fn hive_paths_must_be_absolute_and_stay_in_place() {
        assert!(validate_hive_path(Path::new("/mnt/win/Windows/System32/config/SYSTEM")).is_ok());
        assert!(validate_hive_path(Path::new("Windows/System32/config/SYSTEM")).is_err());
        assert!(validate_hive_path(Path::new("/mnt/../etc/SYSTEM")).is_err());
    }

    #[test]
    fn empty_diff_means_nothing_to_write() {
        let diff = WindowsPersonalizationDiff::default();
        assert!(diff.is_empty());
        let diff = WindowsPersonalizationDiff {
            interfaces: vec![InterfaceChange {
                id: "{A}".into(),
                action: InterfaceAction::AlreadyDhcp,
                static_ipv4: Vec::new(),
            }],
            ..Default::default()
        };
        assert!(diff.is_empty(), "a DHCP interface needs no change");
    }
}
