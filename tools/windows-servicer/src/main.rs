//! Windows servicing helper for Diskless Manager.
//!
//! Protocol:
//!   diskless-windows-servicer.exe capabilities
//!   diskless-windows-servicer.exe version
//!   diskless-windows-servicer.exe prepare
//!   diskless-windows-servicer.exe report-client [config] [--retry]
//!   diskless-windows-servicer.exe collect-nic <report-config>
//!   diskless-windows-servicer.exe collect-drivers <dest>
//!   diskless-windows-servicer.exe install-nic-drivers <driver-root>
//!
//! Run with `help` (or no arguments off Windows) for the full command list.
//!   diskless-windows-servicer.exe list-boot-nics
//!   diskless-windows-servicer.exe prepare-client  (JSON on stdin; preview by default)
//!
//! The prepare command reads WindowsImagePreparationRequest JSON from stdin and
//! writes a versioned JSON response to stdout.

#![cfg_attr(all(windows, feature = "gui"), windows_subsystem = "windows")]

#[cfg(all(windows, feature = "gui"))]
mod gui;
#[path = "../../../src-tauri/src/infrastructure/pxe/windows_boot_arming.rs"]
mod windows_boot_arming;
mod windows_client_report;
#[path = "../../../src-tauri/src/infrastructure/pxe/windows_driver_injection.rs"]
mod windows_driver_injection;
mod windows_gpt_identity;
#[path = "../../../src-tauri/src/infrastructure/pxe/windows_inf.rs"]
mod windows_inf;
#[path = "../../../src-tauri/src/infrastructure/pxe/windows_iscsi_servicing.rs"]
mod windows_iscsi_servicing;
mod windows_local_boot;
#[path = "../../../src-tauri/src/infrastructure/pxe/windows_nexboot_adaptation.rs"]
mod windows_nexboot_adaptation;
#[path = "../../../src-tauri/src/infrastructure/pxe/windows_personalization.rs"]
mod windows_personalization;
#[path = "../../../src-tauri/src/infrastructure/pxe/windows_volume_safety.rs"]
mod windows_volume_safety;

pub use windows_boot_arming::{WindowsBootArmConfig, WindowsBootArmResult, WindowsBootArmer};
pub use windows_driver_injection::WindowsDriverInjector;

#[path = "../../../src-tauri/src/infrastructure/pxe/windows_image_preparation.rs"]
mod windows_image_preparation;

use serde::Serialize;
use std::io::{Read, Write};
use windows_image_preparation::{
    servicing_available as windows_servicing_available, WindowsImagePreparationRequest,
    WindowsImagePreparationResult, WindowsImagePreparer,
};

const PROTOCOL_VERSION: u32 = 5;
/// Deployed helper version, surfaced through `capabilities` and the `version`
/// verb so an administrator can compare installations. Updates flow through
/// re-running `install-client.ps1`; the helper never downloads executables
/// from the server.
pub const HELPER_VERSION: &str = env!("CARGO_PKG_VERSION");
const MAX_REQUEST_BYTES: u64 = 1024 * 1024;
/// Single source for command help so the two error paths and `help` cannot
/// drift apart.
const USAGE: &str = "\
usage: diskless-windows-servicer <command> [args]

commands:
  capabilities                       print the capability JSON
  version                            print the helper version JSON
  prepare                            prepare a Windows image (JSON on stdin)
  prepare-volume | adapt-volume      service a job-owned volume (JSON on stdin)
  check-volume | check-volume-local  volume safety checks (JSON on stdin)
  list-boot-nics                     list local network adapters as JSON
  prepare-client                     prepare this client (JSON on stdin)
  report-client [config] [--retry]   upload the session-bound client report
  collect-nic <config>               collect and upload all local NIC packages
  collect-drivers <dest>             export every third-party driver package
  install-nic-drivers <root>         install the INF packages under <root>
  report-service [config]            run the boot-time reporter service

Run without arguments on a Windows client to open the worker GUI.";

