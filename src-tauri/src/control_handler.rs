use crate::command_builder::CommandBuilder;
use crate::error::AppError;
use crate::os_detector::OsDetector;
use crate::remote_desktop_launcher::{RemoteDesktopLauncher, RemoteDesktopResponse};
use crate::ssh_executor::SshExecutor;
use crate::core::client::Client;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Response from a control operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlResponse {
    pub success: bool,
    pub message: String,
    pub operation_id: Option<String>,
    pub timestamp: String,
}

/// Result of a control operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationResult {
    Success,
    Failed(String),
    Timeout,
    Cancelled,
}

impl std::fmt::Display for OperationResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OperationResult::Success => write!(f, "success"),
            OperationResult::Failed(msg) => write!(f, "failed: {}", msg),
            OperationResult::Timeout => write!(f, "timeout"),
            OperationResult::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Audit log entry for control operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub id: String,
    pub client_id: String,
    pub client_name: String,
    pub client_ip: String,
    pub os_type: String,
    pub operation_type: String,
    pub operation_mode: String,
    pub delay_minutes: Option<u32>,
    pub result: String,
    pub result_message: Option<String>,
    pub duration_ms: u64,
    pub timestamp: String,
}

/// Control handler for managing shutdown, reboot, and remote operations
pub struct ControlHandler {
    ssh_executor: Arc<SshExecutor>,
    remote_desktop_launcher: Arc<RemoteDesktopLauncher>,
}

impl ControlHandler {
    /// Create a new control handler
    pub fn new(ssh_executor: Arc<SshExecutor>) -> Self {
        let remote_desktop_launcher = Arc::new(RemoteDesktopLauncher::new(ssh_executor.clone()));
        Self {
            ssh_executor,
            remote_desktop_launcher,
        }
    }

    /// Handle shutdown request for a client
    ///
    /// # Arguments
    /// * `client` - The client to shutdown
    /// * `force` - If true, use force shutdown; if false, use graceful shutdown
    /// * `delay_minutes` - Optional delay in minutes before shutdown
    /// * `master_os` - The OS type from the master image
    ///
    /// # Returns
    /// A ControlResponse with the result of the operation
    pub async fn handle_shutdown(
        &self,
        client: &Client,
        force: bool,
        delay_minutes: Option<u32>,
        master_os: Option<&str>,
    ) -> Result<(ControlResponse, AuditLogEntry), AppError> {
        let start_time = std::time::Instant::now();
        let operation_id = uuid::Uuid::new_v4().to_string();

        debug!(
            "Handling shutdown request for client {} (force={}, delay={:?})",
            client.name, force, delay_minutes
        );

        // Detect OS type
        let os_type = OsDetector::get_os_type_with_fallback(client, master_os);

        // Build shutdown command
        let command = match CommandBuilder::build_shutdown_command(os_type, force, delay_minutes) {
            Ok(cmd) => cmd,
            Err(e) => {
                error!("Failed to build shutdown command for {}: {}", client.name, e);
                let response = ControlResponse {
                    success: false,
                    message: format!("Failed to build shutdown command: {}", e),
                    operation_id: Some(operation_id.clone()),
                    timestamp: Utc::now().to_rfc3339(),
                };
                let audit_entry = AuditLogEntry {
                    id: operation_id,
                    client_id: client.id.clone(),
                    client_name: client.name.clone(),
                    client_ip: client.ip.clone(),
                    os_type: os_type.to_string(),
                    operation_type: "shutdown".to_string(),
                    operation_mode: if force { "force" } else { "graceful" }.to_string(),
                    delay_minutes,
                    result: "failed".to_string(),
                    result_message: Some(e.to_string()),
                    duration_ms: start_time.elapsed().as_millis() as u64,
                    timestamp: Utc::now().to_rfc3339(),
                };
                return Ok((response, audit_entry));
            }
        };

        // Execute shutdown command
        match self.ssh_executor.execute_with_retry(&client.ip, &command).await {
            Ok(result) => {
                if result.exit_code == 0 {
                    info!(
                        "Shutdown command executed successfully on {} ({})",
                        client.name, client.ip
                    );
                    let response = ControlResponse {
                        success: true,
                        message: format!(
                            "Shutdown command sent to {} ({})",
                            client.name, client.ip
                        ),
                        operation_id: Some(operation_id.clone()),
                        timestamp: Utc::now().to_rfc3339(),
                    };
                    let audit_entry = AuditLogEntry {
                        id: operation_id,
                        client_id: client.id.clone(),
                        client_name: client.name.clone(),
                        client_ip: client.ip.clone(),
                        os_type: os_type.to_string(),
                        operation_type: "shutdown".to_string(),
                        operation_mode: if force { "force" } else { "graceful" }.to_string(),
                        delay_minutes,
                        result: "success".to_string(),
                        result_message: None,
                        duration_ms: start_time.elapsed().as_millis() as u64,
                        timestamp: Utc::now().to_rfc3339(),
                    };
                    Ok((response, audit_entry))
                } else {
                    warn!(
                        "Shutdown command failed on {} with exit code {}: {}",
                        client.name, result.exit_code, result.stderr
                    );
                    let error_msg = format!(
                        "Shutdown command failed with exit code {}: {}",
                        result.exit_code, result.stderr
                    );
                    let response = ControlResponse {
                        success: false,
                        message: error_msg.clone(),
                        operation_id: Some(operation_id.clone()),
                        timestamp: Utc::now().to_rfc3339(),
                    };
                    let audit_entry = AuditLogEntry {
                        id: operation_id,
                        client_id: client.id.clone(),
                        client_name: client.name.clone(),
                        client_ip: client.ip.clone(),
                        os_type: os_type.to_string(),
                        operation_type: "shutdown".to_string(),
                        operation_mode: if force { "force" } else { "graceful" }.to_string(),
                        delay_minutes,
                        result: "failed".to_string(),
                        result_message: Some(error_msg),
                        duration_ms: start_time.elapsed().as_millis() as u64,
                        timestamp: Utc::now().to_rfc3339(),
                    };
                    Ok((response, audit_entry))
                }
            }
            Err(e) => {
                error!("SSH execution failed for shutdown on {}: {}", client.name, e);
                let error_msg = e.to_string();
                let response = ControlResponse {
                    success: false,
                    message: format!("Shutdown operation failed: {}", error_msg),
                    operation_id: Some(operation_id.clone()),
                    timestamp: Utc::now().to_rfc3339(),
                };
                let audit_entry = AuditLogEntry {
                    id: operation_id,
                    client_id: client.id.clone(),
                    client_name: client.name.clone(),
                    client_ip: client.ip.clone(),
                    os_type: os_type.to_string(),
                    operation_type: "shutdown".to_string(),
                    operation_mode: if force { "force" } else { "graceful" }.to_string(),
                    delay_minutes,
                    result: "failed".to_string(),
                    result_message: Some(error_msg),
                    duration_ms: start_time.elapsed().as_millis() as u64,
                    timestamp: Utc::now().to_rfc3339(),
                };
                Ok((response, audit_entry))
            }
        }
    }

