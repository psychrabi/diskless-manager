mod dhcp;
mod http;
mod nfs;
mod samba;
mod tftp;

pub use dhcp::DhcpService;
pub use http::HttpService;
pub use nfs::NfsService;
pub use samba::SambaService;
pub use tftp::TftpService;

use std::process::Stdio;
use std::sync::Arc;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::core::config::Settings;
use crate::error::AppError;
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct ServiceStatus {
    pub running: bool,
    pub pid: Option<u32>,
}

pub struct ServiceManager {
    pub settings: Arc<Settings>,
    pub dhcp: DhcpService,
    pub tftp: TftpService,
    pub nfs: NfsService,
    pub http: HttpService,
    pub samba: SambaService,
}

impl ServiceManager {
    pub fn new(settings: Settings, db_pool: SqlitePool) -> Self {
        let shared = Arc::new(settings);

        Self {
            dhcp: DhcpService {
                settings: Arc::clone(&shared),
                db_pool,
            },

            tftp: TftpService {
                settings: Arc::clone(&shared),
            },

            nfs: NfsService {
                settings: Arc::clone(&shared),
            },

            http: HttpService {
                settings: Arc::clone(&shared),
            },

            samba: SambaService {
                settings: Arc::clone(&shared),
            },

            settings: shared,
        }
    }

    // ========================================================================
    // iSCSI SERVICE MANAGEMENT
    // ========================================================================

    /// Persist the current LIO configuration for boot.
    ///
    /// iSCSI storage provisioning itself is handled by the application
    /// StorageService. This method only persists the configuration.
    async fn generate_iscsi_config(&self) -> anyhow::Result<()> {
        crate::infrastructure::iscsi::configfs::ConfigfsProvisioner::save_config()
            .map_err(|error| anyhow::anyhow!("failed to save iSCSI configuration: {:#}", error))
    }

    /// Start the Linux LIO target service.
    async fn start_iscsi(&self) -> anyhow::Result<()> {
        let service = crate::platform::detect().iscsi_service();
        run_sudo_command(["systemctl", "start", service])
            .await
            .map_err(|error| anyhow::anyhow!("failed to start iSCSI service: {}", error))
    }

    /// Stop the Linux LIO target service.
    async fn stop_iscsi(&self) -> anyhow::Result<()> {
        let service = crate::platform::detect().iscsi_service();
        run_sudo_command(["systemctl", "stop", service])
            .await
            .map_err(|error| anyhow::anyhow!("failed to stop iSCSI service: {}", error))
    }

    /// Reload/persist the Linux LIO target configuration.
    async fn reload_iscsi(&self) -> anyhow::Result<()> {
        self.generate_iscsi_config().await
    }

