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
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

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
        let base = self.server_url.trim();
        anyhow::ensure!(
            !base.chars().any(char::is_whitespace)
                && !base.chars().any(char::is_control)
                && !base.contains('?')
                && !base.contains('#'),
            "reporter server_url must not contain whitespace, query parameters, or a fragment"
        );
        let rest = base
            .strip_prefix("http://")
            .or_else(|| base.strip_prefix("https://"))
            .context("reporter server_url must use http or https")?;
        let authority = rest.split('/').next().unwrap_or_default();
        anyhow::ensure!(!authority.is_empty(), "reporter server_url has no host");
        anyhow::ensure!(
            !authority.contains('@'),
            "reporter server_url must not contain credentials"
        );
        let base = base.trim_end_matches('/');
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
    #[serde(rename = "PnPDeviceID")]
    pub pnp_device_id: String,
    #[serde(rename = "HardwareIds", default)]
    pub hardware_ids: Vec<String>,
    #[serde(rename = "PublishedInf", default)]
    pub published_inf: String,
}

fn run_bounded(program: &str, args: &[&str]) -> Result<String> {
    run_with_timeout(program, args, COMMAND_TIMEOUT)
}

fn run_with_timeout(program: &str, args: &[&str], timeout: Duration) -> Result<String> {
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to execute {program}"))?;
    let mut stdout = child.stdout.take().context("child stdout unavailable")?;
    let mut stderr = child.stderr.take().context("child stderr unavailable")?;
    let stdout_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let stderr_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).map(|_| bytes)
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .context("failed waiting for child process")?
        {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("{program} exceeded the {timeout:?} timeout");
        }
        thread::sleep(Duration::from_millis(50));
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| anyhow::anyhow!("{program} stdout reader panicked"))??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| anyhow::anyhow!("{program} stderr reader panicked"))??;
    if !status.success() {
        bail!(
            "{program} failed: {}",
            String::from_utf8_lossy(&stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&stdout).into_owned())
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

/// Normalize a raw architecture token to the catalog spelling.
pub fn normalize_architecture(token: &str) -> String {
    match token.trim().to_ascii_lowercase().as_str() {
        "amd64" | "x86_64" | "x64" | "64-bit" => "x64".to_string(),
        "i386" | "i686" | "x86" | "32-bit" => "x86".to_string(),
        "aarch64" | "arm64" => "arm64".to_string(),
        other => other.to_string(),
    }
}

fn is_exportable_published_inf(inf_name: &str) -> bool {
    let inf_name = inf_name.to_ascii_lowercase();
    let Some(number) = inf_name
        .strip_prefix("oem")
        .and_then(|name| name.strip_suffix(".inf"))
    else {
        return false;
    };
    !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
}

/// Invoke the client reporting flow. Windows-only; every step runs as a
/// bounded subprocess so a hung tool cannot block boot.
pub fn run_report_client(config_path: &Path) -> Result<()> {
    anyhow::ensure!(
        cfg!(windows),
        "client reporting runs on a managed Windows client"
    );
    let config = ReporterConfig::load(config_path)?;
    let endpoint = config.report_endpoint()?;
    let inventory_json = run_bounded(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$sessions = @(Get-IscsiConnection | Select-Object -ExpandProperty InitiatorAddress -Unique); if ($sessions.Count -ne 1) { throw 'Expected one iSCSI boot source address' }; $ips = @(Get-NetIPAddress -IPAddress $sessions[0] -ErrorAction Stop); if ($ips.Count -ne 1) { throw 'Could not uniquely resolve iSCSI boot interface' }; $a = @(Get-NetAdapter -InterfaceIndex $ips[0].InterfaceIndex -Physical); if ($a.Count -ne 1) { throw 'Could not uniquely resolve iSCSI boot adapter' }; $d = Get-PnpDevice -InstanceId $a[0].PnPDeviceID -ErrorAction Stop; $ids = @((Get-PnpDeviceProperty -InstanceId $d.InstanceId -KeyName 'DEVPKEY_Device_HardwareIds' -ErrorAction Stop).Data); $signed = @(Get-CimInstance Win32_PnPSignedDriver | Where-Object { $_.DeviceID -eq $d.InstanceId } | Select-Object -ExpandProperty InfName -Unique); if ($signed.Count -gt 1) { throw 'Could not uniquely resolve Boot NIC published INF' }; $inf = if ($signed.Count -eq 1) { $signed[0] } else { '' }; $o = [pscustomobject]@{Name=$a[0].Name;MacAddress=$a[0].MacAddress;InterfaceDescription=$a[0].InterfaceDescription;Status=$a[0].Status;ServiceName=$a[0].ServiceName;PnPDeviceID=$d.InstanceId;HardwareIds=[string[]]$ids;PublishedInf=$inf}; $o | ConvertTo-Json -Depth 3 -Compress",
        ],
    )?;
    let mut adapters = parse_net_adapters(&inventory_json)?;
    anyhow::ensure!(
        adapters.len() == 1,
        "iSCSI boot adapter inventory is ambiguous"
    );
    let boot = adapters
        .pop()
        .context("iSCSI boot adapter was not reported")?;
    anyhow::ensure!(
        !boot.hardware_ids.is_empty(),
        "Boot NIC has no PnP hardware IDs"
    );
    let os = run_bounded(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$os = Get-ComputerInfo -Property OsName,OsArchitecture,WindowsVersion | ConvertTo-Json -Compress; $os",
        ],
    )?;
    let (windows_version, architecture) = parse_os_info(&os)?;

    let report = ClientReport {
        client_id: config.client_id.clone(),
        windows_version,
        architecture,
        boot_nic_ids: boot.hardware_ids.clone(),
        image_revision: config
            .image_revision
            .clone()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        boot_nic_id: boot.hardware_ids.first().cloned(),
    };
    report.validate()?;

    let exported = if config.export_package && is_exportable_published_inf(&boot.published_inf) {
        Some(export_boot_driver(&boot.published_inf)?)
    } else {
        None
    };
    post_report(
        &endpoint,
        &report,
        exported.as_ref().map(|(_, path)| path.as_path()),
    )?;
    Ok(())
}