    /// Handle reboot request for a client
    ///
    /// # Arguments
    /// * `client` - The client to reboot
    /// * `force` - If true, use force reboot; if false, use graceful reboot
    /// * `delay_minutes` - Optional delay in minutes before reboot
    /// * `master_os` - The OS type from the master image
    ///
    /// # Returns
    /// A ControlResponse with the result of the operation
    pub async fn handle_reboot(
        &self,
        client: &Client,
        force: bool,
        delay_minutes: Option<u32>,
        master_os: Option<&str>,
    ) -> Result<(ControlResponse, AuditLogEntry), AppError> {
        let start_time = std::time::Instant::now();
        let operation_id = uuid::Uuid::new_v4().to_string();

        debug!(
            "Handling reboot request for client {} (force={}, delay={:?})",
            client.name, force, delay_minutes
        );

        // Detect OS type
        let os_type = OsDetector::get_os_type_with_fallback(client, master_os);

        // Build reboot command
        let command = match CommandBuilder::build_reboot_command(os_type, force, delay_minutes) {
            Ok(cmd) => cmd,
            Err(e) => {
                error!("Failed to build reboot command for {}: {}", client.name, e);
                let response = ControlResponse {
                    success: false,
                    message: format!("Failed to build reboot command: {}", e),
                    operation_id: Some(operation_id.clone()),
                    timestamp: Utc::now().to_rfc3339(),
                };
                let audit_entry = AuditLogEntry {
                    id: operation_id,
                    client_id: client.id.clone(),
                    client_name: client.name.clone(),
                    client_ip: client.ip.clone(),
                    os_type: os_type.to_string(),
                    operation_type: "reboot".to_string(),
                    operation_mode: if force { "force" } else { "graceful" }.to_string(),
                    delay_minutes,
                    result: "failed".to_string(),
                    result_message: Some(e.to_string()),
                    duration_ms: start_time.elapsed().as_millis() as u64,
                    timestamp: Utc::now().to_rfc3339(),
                };
                return Ok((response, audit_entry));
            }
        };

        // Execute reboot command
        match self.ssh_executor.execute_with_retry(&client.ip, &command).await {
            Ok(result) => {
                if result.exit_code == 0 {
                    info!(
                        "Reboot command executed successfully on {} ({})",
                        client.name, client.ip
                    );
                    let response = ControlResponse {
                        success: true,
                        message: format!(
                            "Reboot command sent to {} ({})",
                            client.name, client.ip
                        ),
                        operation_id: Some(operation_id.clone()),
                        timestamp: Utc::now().to_rfc3339(),
                    };
                    let audit_entry = AuditLogEntry {
                        id: operation_id,
                        client_id: client.id.clone(),
                        client_name: client.name.clone(),
                        client_ip: client.ip.clone(),
                        os_type: os_type.to_string(),
                        operation_type: "reboot".to_string(),
                        operation_mode: if force { "force" } else { "graceful" }.to_string(),
                        delay_minutes,
                        result: "success".to_string(),
                        result_message: None,
                        duration_ms: start_time.elapsed().as_millis() as u64,
                        timestamp: Utc::now().to_rfc3339(),
                    };
                    Ok((response, audit_entry))
                } else {
                    warn!(
                        "Reboot command failed on {} with exit code {}: {}",
                        client.name, result.exit_code, result.stderr
                    );
                    let error_msg = format!(
                        "Reboot command failed with exit code {}: {}",
                        result.exit_code, result.stderr
                    );
                    let response = ControlResponse {
                        success: false,
                        message: error_msg.clone(),
                        operation_id: Some(operation_id.clone()),
                        timestamp: Utc::now().to_rfc3339(),
                    };
                    let audit_entry = AuditLogEntry {
                        id: operation_id,
                        client_id: client.id.clone(),
                        client_name: client.name.clone(),
                        client_ip: client.ip.clone(),
                        os_type: os_type.to_string(),
                        operation_type: "reboot".to_string(),
                        operation_mode: if force { "force" } else { "graceful" }.to_string(),
                        delay_minutes,
                        result: "failed".to_string(),
                        result_message: Some(error_msg),
                        duration_ms: start_time.elapsed().as_millis() as u64,
                        timestamp: Utc::now().to_rfc3339(),
                    };
                    Ok((response, audit_entry))
                }
            }
            Err(e) => {
                error!("SSH execution failed for reboot on {}: {}", client.name, e);
                let error_msg = e.to_string();
                let response = ControlResponse {
                    success: false,
                    message: format!("Reboot operation failed: {}", error_msg),
                    operation_id: Some(operation_id.clone()),
                    timestamp: Utc::now().to_rfc3339(),
                };
                let audit_entry = AuditLogEntry {
                    id: operation_id,
                    client_id: client.id.clone(),
                    client_name: client.name.clone(),
                    client_ip: client.ip.clone(),
                    os_type: os_type.to_string(),
                    operation_type: "reboot".to_string(),
                    operation_mode: if force { "force" } else { "graceful" }.to_string(),
                    delay_minutes,
                    result: "failed".to_string(),
                    result_message: Some(error_msg),
                    duration_ms: start_time.elapsed().as_millis() as u64,
                    timestamp: Utc::now().to_rfc3339(),
                };
                Ok((response, audit_entry))
            }
        }
    }

