use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::process::{Command, Stdio};
use std::sync::Arc;
use tracing::{debug, error, info};

use crate::audit_logger::{AuditLogFilter, AuditLogger, ControlOperation, OperationResult};
use crate::state::AppState;
use chrono::Utc;
use sqlx::sqlite::SqlitePool;

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

/// A power operation (shutdown/reboot) delayed until a scheduled time.
#[derive(Clone)]
struct ScheduledPowerOp {
    operation_id: String,
    client_id: String,
    client_name: String,
    client_ip: String,
    operation_type: String,
    master_os: String,
    force: bool,
}

/// Build the OS-specific command used to shutdown or reboot a client.
fn build_power_command(operation_type: &str, ip: &str, master_os: &str, force: bool) -> Command {
    let is_shutdown = operation_type == "shutdown";
    if master_os.contains("linux") {
        let action: &str = match (is_shutdown, force) {
            (true, true) => "poweroff -f",
            (true, false) => "shutdown -h now",
            (false, true) => "reboot -f",
            (false, false) => "shutdown -r now",
        };
        let mut cmd = Command::new("ssh");
        cmd.args(["-o", "StrictHostKeyChecking=no", "-o", "ConnectTimeout=5"])
            .arg(format!("root@{}", ip))
            .arg(action);
        cmd
    } else if is_shutdown {
        let mut cmd = Command::new("net");
        cmd.args(["rpc", "shutdown", "-I", ip, "-U", "diskless%1", "-t", "0"]);
        if force {
            cmd.arg("-f");
        }
        cmd
    } else {
        let mut cmd = Command::new("net");
        cmd.args([
            "rpc",
            "shutdown",
            "-r",
            "-I",
            ip,
            "-U",
            "diskless%1",
            "-t",
            "0",
        ]);
        if force {
            cmd.arg("-f");
        }
        cmd
    }
}

/// Run a previously scheduled power operation once its delay has elapsed.
async fn run_scheduled_power_op(pool: SqlitePool, op: ScheduledPowerOp, delay_secs: u64) {
    tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;

    // Bail out if the operation was cancelled while we were waiting.
    let pending = match sqlx::query_scalar::<_, Option<String>>(
        "SELECT result FROM scheduled_operations WHERE id = ?",
    )
    .bind(&op.operation_id)
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(result)) => {
            let result = result.unwrap_or_default();
            result.is_empty() || result == "pending"
        }
        _ => true,
    };
    if !pending {
        debug!(
            "Scheduled operation {} skipped (no longer pending)",
            op.operation_id
        );
        return;
    }

    let os_label = if op.master_os.contains("linux") {
        "Linux"
    } else {
        "Windows"
    };
    let verb = if op.operation_type == "shutdown" {
        "Shutdown"
    } else {
        "Reboot"
    };

    let (success, message) = match crate::api::util::run_command(&mut build_power_command(
        &op.operation_type,
        &op.client_ip,
        &op.master_os,
        op.force,
    ))
    .await
    {
        Ok(output) if output.status.success() => (
            true,
            format!(
                "{} command sent to {} ({})",
                verb, op.client_name, op.client_ip
            ),
        ),
        Ok(output) => {
            let msg = format!(
                "Failed to {} {} client ({}): {}",
                op.operation_type,
                os_label,
                op.client_ip,
                String::from_utf8_lossy(&output.stderr)
            );
            error!("{}", msg);
            (false, msg)
        }
        Err(e) => {
            let msg = format!("Failed to execute SSH: {}", e);
            error!("{}", msg);
            (false, msg)
        }
    };

    let result_column = if success {
        "success".to_string()
    } else {
        format!("failed: {}", message)
    };
    if let Err(e) = sqlx::query("UPDATE scheduled_operations SET result = ? WHERE id = ?")
        .bind(&result_column)
        .bind(&op.operation_id)
        .execute(&pool)
        .await
    {
        error!(
            "Failed to update scheduled operation {}: {}",
            op.operation_id, e
        );
    }

    let audit_logger = AuditLogger::new(Arc::new(pool.clone()));
    let audit_operation = ControlOperation {
        client_id: op.client_id.clone(),
        client_name: op.client_name.clone(),
        client_ip: op.client_ip.clone(),
        os_type: op.master_os.clone(),
        operation_type: op.operation_type.clone(),
        operation_mode: if op.force { "force" } else { "graceful" }.to_string(),
        delay_minutes: Some((delay_secs / 60) as u32),
        timestamp: Utc::now(),
        administrator: "system".to_string(),
        result: if success {
            OperationResult::Success
        } else {
            OperationResult::Failed(message)
        },
    };
    if let Err(e) = audit_logger.log_operation(&audit_operation).await {
        error!("Failed to log scheduled operation: {}", e);
    }
    info!(
        "Scheduled {} operation {} finished: {}",
        op.operation_type, op.operation_id, result_column
    );
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
#[derive(Debug)]
pub struct ErrorResponse {
    pub status: u16,
    pub error: String,
    pub details: Option<String>,
}

