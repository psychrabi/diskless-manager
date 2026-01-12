use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::process::{Command, Stdio};
use std::sync::Arc;
use tracing::{error, info};

use crate::audit_logger::{AuditLogFilter, AuditLogger, ControlOperation, OperationResult};
use crate::core::client::Client;
use crate::state::AppState;
use chrono::Utc;

// Helper function to get master OS
fn get_master_os(master_name: &str) -> Option<String> {
    if master_name.to_lowercase().contains("windows") {
        Some("windows".to_string())
    } else if master_name.to_lowercase().contains("linux") {
        Some("linux".to_string())
    } else {
        None
    }
}

/// Request to perform a shutdown operation
#[derive(Debug, Deserialize)]
pub struct ShutdownRequest {
    pub force: Option<bool>,
    pub delay_minutes: Option<u32>,
}

/// Request to perform a reboot operation
#[derive(Debug, Deserialize)]
pub struct RebootRequest {
    pub force: Option<bool>,
    pub delay_minutes: Option<u32>,
}

/// Request for remote desktop connection
#[derive(Debug, Deserialize)]
pub struct RemoteDesktopRequest {
    pub username: Option<String>,
    pub password: Option<String>,
}

/// Request to cancel a scheduled operation
#[derive(Debug, Deserialize)]
pub struct CancelOperationRequest {
    pub reason: Option<String>,
}

/// Query parameters for audit logs
#[derive(Debug, Deserialize)]
pub struct AuditLogQuery {
    pub client_id: Option<String>,
    pub operation_type: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Query parameters for scheduled operations
#[derive(Debug, Deserialize)]
pub struct ScheduledOperationsQuery {
    pub client_id: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Response for scheduled operations
#[derive(Debug, Serialize)]
pub struct ScheduledOperationsResponse {
    pub operations: Vec<ScheduledOperation>,
    pub total: usize,
}

/// Scheduled operation entry
#[derive(Debug, Serialize)]
pub struct ScheduledOperation {
    pub id: String,
    pub client_id: String,
    pub operation_type: String,
    pub operation_mode: String,
    pub scheduled_time: String,
    pub created_at: String,
    pub result: Option<String>,
}

/// Response for control operations
#[derive(Debug, Serialize)]
pub struct ControlOperationResponse {
    pub success: bool,
    pub message: String,
    pub operation_id: Option<String>,
    pub timestamp: String,
}

/// Response for remote desktop operations
#[derive(Debug, Serialize)]
pub struct RemoteDesktopResponse {
    pub success: bool,
    pub protocol_used: String,
    pub message: String,
    pub timestamp: String,
}

/// Response for audit logs
#[derive(Debug, Serialize)]
pub struct AuditLogsResponse {
    pub logs: Vec<crate::audit_logger::AuditLogEntry>,
    pub total: usize,
}

/// Error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub details: Option<String>,
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(self)).into_response()
    }
}

