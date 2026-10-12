//! Managed-client driver reporting.
//!
//! `report-client` runs inside a working managed Windows client. It determines
//! the Boot NIC, exports that adapter's third-party driver package with
//! `pnputil /export-driver`, and POSTs the report contract to the server's
//! public report endpoint. Trust comes from the client's live iSCSI session,
//! so the per-client configuration carries only the server URL and client ID:
//! no shared credential is embedded in the common image, and archive contents
//! are never logged.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(120);

/// Identity and inventory posted to `/api/pxe/network-drivers/reports`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientReport {
    pub client_id: String,
    pub windows_version: String,
    pub architecture: String,
    pub boot_nic_ids: Vec<String>,
    /// Exact candidate image revision under test. Set only on a designated
    /// test client via the reporter configuration; otherwise absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_revision: Option<String>,
    /// Boot NIC hardware ID observed for this boot, sent with a test claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boot_nic_id: Option<String>,
}

impl ClientReport {
    pub fn validate(&self) -> Result<()> {
        if self.client_id.trim().is_empty()
            || self.windows_version.trim().is_empty()
            || self.architecture.trim().is_empty()
        {
            bail!("client_id, windows_version, and architecture are required");
        }
        if self.boot_nic_ids.is_empty() {
            bail!("at least one Boot NIC hardware ID is required");
        }
        Ok(())
    }
}

/// Per-client reporter configuration. Protected by file ACLs (SYSTEM +
/// Administrators); contains no secrets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReporterConfig {
    pub server_url: String,
    pub client_id: String,
    #[serde(default)]
    pub export_package: bool,
    /// Candidate revision this client is designated to test. Only set on the
    /// disposable test client; the server promotes the revision solely when
    /// this claim matches the open candidate and the iSCSI session is live.
    #[serde(default)]
    pub image_revision: Option<String>,
}

impl ReporterConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read reporter config {}", path.display()))?;
        let config: Self =
            serde_json::from_str(&content).context("invalid reporter configuration JSON")?;
        if config.server_url.trim().is_empty() || config.client_id.trim().is_empty() {
            bail!("reporter configuration requires server_url and client_id");
        }
        Ok(config)
    }

    pub fn report_endpoint(&self) -> Result<String> {
        let base = self.server_url.trim_end_matches('/');
        if base.is_empty() {
            bail!("reporter configuration requires server_url");
        }
        Ok(format!("{base}/api/pxe/network-drivers/reports"))
    }
}

/// A physical network adapter as reported by PowerShell.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct NetAdapter {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "MacAddress")]
    pub mac_address: String,
    #[serde(rename = "InterfaceDescription")]
    pub description: String,
    #[serde(rename = "Status")]
    pub status: String,
    #[serde(rename = "ServiceName", default)]
    pub service_name: String,
}

/// A PnP device entry carrying the instance (device) ID.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct PnpDevice {
    #[serde(rename = "InstanceId")]
    pub instance_id: String,
    #[serde(rename = "FriendlyName", default)]
    pub friendly_name: String,
}

/// A published third-party driver from `pnputil /enum-drivers`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedDriver {
    pub published_name: String,
    pub original_name: String,
    pub provider: String,
    pub class: String,
    pub service_name: String,
}

fn run_bounded(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to execute {program}"))?;
    if !output.status.success() {
        bail!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Parse `Get-NetAdapter ... | ConvertTo-Json` output.
pub fn parse_net_adapters(json: &str) -> Result<Vec<NetAdapter>> {
    let trimmed = json.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return Ok(Vec::new());
    }
    if trimmed.starts_with('[') {
        Ok(serde_json::from_str(trimmed).context("invalid adapter inventory JSON")?)
    } else {
        Ok(vec![
            serde_json::from_str(trimmed).context("invalid adapter inventory JSON")?
        ])
    }
}

/// Parse `Get-PnpDevice -Class Net ... | ConvertTo-Json` output.
pub fn parse_pnp_devices(json: &str) -> Result<Vec<PnpDevice>> {
    let trimmed = json.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return Ok(Vec::new());
    }
    if trimmed.starts_with('[') {
        Ok(serde_json::from_str(trimmed).context("invalid PnP inventory JSON")?)
    } else {
        Ok(vec![
            serde_json::from_str(trimmed).context("invalid PnP inventory JSON")?
        ])
    }
}