/// Capability payload reported to the server and displayed by the GUI. The GUI
/// reuses this constructor so the two surfaces cannot drift.
#[derive(Debug, Serialize)]
pub struct WindowsServicerCapabilities {
    pub protocol_version: u32,
    pub version: &'static str,
    pub platform: &'static str,
    pub available: bool,
    pub elevated: bool,
    pub iscsi_volume_servicing: bool,
    pub volume_safety: bool,
    pub nexboot_boot_time: bool,
    pub nic_collection: bool,
    pub local_nic_installation: bool,
    pub personalization: bool,
    pub local_boot_preparation: bool,
}

pub fn capabilities() -> WindowsServicerCapabilities {
    WindowsServicerCapabilities {
        protocol_version: PROTOCOL_VERSION,
        version: HELPER_VERSION,
        platform: std::env::consts::OS,
        available: windows_servicing_available(),
        elevated: is_elevated(),
        iscsi_volume_servicing: cfg!(windows),
        volume_safety: cfg!(windows),
        nexboot_boot_time: cfg!(windows),
        nic_collection: cfg!(windows),
        local_nic_installation: cfg!(windows),
        personalization: cfg!(windows),
        local_boot_preparation: cfg!(windows),
    }
}

#[derive(Debug, Serialize)]
struct WindowsServicerResponse {
    protocol_version: u32,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<WindowsImagePreparationResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn main() {
    let exit_code = match std::env::args().nth(1).as_deref() {
        Some(command @ ("list-boot-nics" | "prepare-client")) => {
            let result = (|| -> anyhow::Result<serde_json::Value> {
                anyhow::ensure!(
                    std::env::args().len() == 2,
                    "{command} accepts no arguments; prepare-client reads JSON from stdin"
                );
                if command == "list-boot-nics" {
                    Ok(serde_json::to_value(windows_local_boot::list_adapters()?)?)
                } else {
                    let request = serde_json::from_slice(&read_request()?)?;
                    Ok(serde_json::to_value(windows_local_boot::prepare(
                        &request,
                    )?)?)
                }
            })();
            let (code, response) = match result {
                Ok(result) => (0, serde_json::json!({"ok": true, "result": result})),
                Err(error) => (
                    1,
                    serde_json::json!({"ok": false, "error": format!("{error:#}")}),
                ),
            };
            write_json(&response)
                .map(|_| code)
                .unwrap_or_else(report_output_error)
        }
        Some("capabilities") => write_json(&capabilities())
            .map(|_| 0)
            .unwrap_or_else(report_output_error),
        Some("version") => write_json(&serde_json::json!({
            "ok": true,
            "version": HELPER_VERSION,
        }))
        .map(|_| 0)
        .unwrap_or_else(report_output_error),
        Some("help" | "--help" | "-h") => {
            println!("{USAGE}");
            0
        }
        Some(command @ ("prepare" | "prepare-volume" | "adapt-volume")) => {
            match run_prepare(command) {
                Ok(result) => write_json(&WindowsServicerResponse {
                    protocol_version: PROTOCOL_VERSION,
                    ok: true,
                    result: Some(result),
                    error: None,
                })
                .map(|_| 0)
                .unwrap_or_else(report_output_error),
                Err(error) => {
                    let _ = write_json(&WindowsServicerResponse {
                        protocol_version: PROTOCOL_VERSION,
                        ok: false,
                        result: None,
                        error: Some(format!("{error:#}")),
                    });
                    1
                }
            }
        }
        Some(command @ ("check-volume" | "check-volume-local")) => {
            let result =
                (|| -> anyhow::Result<windows_volume_safety::WindowsVolumeSafetyResult> {
                    anyhow::ensure!(
                        cfg!(windows) && is_elevated(),
                        "volume safety requires an elevated Windows worker"
                    );
                    let payload = read_request()?;
                    if command == "check-volume" {
                        let connection: windows_iscsi_servicing::WindowsIscsiConnection =
                            serde_json::from_slice(&payload)?;
                        windows_iscsi_servicing::check_volume(&connection)
                    } else {
                        #[derive(serde::Deserialize)]
                        struct Local {
                            image_path: std::path::PathBuf,
                        }
                        let request: Local = serde_json::from_slice(&payload)?;
                        windows_volume_safety::check_offline_volume(&request.image_path)
                    }
                })();
            let response = match result {
                Ok(result) => {
                    serde_json::json!({"protocol_version": PROTOCOL_VERSION, "ok": true, "result": result})
                }
                Err(error) => {
                    serde_json::json!({"protocol_version": PROTOCOL_VERSION, "ok": false, "error": format!("{error:#}")})
                }
            };
            let code = if response["ok"] == true { 0 } else { 1 };
            write_json(&response)
                .map(|_| code)
                .unwrap_or_else(report_output_error)
        }
        Some(command @ ("gpt-capture" | "gpt-restore")) => {
            let response = match run_gpt(command) {
                Ok(identity) => serde_json::json!({"ok": true, "result": identity}),
                Err(error) => serde_json::json!({"ok": false, "error": format!("{error:#}")}),
            };
            let code = if response["ok"] == true { 0 } else { 1 };
            write_json(&response)
                .map(|_| code)
                .unwrap_or_else(report_output_error)
        }
        Some("report-client") => {
            let args = std::env::args().skip(2).collect::<Vec<_>>();
            let run = parse_report_client_args(&args).and_then(|(config, retry)| {
                let path = std::path::Path::new(&config);
                if retry {
                    windows_client_report::run_report_client_with_retry(
                        path,
                        windows_client_report::REPORT_ATTEMPTS,
                        windows_client_report::REPORT_RETRY_DELAY,
                    )
                } else {
                    windows_client_report::run_report_client(path)
                }
            });
            match run {
                Ok(()) => write_json(&serde_json::json!({"ok": true}))
                    .map(|_| 0)
                    .unwrap_or_else(report_output_error),
                Err(error) => {
                    let _ = write_json(&serde_json::json!({
                        "ok": false,
                        "error": format!("{error:#}"),
                    }));
                    1
                }
            }
        }
        Some("collect-nic") => {
            let config = std::env::args().nth(2).unwrap_or_else(|| {
                r"C:\ProgramData\Diskless Manager\client-report.json".to_string()
            });
            match windows_client_report::run_collect_nic(std::path::Path::new(&config)) {
                Ok(()) => write_json(&serde_json::json!({"ok": true, "operation": "collect-nic"}))
                    .map(|_| 0)
                    .unwrap_or_else(report_output_error),
                Err(error) => {
                    let _ = write_json(
                        &serde_json::json!({"ok": false, "error": format!("{error:#}")}),
                    );
                    1
                }
            }
        }
        Some("install-nic-drivers") => {
            let root = std::env::args()
                .nth(2)
                .unwrap_or_else(|| r"C:\ProgramData\Diskless Manager\Drivers".to_string());
            match windows_client_report::install_nic_drivers(std::path::Path::new(&root)) {
                Ok(result) => write_json(&serde_json::json!({
                    "ok": true,
                    "operation": "install-nic-drivers",
                    "inf_count": result.inf_count,
                    "installed": result.installed_count(),
                    "no_matching_device": result.no_matching_device_count(),
                    "reboot_required": result.reboot_required(),
                    "packages": result.packages,
                }))
                .map(|_| 0)
                .unwrap_or_else(report_output_error),
                Err(error) => {
                    let _ = write_json(
                        &serde_json::json!({"ok": false, "error": format!("{error:#}")}),
                    );
                    1
                }
            }
        }
        Some("report-service") => {
            let args = std::env::args().skip(2).collect::<Vec<_>>();
            match parse_report_service_config(&args).and_then(start_report_service) {
                Ok(()) => 0,
                Err(error) => {
                    eprintln!("report service failed: {error:#}");
                    1
                }
            }
        }
        Some("collect-drivers") => {
            let dest = std::env::args()
                .nth(2)
                .unwrap_or_else(|| r"C:\ProgramData\Diskless Manager\Drivers".to_string());
            match windows_client_report::collect_all_drivers(std::path::Path::new(&dest)) {
                Ok(count) => write_json(&serde_json::json!({
                    "ok": true,
                    "operation": "collect-drivers",
                    "destination": dest,
                    "package_count": count,
                }))
                .map(|_| 0)
                .unwrap_or_else(report_output_error),
                Err(error) => {
                    let _ = write_json(
                        &serde_json::json!({"ok": false, "error": format!("{error:#}")}),
                    );
                    1
                }
            }
        }
        Some(other) => {
            eprintln!("unknown command: {other}");
            eprintln!("{USAGE}");
            2
        }
        None if cfg!(all(windows, feature = "gui")) => {
            #[cfg(all(windows, feature = "gui"))]
            {
                gui::run().map(|_| 0).unwrap_or_else(|error| {
                    eprintln!("GUI failed: {error:#}");
                    1
                })
            }
            #[cfg(not(all(windows, feature = "gui")))]
            unreachable!();
        }
        None => {
            eprintln!("{USAGE}");
            2
        }
    };

    std::process::exit(exit_code);
}

const DEFAULT_REPORT_CONFIG: &str = r"C:\ProgramData\Diskless Manager\client-report.json";

fn parse_report_service_config(args: &[String]) -> anyhow::Result<std::path::PathBuf> {
    anyhow::ensure!(
        args.len() <= 1,
        "report-service accepts at most one config path"
    );
    Ok(args
        .first()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(DEFAULT_REPORT_CONFIG)))
}

