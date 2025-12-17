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
// pub use samba::SambaService; // Exported but not used in ServiceManager yet
pub use tftp::TftpService;

use crate::core::config::Settings;
use crate::core::image::Image;
use sqlx::SqlitePool;
use std::collections::HashMap;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct ServiceStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub message: String,
}

pub struct ServiceManager {
    settings: Settings,
    dhcp: DhcpService,
    tftp: TftpService,
    iscsi: IscsiService,
    nfs: NfsService,
    http: HttpService, // TODO: Implement HTTP service
}

impl ServiceManager {
    pub fn new(settings: Settings, db_pool: SqlitePool) -> Self {
        Self {
            dhcp: DhcpService::new(settings.clone(), db_pool),
            tftp: TftpService::new(settings.clone()),
            iscsi: IscsiService::new(settings.clone()),
            nfs: NfsService::new(settings.clone()),
            http: HttpService::new(settings.clone()),
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
        // HTTP config is generated on start
        self.http.generate_config().await?;
        Ok(())
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
        Ok(())
    }

    pub async fn status_all(&self) -> anyhow::Result<HashMap<String, ServiceStatus>> {
        let mut statuses = HashMap::new();
        statuses.insert("dhcp".to_string(), self.dhcp.status().await?);
        statuses.insert("tftp".to_string(), self.tftp.status().await?);
        statuses.insert("iscsi".to_string(), self.iscsi.status().await?);
        statuses.insert("nfs".to_string(), self.nfs.status().await?);
        statuses.insert("http".to_string(), self.http.status().await?);
        Ok(statuses)
    }

    pub async fn start(&self, service: &str) -> anyhow::Result<()> {
        match service {
            "dhcp" => self.dhcp.start().await,
            "tftp" => self.tftp.start().await,
            "iscsi" => self.iscsi.start().await,
            "nfs" => self.nfs.start().await,
            "http" => self.http.start().await,
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
            _ => Err(anyhow::anyhow!("Unknown service: {}", service)),
        }
    }

    pub async fn regenerate_dhcp_config(&self) -> anyhow::Result<()> {
        self.dhcp.generate_config().await?;
        self.dhcp.reload().await
    }

    pub async fn create_iscsi_target(&self, image: &Image) -> anyhow::Result<()> {
        self.iscsi.create_target(image).await
    }

    pub async fn remove_iscsi_target(&self, name: &str) -> anyhow::Result<()> {
        self.iscsi.remove_target(name).await
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
        .args(["show", "-p", "MainPID", service])
        .output()
        .await?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some(pid_str) = stdout.strip_prefix("MainPID=") {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                if pid > 0 {
                    return Ok(Some(pid));
                }
            }
        }
    }
    Ok(None)
}
