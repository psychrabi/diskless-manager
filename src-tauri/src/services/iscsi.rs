use crate::core::client::Client;
use crate::core::config::Settings;
use crate::services::{
    get_service_pid, is_systemd_service_running, run_sudo_command, ServiceStatus,
};
use log::info;
use std::path::PathBuf;
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

    pub async fn create_target(&self, client: &Client) -> anyhow::Result<()> {
        let block_device = client.block_device.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Client {} does not have a block device configured",
                client.name
            )
        })?;
        let block_store = client.block_store.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Client {} does not have a block store configured",
                client.name
            )
        })?;
        let target_iqn = client.target_iqn.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Client {} does not have a target IQN configured",
                client.name
            )
        })?;
        info!("Creating iSCSI target for client: {}", &client.name);

        // Check if backstore already exists and remove it if it does
        // targetcli /backstores/block delete name={name}
        let _ = self
            .run_targetcli(&format!("/backstores/block delete name={}", block_device))
            .await
            .inspect_err(|e| {
                tracing::debug!("Backstore {} may not have existed: {}", block_device, e);
            });

        // 1. Create Backstore (Block)
        // targetcli /backstores/block create name={name} dev={path}
        self.run_targetcli(&format!(
            "/backstores/block create name={} dev={} ",
            block_device, block_store
        ))
        .await?;

        // Check if target already exists and remove it if it does
        // targetcli /iscsi delete wwn={iqn}
        let _ = self
            .run_targetcli(&format!("/iscsi delete wwn={}", target_iqn))
            .await
            .inspect_err(|e| {
                tracing::debug!("Target {} may not have existed: {}", target_iqn, e);
            });

        // 2. Create iSCSI Target
        // targetcli /iscsi create wwn={iqn}
        self.run_targetcli(&format!("/iscsi create wwn={}", target_iqn))
            .await?;

        // 3. Create LUN
        // targetcli /iscsi/{iqn}/tpg1/luns create storage_object=/backstores/block/{name}
        self.run_targetcli(&format!(
            "/iscsi/{}/tpg1/luns create storage_object=/backstores/block/{}",
            target_iqn, block_device
        ))
        .await?;

        // 4. Set ACLs (Allow all for PXE)
        // targetcli /iscsi/{iqn}/tpg1 set attribute generate_node_acls=1
        self.run_targetcli(&format!(
            "/iscsi/{}/tpg1 set attribute generate_node_acls=1",
            target_iqn
        ))
        .await?;

        // targetcli /iscsi/{iqn}/tpg1 set attribute cache_dynamic_acls=1
        self.run_targetcli(&format!(
            "/iscsi/{}/tpg1 set attribute cache_dynamic_acls=1",
            target_iqn
        ))
        .await?;

        // targetcli /iscsi/{iqn}/tpg1 set attribute demo_mode_write_protect=0
        self.run_targetcli(&format!(
            "/iscsi/{}/tpg1 set attribute demo_mode_write_protect=0",
            target_iqn
        ))
        .await?;

        // targetcli /iscsi/{iqn}/tpg1 set attribute authentication=0
        self.run_targetcli(&format!(
            "/iscsi/{}/tpg1 set attribute authentication=0",
            target_iqn
        ))
        .await?;

        // 5. Save configuration
        self.save_config().await?;

        info!("iSCSI target created: {}", target_iqn);
        Ok(())
    }

    pub async fn remove_target(&self, name: &str) -> anyhow::Result<()> {
        // Construct the backstore name using the same pattern as in client creation
        let backstore_name = format!("block_{}", name.to_lowercase());

        // Try client format first: {prefix}:client.{name}
        let client_target_iqn = format!(
            "{}:client.{}",
            self.settings.iscsi.target_prefix,
            name.to_lowercase()
        );
        let client_result = self
            .run_targetcli(&format!("/iscsi delete wwn={}", client_target_iqn))
            .await;

        // If client format fails, try direct format: {prefix}:{name}
        if client_result.is_err() {
            let direct_target_iqn = format!(
                "{}:{}",
                self.settings.iscsi.target_prefix,
                name.to_lowercase()
            );
            let _ = self
                .run_targetcli(&format!("/iscsi delete wwn={}", direct_target_iqn))
                .await;
        }

        // 2. Delete Backstore
        // targetcli /backstores/block delete name={name}
        let _ = self
            .run_targetcli(&format!("/backstores/block delete name={}", backstore_name))
            .await
            .inspect_err(|e| {
                tracing::debug!(
                    "Failed to remove backstore (this is OK if backstore doesn't exist): {}",
                    e
                )
            });

        // 3. Save configuration
        self.save_config().await?;

        info!("iSCSI target removed for: {}", name);
        Ok(())
    }

    pub async fn remove_target_by_iqn(
        &self,
        target_iqn: &str,
        block_device: &Option<String>,
    ) -> anyhow::Result<()> {
        // Remove the specific target by its full IQN
        let _ = self
            .run_targetcli(&format!("/iscsi delete wwn={}", target_iqn))
            .await
            .inspect_err(|e| tracing::debug!("Target {} may not have existed: {}", target_iqn, e));

        // Remove the associated backstore if block_device is provided
        if let Some(device) = block_device {
            let _ = self
                .run_targetcli(&format!("/backstores/block delete name={}", device))
                .await
                .inspect_err(|e| {
                    tracing::debug!("Backstore {} may not have existed: {}", device, e)
                });
        }

        // Save configuration
        self.save_config().await?;

        info!("iSCSI target removed: {}", target_iqn);
        Ok(())
    }

    async fn run_targetcli(&self, command: &str) -> anyhow::Result<()> {
        let output = Command::new("sudo")
            .arg("-n")
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
        run_sudo_command(["systemctl", "start", "rtslib-fb-targetctl"]).await?;
        info!("iSCSI service started");
        Ok(())
    }

    pub async fn stop(&self) -> anyhow::Result<()> {
        run_sudo_command(["systemctl", "stop", "rtslib-fb-targetctl"]).await?;
        info!("iSCSI service stopped");
        Ok(())
    }

    pub async fn reload(&self) -> anyhow::Result<()> {
        // targetcli doesn't strictly need reload if we're using the CLI tool,
        // but we can ensure config is saved or restart the service.
        // For LIO, 'restoreconfig' might be the equivalent, but usually 'target' service handles it.
        // Let's just save config to be sure.
        self.save_config().await?;
        info!("iSCSI configuration saved");
        Ok(())
    }

    pub async fn get_config(&self) -> anyhow::Result<String> {
        // Use targetcli to get the current configuration
        let output = tokio::process::Command::new("sudo")
            .arg("-n")
            .arg("targetcli")
            .arg("ls")
            .output()
            .await?;

        if output.status.success() {
            let config = String::from_utf8_lossy(&output.stdout);
            Ok(config.to_string())
        } else {
            // If targetcli fails, fall back to reading the saveconfig.json file
            let config_path = PathBuf::from("/etc/target/saveconfig.json");
            if config_path.exists() {
                let content = std::fs::read_to_string(&config_path)?;
                Ok(content)
            } else {
                // Return a default configuration structure
                let default_config = r#"{
    "storage_objects": {},
    "targets": {}
}"#;
                Ok(default_config.to_string())
            }
        }
    }

    pub async fn status(&self) -> anyhow::Result<ServiceStatus> {
        let running = is_systemd_service_running("rtslib-fb-targetctl").await?;
        // Check the actual service that we start/stop

        let pid = get_service_pid("rtslib-fb-targetctl").await?;

        Ok(ServiceStatus {
            running,
            pid,
            message: if running {
                "iSCSI target service is active".to_string()
            } else {
                "iSCSI target service is not active".to_string()
            },
        })
    }
}