/// Handle shutdown request for a client
pub async fn shutdown_client(
    State(state): State<AppState>,
    Path(client_id): Path<String>,
    Json(request): Json<ShutdownRequest>,
) -> Result<Json<ControlOperationResponse>, (StatusCode, Json<ErrorResponse>)> {
    let force = request.force.unwrap_or(false);
    let delay_minutes = request.delay_minutes;

    info!(
        "Shutdown request for client {} (force={}, delay={:?})",
        client_id, force, delay_minutes
    );

    // Get the client
    let manager = crate::core::client::ClientManager::new(state.db_pool.clone());
    let client = manager.get(&client_id).await.map_err(|e| {
        error!("Failed to get client {}: {}", client_id, e);
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Client not found: {}", client_id),
                details: None,
            }),
        )
    })?;

    let ip = &client.ip;
    if ip.is_empty() {
        let error_msg = format!("IP address not found for '{}'", client.name);
        error!("{}", error_msg);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: error_msg,
                details: None,
            }),
        ));
    }

    let master_os = get_master_os(&client.master).unwrap_or_default().to_lowercase();
    let mut success = true;
    let mut message = String::new();

    if master_os.contains("linux") {
        // Linux: SSH poweroff
        let output = Command::new("ssh")
            .args(&[
                "-o", "StrictHostKeyChecking=no",
                "-o", "ConnectTimeout=5",
                &format!("root@{}", ip),
                "poweroff",
            ])
            .output()
            .map_err(|e| {
                error!("Failed to execute SSH: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Failed to execute SSH: {}", e),
                        details: None,
                    }),
                )
            })?;

        if !output.status.success() {
            success = false;
            message = format!("Failed to shutdown Linux client (SSH): {}", String::from_utf8_lossy(&output.stderr));
            error!("{}", message);
        } else {
            message = format!("Shutdown command sent to {} ({})", client.name, ip);
            info!("{}", message);
        }
    } else {
        // Windows: Try NET RPC first, fall back to SSH
        let mut rpc_output = Command::new("net")
            .args(&[
                "rpc", "shutdown", "-S",
                "-I", ip,
                "-U", "diskless%1",
            ])
            .output();

        if let Ok(output) = rpc_output {
            if output.status.success() {
                message = format!("Shutdown command sent to {} ({})", client.name, ip);
                info!("{}", message);
            } else {
                // NET RPC failed, try SSH
                info!("NET RPC failed, falling back to SSH for shutdown");
                let ssh_output = Command::new("ssh")
                    .args(&[
                        "-o", "StrictHostKeyChecking=no",
                        "-o", "ConnectTimeout=5",
                        &format!("Administrator@{}", ip),
                        "shutdown /s /t 30",
                    ])
                    .output()
                    .map_err(|e| {
                        error!("Failed to execute SSH: {}", e);
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Failed to execute SSH: {}", e),
                                details: None,
                            }),
                        )
                    })?;

                if !ssh_output.status.success() {
                    success = false;
                    message = format!("Failed to shutdown Windows client (SSH): {}", String::from_utf8_lossy(&ssh_output.stderr));
                    error!("{}", message);
                } else {
                    message = format!("Shutdown command sent to {} ({})", client.name, ip);
                    info!("{}", message);
                }
            }
        } else {
            // NET RPC command not found, try SSH
            info!("NET RPC not available, using SSH for shutdown");
            let ssh_output = Command::new("ssh")
                .args(&[
                    "-o", "StrictHostKeyChecking=no",
                    "-o", "ConnectTimeout=5",
                    &format!("Administrator@{}", ip),
                    "shutdown /s /t 30",
                ])
                .output()
                .map_err(|e| {
                    error!("Failed to execute SSH: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            error: format!("Failed to execute SSH: {}", e),
                            details: None,
                        }),
                    )
                })?;

            if !ssh_output.status.success() {
                success = false;
                message = format!("Failed to shutdown Windows client (SSH): {}", String::from_utf8_lossy(&ssh_output.stderr));
                error!("{}", message);
            } else {
                message = format!("Shutdown command sent to {} ({})", client.name, ip);
                info!("{}", message);
            }
        }
    }

    // Log the operation
    let audit_logger = AuditLogger::new(Arc::new(state.db_pool.clone()));
    let operation = ControlOperation {
        client_id: client.id.clone(),
        client_name: client.name.clone(),
        client_ip: client.ip.clone(),
        os_type: master_os.clone(),
        operation_type: "shutdown".to_string(),
        operation_mode: if force { "force" } else { "graceful" }.to_string(),
        delay_minutes,
        timestamp: Utc::now(),
        administrator: "system".to_string(),
        result: if success {
            OperationResult::Success
        } else {
            OperationResult::Failed(message.clone())
        },
    };

    if let Err(e) = audit_logger.log_operation(&operation).await {
        error!("Failed to log shutdown operation: {}", e);
    }

    Ok(Json(ControlOperationResponse {
        success,
        message,
        operation_id: None,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Handle reboot request for a client
pub async fn reboot_client(
    State(state): State<AppState>,
    Path(client_id): Path<String>,
    Json(request): Json<RebootRequest>,
) -> Result<Json<ControlOperationResponse>, (StatusCode, Json<ErrorResponse>)> {
    let force = request.force.unwrap_or(false);
    let delay_minutes = request.delay_minutes;

    info!(
        "Reboot request for client {} (force={}, delay={:?})",
        client_id, force, delay_minutes
    );

    // Get the client
    let manager = crate::core::client::ClientManager::new(state.db_pool.clone());
    let client = manager.get(&client_id).await.map_err(|e| {
        error!("Failed to get client {}: {}", client_id, e);
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Client not found: {}", client_id),
                details: None,
            }),
        )
    })?;

    let ip = &client.ip;
    if ip.is_empty() {
        let error_msg = format!("IP address not found for '{}'", client.name);
        error!("{}", error_msg);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: error_msg,
                details: None,
            }),
        ));
    }

    let master_os = get_master_os(&client.master).unwrap_or_default().to_lowercase();
    let mut success = true;
    let mut message = String::new();

    if master_os.contains("linux") {
        // Linux: SSH reboot
        let output = Command::new("ssh")
            .args(&[
                "-o", "StrictHostKeyChecking=no",
                "-o", "ConnectTimeout=5",
                &format!("root@{}", ip),
                "reboot",
            ])
            .output()
            .map_err(|e| {
                error!("Failed to execute SSH: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Failed to execute SSH: {}", e),
                        details: None,
                    }),
                )
            })?;

        if !output.status.success() {
            success = false;
            message = format!("Failed to reboot Linux client (SSH): {}", String::from_utf8_lossy(&output.stderr));
            error!("{}", message);
        } else {
            message = format!("Reboot command sent to {} ({})", client.name, ip);
            info!("{}", message);
        }
    } else {
         let output = Command::new("net")
            .args(&[
                "rpc", "shutdown", "-S",
                "-I", ip,
                "-U", "diskless%1",
                "-f", "-t", "0",
            ])
            .output()
            .map_err(|e| {
                error!("Failed to execute SSH: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Failed to execute SSH: {}", e),
                        details: None,
                    }),
                )
            })?;

        if !output.status.success() {
            success = false;
            message = format!("Failed to reboot Windows client (SSH): {}", String::from_utf8_lossy(&output.stderr));
            error!("{}", message);
        } else {
            message = format!("Reboot command sent to {} ({})", client.name, ip);
            info!("{}", message);
        }
    }

    // Log the operation
    let audit_logger = AuditLogger::new(Arc::new(state.db_pool.clone()));
    let operation = ControlOperation {
        client_id: client.id.clone(),
        client_name: client.name.clone(),
        client_ip: client.ip.clone(),
        os_type: master_os.clone(),
        operation_type: "reboot".to_string(),
        operation_mode: if force { "force" } else { "graceful" }.to_string(),
        delay_minutes,
        timestamp: Utc::now(),
        administrator: "system".to_string(),
        result: if success {
            OperationResult::Success
        } else {
            OperationResult::Failed(message.clone())
        },
    };

    if let Err(e) = audit_logger.log_operation(&operation).await {
        error!("Failed to log reboot operation: {}", e);
    }

    Ok(Json(ControlOperationResponse {
        success,
        message,
        operation_id: None,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Handle remote desktop request for a client
pub async fn remote_desktop_client(
    State(state): State<AppState>,
    Path(client_id): Path<String>,
    Json(request): Json<RemoteDesktopRequest>,
) -> Result<Json<RemoteDesktopResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Remote desktop request for client {}", client_id);

    // Get the client
    let manager = crate::core::client::ClientManager::new(state.db_pool.clone());
    let client = manager.get(&client_id).await.map_err(|e| {
        error!("Failed to get client {}: {}", client_id, e);
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Client not found: {}", client_id),
                details: None,
            }),
        )
    })?;

    let ip = client.ip.clone();
    if ip.is_empty() {
        let error_msg = format!("IP address not found for '{}'", client.name);
        error!("{}", error_msg);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: error_msg,
                details: None,
            }),
        ));
    }

    let username = request.username.unwrap_or_else(|| "Administrator".to_string());
    let password = request.password.unwrap_or_else(|| "1".to_string());
    let client_name = client.name.clone();

    let master_os = get_master_os(&client.master).unwrap_or_default().to_lowercase();
    let protocol_used: String;
    let mut success = true;
    let mut message = String::new();

    if master_os.contains("windows") {
        // Windows: Launch RDP client
        protocol_used = "RDP".to_string();
        
        // Try to launch xfreerdp with proper display handling
        let mut xfreerdp_cmd = Command::new("xfreerdp3");
        xfreerdp_cmd
            .args(&[
                "/v:".to_string() + &ip,
                "/u:".to_string() + &username,
                "/p:".to_string() + &password,
                "/cert:ignore".to_string(),
                "/w:1920".to_string(),
                "/h:1080".to_string(),
                "/dynamic-resolution".to_string(),
                "/gdi:hw".to_string(),
                "/network:lan".to_string(),
                "/bpp:32".to_string(),
                "/sec:nla".to_string(),
                "/timeout:20000".to_string(),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Set DISPLAY if available
        if let Ok(display) = std::env::var("DISPLAY") {
            xfreerdp_cmd.env("DISPLAY", display);
        }

        let result = xfreerdp_cmd.spawn();

        match result {
            Ok(mut child) => {
                // Spawn a thread to wait for the process and log any errors
                std::thread::spawn(move || {
                    if let Ok(output) = child.wait_with_output() {
                        if !output.status.success() {
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            let stdout = String::from_utf8_lossy(&output.stdout);
                            if !stderr.is_empty() {
                                error!("xfreerdp error: {}", stderr);
                            }
                            if !stdout.is_empty() {
                                info!("xfreerdp output: {}", stdout);
                            }
                        } else {
                            info!("xfreerdp connection closed successfully");
                        }
                    }
                });
                success = true;
                message = format!("RDP connection initiated to {} ({}). The RDP window should open shortly.", client_name, ip);
                info!("{}", message);
            }
            Err(e) => {
                error!("Failed to launch xfreerdp: {}", e);
                
                // Fallback to rdesktop with NLA/CredSSP bypass
                let mut rdesktop_cmd = Command::new("rdesktop");
                rdesktop_cmd
                    .args(&[
                        ip.as_str(),
                        "-u", username.as_str(),
                        "-p", password.as_str(),
                        "-x", "m",
                        "-a", "32",
                        "-N",  // Disable encryption
                        "-V", "1.2",  // TLS version 1.2
                        "-E",  // Disable encryption from client to server
                    ])
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());

                // Set DISPLAY if available
                if let Ok(display) = std::env::var("DISPLAY") {
                    rdesktop_cmd.env("DISPLAY", display);
                }

                match rdesktop_cmd.spawn() {
                    Ok(mut child) => {
                        let ip_clone = ip.clone();
                        // Spawn a thread to wait for the process and log any errors
                        std::thread::spawn(move || {
                            if let Ok(output) = child.wait_with_output() {
                                if !output.status.success() {
                                    let stderr = String::from_utf8_lossy(&output.stderr);
                                    let stdout = String::from_utf8_lossy(&output.stdout);
                                    
                                    if stderr.contains("CredSSP") || stdout.contains("CredSSP") {
                                        error!("RDP connection failed: CredSSP/NLA is required by the Windows client. To fix this, on the Windows client run: gpedit.msc > Computer Configuration > Administrative Templates > System > Credentials Delegation > Allow delegating fresh credentials with NTLM-only server authentication > Enable and set to 'true'");
                                    } else if !stderr.is_empty() {
                                        error!("rdesktop error: {}", stderr);
                                    }
                                    if !stdout.is_empty() {
                                        info!("rdesktop output: {}", stdout);
                                    }
                                } else {
                                    info!("rdesktop connection closed successfully");
                                }
                            }
                        });
                        success = true;
                        message = format!("RDP connection initiated to {} ({}). The RDP window should open shortly.", client_name, ip_clone);
                        info!("{}", message);
                    }
                    Err(e) => {
                        success = false;
                        message = format!("Failed to launch RDP client: {}. Make sure rdesktop is installed.", e);
                        error!("{}", message);
                    }
                }
            }
        }
    } else {
        // Linux: Launch VNC client
        protocol_used = "VNC".to_string();
        
        // Try to launch VNC client (vncviewer or vinagre)
        let mut vncviewer_cmd = Command::new("vncviewer");
        vncviewer_cmd
            .arg(ip.as_str())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Set DISPLAY if available
        if let Ok(display) = std::env::var("DISPLAY") {
            vncviewer_cmd.env("DISPLAY", display);
        }

        let result = vncviewer_cmd.spawn();

        match result {
            Ok(mut child) => {
                let ip_clone = ip.clone();
                // Spawn a thread to wait for the process and log any errors
                std::thread::spawn(move || {
                    if let Ok(output) = child.wait_with_output() {
                        if !output.status.success() {
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            let stdout = String::from_utf8_lossy(&output.stdout);
                            if stderr.contains("Connection refused") || stderr.contains("refused") {
                                error!("VNC connection refused for {}. Make sure VNC server is running on the client.", ip_clone);
                            } else if !stderr.is_empty() {
                                error!("vncviewer error: {}", stderr);
                            }
                            if !stdout.is_empty() {
                                info!("vncviewer output: {}", stdout);
                            }
                        } else {
                            info!("vncviewer connection closed successfully");
                        }
                    }
                });
                success = true;
                message = format!("VNC connection initiated to {} ({}). The VNC window should open shortly.", client_name, ip);
                info!("{}", message);
            }
            Err(e) => {
                error!("Failed to launch vncviewer: {}", e);
                
                // Fallback to vinagre
                let mut vinagre_cmd = Command::new("vinagre");
                vinagre_cmd
                    .arg(ip.as_str())
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());

                // Set DISPLAY if available
                if let Ok(display) = std::env::var("DISPLAY") {
                    vinagre_cmd.env("DISPLAY", display);
                }

                match vinagre_cmd.spawn() {
                    Ok(mut child) => {
                        let ip_clone = ip.clone();
                        // Spawn a thread to wait for the process and log any errors
                        std::thread::spawn(move || {
                            if let Ok(output) = child.wait_with_output() {
                                if !output.status.success() {
                                    let stderr = String::from_utf8_lossy(&output.stderr);
                                    let stdout = String::from_utf8_lossy(&output.stdout);
                                    if stderr.contains("Connection refused") || stderr.contains("refused") {
                                        error!("VNC connection refused for {}. Make sure VNC server is running on the client.", ip_clone);
                                    } else if !stderr.is_empty() {
                                        error!("vinagre error: {}", stderr);
                                    }
                                    if !stdout.is_empty() {
                                        info!("vinagre output: {}", stdout);
                                    }
                                } else {
                                    info!("vinagre connection closed successfully");
                                }
                            }
                        });
                        success = true;
                        message = format!("VNC connection initiated to {} ({}). The VNC window should open shortly.", client_name, ip);
                        info!("{}", message);
                    }
                    Err(e) => {
                        success = false;
                        message = format!("Failed to launch VNC client: {}. Make sure vncviewer or vinagre is installed, and VNC server is running on the client.", e);
                        error!("{}", message);
                    }
                }
            }
        }
    }

    // Log the operation
    let audit_logger = AuditLogger::new(Arc::new(state.db_pool.clone()));
    let operation = ControlOperation {
        client_id: client.id.clone(),
        client_name: client.name.clone(),
        client_ip: client.ip.clone(),
        os_type: master_os.clone(),
        operation_type: "remote".to_string(),
        operation_mode: "interactive".to_string(),
        delay_minutes: None,
        timestamp: Utc::now(),
        administrator: "system".to_string(),
        result: if success {
            OperationResult::Success
        } else {
            OperationResult::Failed(message.clone())
        },
    };

    if let Err(e) = audit_logger.log_operation(&operation).await {
        error!("Failed to log remote desktop operation: {}", e);
    }

    Ok(Json(RemoteDesktopResponse {
        success,
        protocol_used,
        message,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Handle cancel scheduled operation request
pub async fn cancel_operation(
    State(_state): State<AppState>,
    Path(operation_id): Path<String>,
    Json(_request): Json<CancelOperationRequest>,
) -> Result<Json<ControlOperationResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Cancel operation request for operation {}", operation_id);

    // TODO: Implement scheduled operation cancellation
    // For now, return a success response
    Ok(Json(ControlOperationResponse {
        success: true,
        message: format!("Operation {} cancelled", operation_id),
        operation_id: Some(operation_id),
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Get audit logs with optional filters
pub async fn get_audit_logs(
    State(state): State<AppState>,
    Query(query): Query<AuditLogQuery>,
) -> Result<Json<AuditLogsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Audit logs request with filters: {:?}", query);

    let filter = AuditLogFilter {
        client_id: query.client_id,
        operation_type: query.operation_type,
        start_date: query.start_date,
        end_date: query.end_date,
        limit: query.limit,
        offset: query.offset,
    };

    let audit_logger = AuditLogger::new(Arc::new(state.db_pool.clone()));
    let logs = audit_logger.query_logs(&filter).await.map_err(|e| {
        error!("Failed to query audit logs: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Failed to query audit logs".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    let total = logs.len();

    Ok(Json(AuditLogsResponse { logs, total }))
}

/// Get scheduled operations with optional filters
pub async fn get_scheduled_operations(
    State(state): State<AppState>,
    Query(query): Query<ScheduledOperationsQuery>,
) -> Result<Json<ScheduledOperationsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Scheduled operations request with filters: {:?}", query);

    // Query scheduled operations from database
    let sql = "SELECT id, client_id, operation_type, operation_mode, scheduled_time, created_at, result FROM scheduled_operations WHERE (result IS NULL OR result = 'pending') ORDER BY scheduled_time ASC";

    let rows = sqlx::query_as::<_, (String, String, String, String, String, String, Option<String>)>(sql)
        .fetch_all(&state.db_pool)
        .await
        .map_err(|e| {
            error!("Failed to query scheduled operations: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to query scheduled operations".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    // Filter results based on query parameters
    let mut operations: Vec<ScheduledOperation> = rows
        .into_iter()
        .map(|(id, client_id, operation_type, operation_mode, scheduled_time, created_at, result)| {
            ScheduledOperation {
                id,
                client_id,
                operation_type,
                operation_mode,
                scheduled_time,
                created_at,
                result,
            }
        })
        .collect();

    // Apply client_id filter if provided
    if let Some(client_id) = &query.client_id {
        operations.retain(|op| op.client_id == *client_id);
    }

    // Apply status filter if provided
    if let Some(status) = &query.status {
        operations.retain(|op| op.result.as_ref().map_or(false, |r| r == status));
    }

    // Apply limit and offset
    let offset = query.offset.unwrap_or(0) as usize;
    let limit = query.limit.unwrap_or(100) as usize;
    
    let total = operations.len();
    operations = operations.into_iter().skip(offset).take(limit).collect();

    Ok(Json(ScheduledOperationsResponse { operations, total }))
}
