use crate::core::config::Settings;
use crate::core::image::Image;
use crate::error::AppError;
use crate::services::{
    get_service_pid, is_systemd_service_running, run_sudo_command, ServiceStatus,
};
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

    pub async fn create_target(&self, image: &Image) -> anyhow::Result<()> {
        let backstore_name = &image.name;
        let target_iqn = format!("{}:{}", self.settings.iscsi.target_prefix, image.name);
        let image_path = image.path.to_string_lossy();

        // For master images which are likely files or ZVOLs treated as files
        self.create_iscsi_target_internal(backstore_name, &image_path, &target_iqn, "fileio")
            .await
    }

    pub async fn create_client_target(&self, name: &str, path: &str) -> anyhow::Result<String> {
        let backstore_name = format!("{}-disk", name);
        let target_iqn = format!("{}:client.{}", self.settings.iscsi.target_prefix, name);

        // For client clones (ZVOLs), use block backstore for better performance
        self.create_iscsi_target_internal(&backstore_name, path, &target_iqn, "block")
            .await?;

        Ok(target_iqn)
    }

    async fn create_iscsi_target_internal(
        &self,
        backstore_name: &str,
        path: &str,
        target_iqn: &str,
        backstore_type: &str,
    ) -> anyhow::Result<()> {
        // 1. Handle Backstore
        // Check if backstore exists
        if self
            .backstore_exists(backstore_type, backstore_name)
            .await?
        {
            // Delete it to ensure we are pointing to the correct path (especially for updates)
            tracing::info!(
                "Backstore {} already exists, deleting to update.",
                backstore_name
            );
            let delete_cmd = format!(
                "/backstores/{}/ delete name={}",
                backstore_type, backstore_name
            );
            self.run_targetcli(&delete_cmd).await?;
        }

        // Create Backstore
        let create_backstore_cmd = format!(
            "/backstores/{} create name={} file_or_dev={}",
            backstore_type, backstore_name, path
        );
        self.run_targetcli(&create_backstore_cmd)
            .await
            .map_err(|e| {
                // Enhance error message
                anyhow::anyhow!("Failed to create backstore {}: {}", backstore_name, e)
            })?;

        // 2. Create iSCSI Target if not exists
        if !self.target_exists(target_iqn).await? {
            tracing::info!("Creating iSCSI target: {}", target_iqn);
            self.run_targetcli(&format!("/iscsi create wwn={}", target_iqn))
                .await?;

            // Set Attributes
            let tpg_path = format!("/iscsi/{}/tpg1", target_iqn);
            self.run_targetcli(&format!("{} set attribute generate_node_acls=1", tpg_path))
                .await?;
            self.run_targetcli(&format!("{} set attribute cache_dynamic_acls=1", tpg_path))
                .await?;
            self.run_targetcli(&format!(
                "{} set attribute demo_mode_write_protect=0",
                tpg_path
            ))
            .await?;
            self.run_targetcli(&format!("{} set attribute authentication=0", tpg_path))
                .await?;
        } else {
            tracing::info!(
                "iSCSI target {} already exists, skipping creation.",
                target_iqn
            );
        }

        // 3. Create LUN if not exists
        // Note: LUN check is a bit tricky with just LS, but typically we want LUN 0.
        // Let's assume we want to map this backstore to a LUN.
        // We can check if *any* LUN points to this backstore, or just try to create.
        // The reference implementation checks `lun_exists` by grepping the path.

        let lun_path = format!("/backstores/{}/{}", backstore_type, backstore_name);
        if !self.lun_exists(target_iqn, &lun_path).await? {
            tracing::info!("Creating LUN for backstore {}", backstore_name);
            self.run_targetcli(&format!(
                "/iscsi/{}/tpg1/luns create storage_object={}",
                target_iqn, lun_path
            ))
            .await?;
        }

        // 4. Ensure Portal (0.0.0.0:3260)
        if !self.portal_exists(target_iqn).await? {
            tracing::info!("Creating portal for target {}", target_iqn);
            self.run_targetcli(&format!(
                "/iscsi/{}/tpg1/portals/ create 0.0.0.0 3260",
                target_iqn
            ))
            .await?;
        }

        // 5. Save configuration
        self.save_config().await?;

        tracing::info!("iSCSI target ensured: {}", target_iqn);
        Ok(())
    }

    async fn backstore_exists(&self, backstore_type: &str, name: &str) -> anyhow::Result<bool> {
        let output = self
            .run_targetcli_output(&format!("/backstores/{} ls", backstore_type))
            .await?;
        Ok(output.contains(name))
    }

    async fn target_exists(&self, iqn: &str) -> anyhow::Result<bool> {
        let output = self.run_targetcli_output("/iscsi ls").await?;
        Ok(output.contains(iqn))
    }

    async fn lun_exists(&self, iqn: &str, lun_path: &str) -> anyhow::Result<bool> {
        let output = self
            .run_targetcli_output(&format!("/iscsi/{}/tpg1/luns ls", iqn))
            .await?;
        Ok(output.contains(lun_path))
    }

    async fn portal_exists(&self, iqn: &str) -> anyhow::Result<bool> {
        let output = self
            .run_targetcli_output(&format!("/iscsi/{}/tpg1/portals/ ls", iqn))
            .await?;
        Ok(output.contains("0.0.0.0"))
    }

    async fn run_targetcli_output(&self, command: &str) -> anyhow::Result<String> {
        let output = Command::new("sudo")
            .arg("-n")
            .arg("targetcli")
            .arg(command)
            .output()
            .await?;

        if !output.status.success() {
            // For existence checks, sometimes failure means not found, but 'ls' usually succeeds.
            // If ls fails, it's an error.
            return Err(anyhow::anyhow!("targetcli command failed: {}", command));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    pub async fn remove_target(&self, name: &str) -> anyhow::Result<()> {
        let backstore_name = format!("{}-disk", name); // Assuming client naming convention for now, or we need to pass strict name
                                                       // Wait, remove_target uses `image.name` usually.
                                                       // If it is a client, name is client name.
                                                       // Since we have separate `create_client_target`, we should probably handle both logic or split.
                                                       // But `remove_target` signature takes just name.
                                                       // Let's try removing both possibilities for backstore name key.

        let target_iqn = format!("{}:client.{}", self.settings.iscsi.target_prefix, name);
        // Also possibly master image IQN?
        let target_iqn_master = format!("{}:{}", self.settings.iscsi.target_prefix, name);

        // 1. Delete iSCSI Target
        // Try client IQN
        let _ = self
            .run_targetcli(&format!("/iscsi delete wwn={}", target_iqn))
            .await;
        // Try master IQN
        let _ = self
            .run_targetcli(&format!("/iscsi delete wwn={}", target_iqn_master))
            .await;

        // 2. Delete Backstore
        // Try block backstore (client)
        let _ = self
            .run_targetcli(&format!("/backstores/block delete name={}", backstore_name))
            .await;

        // Try fileio backstore (master/image name)
        let _ = self
            .run_targetcli(&format!("/backstores/fileio delete name={}", name))
            .await;

        // 3. Save configuration
        self.save_config().await?;

        tracing::info!("iSCSI target removed for: {}", name);
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
        tracing::info!("iSCSI service started");
        Ok(())
    }

    pub async fn stop(&self) -> anyhow::Result<()> {
        run_sudo_command(["systemctl", "stop", "rtslib-fb-targetctl"]).await?;
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
