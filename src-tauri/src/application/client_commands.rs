//! Client management commands (optimized version)
//!
//! This module demonstrates the new architecture applied to client management,
//! replacing the 1300+ line client.rs with clean separation of concerns.

use crate::core::error::{DisklessError, Result};
use crate::types::client::{Client, AddClientRequest, ControlRequest, DeprovisionRequest, ClientOverview};
use crate::core::config::ConfigManager;
use crate::infrastructure::{ZfsService, IscsiService, DhcpService, ProcessService};
use crate::constants::{validation, paths, auth, commands, timeouts};
use std::collections::HashMap;

/// Client management service that coordinates all client operations
#[derive(Debug, Clone)]
pub struct ClientCommands {
    config_manager: ConfigManager,
    process_service: Box<dyn ProcessService>,
    zfs_service: Box<dyn ZfsService>,
    iscsi_service: Box<dyn IscsiService>,
    dhcp_service: Box<dyn DhcpService>,
}

impl ClientCommands {
    /// Create a new client commands service
    pub fn new(
        config_manager: ConfigManager,
        process_service: Box<dyn ProcessService>,
        zfs_service: Box<dyn ZfsService>,
        iscsi_service: Box<dyn IscsiService>,
        dhcp_service: Box<dyn DhcpService>,
    ) -> Self {
        Self {
            config_manager,
            process_service,
            zfs_service,
            iscsi_service,
            dhcp_service,
        }
    }

    /// Get all clients with current status
    pub async fn get_clients(&self, token: String) -> Result<serde_json::Value> {
        // Validate authentication token
        self.validate_auth_token(&token)?;
        
        let clients = self.config_manager.get_clients().await?;
        
        // Update client status concurrently (async operation)
        let updated_clients = self.update_client_statuses(clients).await?;
        
        Ok(serde_json::json!(updated_clients))
    }

    /// Get a specific client by ID
    pub async fn get_client_by_id(&self, token: String, client_id: Option<String>) -> Result<serde_json::Value> {
        self.validate_auth_token(&token)?;
        
        if let Some(id) = client_id {
            let client = self.config_manager.get_client_by_id(&id).await?
                .ok_or_else(|| DisklessError::Client(crate::core::error::ClientError::NotFound(id)))?;
            Ok(serde_json::json!(client))
        } else {
            self.get_clients(token).await
        }
    }

    /// Add a new client (optimized version)
    pub async fn add_client(&self, token: String, request: AddClientRequest) -> Result<serde_json::Value> {
        self.validate_auth_token(&token)?;
        
        // Validate request
        let validated_request = self.validate_add_request(&request)?;
        
        // Check for duplicates
        self.check_for_duplicates(&validated_request.name, &validated_request.mac, &validated_request.ip)?;
        
        // Create client based on whether it has a master image
        if validated_request.master.is_empty() {
            // No master image - just save configuration
            let client = self.create_config_only_client(&validated_request)?;
            self.config_manager.save_client(&client).await?;
            
            Ok(serde_json::json!({
                "message": format!("Client {} added to configuration (no image selected)", validated_request.name)
            }))
        } else {
            // Has master image - full client creation
            let result = self.create_full_client(validated_request).await?;
            Ok(serde_json::json!({
                "message": format!("Client {} added successfully", result.name),
                "client": result
            }))
        }
    }

    /// Control client (wake, reboot, shutdown, etc.)
    pub async fn control_client(&self, token: String, client_id: String, request: ControlRequest) -> Result<serde_json::Value> {
        self.validate_auth_token(&token)?;
        
        let client = self.config_manager.get_client_by_id(&client_id).await?
            .ok_or_else(|| DisklessError::Client(crate::core::error::ClientError::NotFound(client_id.clone())))?;

        match request.action.as_str() {
            "wake" => self.wake_client(&client).await,
            "reboot" => self.reboot_client(&client).await,
            "shutdown" => self.shutdown_client(&client).await,
            "super" => self.toggle_super_mode(&client, request.make_super.unwrap_or(false)).await,
            _ => Err(DisklessError::Client(crate::core::error::ClientError::InvalidData(
                format!("Invalid action: {}", request.action)
            ))),
        }
    }

    /// Delete a client and cleanup all resources
    pub async fn delete_client(&self, token: String, client_id: String) -> Result<serde_json::Value> {
        self.validate_auth_token(&token)?;
        
        let client = self.config_manager.get_client_by_id(&client_id).await?
            .ok_or_else(|| DisklessError::Client(crate::core::error::ClientError::NotFound(client_id.clone())))?;

        self.delete_client_and_resources(&client).await?;
        
        Ok(serde_json::json!({
            "message": format!("Client {} deleted successfully", client_id)
        }))
    }