    /// Return the persisted LIO configuration, redacted.
    ///
    /// Reads the on-disk configuration rather than querying the live kernel
    /// state: `targetcli ls` prints a tree, not JSON, so the redaction below
    /// would only ever return a placeholder for it.
    async fn get_iscsi_config(&self) -> anyhow::Result<String> {
        const EMPTY: &str = r#"{
    "storage_objects": {},
    "targets": {}
}"#;
        let config_path = std::path::Path::new("/etc/target/saveconfig.json");
        match crate::infrastructure::command::read_file_with_sudo(config_path) {
            Ok(Some(content)) => Ok(redact_iscsi_config(&content)),
            Ok(None) => Ok(EMPTY.to_string()),
            Err(error) => Err(anyhow::anyhow!(
                "failed to read iSCSI configuration: {}",
                error
            )),
        }
    }

    /// Return the status of the Linux LIO target service.
    async fn iscsi_status(&self) -> anyhow::Result<ServiceStatus> {
        let service = crate::platform::detect().iscsi_service();
        let running = is_systemd_service_running(service).await?;
        let pid = get_service_pid(service).await?;

        Ok(ServiceStatus { running, pid })
    }

    // ========================================================================
    // CONFIGURATION
    // ========================================================================

    pub async fn generate_all_configs(&self) -> anyhow::Result<()> {
        self.settings.validate()?;
        if self.settings.dhcp.enabled {
            self.dhcp.generate_config().await?;
        }

        if self.settings.iscsi.enabled {
            self.generate_iscsi_config().await?;
        }

        if self.settings.nfs.enabled {
            self.nfs.generate_config().await?;
        }

        if self.settings.samba.enabled {
            self.samba.generate_config().await?;
        }

        // HTTP configuration is generated on start.
        self.http.generate_config().await?;

        Ok(())
    }

    pub async fn generate_service_config(&self, service: &str) -> anyhow::Result<()> {
        self.settings.validate()?;
        match service {
            "dhcp" => self.dhcp.generate_config().await,
            "tftp" => self.tftp.generate_config().await,
            "iscsi" => self.generate_iscsi_config().await,
            "nfs" => self.nfs.generate_config().await,
            "http" => self.http.generate_config().await,
            "samba" => self.samba.generate_config().await,
            _ => Err(anyhow::anyhow!("Unknown service: {}", service)),
        }
    }

    // ========================================================================
    // START / STOP / RESTART
    // ========================================================================

    pub async fn start_all(&self) -> anyhow::Result<()> {
        // Persist across reboots first (Ubuntu parity); individual starts
        // below are idempotent re-enables.
        self.enable_boot_all().await?;

        if self.settings.dhcp.enabled {
            self.dhcp.start().await?;
        }

        if self.settings.tftp.enabled {
            self.tftp.start().await?;
        }

        if self.settings.iscsi.enabled {
            self.start_iscsi().await?;
        }

        if self.settings.nfs.enabled {
            self.nfs.start().await?;
        }

        if self.settings.samba.enabled {
            self.samba.start().await?;
        }

        // Always start HTTP for iPXE boot.
        self.http.start().await?;

        Ok(())
    }

    pub async fn stop_all(&self) -> anyhow::Result<()> {
        self.http.stop().await?;
        self.dhcp.stop().await?;
        self.tftp.stop().await?;
        self.stop_iscsi().await?;
        self.nfs.stop().await?;
        self.samba.stop().await?;

        Ok(())
    }

    pub async fn restart_all(&self) -> anyhow::Result<()> {
        self.http.reload().await?;
        self.dhcp.reload().await?;
        self.tftp.reload().await?;
        self.reload_iscsi().await?;
        self.nfs.reload().await?;
        self.samba.reload().await?;

        Ok(())
    }

    // ========================================================================
    // STATUS
    // ========================================================================

    // ========================================================================
    // INDIVIDUAL SERVICE CONTROL
    // ========================================================================

    pub async fn start(&self, service: &str) -> anyhow::Result<()> {
        // Ubuntu parity: starting a service persists it across reboots.
        // Fedora ships every unit disabled, so a bare `start` would only
        // last until the next boot.
        self.enable_boot(service).await?;
        match service {
            "dhcp" => self.dhcp.start().await,
            "tftp" => self.tftp.start().await,
            "iscsi" => self.start_iscsi().await,
            "nfs" => self.nfs.start().await,
            "http" => self.http.start().await,
            "samba" => self.samba.start().await,
            _ => Err(anyhow::anyhow!("Unknown service: {}", service)),
        }
    }

    pub async fn stop(&self, service: &str) -> anyhow::Result<()> {
        match service {
            "dhcp" => self.dhcp.stop().await,
            "tftp" => self.tftp.stop().await,
            "iscsi" => self.stop_iscsi().await,
            "nfs" => self.nfs.stop().await,
            "http" => self.http.stop().await,
            "samba" => self.samba.stop().await,
            _ => Err(anyhow::anyhow!("Unknown service: {}", service)),
        }
    }

    pub async fn status(&self, service: &str) -> anyhow::Result<ServiceStatus> {
        match service {
            "dhcp" => self.dhcp.status().await,
            "tftp" => self.tftp.status().await,
            "iscsi" => self.iscsi_status().await,
            "nfs" => self.nfs.status().await,
            "http" => self.http.status().await,
            "samba" => self.samba.status().await,
            _ => Err(anyhow::anyhow!("Unknown service: {}", service)),
        }
    }

    pub async fn reload(&self, service: &str) -> anyhow::Result<()> {
        match service {
            "dhcp" => self.dhcp.reload().await,
            "tftp" => self.tftp.reload().await,
            "iscsi" => self.reload_iscsi().await,
            "nfs" => self.nfs.reload().await,
            "http" => self.http.reload().await,
            "samba" => self.samba.reload().await,
            _ => Err(anyhow::anyhow!("Unknown service: {}", service)),
        }
    }

    // ========================================================================
    // START ON BOOT
    // ========================================================================

    /// Whether every boot unit for a service is enabled.
    pub async fn boot_enabled(&self, service: &str) -> anyhow::Result<bool> {
        let units = crate::platform::detect().boot_units(service);
        if units.is_empty() {
            anyhow::bail!("Unknown service: {}", service);
        }
        for unit in units {
            if !is_boot_unit_enabled(unit).await? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Persist a service across reboots without changing its running state.
    pub async fn enable_boot(&self, service: &str) -> anyhow::Result<()> {
        let units = crate::platform::detect().boot_units(service);
        if units.is_empty() {
            anyhow::bail!("Unknown service: {}", service);
        }
        set_boot_units_enabled(units, true).await
    }

    /// Stop persisting a service across reboots without changing its
    /// running state. Use `stop` to halt a running service now.
    pub async fn disable_boot(&self, service: &str) -> anyhow::Result<()> {
        let units = crate::platform::detect().boot_units(service);
        if units.is_empty() {
            anyhow::bail!("Unknown service: {}", service);
        }
        set_boot_units_enabled(units, false).await
    }

    /// Enable every settings-enabled service for start on boot.
    pub async fn enable_boot_all(&self) -> anyhow::Result<()> {
        for service in ["dhcp", "tftp", "iscsi", "nfs", "samba"] {
            let managed = match service {
                "dhcp" => self.settings.dhcp.enabled,
                "tftp" => self.settings.tftp.enabled,
                "iscsi" => self.settings.iscsi.enabled,
                "nfs" => self.settings.nfs.enabled,
                "samba" => self.settings.samba.enabled,
                _ => false,
            };
            if managed {
                self.enable_boot(service).await?;
            }
        }
        // HTTP is always required for iPXE boot.
        self.enable_boot("http").await?;
        Ok(())
    }

    // ========================================================================
    // DHCP HELPERS
    // ========================================================================

    // ========================================================================
    // SERVICE CONFIGURATION
    // ========================================================================

    pub async fn get_config(&self, service: &str) -> anyhow::Result<String> {
        // Map service names that may come from the frontend to internal names.
        let internal_service = match service {
            "apache2" | "httpd" => "http",
            "smbd" | "smb" => "samba",
            "tftpd-hpa" | "tftpd" => "tftp",
            "isc-dhcp-server" | "dhcpd" | "dhcpd4" => "dhcp",
            "nfs-kernel-server" | "nfs-server" => "nfs",
            "rtslib-fb-targetctl" | "target" => "iscsi",

            "http" | "samba" | "tftp" | "dhcp" | "nfs" | "iscsi" => service,

            _ => return Err(anyhow::anyhow!("Unknown service: {}", service)),
        };

        match internal_service {
            "dhcp" => self.dhcp.get_config().await,
            "tftp" => self.tftp.get_config().await,
            "iscsi" => self.get_iscsi_config().await,
            "nfs" => self.nfs.get_config().await,
            "http" => self.http.get_config().await,
            "samba" => self.samba.get_config().await,
            _ => Err(anyhow::anyhow!("Unknown service: {}", internal_service)),
        }
    }
}

fn redact_iscsi_config(config: &str) -> String {
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(config) else {
        return "[redacted: iSCSI configuration is not JSON]".to_string();
    };

    fn redact(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(object) => {
                object.retain(|key, _| {
                    !matches!(key.to_ascii_lowercase().as_str(), "password" | "chap_secret")
                });
                for value in object.values_mut() {
                    redact(value);
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    redact(value);
                }
            }
            _ => {}
        }
    }

    redact(&mut value);
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| config.to_string())
}