fn parse_report_client_args(args: &[String]) -> anyhow::Result<(std::path::PathBuf, bool)> {
    let mut config: Option<std::path::PathBuf> = None;
    let mut retry = false;
    for arg in args {
        match arg.as_str() {
            "--retry" if !retry => retry = true,
            value if config.is_none() && !value.starts_with("--") => {
                config = Some(std::path::PathBuf::from(value));
            }
            _ => anyhow::bail!("report-client accepts [config-path] [--retry]"),
        }
    }
    Ok((
        config.unwrap_or_else(|| std::path::PathBuf::from(DEFAULT_REPORT_CONFIG)),
        retry,
    ))
}

fn start_report_service(config_path: std::path::PathBuf) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        windows_report_service::run(config_path)
    }
    #[cfg(not(windows))]
    {
        let _ = config_path;
        anyhow::bail!("report-service requires Windows")
    }
}

fn read_request() -> anyhow::Result<Vec<u8>> {
    let mut stdin = std::io::stdin().take(MAX_REQUEST_BYTES + 1);
    let mut payload = Vec::new();
    stdin.read_to_end(&mut payload)?;
    if payload.len() as u64 > MAX_REQUEST_BYTES {
        anyhow::bail!("servicing request exceeds {} bytes", MAX_REQUEST_BYTES);
    }
    if payload.is_empty() {
        anyhow::bail!("servicing request JSON is required on stdin");
    }
    Ok(payload)
}