/// Parse `Get-ComputerInfo` JSON into `(windows_version, architecture)`.
pub fn parse_os_info(json: &str) -> Result<(String, String)> {
    #[derive(Deserialize)]
    struct OsInfo {
        #[serde(rename = "OsName", default)]
        os_name: String,
        #[serde(rename = "OsArchitecture", default)]
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
fn export_boot_driver(inf_name: &str) -> Result<(tempfile::TempDir, PathBuf)> {
    anyhow::ensure!(
        is_exportable_published_inf(inf_name),
        "Boot NIC INF is not an exportable third-party package"
    );
    let staging = tempfile::tempdir().context("failed to create driver export directory")?;
    let target = staging.path().join("export");
    std::fs::create_dir_all(&target)?;
    run_bounded(
        "pnputil",
        &[
            "/export-driver",
            inf_name,
            target
                .to_str()
                .context("driver export path is not valid Unicode")?,
        ],
    )
    .with_context(|| format!("failed to export driver package {inf_name}"))?;
    let archive = staging.path().join("driver.zip");
    run_bounded(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!(
                "Compress-Archive -Path '{}/*' -DestinationPath '{}' -Force",
                target.display().to_string().replace('\'', "''"),
                archive.display().to_string().replace('\'', "''")
            ),
        ],
    )
    .context("failed to archive the exported driver package")?;
    Ok((staging, archive))
}

/// POST the multipart report. Archive bytes travel in the request body only;
/// nothing about the package or configuration is logged.
fn post_report(endpoint: &str, report: &ClientReport, archive: Option<&Path>) -> Result<()> {
    let boundary = "diskless-client-report";
    let temp = tempfile::tempdir().context("failed to create report upload staging directory")?;
    let body_path = temp.path().join("request.multipart");
    let mut body = std::fs::File::create(&body_path)?;
    write_multipart_body(&mut body, report, archive, boundary)?;
    drop(body);

    let response = run_bounded(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!(
                "$response = Invoke-WebRequest -Uri '{}' -Method Post -ContentType 'multipart/form-data; boundary={}' -InFile '{}' -UseBasicParsing -TimeoutSec {}; if ($response.StatusCode -ge 400) {{ throw \"report rejected: $($response.StatusCode)\" }}",
                endpoint.replace('\'', "''"),
                boundary,
                body_path.display().to_string().replace('\'', "''"),
                COMMAND_TIMEOUT.as_secs(),
            ),
        ],
    );
    response.map(|_| ())
}