// ============================================================================
// SYSTEMD HELPERS
// ============================================================================

/// Check whether a systemd unit is enabled for start on boot.
///
/// `is-enabled` exits non-zero for disabled/missing units, which is a
/// normal negative answer — not an error.
pub async fn is_boot_unit_enabled(unit: &str) -> anyhow::Result<bool> {
    let output = Command::new("systemctl")
        .args(["is-enabled", unit])
        .output()
        .await?;

    Ok(output.status.success())
}

/// Enable or disable systemd units for start on boot (no sudo needed to
/// query; elevation required to change).
pub async fn set_boot_units_enabled(units: &[&str], enabled: bool) -> anyhow::Result<()> {
    let action = if enabled { "enable" } else { "disable" };
    let mut args = vec![String::from("systemctl"), String::from(action)];
    args.extend(units.iter().map(|unit| unit.to_string()));
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_sudo_command(arg_refs)
        .await
        .map_err(|error| {
            anyhow::anyhow!("failed to {} boot units {:?}: {}", action, units, error)
        })
}

/// Check whether a systemd service is running.
pub async fn is_systemd_service_running(service: &str) -> anyhow::Result<bool> {
    let output = Command::new("systemctl")
        .args(["is-active", service])
        .output()
        .await?;

    Ok(output.status.success())
}