impl Serialize for ErrorResponse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        crate::api::error::serialize_api_error(
            self.status,
            &self.error,
            self.details.as_ref().map_or_else(
                || serde_json::json!({}),
                |details| serde_json::json!({ "reason": details }),
            ),
            serializer,
        )
    }
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> Response {
        (
            StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(self),
        )
            .into_response()
    }
}

/// Minimal transport-facing view used by the legacy control handlers.
/// Client loading itself is delegated to the authoritative application service.
struct ControlClient {
    id: String,
    name: String,
    ip: String,
    master: String,
}

async fn load_control_client(
    state: &AppState,
    client_id: &str,
) -> Result<ControlClient, (StatusCode, Json<ErrorResponse>)> {
    let client = state
        .application
        .clients
        .get_by_string(client_id)
        .await
        .map_err(|e| {
            error!("Failed to get client {}: {}", client_id, e);
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    status: StatusCode::NOT_FOUND.as_u16(),
                    error: format!("Client not found: {}", client_id),
                    details: None,
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    status: StatusCode::NOT_FOUND.as_u16(),
                    error: format!("Client not found: {}", client_id),
                    details: None,
                }),
            )
        })?;

    Ok(ControlClient {
        id: client.id.to_string(),
        name: client.name,
        ip: client.ip.to_string(),
        master: client.master,
    })
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

    let client = load_control_client(&state, &client_id).await?;

    let ip = &client.ip;
    if ip.is_empty() {
        let error_msg = format!("IP address not found for '{}'", client.name);
        error!("{}", error_msg);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                error: error_msg,
                details: None,
            }),
        ));
    }

    let master_os = get_master_os(&client.master)
        .unwrap_or_default()
        .to_lowercase();

    if let Some(delay_minutes) = delay_minutes {
        if delay_minutes > 0 {
            let operation_id = uuid::Uuid::new_v4().to_string();
            let now = Utc::now();
            let scheduled_time = now + chrono::Duration::minutes(i64::from(delay_minutes));
            let op_mode = if force { "force" } else { "graceful" };

            sqlx::query(
                "INSERT INTO scheduled_operations (id, client_id, operation_type, operation_mode, scheduled_time, created_at) VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&operation_id)
            .bind(&client.id)
            .bind("shutdown")
            .bind(op_mode)
            .bind(scheduled_time.to_rfc3339())
            .bind(now.to_rfc3339())
            .execute(&state.db_pool)
            .await
            .map_err(|e| {
                error!(
                    "Failed to schedule shutdown operation for {}: {}",
                    client.name, e
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                        error: "Failed to schedule shutdown operation".to_string(),
                        details: Some(e.to_string()),
                    }),
                )
            })?;

            let pool = state.db_pool.clone();
            let op = ScheduledPowerOp {
                operation_id: operation_id.clone(),
                client_id: client.id.clone(),
                client_name: client.name.clone(),
                client_ip: client.ip.clone(),
                operation_type: "shutdown".to_string(),
                master_os: master_os.clone(),
                force,
            };
            let delay_secs = u64::from(delay_minutes) * 60;
            tokio::spawn(async move {
                run_scheduled_power_op(pool, op, delay_secs).await;
            });

            let message = format!(
                "Shutdown for {} ({}) scheduled for {}",
                client.name,
                client.ip,
                scheduled_time.format("%Y-%m-%d %H:%M:%S")
            );
            info!("{}", message);
            return Ok(Json(ControlOperationResponse {
                success: true,
                message,
                operation_id: Some(operation_id),
                timestamp: now.to_rfc3339(),
            }));
        }
    }

    let (success, message) = match crate::api::util::run_command(&mut build_power_command(
        "shutdown", ip, &master_os, force,
    ))
    .await
    {
        Ok(output) if output.status.success() => {
            let msg = format!("Shutdown command sent to {} ({})", client.name, ip);
            info!("{}", msg);
            (true, msg)
        }
        Ok(output) => {
            let msg = format!(
                "Failed to shutdown {} client ({}): {}",
                if master_os.contains("linux") {
                    "Linux"
                } else {
                    "Windows"
                },
                ip,
                String::from_utf8_lossy(&output.stderr)
            );
            error!("{}", msg);
            (false, msg)
        }
        Err(e) => {
            error!("Failed to execute SSH: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                    error: format!("Failed to execute SSH: {}", e),
                    details: None,
                }),
            ));
        }
    };

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

    let client = load_control_client(&state, &client_id).await?;

    let ip = &client.ip;
    if ip.is_empty() {
        let error_msg = format!("IP address not found for '{}'", client.name);
        error!("{}", error_msg);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                error: error_msg,
                details: None,
            }),
        ));
    }

    let master_os = get_master_os(&client.master)
        .unwrap_or_default()
        .to_lowercase();

    if let Some(delay_minutes) = delay_minutes {
        if delay_minutes > 0 {
            let operation_id = uuid::Uuid::new_v4().to_string();
            let now = Utc::now();
            let scheduled_time = now + chrono::Duration::minutes(i64::from(delay_minutes));
            let op_mode = if force { "force" } else { "graceful" };

            sqlx::query(
                "INSERT INTO scheduled_operations (id, client_id, operation_type, operation_mode, scheduled_time, created_at) VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&operation_id)
            .bind(&client.id)
            .bind("reboot")
            .bind(op_mode)
            .bind(scheduled_time.to_rfc3339())
            .bind(now.to_rfc3339())
            .execute(&state.db_pool)
            .await
            .map_err(|e| {
                error!(
                    "Failed to schedule reboot operation for {}: {}",
                    client.name, e
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                        error: "Failed to schedule reboot operation".to_string(),
                        details: Some(e.to_string()),
                    }),
                )
            })?;

            let pool = state.db_pool.clone();
            let op = ScheduledPowerOp {
                operation_id: operation_id.clone(),
                client_id: client.id.clone(),
                client_name: client.name.clone(),
                client_ip: client.ip.clone(),
                operation_type: "reboot".to_string(),
                master_os: master_os.clone(),
                force,
            };
            let delay_secs = u64::from(delay_minutes) * 60;
            tokio::spawn(async move {
                run_scheduled_power_op(pool, op, delay_secs).await;
            });

            let message = format!(
                "Reboot for {} ({}) scheduled for {}",
                client.name,
                client.ip,
                scheduled_time.format("%Y-%m-%d %H:%M:%S")
            );
            info!("{}", message);
            return Ok(Json(ControlOperationResponse {
                success: true,
                message,
                operation_id: Some(operation_id),
                timestamp: now.to_rfc3339(),
            }));
        }
    }

    let (success, message) = match crate::api::util::run_command(&mut build_power_command(
        "reboot", ip, &master_os, force,
    ))
    .await
    {
        Ok(output) if output.status.success() => {
            let msg = format!("Reboot command sent to {} ({})", client.name, ip);
            info!("{}", msg);
            (true, msg)
        }
        Ok(output) => {
            let msg = format!(
                "Failed to reboot {} client ({}): {}",
                if master_os.contains("linux") {
                    "Linux"
                } else {
                    "Windows"
                },
                ip,
                String::from_utf8_lossy(&output.stderr)
            );
            error!("{}", msg);
            (false, msg)
        }
        Err(e) => {
            error!("Failed to execute SSH: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                    error: format!("Failed to execute SSH: {}", e),
                    details: None,
                }),
            ));
        }
    };

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
#[allow(unused_assignments)]
pub async fn remote_desktop_client(
    State(state): State<AppState>,
    Path(client_id): Path<String>,
    Json(request): Json<RemoteDesktopRequest>,
) -> Result<Json<RemoteDesktopResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Remote desktop request for client {}", client_id);

    let client = load_control_client(&state, &client_id).await?;

    let ip = client.ip.clone();
    if ip.is_empty() {
        let error_msg = format!("IP address not found for '{}'", client.name);
        error!("{}", error_msg);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                error: error_msg,
                details: None,
            }),
        ));
    }

    let username = request
        .username
        .unwrap_or_else(|| "Administrator".to_string());
    let password = request.password.unwrap_or_else(|| "1".to_string());
    let client_name = client.name.clone();

    let master_os = get_master_os(&client.master)
        .unwrap_or_default()
        .to_lowercase();
    let protocol_used: String;
    let mut success = true;
    let mut message = String::new();

    if master_os.contains("windows") {
        // Windows: Launch RDP client
        protocol_used = "RDP".to_string();

        // Try to launch xfreerdp (v2/v3 binary naming) with proper display
        // handling. Distros ship the FreeRDP 3 binary either as 'xfreerdp3'
        // (Debian/Ubuntu) or 'xfreerdp' (Fedora/RHEL).
        let freerdp_names = ["xfreerdp3", "xfreerdp"];
        let mut freerdp_child = None;
        let mut freerdp_error: Option<std::io::Error> = None;

        for freerdp_name in freerdp_names {
            let mut freerdp_cmd = Command::new(freerdp_name);
            freerdp_cmd
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
                freerdp_cmd.env("DISPLAY", display);
            }

            match freerdp_cmd.spawn() {
                Ok(child) => {
                    freerdp_child = Some(child);
                    break;
                }
                Err(e) => {
                    debug!("Failed to launch {}: {}", freerdp_name, e);
                    freerdp_error = Some(e);
                }
            }
        }

        match freerdp_child {
            Some(child) => {
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
                message = format!(
                    "RDP connection initiated to {} ({}). The RDP window should open shortly.",
                    client_name, ip
                );
                info!("{}", message);
            }
            None => {
                let launch_error = freerdp_error
                    .as_ref()
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "unknown error".to_string());

                error!("Failed to launch xfreerdp: {}", launch_error);

                // Fallback to rdesktop with NLA/CredSSP bypass
                let mut rdesktop_cmd = Command::new("rdesktop");
                rdesktop_cmd
                    .args([
                        ip.as_str(),
                        "-u",
                        username.as_str(),
                        "-p",
                        password.as_str(),
                        "-x",
                        "m",
                        "-a",
                        "32",
                        "-N", // Disable encryption
                        "-V",
                        "1.2", // TLS version 1.2
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
                    Ok(child) => {
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
                        message = format!(
                            "Failed to launch RDP client: {}. Make sure rdesktop is installed.",
                            e
                        );
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
            Ok(child) => {
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
                message = format!(
                    "VNC connection initiated to {} ({}). The VNC window should open shortly.",
                    client_name, ip
                );
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
                    Ok(child) => {
                        let ip_clone = ip.clone();
                        // Spawn a thread to wait for the process and log any errors
                        std::thread::spawn(move || {
                            if let Ok(output) = child.wait_with_output() {
                                if !output.status.success() {
                                    let stderr = String::from_utf8_lossy(&output.stderr);
                                    let stdout = String::from_utf8_lossy(&output.stdout);
                                    if stderr.contains("Connection refused")
                                        || stderr.contains("refused")
                                    {
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
    State(state): State<AppState>,
    Path(operation_id): Path<String>,
    request: Option<Json<CancelOperationRequest>>,
) -> Result<Json<ControlOperationResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Cancel operation request for operation {}", operation_id);

    let reason = request
        .and_then(|Json(req)| req.reason)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let existing = sqlx::query_as::<_, (Option<String>,)>(
        "SELECT result FROM scheduled_operations WHERE id = ? LIMIT 1",
    )
    .bind(&operation_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        error!(
            "Failed to query scheduled operation {}: {}",
            operation_id, e
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                error: "Failed to query scheduled operation".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    let Some((current_result,)) = existing else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                status: StatusCode::NOT_FOUND.as_u16(),
                error: format!("Scheduled operation {} not found", operation_id),
                details: None,
            }),
        ));
    };

    if current_result
        .as_deref()
        .is_some_and(|result| result != "pending")
    {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                status: StatusCode::CONFLICT.as_u16(),

                error: format!(
                    "Scheduled operation {} is already finalized and cannot be cancelled",
                    operation_id
                ),
                details: None,
            }),
        ));
    }

    let now = Utc::now().to_rfc3339();
    let result_value = reason
        .as_ref()
        .map(|r| format!("cancelled: {}", r))
        .unwrap_or_else(|| "cancelled".to_string());

    sqlx::query("UPDATE scheduled_operations SET result = ?, cancelled_at = ? WHERE id = ?")
        .bind(&result_value)
        .bind(&now)
        .bind(&operation_id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            error!(
                "Failed to cancel scheduled operation {} in database: {}",
                operation_id, e
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                    error: "Failed to cancel scheduled operation".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    Ok(Json(ControlOperationResponse {
        success: true,
        message: format!("Operation {} cancelled", operation_id),
        operation_id: Some(operation_id),
        timestamp: now,
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
                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
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

    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            String,
            String,
            Option<String>,
        ),
    >(sql)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        error!("Failed to query scheduled operations: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                error: "Failed to query scheduled operations".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    // Filter results based on query parameters
    let mut operations: Vec<ScheduledOperation> = rows
        .into_iter()
        .map(
            |(
                id,
                client_id,
                operation_type,
                operation_mode,
                scheduled_time,
                created_at,
                result,
            )| {
                ScheduledOperation {
                    id,
                    client_id,
                    operation_type,
                    operation_mode,
                    scheduled_time,
                    created_at,
                    result,
                }
            },
        )
        .collect();

    // Apply client_id filter if provided
    if let Some(client_id) = &query.client_id {
        operations.retain(|op| op.client_id == *client_id);
    }

    // Apply status filter if provided
    if let Some(status) = &query.status {
        operations.retain(|op| op.result.as_ref() == Some(status));
    }

    // Apply limit and offset
    let offset = query.offset.unwrap_or(0) as usize;
    let limit = query.limit.unwrap_or(100) as usize;

    let total = operations.len();
    operations = operations.into_iter().skip(offset).take(limit).collect();

    Ok(Json(ScheduledOperationsResponse { operations, total }))
}
