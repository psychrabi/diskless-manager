//! Windows servicing helper for Diskless Manager.
//!
//! Protocol:
//!   diskless-windows-servicer.exe capabilities
//!   diskless-windows-servicer.exe prepare
//!   diskless-windows-servicer.exe collect-nic <report-config>
//!   diskless-windows-servicer.exe install-nic-drivers <driver-root>
//!
//! The prepare command reads WindowsImagePreparationRequest JSON from stdin and
//! writes a versioned JSON response to stdout.

#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
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
#[path = "../../../src-tauri/src/infrastructure/pxe/windows_nexboot_adaptation.rs"]
mod windows_nexboot_adaptation;
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

const PROTOCOL_VERSION: u32 = 3;
const MAX_REQUEST_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Serialize)]
struct WindowsServicerCapabilities {
    protocol_version: u32,
    platform: &'static str,
    available: bool,
    elevated: bool,
    iscsi_volume_servicing: bool,
    volume_safety: bool,
    nexboot_boot_time: bool,
    nic_collection: bool,
    local_nic_installation: bool,
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
        Some("capabilities") => write_json(&WindowsServicerCapabilities {
            protocol_version: PROTOCOL_VERSION,
            platform: std::env::consts::OS,
            available: windows_servicing_available(),
            elevated: is_elevated(),
            iscsi_volume_servicing: cfg!(windows),
            volume_safety: cfg!(windows),
            nexboot_boot_time: cfg!(windows),
            nic_collection: cfg!(windows),
            local_nic_installation: cfg!(windows),
        })
        .map(|_| 0)
        .unwrap_or_else(report_output_error),
        Some(command @ ("prepare" | "prepare-volume")) => match run_prepare(command) {
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
        },
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
            let config = std::env::args().nth(2).unwrap_or_else(|| {
                r"C:\ProgramData\Diskless Manager\client-report.json".to_string()
            });
            match windows_client_report::run_report_client(std::path::Path::new(&config)) {
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
                    "reboot_required": result.reboot_required,
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
        Some(other) => {
            eprintln!("unknown command: {other}");
            eprintln!("usage: diskless-windows-servicer <capabilities|prepare|report-client|collect-nic|install-nic-drivers|report-service>");
            2
        }
        None if cfg!(windows) => {
            #[cfg(windows)]
            {
                gui::run().map(|_| 0).unwrap_or_else(|error| {
                    eprintln!("GUI failed: {error:#}");
                    1
                })
            }
            #[cfg(not(windows))]
            unreachable!();
        }
        None => {
            eprintln!("usage: diskless-windows-servicer <capabilities|prepare|report-client|collect-nic|install-nic-drivers|report-service>");
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
    if command == "prepare-volume" {
        preparer.prepare_volume(request)
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
}