fn write_multipart_body(
    mut body: &mut impl Write,
    report: &ClientReport,
    archive: Option<&Path>,
    boundary: &str,
) -> Result<()> {
    let part = |name: &str, filename: Option<&str>, content_type: &str| {
        let mut header = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"");
        if let Some(filename) = filename {
            header.push_str(&format!("; filename=\"{filename}\""));
        }
        header.push_str(&format!("\r\nContent-Type: {content_type}\r\n\r\n"));
        header
    };
    body.write_all(part("report", None, "application/json").as_bytes())?;
    serde_json::to_writer(&mut body, report)?;
    body.write_all(b"\r\n")?;
    if let Some(archive) = archive {
        body.write_all(part("file", Some("driver.zip"), "application/zip").as_bytes())?;
        std::io::copy(&mut std::fs::File::open(archive)?, &mut body)
            .context("failed to stream the exported driver archive")?;
        body.write_all(b"\r\n")?;
    }
    write!(body, "--{boundary}--\r\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_and_pnp_inventory_parse_single_and_multiple() {
        let single = r#"{"Name":"Ethernet","MacAddress":"02-00-00-00-00-01","InterfaceDescription":"Intel NIC","Status":"Up","ServiceName":"e1dexpress","PnPDeviceID":"PCI\\VEN_8086&DEV_15F3","HardwareIds":["PCI\\VEN_8086&DEV_15F3","PCI\\VEN_8086"],"PublishedInf":"oem10.inf"}"#;
        assert_eq!(parse_net_adapters(single).unwrap().len(), 1);
        let array = format!("[{single},{single}]");
        assert_eq!(parse_net_adapters(&array).unwrap().len(), 2);
        assert!(parse_net_adapters("").unwrap().is_empty());
        let adapter = parse_net_adapters(single).unwrap().remove(0);
        assert_eq!(
            adapter.hardware_ids,
            ["PCI\\VEN_8086&DEV_15F3", "PCI\\VEN_8086"]
        );
        assert_eq!(adapter.published_inf, "oem10.inf");
    }

    #[test]
    fn only_published_oem_inf_names_are_exportable() {
        assert!(is_exportable_published_inf("oem10.inf"));
        assert!(is_exportable_published_inf("OEM10.INF"));
        assert!(!is_exportable_published_inf("nete1.inf"));
        assert!(!is_exportable_published_inf(""));
        assert!(!is_exportable_published_inf("oem.inf"));
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
    fn reporter_rejects_credentials_and_non_base_urls() {
        let config = |server_url: &str| ReporterConfig {
            server_url: server_url.into(),
            client_id: "client-1".into(),
            export_package: true,
            image_revision: None,
        };
        assert!(config("https://user:secret@example.test")
            .report_endpoint()
            .is_err());
        assert!(config("https://example.test/?token=secret")
            .report_endpoint()
            .is_err());
        assert!(config("https://example.test/#secret")
            .report_endpoint()
            .is_err());
        assert!(config("https://example.test\n'; exit 1; '")
            .report_endpoint()
            .is_err());
    }

    #[test]
    fn multipart_upload_streams_archive_bytes_in_the_file_part() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("driver.zip");
        std::fs::write(&archive, b"zip-data").unwrap();
        let report = ClientReport {
            client_id: "client-1".into(),
            windows_version: "Windows 11 23H2".into(),
            architecture: "x64".into(),
            boot_nic_ids: vec!["PCI\\VEN_8086&DEV_15F3".into()],
            image_revision: None,
            boot_nic_id: None,
        };
        let mut body = Vec::new();
        write_multipart_body(&mut body, &report, Some(&archive), "boundary").unwrap();
        let body = String::from_utf8(body).unwrap();
        assert!(body.contains("name=\"report\""));
        assert!(body.contains("name=\"file\"; filename=\"driver.zip\""));
        assert!(body.contains("zip-data"));
        assert!(body.ends_with("--boundary--\r\n"));
    }

    #[test]
    fn os_info_parses_and_normalizes() {
        let (version, arch) = parse_os_info(
            r#"{"OsName":"Microsoft Windows 11 Pro","OsArchitecture":"64-bit","WindowsVersion":"23H2"}"#,
        )
        .unwrap();
        assert!(version.contains("Windows 11"));
        assert_eq!(arch, "x64");
        assert_eq!(normalize_architecture("AMD64"), "x64");
        assert_eq!(normalize_architecture("ARM64"), "arm64");
        assert!(parse_os_info(r#"{"OsName":""}"#).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn child_process_timeout_kills_a_hung_command() {
        let started = Instant::now();
        let error = run_with_timeout("sleep", &["5"], Duration::from_millis(100)).unwrap_err();
        assert!(error.to_string().contains("timeout"));
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