fn run_gpt(command: &str) -> anyhow::Result<windows_gpt_identity::GptIdentity> {
    anyhow::ensure!(
        cfg!(windows) && is_elevated(),
        "GPT servicing requires an elevated Windows worker"
    );
    #[derive(serde::Deserialize)]
    struct Request {
        disk_number: u32,
        size_bytes: u64,
        sector_size: u64,
        #[serde(default)]
        identity: Option<windows_gpt_identity::GptIdentity>,
    }
    let request: Request = serde_json::from_slice(&read_request()?)?;
    anyhow::ensure!(request.disk_number <= 65535, "invalid physical disk number");
    let mut disk = std::fs::OpenOptions::new()
        .read(true)
        .write(command == "gpt-restore")
        .open(format!(r"\\.\PhysicalDrive{}", request.disk_number))?;
    if command == "gpt-restore" {
        let saved = request
            .identity
            .ok_or_else(|| anyhow::anyhow!("saved GPT identity is required"))?;
        anyhow::ensure!(
            saved.size_bytes == request.size_bytes && saved.sector_size == request.sector_size,
            "GPT geometry does not match saved identity"
        );
        windows_gpt_identity::restore(&mut disk, &saved)?;
        disk.sync_all()?;
    }
    windows_gpt_identity::capture(&mut disk, request.size_bytes, request.sector_size)
}