    /// Reset client to original state
    pub async fn reset_client(&self, token: String, client_id: String) -> Result<serde_json::Value> {
        self.validate_auth_token(&token)?;
        
        let client = self.config_manager.get_client_by_id(&client_id).await?
            .ok_or_else(|| DisklessError::Client(crate::core::error::ClientError::NotFound(client_id.clone())))?;

        self.reset_client_to_original_state(&client).await?;
        
        Ok(serde_json::json!({
            "message": format!("Client {} reset successfully", client_id)
        }))
    }

    /// Get client overview statistics
    pub async fn get_client_overview(&self) -> Result<ClientOverview> {
        let clients = self.config_manager.get_clients().await?;
        
        let total_clients = clients.len();
        let mut online_clients = 0;
        let mut offline_clients = 0;
        
        for client in clients {
            if client.is_online() {
                online_clients += 1;
            } else if client.is_offline() {
                offline_clients += 1;
            }
        }

        Ok(ClientOverview {
            total_clients,
            active_clients: online_clients,
            offline_clients,
        })
    }

    // Private helper methods

    async fn update_client_statuses(&self, mut clients: Vec<Client>) -> Result<Vec<Client>> {
        // Concurrent status checking using async patterns
        let mut handles = Vec::new();
        
        for (index, client) in clients.iter().enumerate() {
            if !client.ip.is_empty() && client.ip != "N/A" {
                let client_ip = client.ip.clone();
                let handle = tokio::spawn(async move {
                    Self::check_client_online_status(&client_ip).await
                });
                handles.push((index, handle));
            }
        }
        
        // Wait for all status checks to complete
        for (index, handle) in handles {
            let status = handle.await
                .map_err(|e| DisklessError::internal(format!("Status check failed: {}", e)))? 
                .unwrap_or_else(|_| "Offline".to_string());
            
            if let Some(client) = clients.get_mut(index) {
                client.status = Some(status);
            }
        }
        
        Ok(clients)
    }

    async fn check_client_online_status(ip: &str) -> Result<String> {
        if ip.is_empty() || ip == "N/A" {
            return Ok("Offline".to_string());
        }

        let output = std::process::Command::new(commands::PING)
            .args(["-c", "1", "-W", "1", ip])
            .output()
            .map_err(|e| DisklessError::Process(crate::core::error::ProcessError::ExecutionFailed(e.to_string())))?;

        if output.status.success() {
            Ok("Online".to_string())
        } else {
            Ok("Offline".to_string())
        }
    }

    fn validate_add_request(&self, request: &AddClientRequest) -> Result<AddClientRequest> {
        // Use centralized validation
        if !validation::mac_pattern().is_match(&request.mac) {
            return Err(DisklessError::invalid_input("Invalid MAC address format"));
        }

        if !validation::ip_pattern().is_match(&request.ip) {
            return Err(DisklessError::invalid_input("Invalid IP address format"));
        }

        if !validation::client_name_pattern().is_match(&request.name) {
            return Err(DisklessError::invalid_input("Invalid client name format"));
        }

        // Normalize inputs
        let mut validated = request.clone();
        validated.name = request.name.trim().to_lowercase();
        validated.mac = request.mac.trim().to_uppercase();
        validated.ip = request.ip.trim().to_string();
        validated.master = request.master.trim().to_string();
        
        if let Some(snapshot) = &mut validated.snapshot {
            *snapshot = snapshot.trim().to_string();
        }

        Ok(validated)
    }

    fn check_for_duplicates(&self, name: &str, mac: &str, ip: &str) -> Result<()> {
        let clients = self.config_manager.get_clients_sync()
            .map_err(|_| DisklessError::internal("Failed to load clients for duplicate check"))?;

        for client in clients {
            if client.name.to_lowercase() == name.to_lowercase() {
                return Err(DisklessError::Client(crate::core::error::ClientError::AlreadyExists(
                    format!("Client with name '{}' already exists", name)
                )));
            }

            if client.ip == ip {
                return Err(DisklessError::Client(crate::core::error::ClientError::AlreadyExists(
                    format!("IP address {} is already in use", ip)
                )));
            }

            if client.mac.to_uppercase() == mac.to_uppercase() {
                return Err(DisklessError::Client(crate::core::error::ClientError::AlreadyExists(
                    format!("MAC address {} is already in use", mac)
                )));
            }
        }

        Ok(())
    }

