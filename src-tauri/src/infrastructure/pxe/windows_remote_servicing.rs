//! Remote Windows servicing transport.
//!
//! The Linux Diskless Manager server can invoke the dedicated Windows servicing
//! helper over SSH. Structured requests travel on stdin, not in the shell command
//! line, so image paths and policy fields do not require shell escaping.

use super::{WindowsImagePreparationRequest, WindowsImagePreparationResult};
use crate::ssh_executor::{SshConfig, SshExecutor};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

const PROTOCOL_VERSION: u32 = 1;
const DEFAULT_HELPER: &str =
    r"C:\Program Files\Diskless Manager\diskless-windows-servicer.exe";

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteWindowsServicingRequest {
    pub host: String,
    pub username: String,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub executable_path: Option<String>,
    pub preparation: WindowsImagePreparationRequest,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteWindowsCapabilitiesRequest {
    pub host: String,
    pub username: String,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub executable_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteWindowsServicingCapabilities {
    pub protocol_version: u32,
    pub platform: String,
    pub available: bool,
}

#[derive(Debug, Clone)]
pub struct StagedDriverPackage {
    pub id: String,
    pub source_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteWindowsCatalogPreparationResult {
    pub preparation: WindowsImagePreparationResult,
    pub staged_package_count: usize,
    pub uploaded_bytes: u64,
    pub staging_cleanup_succeeded: bool,
}

#[derive(Debug, Deserialize)]
struct WindowsServicerResponse {
    protocol_version: u32,
    ok: bool,
    #[serde(default)]
    result: Option<WindowsImagePreparationResult>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RemoteWindowsServicer {
    connection_timeout: u64,
    command_timeout: u64,
}

impl Default for RemoteWindowsServicer {
    fn default() -> Self {
        Self {
            connection_timeout: 10,
            command_timeout: 60 * 60,
        }
    }
}

impl RemoteWindowsServicer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_timeouts(connection_timeout: u64, command_timeout: u64) -> Self {
        Self {
            connection_timeout,
            command_timeout,
        }
    }

    pub async fn capabilities(
        &self,
        request: RemoteWindowsCapabilitiesRequest,
    ) -> Result<RemoteWindowsServicingCapabilities> {
        validate_remote_identity(&request.host, &request.username)?;
        let executable = validate_helper_path(
            request
                .executable_path
                .as_deref()
                .unwrap_or(DEFAULT_HELPER),
        )?;
        let executor = self.executor(request.username, request.password);
        let command = powershell_helper_command(&executable, "capabilities");

        let output = executor
            .execute_command(&request.host, &command)
            .await
            .map_err(|error| anyhow::anyhow!("remote Windows servicing capability check failed: {error}"))?;

        if output.exit_code != 0 {
            bail!(
                "remote Windows servicing helper exited with {}: {}",
                output.exit_code,
                output.stderr.trim()
            );
        }

        let capabilities: RemoteWindowsServicingCapabilities =
            serde_json::from_str(output.stdout.trim())
                .context("remote Windows servicing helper returned invalid capability JSON")?;
        validate_protocol(capabilities.protocol_version)?;
        Ok(capabilities)
    }

    pub async fn prepare(
        &self,
        request: RemoteWindowsServicingRequest,
    ) -> Result<WindowsImagePreparationResult> {
        validate_remote_identity(&request.host, &request.username)?;
        let executable = validate_helper_path(
            request
                .executable_path
                .as_deref()
                .unwrap_or(DEFAULT_HELPER),
        )?;
        let payload = serde_json::to_vec(&request.preparation)
            .context("failed to serialize Windows servicing request")?;

        let executor = self.executor(request.username, request.password);
        let command = powershell_helper_command(&executable, "prepare");
        let output = executor
            .execute_command_with_input(&request.host, &command, &payload)
            .await
            .map_err(|error| anyhow::anyhow!("remote Windows image preparation failed: {error}"))?;

        let response: WindowsServicerResponse = serde_json::from_str(output.stdout.trim())
            .with_context(|| {
                format!(
                    "remote Windows servicing helper returned invalid JSON (exit {}): {}",
                    output.exit_code,
                    output.stdout.trim()
                )
            })?;
        validate_protocol(response.protocol_version)?;

        if output.exit_code != 0 || !response.ok {
            bail!(
                "{}",
                response
                    .error
                    .unwrap_or_else(|| format!("remote helper exited with {}", output.exit_code))
            );
        }

        response
            .result
            .ok_or_else(|| anyhow::anyhow!("remote helper reported success without a result"))
    }

    pub async fn prepare_with_driver_packages(
        &self,
        host: String,
        username: String,
        password: Option<String>,
        executable_path: Option<String>,
        mut preparation: WindowsImagePreparationRequest,
        packages: Vec<StagedDriverPackage>,
    ) -> Result<RemoteWindowsCatalogPreparationResult> {
        validate_remote_identity(&host, &username)?;
        if packages.is_empty() {
            bail!("at least one imported network driver package is required");
        }

        let staging_id = Uuid::new_v4().to_string();
        let remote_base =
            format!(r"C:\ProgramData\Diskless Manager\staging\{staging_id}");
        let remote_driver_root = format!(r"{remote_base}\drivers");
        let remote_driver_root_sftp = remote_driver_root.replace('\\', "/");

        let executor = self.executor(username.clone(), password.clone());
        let create_command = powershell_statement(&format!(
            "[void](New-Item -ItemType Directory -Force -LiteralPath {})",
            powershell_literal(&remote_driver_root)
        ));
        let created = executor
            .execute_command(&host, &create_command)
            .await
            .map_err(|error| anyhow::anyhow!("failed to create remote driver staging directory: {error}"))?;
        if created.exit_code != 0 {
            bail!(
                "failed to create remote driver staging directory: {}",
                created.stderr.trim()
            );
        }

        let staging_result = async {
            let mut uploaded_bytes = 0u64;
            for package in &packages {
                let remote_package_root =
                    format!("{}/{}", remote_driver_root_sftp.trim_end_matches('/'), package.id);
                uploaded_bytes += executor
                    .upload_directory(&host, &package.source_dir, &remote_package_root)
                    .await
                    .map_err(|error| {
                        anyhow::anyhow!(
                            "failed to stage network driver package '{}': {error}",
                            package.id
                        )
                    })?;
            }

            preparation.driver_root = PathBuf::from(&remote_driver_root);
            let preparation = self
                .prepare(RemoteWindowsServicingRequest {
                    host: host.clone(),
                    username: username.clone(),
                    password: password.clone(),
                    executable_path: executable_path.clone(),
                    preparation,
                })
                .await?;

            Ok::<_, anyhow::Error>((preparation, uploaded_bytes))
        }
        .await;

        let cleanup_command = powershell_statement(&format!(
            "Remove-Item -LiteralPath {} -Recurse -Force -ErrorAction SilentlyContinue",
            powershell_literal(&remote_base)
        ));
        let cleanup_succeeded = executor
            .execute_command(&host, &cleanup_command)
            .await
            .map(|result| result.exit_code == 0)
            .unwrap_or(false);

        match staging_result {
            Ok((preparation, uploaded_bytes)) => Ok(RemoteWindowsCatalogPreparationResult {
                preparation,
                staged_package_count: packages.len(),
                uploaded_bytes,
                staging_cleanup_succeeded: cleanup_succeeded,
            }),
            Err(error) if cleanup_succeeded => Err(error),
            Err(error) => Err(error.context(
                "remote driver staging cleanup also failed; inspect the Windows worker staging directory",
            )),
        }
    }

    fn executor(&self, username: String, password: Option<String>) -> SshExecutor {
        SshExecutor::with_config(SshConfig {
            connection_timeout: self.connection_timeout,
            command_timeout: self.command_timeout,
            username,
            password,
            disable_host_key_verification: false,
            max_retries: 0,
        })
    }
}

fn validate_protocol(version: u32) -> Result<()> {
    if version != PROTOCOL_VERSION {
        bail!(
            "unsupported Windows servicing protocol version {}; expected {}",
            version,
            PROTOCOL_VERSION
        );
    }
    Ok(())
}

fn validate_remote_identity(host: &str, username: &str) -> Result<()> {
    if host.trim().is_empty() {
        bail!("Windows servicing host is required");
    }
    if username.trim().is_empty() {
        bail!("Windows servicing username is required");
    }
    if host.chars().any(|ch| matches!(ch, '\r' | '\n' | '\0')) {
        bail!("Windows servicing host contains invalid characters");
    }
    Ok(())
}

fn validate_helper_path(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        bail!("Windows servicing helper path is required");
    }
    if value.chars().any(|ch| matches!(ch, '\r' | '\n' | '\0')) {
        bail!("Windows servicing helper path contains invalid characters");
    }
    if !value.to_ascii_lowercase().ends_with(".exe") {
        bail!("Windows servicing helper path must point to an .exe");
    }
    Ok(value.to_string())
}

fn powershell_helper_command(executable: &str, subcommand: &str) -> String {
    let escaped = executable.replace('\'', "''");
    format!(
        "powershell.exe -NoProfile -NonInteractive -Command \"& '{}' {}\"",
        escaped, subcommand
    )
}

fn powershell_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn powershell_statement(statement: &str) -> String {
    let escaped = statement.replace('"', "\\"");
    format!(
        "powershell.exe -NoProfile -NonInteractive -Command \"{}\"",
        escaped
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_path_is_safely_single_quoted_for_powershell() {
        let command =
            powershell_helper_command(r"C:\Program Files\Diskless Manager\worker's.exe", "prepare");
        assert!(command.contains("worker''s.exe"));
        assert!(command.ends_with(" prepare\""));
    }

    #[test]
    fn helper_path_must_be_an_executable() {
        assert!(validate_helper_path(r"C:\helper.ps1").is_err());
        assert!(validate_helper_path(r"C:\helper.exe").is_ok());
    }

    #[test]
    fn protocol_mismatch_is_rejected() {
        assert!(validate_protocol(PROTOCOL_VERSION + 1).is_err());
    }
}
