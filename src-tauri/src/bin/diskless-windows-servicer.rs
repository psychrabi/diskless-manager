//! Headless Windows servicing helper for Diskless Manager.
//!
//! Protocol:
//!   diskless-windows-servicer prepare
//!   < WindowsImagePreparationRequest JSON on stdin
//!   > WindowsServicerResponse JSON on stdout
//!
//! The helper intentionally has no network listener. It is meant to be invoked
//! locally by an administrator or remotely through the server's authenticated SSH
//! transport.

use app_lib::infrastructure::pxe::{
    windows_servicing_available, WindowsImagePreparationRequest, WindowsImagePreparationResult,
    WindowsImagePreparer,
};
use serde::Serialize;
use std::io::{Read, Write};

const PROTOCOL_VERSION: u32 = 1;
const MAX_REQUEST_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Serialize)]
struct WindowsServicerCapabilities {
    protocol_version: u32,
    platform: &'static str,
    available: bool,
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
        })
        .map(|_| 0)
        .unwrap_or_else(report_output_error),
        Some("prepare") => match run_prepare() {
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
        Some(other) => {
            eprintln!("unknown command: {other}");
            eprintln!("usage: diskless-windows-servicer <capabilities|prepare>");
            2
        }
        None => {
            eprintln!("usage: diskless-windows-servicer <capabilities|prepare>");
            2
        }
    };

    std::process::exit(exit_code);
}

fn run_prepare() -> anyhow::Result<WindowsImagePreparationResult> {
    let mut stdin = std::io::stdin().take(MAX_REQUEST_BYTES + 1);
    let mut payload = Vec::new();
    stdin.read_to_end(&mut payload)?;
    if payload.len() as u64 > MAX_REQUEST_BYTES {
        anyhow::bail!("servicing request exceeds {} bytes", MAX_REQUEST_BYTES);
    }
    if payload.is_empty() {
        anyhow::bail!("servicing request JSON is required on stdin");
    }

    let request: WindowsImagePreparationRequest =
        serde_json::from_slice(&payload).map_err(|error| {
            anyhow::anyhow!("invalid WindowsImagePreparationRequest JSON: {error}")
        })?;

    WindowsImagePreparer::new()?.prepare(request)
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
