use crate::core::config::Settings;
use crate::services::{get_service_pid, is_systemd_service_running, ServiceStatus};
use tokio::process::Command;

pub struct TftpService {
    settings: Settings,
}

impl TftpService {
    pub fn new(settings: Settings) -> Self {
        Self { settings }
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        // Configure and start tftpd-hpa
        let tftp_default = format!(
            r#"TFTP_USERNAME="tftp"
TFTP_DIRECTORY="{}"
TFTP_ADDRESS="0.0.0.0:{}"
TFTP_OPTIONS="--secure --verbose"
"#,
            self.settings.tftp.root_dir.display(),
            self.settings.tftp.port
        );

        std::fs::write("/etc/default/tftpd-hpa", tftp_default)?;

        Command::new("systemctl")
            .args(["start", "tftpd-hpa"])
            .status()
            .await?;

        tracing::info!("TFTP service started");
        Ok(())
    }

    pub async fn stop(&self) -> anyhow::Result<()> {
        Command::new("systemctl")
            .args(["stop", "tftpd-hpa"])
            .status()
            .await?;
        tracing::info!("TFTP service stopped");
        Ok(())
    }

    pub async fn reload(&self) -> anyhow::Result<()> {
        Command::new("systemctl")
            .args(["restart", "tftpd-hpa"])
            .status()
            .await?;
        tracing::info!("TFTP service reloaded");
        Ok(())
    }

    pub async fn status(&self) -> anyhow::Result<ServiceStatus> {
        let running = is_systemd_service_running("tftpd-hpa").await?;
        let pid = get_service_pid("tftpd-hpa").await?;
        Ok(ServiceStatus {
            running,
            pid,
            message: if running {
                "TFTP server is running".to_string()
            } else {
                "TFTP server is not running".to_string()
            },
        })
    }
}
