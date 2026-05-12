mod dhcp;
mod http;
mod iscsi;
mod nfs;
mod samba;
mod tftp;
pub use dhcp::DhcpService;
pub use http::HttpService;
pub use iscsi::IscsiService;
pub use nfs::NfsService;
pub use samba::SambaService;
use std::process::Stdio;
pub use tftp::TftpService;
use tokio::io::AsyncWriteExt;

use crate::core::config::Settings;
use crate::error::AppError;
use sqlx::SqlitePool;
use std::collections::HashMap;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct ServiceStatus {
    pub running: bool,
    pub pid: Option<u32>,
    #[allow(dead_code)]
    pub message: String,
}

pub struct ServiceManager {
    pub settings: Settings,
    pub dhcp: DhcpService,
    pub tftp: TftpService,
    pub iscsi: IscsiService,
    pub nfs: NfsService,
    pub http: HttpService,
    pub samba: SambaService,
}

impl ServiceManager {
    pub fn new(settings: Settings, db_pool: SqlitePool) -> Self {
        Self {
            dhcp: DhcpService::new(settings.clone(), db_pool),
            tftp: TftpService::new(settings.clone()),
            iscsi: IscsiService::new(settings.clone()),
            nfs: NfsService::new(settings.clone()),
            http: HttpService::new(settings.clone()),
            samba: SambaService::new(settings.clone()),
            settings,
        }
    }

    pub async fn generate_all_configs(&self) -> anyhow::Result<()> {
        if self.settings.dhcp.enabled {
            self.dhcp.generate_config().await?;
        }
        if self.settings.iscsi.enabled {
            self.iscsi.generate_config().await?;
        }
        if self.settings.nfs.enabled {
            self.nfs.generate_config().await?;
        }
        if self.settings.samba.enabled {
            self.samba.generate_config().await?;
        }
        // HTTP config is generated on start
        self.http.generate_config().await?;
        Ok(())
    }

    pub async fn generate_service_config(&self, service: &str) -> anyhow::Result<()> {
        match service {
            "dhcp" => self.dhcp.generate_config().await,
            "tftp" => self.tftp.generate_config().await,
            "iscsi" => self.iscsi.generate_config().await,
            "nfs" => self.nfs.generate_config().await,
            "http" => self.http.generate_config().await,
            "samba" => self.samba.generate_config().await,
            _ => Err(anyhow::anyhow!("Unknown service: {}", service)),
        }
    }

    pub async fn start_all(&self) -> anyhow::Result<()> {
        if self.settings.dhcp.enabled {
            self.dhcp.start().await?;
        }
        if self.settings.tftp.enabled {
            self.tftp.start().await?;
        }
        if self.settings.iscsi.enabled {
            self.iscsi.start().await?;
        }
        if self.settings.nfs.enabled {
            self.nfs.start().await?;
        }
        if self.settings.samba.enabled {
            self.samba.start().await?;
        }
        // Always start HTTP for iPXE boot
        self.http.start().await?;
        Ok(())
    }

    pub async fn stop_all(&self) -> anyhow::Result<()> {
        self.http.stop().await?;
        self.dhcp.stop().await?;
        self.tftp.stop().await?;
        self.iscsi.stop().await?;
        self.nfs.stop().await?;
        self.samba.stop().await?;
        Ok(())
    }

    pub async fn restart_all(&self) -> anyhow::Result<()> {
        self.http.reload().await?;
        self.dhcp.reload().await?;
        self.tftp.reload().await?;
        self.iscsi.reload().await?;
        self.nfs.reload().await?;
        self.samba.reload().await?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn status_all(&self) -> anyhow::Result<HashMap<String, ServiceStatus>> {
        let mut statuses = HashMap::new();
        statuses.insert("dhcp".to_string(), self.dhcp.status().await?);
        statuses.insert("tftp".to_string(), self.tftp.status().await?);
        statuses.insert("iscsi".to_string(), self.iscsi.status().await?);
        statuses.insert("nfs".to_string(), self.nfs.status().await?);
        statuses.insert("http".to_string(), self.http.status().await?);
        statuses.insert("samba".to_string(), self.samba.status().await?);
        Ok(statuses)
    }

    pub async fn start(&self, service: &str) -> anyhow::Result<()> {
        match service {
            "dhcp" => self.dhcp.start().await,
            "tftp" => self.tftp.start().await,
            "iscsi" => self.iscsi.start().await,
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
            "iscsi" => self.iscsi.stop().await,
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
            "iscsi" => self.iscsi.status().await,
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
            "iscsi" => self.iscsi.reload().await,
            "nfs" => self.nfs.reload().await,
            "http" => self.http.reload().await,
            "samba" => self.samba.reload().await,
            _ => Err(anyhow::anyhow!("Unknown service: {}", service)),
        }
    }

    #[allow(dead_code)]
    pub async fn regenerate_dhcp_config(&self) -> anyhow::Result<()> {
        self.dhcp.generate_config().await?;
        self.dhcp.reload().await
    }

    pub async fn get_config(&self, service: &str) -> anyhow::Result<String> {
        // Map the service names that may come from the frontend to internal service names
        let internal_service = match service {
            "apache2" => "http",
            "smbd" => "samba",
            "tftpd-hpa" => "tftp",
            "isc-dhcp-server" => "dhcp",
            "nfs-kernel-server" => "nfs",
            "rtslib-fb-targetctl" => "iscsi",
            // If it's already an internal service name, use it as is
            "http" | "samba" | "tftp" | "dhcp" | "nfs" | "iscsi" => service,
            _ => return Err(anyhow::anyhow!("Unknown service: {}", service)),
        };

        match internal_service {
            "dhcp" => self.dhcp.get_config().await,
            "tftp" => self.tftp.get_config().await,
            "iscsi" => self.iscsi.get_config().await,
            "nfs" => self.nfs.get_config().await,
            "http" => self.http.get_config().await,
            "samba" => self.samba.get_config().await,
            _ => Err(anyhow::anyhow!("Unknown service: {}", internal_service)),
        }
    }
}

// Helper function to check if a systemd service is running
pub async fn is_systemd_service_running(service: &str) -> anyhow::Result<bool> {
    let output = Command::new("systemctl")
        .args(["is-active", service])
        .output()
        .await?;

    Ok(output.status.success())
}

// Helper function to get PID of a service
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

// Helper function to run a command with sudo -n (async)
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

// Helper function to Write content to path using sudo tee (async)
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