/// Parse `pnputil /enum-drivers` text output into published-driver records.
pub fn parse_enum_drivers(text: &str) -> Vec<PublishedDriver> {
    let mut drivers = Vec::new();
    let mut current = PublishedDriver {
        published_name: String::new(),
        original_name: String::new(),
        provider: String::new(),
        class: String::new(),
        service_name: String::new(),
    };
    let mut in_record = false;
    for line in text.lines() {
        let line = line.trim();
        if line.eq_ignore_ascii_case("published name :") || line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key.trim().to_ascii_lowercase().as_str() {
            "published name" => {
                if in_record && !current.published_name.is_empty() {
                    drivers.push(std::mem::replace(
                        &mut current,
                        PublishedDriver {
                            published_name: value.trim().to_string(),
                            original_name: String::new(),
                            provider: String::new(),
                            class: String::new(),
                            service_name: String::new(),
                        },
                    ));
                } else {
                    current.published_name = value.trim().to_string();
                    in_record = true;
                }
            }
            "original name" => current.original_name = value.trim().to_string(),
            "provider name" => current.provider = value.trim().to_string(),
            "class name" => current.class = value.trim().to_string(),
            "service name" => current.service_name = value.trim().to_string(),
            _ => {}
        }
    }
    if in_record && !current.published_name.is_empty() {
        drivers.push(current);
    }
    drivers
}

/// Normalize a raw architecture token to the catalog spelling.
pub fn normalize_architecture(token: &str) -> String {
    match token.trim().to_ascii_lowercase().as_str() {
        "amd64" | "x86_64" | "x64" => "x64".to_string(),
        "i386" | "i686" | "x86" => "x86".to_string(),
        "aarch64" | "arm64" => "arm64".to_string(),
        other => other.to_string(),
    }
}

/// Select the Boot NIC: the up, physical adapter most likely carrying iSCSI
/// boot traffic. Prefers the adapter owning the default route's interface;
/// falls back to the first up physical adapter. Virtual/tunnel adapters and
/// disconnected ports are never selected.
pub fn select_boot_nic<'a>(
    adapters: &'a [NetAdapter],
    default_route_interface: Option<&str>,
) -> Result<&'a NetAdapter> {
    fn physical(adapter: &NetAdapter) -> bool {
        let description = adapter.description.to_ascii_lowercase();
        !description.contains("virtual")
            && !description.contains("hyper-v")
            && !description.contains("tunnel")
            && !description.contains("vpn")
            && !description.contains("bluetooth")
    }
    if let Some(name) = default_route_interface {
        if let Some(adapter) = adapters
            .iter()
            .filter(|adapter| physical(adapter))
            .find(|adapter| adapter.name.eq_ignore_ascii_case(name))
        {
            return Ok(adapter);
        }
    }
    adapters
        .iter()
        .find(|adapter| physical(adapter) && adapter.status.eq_ignore_ascii_case("up"))
        .context("no usable physical network adapter found for Boot NIC reporting")
}

/// Match the published Net-class driver for the Boot NIC's service name.
pub fn match_boot_driver<'a>(
    drivers: &'a [PublishedDriver],
    service_name: &str,
) -> Result<&'a PublishedDriver> {
    let candidates: Vec<&PublishedDriver> = drivers
        .iter()
        .filter(|driver| {
            driver.class.eq_ignore_ascii_case("net")
                && driver.service_name.eq_ignore_ascii_case(service_name)
        })
        .collect();
    match candidates.as_slice() {
        [driver] => Ok(driver),
        [] => bail!("no published Net-class driver for service '{service_name}'"),
        _ => bail!("multiple published drivers claim service '{service_name}'"),
    }
}