fn run_prepare(command: &str) -> anyhow::Result<WindowsImagePreparationResult> {
    let payload = read_request()?;

    let request: WindowsImagePreparationRequest = serde_json::from_slice(&payload)
        .map_err(|error| anyhow::anyhow!("invalid WindowsImagePreparationRequest JSON: {error}"))?;

    let preparer = WindowsImagePreparer::new()?;
    if matches!(command, "prepare-volume" | "adapt-volume") {
        preparer.prepare_volume(request, command == "adapt-volume")
    } else {
        preparer.prepare(request)
    }
}

fn write_json(value: &impl Serialize) -> std::io::Result<()> {
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    serde_json::to_writer(&mut output, value).map_err(std::io::Error::other)?;
    output.write_all(b"\n")?;
    output.flush()
}

fn report_output_error(error: std::io::Error) -> i32 {
    eprintln!("failed to write response: {error}");
    1
}

fn is_elevated() -> bool {
    #[cfg(windows)]
    {
        let output = match std::process::Command::new("whoami.exe")
            .args(["/groups", "/fo", "csv", "/nh"])
            .output()
        {
            Ok(output) if output.status.success() => output,
            _ => return false,
        };

        let groups = String::from_utf8_lossy(&output.stdout).to_ascii_uppercase();
        // High Mandatory Level and System Mandatory Level respectively.
        groups.contains("S-1-16-12288") || groups.contains("S-1-16-16384")
    }

    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
mod windows_report_service;

#[cfg(test)]
mod report_service_tests {
    use super::*;

    #[test]
    fn report_service_accepts_zero_or_one_config_path() {
        assert_eq!(
            parse_report_service_config(&[]).unwrap(),
            std::path::PathBuf::from(DEFAULT_REPORT_CONFIG)
        );
        assert_eq!(
            parse_report_service_config(&["custom.json".to_string()]).unwrap(),
            std::path::PathBuf::from("custom.json")
        );
    }

    #[test]
    fn report_service_rejects_extra_arguments() {
        assert!(parse_report_service_config(&["a".into(), "b".into()]).is_err());
    }

    /// The GUI must display the shared capability payload instead of a
    /// hand-built stale copy (it once reported protocol version 3).
    #[test]
    fn gui_capabilities_come_from_the_shared_helper() {
        let source = include_str!("gui.rs");
        assert!(source.contains("crate::capabilities()"));
        assert!(!source.contains("protocol_version"));
    }

    /// The GUI must save the complete ReporterConfig contract instead of a
    /// hand-built JSON subset (it once reset export_package to false) and must
    /// load the on-disk configuration at startup.
    #[test]
    fn gui_uses_the_reporter_config_contract() {
        let source = include_str!("gui.rs");
        assert!(source.contains("ReporterConfig {"));
        assert!(source.contains("ReporterConfig::load"));
        assert!(!source.contains(r#""export_package""#));
    }

    #[test]
    fn report_client_accepts_optional_config_and_retry_flag() {
        assert_eq!(
            parse_report_client_args(&[]).unwrap(),
            (std::path::PathBuf::from(DEFAULT_REPORT_CONFIG), false)
        );
        assert_eq!(
            parse_report_client_args(&["custom.json".to_string()]).unwrap(),
            (std::path::PathBuf::from("custom.json"), false)
        );
        assert_eq!(
            parse_report_client_args(&["--retry".to_string()]).unwrap(),
            (std::path::PathBuf::from(DEFAULT_REPORT_CONFIG), true)
        );
        assert_eq!(
            parse_report_client_args(&["a".into(), "--retry".into()]).unwrap(),
            (std::path::PathBuf::from("a"), true)
        );
        assert_eq!(
            parse_report_client_args(&["--retry".into(), "a".into()]).unwrap(),
            (std::path::PathBuf::from("a"), true)
        );
        assert!(parse_report_client_args(&["a".into(), "b".into()]).is_err());
        assert!(parse_report_client_args(&["--rety".into()]).is_err());
    }
}