/// Get the PID of a systemd service.
pub async fn get_service_pid(service: &str) -> anyhow::Result<Option<u32>> {
    let output = Command::new("systemctl")
        .args(["show", "-p", "ExecMainPID", service])
        .output()
        .await?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);

        if let Some(pid_str) = stdout.strip_prefix("ExecMainPID=") {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                if pid > 0 {
                    return Ok(Some(pid));
                }
            }
        }
    }

    Ok(None)
}

/// Run a command through `sudo -n`.
pub async fn run_sudo_command<II>(args: II) -> Result<(), AppError>
where
    II: IntoIterator,
    II::Item: AsRef<std::ffi::OsStr> + std::fmt::Debug,
{
    let args_vec: Vec<_> = args.into_iter().collect();

    let status = Command::new("sudo")
        .arg("-n")
        .args(&args_vec)
        .status()
        .await
        .map_err(|e| AppError::Command(format!("Failed to execute sudo command: {}", e)))?;

    if !status.success() {
        return Err(AppError::Command(format!(
            "Command 'sudo -n {:?}' failed with status {}",
            args_vec, status
        )));
    }

    Ok(())
}

/// Write content to a path using `sudo tee`.
pub async fn write_with_sudo_tee(path: &str, content: &str) -> Result<(), AppError> {
    let mut child = Command::new("sudo")
        .arg("-n")
        .arg("tee")
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::Command(format!("Failed to spawn sudo tee for {}: {}", path, e)))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(content.as_bytes()).await.map_err(|e| {
            AppError::Io(std::io::Error::other(format!(
                "Failed to write to stdin for {}: {}",
                path, e
            )))
        })?;
    }

    let status = child
        .wait()
        .await
        .map_err(|e| AppError::Command(format!("Failed to wait for tee on {}: {}", path, e)))?;

    if !status.success() {
        Err(AppError::Command(format!(
            "Failed to write {}: Command exited with status {}",
            path, status
        )))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod config_tests {
    use super::redact_iscsi_config;

    #[test]
    fn iscsi_config_redaction_removes_password_fields() {
        let redacted = redact_iscsi_config(
            r#"{"targets":{"target":{"password":"secret","name":"client"}}}"#,
        );
        assert!(!redacted.contains("secret"));
        assert!(redacted.contains("client"));
    }

    #[test]
    fn malformed_iscsi_config_is_not_returned() {
        let redacted = redact_iscsi_config("password=secret");
        assert!(!redacted.contains("secret"));
    }
}