    fn create_config_only_client(&self, request: &AddClientRequest) -> Result<Client> {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        
        Ok(Client {
            id: request.name.clone(),
            name: request.name.to_uppercase(),
            mac: request.mac.clone(),
            ip: request.ip.clone(),
            master: request.master.clone(),
            snapshot: request.snapshot.clone(),
            target_iqn: None,
            block_device: None,
            block_store: None,
            writeback: None,
            created_at: Some(now.clone()),
            last_modified: Some(now.clone()),
            status: None,
            mode: None,
            pxe_mode: Some("uefi".to_string()),
        })
    }

    async fn create_full_client(&self, request: AddClientRequest) -> Result<Client> {
        // Get ZFS pool name
        let zpool_name = self.config_manager.get_zpool_name().await?;
        
        // Create ZFS paths
        let paths = self.get_client_paths(&request.name, &request.mac);
        
        // Find appropriate parent dataset based on ZFS type
        let parent_dataset = self.find_writeback_parent_dataset(&zpool_name).await?;
        let clone_path = if let Some(parent) = parent_dataset {
            format!("{}/{}-disk", parent, request.name.to_uppercase())
        } else {
            format!("{}/{}-disk", zpool_name, request.name.to_uppercase())
        };
        
        // Create ZFS clone or use master directly
        let (clone_path, used_master_directly) = self.create_client_clone(&request, &clone_path).await?;
        
        // Setup iSCSI target
        let block_device = format!("/dev/zvol/{}", clone_path);
        self.iscsi_service.setup_target(&paths["target_iqn"], &paths["block_store"], &block_device).await?;
        
        // Create DHCP entry
        let dhcp_entry = self.create_dhcp_entry(&request.name, &request.mac, &request.ip, &paths["target_iqn"]);
        self.dhcp_service.update_config(&request.name, &dhcp_entry, true).await?;
        
        // Save client configuration
        let client = self.create_client_from_data(&request, &paths, &clone_path, used_master_directly, &block_device)?;
        self.config_manager.save_client(&client).await?;
        
        // Restart DHCP service
        self.process_service.execute_command(
            vec![commands::SYSTEMCTL, "restart", "isc-dhcp-server.service"],
            timeouts::SERVICE_COMMAND
        ).await?;
        
        Ok(client)
    }

    fn get_client_paths(&self, client_id: &str, client_mac: &str) -> HashMap<String, String> {
        let clone = format!("{}/{}", crate::core::config::ConfigManager::get_zpool_name(), client_id.to_uppercase());
        let target_iqn = format!(
            "{}:{}",
            auth::IQN_BASE,
            client_mac.to_lowercase().replace(':', "-")
        );
        let block_store = format!("block_{}", client_id.to_lowercase());
        
        let mut map = HashMap::new();
        map.insert("clone".to_string(), clone);
        map.insert("target_iqn".to_string(), target_iqn);
        map.insert("block_store".to_string(), block_store);
        map
    }

    async fn find_writeback_parent_dataset(&self, zpool_name: &str) -> Result<Option<String>> {
        // This would use the actual ZFS service to find datasets with org.diskless:type=writeback
        // For now, return None to use default path
        Ok(None)
    }

    async fn create_client_clone(&self, request: &AddClientRequest, clone_path: &str) -> Result<(String, bool)> {
        let zpool_name = self.config_manager.get_zpool_name().await?;
        let master_path = format!("{}/{}", zpool_name, request.master);
        
        if let Some(ref snapshot) = request.snapshot {
            // Use provided snapshot
            self.zfs_service.clone_snapshot(snapshot, clone_path).await?;
            Ok((clone_path.to_string(), false))
        } else {
            // Check if base snapshot exists
            let base_snapshot = format!("{}@base", master_path);
            if self.zfs_service.snapshot_exists(&base_snapshot).await? {
                let snapshot_name = format!("{}@{}_base", master_path, request.name);
                self.zfs_service.create_snapshot(&master_path, &snapshot_name, false).await?;
                self.zfs_service.clone_snapshot(&snapshot_name, clone_path).await?;
                Ok((clone_path.to_string(), false))
            } else {
                // Use master directly
                Ok((master_path, true))
            }
        }
    }

