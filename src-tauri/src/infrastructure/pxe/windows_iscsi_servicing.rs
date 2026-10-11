//! Attach only a job-owned iSCSI clone and release it before returning success.
use super::{WindowsImagePreparationRequest, WindowsImagePreparationResult};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::process::{Command, Stdio};

pub const SERVICING_IQN_PREFIX: &str = "iqn.2024-01.com.diskless:servicing.";

#[derive(Clone, Serialize, Deserialize)]
pub struct WindowsIscsiConnection {
    pub portal: String,
    pub target_iqn: String,
    pub chap_username: String,
    pub chap_password: String,
    pub expected_size_bytes: u64,
}

impl std::fmt::Debug for WindowsIscsiConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowsIscsiConnection")
            .field("portal", &self.portal)
            .field("target_iqn", &self.target_iqn)
            .finish_non_exhaustive()
    }
}

impl WindowsIscsiConnection {
    pub fn validate(&self) -> Result<()> {
        self.portal
            .parse::<std::net::Ipv4Addr>()
            .context("servicing portal must be an IPv4 address")?;
        let job = self
            .target_iqn
            .strip_prefix(SERVICING_IQN_PREFIX)
            .context("only job-owned servicing targets may be attached")?;
        if job.len() != 36 || !job.bytes().all(|ch| ch.is_ascii_hexdigit() || ch == b'-') {
            bail!("invalid servicing target job identifier");
        }
        if self.chap_username.is_empty()
            || !self
                .chap_username
                .bytes()
                .all(|ch| ch.is_ascii_alphanumeric() || b"-_.".contains(&ch))
            || !(12..=16).contains(&self.chap_password.len())
            || !self
                .chap_password
                .bytes()
                .all(|ch| ch.is_ascii_alphanumeric())
        {
            bail!("invalid servicing CHAP credentials");
        }
        if self.expected_size_bytes == 0 {
            bail!("expected clone size is required");
        }
        Ok(())
    }
}

#[derive(Deserialize)]
struct Response {
    ok: bool,
    #[serde(default)]
    result: Option<WindowsImagePreparationResult>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    released: bool,
}

pub fn prepare(
    connection: &WindowsIscsiConnection,
    request: &WindowsImagePreparationRequest,
) -> Result<WindowsImagePreparationResult> {
    connection.validate()?;
    if !cfg!(windows) {
        bail!("iSCSI volume servicing must run on the Windows worker");
    }
    let temporary = tempfile::tempdir()?;
    let script = temporary.path().join("service-iscsi.ps1");
    std::fs::write(&script, include_str!("windows_iscsi_servicing.ps1"))?;
    let mut offline_request = request.clone();
    offline_request.iscsi = None;
    let payload = serde_json::to_vec(&serde_json::json!({
        "connection": connection, "preparation": offline_request,
        "executable": std::env::current_exe()?,
    }))?;
    let mut process = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    process
        .stdin
        .take()
        .context("worker stdin is unavailable")?
        .write_all(&payload)?;
    let output = process.wait_with_output()?;
    let response: Response = serde_json::from_slice(&output.stdout)
        .context("iSCSI worker returned invalid JSON; retain clone and check worker sessions")?;
    if !response.ok || !response.released || !output.status.success() {
        bail!(
            "{}",
            response
                .error
                .unwrap_or_else(|| "worker did not confirm disk release; retain clone".into())
        );
    }
    response
        .result
        .context("worker returned success without a preparation result")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refuses_client_targets_and_redacts_credentials() {
        let mut connection = WindowsIscsiConnection {
            portal: "192.168.123.1".into(),
            target_iqn: format!("{SERVICING_IQN_PREFIX}12345678-1234-1234-1234-123456789abc"),
            chap_username: "chap-servicing".into(),
            chap_password: "secret123456".into(),
            expected_size_bytes: 50 << 30,
        };
        assert!(connection.validate().is_ok());
        assert!(!format!("{connection:?}").contains(&connection.chap_password));
        connection.target_iqn = "iqn.2024-01.com.diskless:client.pc002".into();
        assert!(connection.validate().is_err());
    }
}