/// Invoke the client reporting flow. Windows-only; every step runs as a
/// bounded subprocess so a hung tool cannot block boot.
pub fn run_report_client(config_path: &Path) -> Result<()> {
    anyhow::ensure!(
        cfg!(windows),
        "client reporting runs on a managed Windows client"
    );
    let config = ReporterConfig::load(config_path)?;
    let adapters_json = run_bounded(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-NetAdapter -Physical | Select-Object Name,MacAddress,InterfaceDescription,Status,ServiceName | ConvertTo-Json -Depth 2",
        ],
    )?;
    let adapters = parse_net_adapters(&adapters_json)?;
    let route_output = run_bounded(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(Get-NetRoute -DestinationPrefix '0.0.0.0/0' | Sort-Object RouteMetric | Select-Object -First 1).InterfaceAlias",
        ],
    )
    .unwrap_or_default();
    let boot = select_boot_nic(&adapters, Some(route_output.trim()))?;
    anyhow::ensure!(
        !boot.service_name.trim().is_empty(),
        "Boot NIC '{}' has no associated driver service",
        boot.name
    );

    let pnp_json = run_bounded(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-PnpDevice -Class Net -PresentOnly | Select-Object InstanceId,FriendlyName | ConvertTo-Json -Depth 2",
        ],
    )?;
    let devices = parse_pnp_devices(&pnp_json)?;
    let boot_id = hardware_id_for_boot_nic(&devices, boot)?;
    let os = run_bounded(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$os = Get-ComputerInfo -Property OsName,OArchitecture,WindowsVersion | ConvertTo-Json -Compress; $os",
        ],
    )?;
    let (windows_version, architecture) = parse_os_info(&os)?;

    let report = ClientReport {
        client_id: config.client_id.clone(),
        windows_version,
        architecture,
        boot_nic_ids: vec![boot_id.clone()],
        image_revision: config
            .image_revision
            .clone()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        boot_nic_id: Some(boot_id),
    };
    report.validate()?;

    let archive = if config.export_package {
        Some(export_boot_driver(&boot.service_name)?)
    } else {
        None
    };
    let endpoint = config.report_endpoint()?;
    post_report(&endpoint, &report, archive.as_deref())?;
    if let Some(dir) = archive {
        let _ = std::fs::remove_dir_all(dir.parent().unwrap_or(&dir));
    }
    Ok(())
}

/// Resolve the Boot NIC's hardware ID from the PnP inventory by matching the
/// adapter description against friendly names, preferring exact matches.
fn hardware_id_for_boot_nic(devices: &[PnpDevice], boot: &NetAdapter) -> Result<String> {
    if let Some(device) = devices
        .iter()
        .find(|device| device.friendly_name == boot.description)
        .or_else(|| {
            devices.iter().find(|device| {
                device
                    .friendly_name
                    .to_ascii_lowercase()
                    .contains(&boot.service_name.to_ascii_lowercase())
            })
        })
    {
        if device.instance_id.trim().is_empty() {
            bail!("Boot NIC PnP entry has no instance ID");
        }
        return Ok(device.instance_id.trim().replace('/', "\\"));
    }
    bail!("Boot NIC '{}' not found in the PnP inventory", boot.name)
}

/// Parse `Get-ComputerInfo` JSON into `(windows_version, architecture)`.
pub fn parse_os_info(json: &str) -> Result<(String, String)> {
    #[derive(Deserialize)]
    struct OsInfo {
        #[serde(rename = "OsName", default)]
        os_name: String,
        #[serde(rename = "OArchitecture", default)]
        architecture: String,
        #[serde(rename = "WindowsVersion", default)]
        version: String,
    }
    let info: OsInfo = serde_json::from_str(json.trim()).context("invalid OS inventory JSON")?;
    if info.os_name.trim().is_empty() {
        bail!("OS inventory has no OS name");
    }
    let version = if info.version.trim().is_empty() {
        info.os_name.trim().to_string()
    } else {
        format!("{} {}", info.os_name.trim(), info.version.trim())
    };
    Ok((version, normalize_architecture(&info.architecture)))
}