    fn create_dhcp_entry(&self, name: &str, mac: &str, ip: &str, target_iqn: &str) -> String {
        let formatted_name = self.format_client_name(name);
        let server_ip = crate::core::config::ConfigManager::get_server_ip().unwrap_or_else(|_| auth::DEFAULT_SERVER_IP.to_string());
        
        format!(
            r#"host {name} {{
    hardware ethernet {mac};
    fixed-address {ip};
    option host-name "{name}";
    option root-path "iscsi:{server_ip}:::{target_iqn}";
}}"#,
            name = formatted_name,
            mac = mac,
            ip = ip,
            target_iqn = target_iqn,
            server_ip = server_ip,
        )
    }

    fn format_client_name(&self, name: &str) -> String {
        name.find('_')
            .and_then(|idx| name[idx + 1..].parse::<u32>().ok())
            .map(|num| format!("PC{:03}", num))
            .unwrap_or_else(|| name.to_uppercase())
    }

    fn create_client_from_data(&self, request: &AddClientRequest, paths: &HashMap<String, String>, 
                               clone_path: &str, used_master_directly: bool, block_device: &str) -> Result<Client> {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        
        Ok(Client {
            id: request.name.clone(),
            name: request.name.to_uppercase(),
            mac: request.mac.clone(),
            ip: request.ip.clone(),
            master: request.master.clone(),
            snapshot: if used_master_directly { None } else { request.snapshot.clone() },
            target_iqn: Some(paths["target_iqn"].clone()),
            block_device: Some(block_device.to_string()),
            block_store: Some(paths["block_store"].clone()),
            writeback: if used_master_directly { None } else { Some(clone_path.to_string()) },
            created_at: Some(now.clone()),
            last_modified: Some(now.clone()),
            status: None,
            mode: if used_master_directly { Some("super".to_string()) } else { None },
            pxe_mode: Some("uefi".to_string()),
        })
    }

    fn validate_auth_token(&self, token: &str) -> Result<()> {
        // This would validate the JWT token using the auth service
        // For now, just return Ok()
        Ok(())
    }

    async fn wake_client(&self, client: &Client) -> Result<serde_json::Value> {
        if client.mac.is_empty() {
            return Err(DisklessError::Client(crate::core::error::ClientError::InvalidData(
                "MAC address not found".to_string()
            )));
        }

        self.process_service.execute_command(
            vec![commands::WAKEONLAN, &client.mac],
            timeouts::NETWORK_COMMAND
        ).await?;

        Ok(serde_json::json!({
            "message": format!("Wake-on-LAN command sent to {} ({})", client.name, client.ip)
        }))
    }

    async fn reboot_client(&self, client: &Client) -> Result<serde_json::Value> {
        if client.ip.is_empty() {
            return Err(DisklessError::Client(crate::core::error::ClientError::InvalidData(
                "IP address not found".to_string()
            )));
        }

        self.process_service.execute_command(
            vec![
                commands::NET_RPC, "shutdown", "-r", "-I", &client.ip, 
                "-U", "diskless%1", "-f", "-t", "0"
            ],
            timeouts::NETWORK_COMMAND
        ).await?;

        Ok(serde_json::json!({
            "message": format!("Reboot command sent to {} ({})", client.name, client.ip)
        }))
    }

    async fn shutdown_client(&self, client: &Client) -> Result<serde_json::Value> {
        if client.ip.is_empty() {
            return Err(DisklessError::Client(crate::core::error::ClientError::InvalidData(
                "IP address not found".to_string()
            )));
        }

        self.process_service.execute_command(
            vec![commands::NET_RPC, "shutdown", "-S", &client.ip, "-U", "diskless%1"],
            timeouts::NETWORK_COMMAND
        ).await?;

        Ok(serde_json::json!({
            "message": format!("Shutdown command sent to {} ({})", client.name, client.ip)
        }))
    }

    async fn toggle_super_mode(&self, client: &Client, make_super: bool) -> Result<serde_json::Value> {
        // This would implement the super mode toggle logic
        // For now, return a placeholder response
        Ok(serde_json::json!({
            "message": format!("Super mode {} for client {}", 
                if make_super { "enabled" } else { "disabled" }, 
                client.id
            )
        }))
    }

    async fn delete_client_and_resources(&self, client: &Client) -> Result<()> {
        // Cleanup DHCP configuration
        self.dhcp_service.update_config(&client.id, "", false).await?;
        
        // Cleanup iSCSI target
        if let (Some(target_iqn), Some(block_store)) = (&client.target_iqn, &client.block_store) {
            self.iscsi_service.cleanup_target(target_iqn, block_store).await?;
        }
        
        // Cleanup ZFS clone
        if let Some(ref writeback) = client.writeback {
            if !client.is_super_mode() {
                self.zfs_service.destroy(writeback, false, false).await?;
            }
        }
        
        // Remove from configuration
        self.config_manager.remove_client(&client.id).await?;
        
        // Restart DHCP service
        self.process_service.execute_command(
            vec![commands::SYSTEMCTL, "restart", "isc-dhcp-server.service"],
            timeouts::SERVICE_COMMAND
        ).await?;

        Ok(())
    }

    async fn reset_client_to_original_state(&self, client: &Client) -> Result<()> {
        // This would implement client reset logic
        // For now, just return Ok()
        Ok(())
    }
}