    /// Handle remote desktop request for a client
    ///
    /// # Arguments
    /// * `client` - The client to launch remote desktop for
    /// * `master_os` - The OS type from the master image
    ///
    /// # Returns
    /// A RemoteDesktopResponse with the result of the operation
    pub async fn handle_remote_desktop(
        &self,
        client: &Client,
        master_os: Option<&str>,
    ) -> Result<RemoteDesktopResponse, AppError> {
        debug!(
            "Handling remote desktop request for client {} ({})",
            client.name, client.ip
        );

        // Detect OS type
        let os_type = OsDetector::get_os_type_with_fallback(client, master_os);

        // Launch remote desktop
        match self
            .remote_desktop_launcher
            .launch_remote_desktop(client, os_type)
            .await
        {
            Ok(response) => {
                info!(
                    "Remote desktop launched successfully for {} ({}) using {}",
                    client.name, client.ip, response.protocol_used
                );
                Ok(response)
            }
            Err(e) => {
                error!(
                    "Failed to launch remote desktop for {} ({}): {}",
                    client.name, client.ip, e
                );
                Err(e)
            }
        }
    }
}
mod tests {
    use super::*;

    #[test]
    fn test_control_response_creation() {
        let response = ControlResponse {
            success: true,
            message: "Test message".to_string(),
            operation_id: Some("op_123".to_string()),
            timestamp: Utc::now().to_rfc3339(),
        };
        assert!(response.success);
        assert_eq!(response.message, "Test message");
        assert_eq!(response.operation_id, Some("op_123".to_string()));
    }

    #[test]
    fn test_operation_result_display() {
        assert_eq!(OperationResult::Success.to_string(), "success");
        assert_eq!(
            OperationResult::Failed("test error".to_string()).to_string(),
            "failed: test error"
        );
        assert_eq!(OperationResult::Timeout.to_string(), "timeout");
        assert_eq!(OperationResult::Cancelled.to_string(), "cancelled");
    }

    #[test]
    fn test_audit_log_entry_creation() {
        let entry = AuditLogEntry {
            id: "log_123".to_string(),
            client_id: "1".to_string(),
            client_name: "test-client".to_string(),
            client_ip: "192.168.1.100".to_string(),
            os_type: "linux".to_string(),
            operation_type: "shutdown".to_string(),
            operation_mode: "graceful".to_string(),
            delay_minutes: None,
            result: "success".to_string(),
            result_message: None,
            duration_ms: 1250,
            timestamp: Utc::now().to_rfc3339(),
        };
        assert_eq!(entry.client_name, "test-client");
        assert_eq!(entry.operation_type, "shutdown");
        assert_eq!(entry.result, "success");
    }

    #[test]
    fn test_control_handler_creation() {
        let ssh_executor = Arc::new(SshExecutor::new());
        let handler = ControlHandler::new(ssh_executor);
        // Just verify it can be created
        assert!(true);
    }
}
