use crate::core::config::Settings;
use crate::core::image::Image;
use crate::services::{get_service_pid, is_systemd_service_running, ServiceStatus};
use tokio::process::Command;

pub struct IscsiService {
    settings: Settings,
}

impl IscsiService {
    pub fn new(settings: Settings) -> Self {
        Self { settings }
    }

    pub async fn generate_config(&self) -> anyhow::Result<()> {
        // targetcli manages its own configuration file (/etc/target/saveconfig.json)
        // We just ensure the service is running and save any changes we make
        self.save_config().await
    }

    pub async fn create_target(&self, image: &Image) -> anyhow::Result<()> {
        let backstore_name = &image.name;
        let target_iqn = format!("{}:{}", self.settings.iscsi.target_prefix, image.name);
        let image_path = image.path.to_string_lossy();

        // 1. Create Backstore (FileIO)
        // targetcli /backstores/fileio create name={name} file_or_dev={path}
        self.run_targetcli(&format!(
            "/backstores/fileio create name={} file_or_dev={}",
            backstore_name, image_path
        ))
        .await?;

        // 2. Create iSCSI Target
        // targetcli /iscsi create wwn={iqn}
        self.run_targetcli(&format!("/iscsi create wwn={}", target_iqn))
            .await?;

        // 3. Create LUN
        // targetcli /iscsi/{iqn}/tpg1/luns create storage_object=/backstores/fileio/{name}
        self.run_targetcli(&format!(
            "/iscsi/{}/tpg1/luns create storage_object=/backstores/fileio/{}",
            target_iqn, backstore_name
        ))
        .await?;

        // 4. Set ACLs (Allow all for PXE)
        // targetcli /iscsi/{iqn}/tpg1 set attribute generate_node_acls=1
        self.run_targetcli(&format!(
            "/iscsi/{}/tpg1 set attribute generate_node_acls=1",
            target_iqn
        ))
        .await?;

        // targetcli /iscsi/{iqn}/tpg1 set attribute demo_mode_write_protect=0
        self.run_targetcli(&format!(
            "/iscsi/{}/tpg1 set attribute demo_mode_write_protect=0",
            target_iqn
        ))
        .await?;

        // 5. Save configuration
        self.save_config().await?;

        tracing::info!("iSCSI target created: {}", target_iqn);
        Ok(())
    }

    pub async fn remove_target(&self, name: &str) -> anyhow::Result<()> {
        let backstore_name = name;
        let target_iqn = format!("{}:{}", self.settings.iscsi.target_prefix, name);

        // 1. Delete iSCSI Target (recursively deletes TPGs, LUNs, ACLs)
        // targetcli /iscsi delete wwn={iqn}
        // Ignore error if target doesn't exist
        let _ = self
            .run_targetcli(&format!("/iscsi delete wwn={}", target_iqn))
            .await;

        // 2. Delete Backstore
        // targetcli /backstores/fileio delete name={name}
        let _ = self
            .run_targetcli(&format!(
                "/backstores/fileio delete name={}",
                backstore_name
            ))
            .await;

        // 3. Save configuration
        self.save_config().await?;

        tracing::info!("iSCSI target removed: {}", target_iqn);
        Ok(())
    }

    async fn run_targetcli(&self, command: &str) -> anyhow::Result<()> {
        let output = Command::new("pkexec")
            .arg("targetcli")
            .arg(command)
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(anyhow::anyhow!(
                "targetcli command failed: '{}'. Stderr: {}. Stdout: {}",
                command,
                stderr,
                stdout
            ));
        }
        Ok(())
    }

    async fn save_config(&self) -> anyhow::Result<()> {
        self.run_targetcli("saveconfig").await
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        Command::new("pkexec")
            .args(["systemctl", "start", "rtslib-fb-targetctl"])
            .status()
            .await?;
        tracing::info!("iSCSI service started");
        Ok(())
    }

    pub async fn stop(&self) -> anyhow::Result<()> {
        Command::new("pkexec")
            .args(["systemctl", "stop", "rtslib-fb-targetctl"])
            .status()
            .await?;
        tracing::info!("iSCSI service stopped");
        Ok(())
    }

    pub async fn reload(&self) -> anyhow::Result<()> {
        // targetcli doesn't strictly need reload if we're using the CLI tool,
        // but we can ensure config is saved or restart the service.
        // For LIO, 'restoreconfig' might be the equivalent, but usually 'target' service handles it.
        // Let's just save config to be sure.
        self.save_config().await?;
        tracing::info!("iSCSI configuration saved");
        Ok(())
    }

    pub async fn status(&self) -> anyhow::Result<ServiceStatus> {
        let running = is_systemd_service_running("target").await?;
        // target.service usually exits after configuration, but there might be a kernel thread or
        // we can check if the modules are loaded. However, usually checking 'target' service status is enough
        // or checking for 'configfs' mount.
        // For simplicity, let's keep checking the systemd service 'target' (or 'targetcli' depending on distro).
        // On many systems it is 'target.service'.

        let header_pid = get_service_pid("target").await?; // Might be None as it's a oneshot service often

        Ok(ServiceStatus {
            running,
            pid: header_pid,
            message: if running {
                "iSCSI target service is active".to_string()
            } else {
                "iSCSI target service is not active".to_string()
            },
        })
    }
}