/// Export the Boot NIC's published driver package with pnputil. An export
/// failure is fatal: it must never produce a successful report, so the caller
/// fails before anything is posted.
fn export_boot_driver(service_name: &str) -> Result<PathBuf> {
    let listing = run_bounded("pnputil", &["/enum-drivers"])?;
    let parsed = parse_enum_drivers(&listing);
    let driver = match_boot_driver(&parsed, service_name)?;
    // Keep the TempDir alive until the archive is posted; leak-and-cleanup
    // keeps the signature simple while guaranteeing the path stays valid.
    let staging_path = tempfile::tempdir()
        .context("failed to create driver export directory")?
        .keep();
    let target = staging_path.join("export");
    std::fs::create_dir_all(&target)?;
    run_bounded(
        "pnputil",
        &[
            "/export-driver",
            &driver.published_name,
            &target.to_string_lossy(),
        ],
    )
    .with_context(|| {
        let _ = std::fs::remove_dir_all(&staging_path);
        format!("failed to export driver package {}", driver.published_name)
    })?;
    let archive = staging_path.join("driver.zip");
    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!(
                "Compress-Archive -Path '{}/*' -DestinationPath '{}' -Force",
                target.display(),
                archive.display()
            ),
        ])
        .output()
        .context("failed to archive the exported driver package")?;
    if !status.status.success() {
        let _ = std::fs::remove_dir_all(&staging_path);
        bail!("failed to archive the exported driver package");
    }
    Ok(archive)
}

/// POST the multipart report. Archive bytes travel in the request body only;
/// nothing about the package or configuration is logged.
fn post_report(endpoint: &str, report: &ClientReport, archive: Option<&Path>) -> Result<()> {
    let boundary = "diskless-client-report";
    let mut body = Vec::new();
    let part = |name: &str, filename: Option<&str>, content_type: &str| {
        let mut header = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"");
        if let Some(filename) = filename {
            header.push_str(&format!("; filename=\"{filename}\""));
        }
        header.push_str(&format!("\r\nContent-Type: {content_type}\r\n\r\n"));
        header.into_bytes()
    };
    body.extend_from_slice(&part("report", None, "application/json"));
    body.extend_from_slice(&serde_json::to_vec(report)?);
    body.extend_from_slice(b"\r\n");
    if let Some(archive) = archive {
        let bytes = std::fs::read(archive).context("failed to read the exported driver archive")?;
        body.extend_from_slice(&part("file", Some("driver.zip"), "application/zip"));
        body.extend_from_slice(&bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    let response = run_bounded(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!(
                "$bytes = [Convert]::FromBase64String('{}'); $response = Invoke-WebRequest -Uri '{}' -Method Post -ContentType 'multipart/form-data; boundary={}' -Body $bytes -UseBasicParsing -TimeoutSec {}; if ($response.StatusCode -ge 400) {{ throw \"report rejected: $($response.StatusCode)\" }}",
                base64_encode(&body),
                endpoint.replace('\'', "''"),
                boundary,
                COMMAND_TIMEOUT.as_secs(),
            ),
        ],
    );
    response.map(|_| ())
}

fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let mut value: u32 = 0;
        for (index, byte) in chunk.iter().enumerate() {
            value |= u32::from(*byte) << (16 - 8 * index);
        }
        let width = chunk.len() + 1;
        for index in 0..width {
            let shift = 18 - 6 * index;
            out.push(ALPHABET[((value >> shift) & 0x3F) as usize] as char);
        }
        for _ in width..4 {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_and_pnp_inventory_parse_single_and_multiple() {
        let single = r#"{"Name":"Ethernet","MacAddress":"02-00-00-00-00-01","InterfaceDescription":"Intel NIC","Status":"Up","ServiceName":"e1dexpress"}"#;
        assert_eq!(parse_net_adapters(single).unwrap().len(), 1);
        let array = format!("[{single},{single}]");
        assert_eq!(parse_net_adapters(&array).unwrap().len(), 2);
        assert!(parse_net_adapters("").unwrap().is_empty());
        let pnp = r#"{"InstanceId":"PCI\\VEN_8086&DEV_15F3&SUBSYS_00008086&REV_01\\3&11583659&0&C8","FriendlyName":"Intel NIC"}"#;
        assert_eq!(parse_pnp_devices(pnp).unwrap().len(), 1);
        assert!(parse_pnp_devices("not json").is_err());
    }

    #[test]
    fn boot_nic_prefers_default_route_then_first_up_physical() {
        let adapters = vec![
            NetAdapter {
                name: "vEthernet".into(),
                mac_address: String::new(),
                description: "Hyper-V Virtual Adapter".into(),
                status: "Up".into(),
                service_name: "vmswitch".into(),
            },
            NetAdapter {
                name: "Ethernet".into(),
                mac_address: String::new(),
                description: "Intel NIC".into(),
                status: "Up".into(),
                service_name: "e1dexpress".into(),
            },
            NetAdapter {
                name: "Ethernet 2".into(),
                mac_address: String::new(),
                description: "Intel NIC #2".into(),
                status: "Disconnected".into(),
                service_name: "e1dexpress".into(),
            },
        ];
        assert_eq!(
            select_boot_nic(&adapters, Some("Ethernet")).unwrap().name,
            "Ethernet"
        );
        assert_eq!(select_boot_nic(&adapters, None).unwrap().name, "Ethernet");
        assert!(select_boot_nic(&adapters[..1], None).is_err());
    }

    #[test]
    fn enum_drivers_match_requires_unique_net_service() {
        let text = "Published Name :     oem10.inf\nOriginal Name :      netkvm.inf\nProvider Name :      Red Hat\nClass Name :         Net\nService Name :       netkvm\n\nPublished Name :     oem11.inf\nOriginal Name :      stor.inf\nProvider Name :      Vendor\nClass Name :         SCSIAdapter\nService Name :       stor\n";
        let drivers = parse_enum_drivers(text);
        assert_eq!(drivers.len(), 2);
        assert_eq!(
            match_boot_driver(&drivers, "netkvm")
                .unwrap()
                .published_name,
            "oem10.inf"
        );
        assert!(match_boot_driver(&drivers, "stor").is_err());
        assert!(match_boot_driver(&drivers, "missing").is_err());
    }

    #[test]
    fn report_contract_round_trips_and_validates() {
        let report = ClientReport {
            image_revision: Some("rev-9".into()),
            boot_nic_id: Some("PCI\\VEN_8086&DEV_15F3".into()),
            client_id: "client-1".into(),
            windows_version: "Windows 11 23H2".into(),
            architecture: "x64".into(),
            boot_nic_ids: vec!["PCI\\VEN_8086&DEV_15F3".into()],
        };
        report.validate().unwrap();
        let json = serde_json::to_string(&report).unwrap();
        assert_eq!(serde_json::from_str::<ClientReport>(&json).unwrap(), report);
        let mut invalid = report.clone();
        invalid.boot_nic_ids.clear();
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn reporter_config_rejects_missing_identity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report.json");
        std::fs::write(&path, r#"{"server_url":"","client_id":""}"#).unwrap();
        assert!(ReporterConfig::load(&path).is_err());
        std::fs::write(
            &path,
            r#"{"server_url":"http://192.0.2.1:8080/","client_id":"c1"}"#,
        )
        .unwrap();
        let config = ReporterConfig::load(&path).unwrap();
        assert_eq!(
            config.report_endpoint().unwrap(),
            "http://192.0.2.1:8080/api/pxe/network-drivers/reports"
        );
    }

    #[test]
    fn os_info_parses_and_normalizes() {
        let (version, arch) = parse_os_info(
            r#"{"OsName":"Microsoft Windows 11 Pro","OArchitecture":"AMD64","WindowsVersion":"23H2"}"#,
        )
        .unwrap();
        assert!(version.contains("Windows 11"));
        assert_eq!(arch, "x64");
        assert_eq!(normalize_architecture("AMD64"), "x64");
        assert_eq!(normalize_architecture("ARM64"), "arm64");
        assert!(parse_os_info(r#"{"OsName":""}"#).is_err());
    }
}
