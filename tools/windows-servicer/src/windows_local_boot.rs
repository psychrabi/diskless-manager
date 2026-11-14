//! Prepare the running reference Windows installation, before image capture.
//! Uses the server's boot policy planner; never mounts or edits a remote client disk.

use crate::windows_boot_arming::{
    build_plan, BootNicCriticalDevice, RegistryArmChange, RegistryArmValue, WindowsBootArmConfig,
    WindowsBootArmPlan, WindowsBootArmer, WindowsBootInventory, NETWORK_ADAPTER_CLASS_GUID,
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BootAdapter {
    pub id: String,
    pub name: String,
    pub description: String,
    pub mac: String,
    pub status: String,
    pub service: String,
    pub hardware_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalBootRequest {
    pub adapter_id: String,
    /// False returns a preview. True changes only this running Windows installation.
    #[serde(default)]
    pub apply: bool,
}

#[derive(Debug, Serialize)]
pub struct LocalBootResult {
    pub adapter: BootAdapter,
    pub plan: WindowsBootArmPlan,
    pub applied: bool,
    pub backup_path: Option<String>,
    pub reboot_required: bool,
}

/// Discover installed wired PCI NICs; this operation makes no system changes.
pub fn list_adapters() -> Result<Vec<BootAdapter>> {
    let output = powershell(include_str!("../assets/local-boot-inventory.ps1"), &[])?;
    serde_json::from_slice(&output).context("invalid boot adapter inventory")
}

/// Re-resolve the selected PnP instance on every invocation, including apply.
/// The apply script journals original registry values before the first write and
/// rolls back on failure. Driver installation is a separate Windows PnP operation.
pub fn prepare(request: &LocalBootRequest) -> Result<LocalBootResult> {
    ensure!(
        cfg!(windows) && crate::is_elevated(),
        "Run the Windows client as administrator to prepare diskless boot."
    );
    let adapter = list_adapters()?
        .into_iter()
        .find(|nic| nic.id.eq_ignore_ascii_case(&request.adapter_id))
        .context("Selected boot NIC is no longer present. Refresh adapters and select it again.")?;
    let inventory =
        WindowsBootArmer::new()?.inspect_running(std::slice::from_ref(&adapter.service))?;
    let plan = plan_for_adapter(&adapter, inventory)?;
    let backup_path = if request.apply {
        #[derive(Deserialize)]
        struct Applied {
            backup_path: String,
        }
        let output = powershell(
            include_str!("../assets/local-boot-apply.ps1"),
            &serde_json::to_vec(&plan.changes)?,
        )?;
        Some(
            serde_json::from_slice::<Applied>(&output)
                .context("invalid preparation result")?
                .backup_path,
        )
    } else {
        None
    };
    Ok(LocalBootResult {
        adapter,
        plan,
        applied: request.apply,
        backup_path,
        reboot_required: request.apply,
    })
}

fn plan_for_adapter(
    adapter: &BootAdapter,
    mut inventory: WindowsBootInventory,
) -> Result<WindowsBootArmPlan> {
    ensure!(
        adapter.id.to_ascii_uppercase().starts_with("PCI\\") && !adapter.hardware_ids.is_empty(),
        "Select an installed PCI Ethernet adapter."
    );
    ensure!(
        !adapter.service.is_empty(),
        "The selected NIC has no installed driver. Install its signed NIC package first."
    );
    for service in ["iScsiPrt", "Tcpip", "ndis", &adapter.service] {
        ensure!(inventory.driver_services.iter().any(|s| s.eq_ignore_ascii_case(service)), "Required kernel driver {service} is missing; repair Windows or install the NIC driver first.");
    }
    ensure!(
        inventory
            .existing_services
            .iter()
            .any(|s| s.eq_ignore_ascii_case("MSiSCSI")),
        "Microsoft iSCSI Initiator service is missing."
    );
    // Do not promote unrelated adapters or network filters in the reference OS.
    inventory
        .native_nic_services
        .retain(|s| s.eq_ignore_ascii_case(&adapter.service));
    inventory.selected_nic_services = vec![adapter.service.clone()];
    let config = WindowsBootArmConfig {
        disable_wfp_lwf: false,
        expected_boot_nic_ids: adapter.hardware_ids.clone(),
        require_boot_nic_binding: true,
        critical_device_boot_nic: Some(BootNicCriticalDevice {
            service: adapter.service.clone(),
            class_guid: Some(NETWORK_ADAPTER_CLASS_GUID.to_owned()),
            hardware_ids: adapter.hardware_ids.clone(),
        }),
        ..Default::default()
    };
    let mut plan = build_plan(&inventory, &config)?;
    for service in [&adapter.service, "iScsiPrt", "Tcpip", "ndis"] {
        plan.changes.push(RegistryArmChange {
            key: format!(
                "{}\\Services\\{service}\\StartOverride",
                plan.active_control_set
            ),
            name: "0".into(),
            value: RegistryArmValue::Dword(0),
            reason: "allow the selected boot driver to start before the iSCSI disk".into(),
        });
    }
    plan.warnings
        .retain(|warning| !warning.starts_with("MSiSCSI service detected"));
    for (key, name, value, reason) in [
        (
            "Services\\MSiSCSI",
            "Start",
            2,
            "start Microsoft iSCSI session management automatically",
        ),
        (
            "Control\\Session Manager\\Power",
            "HiberbootEnabled",
            0,
            "use a full Windows shutdown before image capture",
        ),
    ] {
        plan.changes.push(RegistryArmChange {
            key: format!("{}\\{key}", plan.active_control_set),
            name: name.into(),
            value: RegistryArmValue::Dword(value),
            reason: reason.into(),
        });
    }
    plan.warnings.push("Preparation is for the selected installed NIC. Stage other clients' drivers through the server's Windows image preparation before publishing a shared image. Validate a real iPXE/iBFT iSCSI boot; these settings alone do not prove boot compatibility.".into());
    Ok(plan)
}

// Input is JSON over stdin; paths and adapter names never become PowerShell code.
fn powershell(script: &str, input: &[u8]) -> Result<Vec<u8>> {
    ensure!(cfg!(windows), "This operation requires Windows 10/11.");
    let executable =
        std::path::PathBuf::from(std::env::var_os("SystemRoot").context("SystemRoot is missing")?)
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let mut child = Command::new(executable)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to start Windows preparation")?;
    child
        .stdin
        .take()
        .context("PowerShell stdin unavailable")?
        .write_all(input)?;
    // Do not time out a registry writer and release its operation lock mid-apply.
    let output = child.wait_with_output()?;
    ensure!(
        output.status.success(),
        "Windows preparation failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn fixture() -> (BootAdapter, WindowsBootInventory) {
        let adapter = BootAdapter {
            id: r"PCI\VEN_8086&DEV_100E\1".into(),
            name: "Ethernet".into(),
            description: "Intel".into(),
            mac: "00-11-22-33-44-55".into(),
            status: "Up".into(),
            service: "e1gexpress".into(),
            hardware_ids: vec![r"PCI\VEN_8086&DEV_100E".into()],
        };
        let services: Vec<String> = [
            "e1gexpress",
            "othernic",
            "iScsiPrt",
            "Tcpip",
            "ndis",
            "MSiSCSI",
            "disk",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let inventory = WindowsBootInventory {
            active_control_set: "ControlSet001".into(),
            native_nic_services: vec!["e1gexpress".into(), "othernic".into()],
            selected_nic_services: vec![],
            existing_services: services.clone(),
            driver_services: services.into_iter().filter(|s| s != "MSiSCSI").collect(),
            driver_boot_flags: BTreeMap::from([("e1gexpress".into(), 4)]),
            hive_dirty: false,
            msiscsi_image_path: None,
            wfp_lwf_start: Some(3),
            enum_pci_devices: vec![],
            critical_device_database: vec![],
        };
        (adapter, inventory)
    }

    #[test]
    fn preparation_arms_selected_nic_preserves_flags_and_keeps_filters() {
        let (adapter, inventory) = fixture();
        let plan = plan_for_adapter(&adapter, inventory).unwrap();
        assert!(plan
            .changes
            .iter()
            .any(|c| c.key.ends_with("Services\\e1gexpress")
                && c.name == "BootFlags"
                && matches!(c.value, RegistryArmValue::Dword(5))));
        assert!(!plan
            .changes
            .iter()
            .any(|c| c.key.contains("othernic") || c.key.contains("WFPLWFS")));
        assert!(plan
            .changes
            .iter()
            .any(|c| c.key.contains("CriticalDeviceDatabase") && c.name == "Service"));
        assert!(plan
            .changes
            .iter()
            .any(|c| c.key.ends_with("Services\\MSiSCSI")
                && matches!(c.value, RegistryArmValue::Dword(2))));
    }

    #[test]
    fn preparation_refuses_missing_kernel_or_nic_drivers() {
        for missing in ["e1gexpress", "iScsiPrt", "Tcpip", "ndis"] {
            let (adapter, mut inventory) = fixture();
            inventory.driver_services.retain(|s| s != missing);
            assert!(plan_for_adapter(&adapter, inventory).is_err());
        }
        let (mut adapter, inventory) = fixture();
        adapter.id = "USB\\adapter".into();
        assert!(plan_for_adapter(&adapter, inventory).is_err());
    }

    #[test]
    fn request_defaults_to_preview_and_rejects_unknown_options() {
        let request: LocalBootRequest = serde_json::from_str(r#"{"adapter_id":"nic"}"#).unwrap();
        assert!(!request.apply);
        assert!(
            serde_json::from_str::<LocalBootRequest>(r#"{"adapter_id":"nic","commit":true}"#)
                .is_err()
        );
    }
}
