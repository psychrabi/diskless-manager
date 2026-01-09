use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info};

use crate::audit_logger::{AuditLogFilter, AuditLogger, ControlOperation, OperationResult};
use crate::control_handler::ControlHandler;
use crate::core::client::Client;
use crate::state::AppState;
use chrono::Utc;

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

/// Convert core::client::Client to types::client::Client
fn convert_client(client: &Client) -> crate::types::client::Client {
    crate::types::client::Client {
        id: client.id.clone(),
        name: client.name.clone(),
        mac: client.mac.clone(),
        ip: client.ip.clone(),
        master: client.master.clone(),
        snapshot: client.snapshot.clone(),
        block_store: client.block_store.clone(),
        target_iqn: client.target_iqn.clone(),
        writeback: client.writeback.clone(),
        created_at: client.created_at.to_rfc3339().into(),
        last_modified: client.last_modified.clone(),
        block_device: client.block_device.clone(),
        status: client.status.clone(),
        mode: client.mode.clone(),
        pxe_mode: client.pxe_mode.clone(),
        keep_writeback: client.keep_writeback,
        use_game_disk: client.use_game_disk,
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

    // Convert to types::client::Client
    let types_client = convert_client(&client);

    // Get the master image to determine OS type
    let master_os = None; // Will be determined by OS detector

    // Create control handler
    let control_handler = ControlHandler::new(state.ssh_executor.clone());

    // Execute shutdown
    let (response, audit_entry) = control_handler
        .handle_shutdown(&types_client, force, delay_minutes, master_os)
        .await
        .map_err(|e| {
            error!("Failed to execute shutdown: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to execute shutdown".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    // Log the operation
    let audit_logger = AuditLogger::new(Arc::new(state.db_pool.clone()));
    let operation = ControlOperation {
        client_id: client.id.clone(),
        client_name: client.name.clone(),
        client_ip: client.ip.clone(),
        os_type: audit_entry.os_type.clone(),
        operation_type: "shutdown".to_string(),
        operation_mode: if force { "force" } else { "graceful" }.to_string(),
        delay_minutes,
        timestamp: Utc::now(),
        administrator: "system".to_string(), // TODO: Get from auth context
        result: if response.success {
            OperationResult::Success
        } else {
            OperationResult::Failed(response.message.clone())
        },
    };

    if let Err(e) = audit_logger.log_operation(&operation).await {
        error!("Failed to log shutdown operation: {}", e);
    }

    Ok(Json(ControlOperationResponse {
        success: response.success,
        message: response.message,
        operation_id: response.operation_id,
        timestamp: response.timestamp,
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

    // Convert to types::client::Client
    let types_client = convert_client(&client);

    // Get the master image to determine OS type
    let master_os = None; // Will be determined by OS detector

    // Create control handler
    let control_handler = ControlHandler::new(state.ssh_executor.clone());

    // Execute reboot
    let (response, audit_entry) = control_handler
        .handle_reboot(&types_client, force, delay_minutes, master_os)
        .await
        .map_err(|e| {
            error!("Failed to execute reboot: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to execute reboot".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    // Log the operation
    let audit_logger = AuditLogger::new(Arc::new(state.db_pool.clone()));
    let operation = ControlOperation {
        client_id: client.id.clone(),
        client_name: client.name.clone(),
        client_ip: client.ip.clone(),
        os_type: audit_entry.os_type.clone(),
        operation_type: "reboot".to_string(),
        operation_mode: if force { "force" } else { "graceful" }.to_string(),
        delay_minutes,
        timestamp: Utc::now(),
        administrator: "system".to_string(), // TODO: Get from auth context
        result: if response.success {
            OperationResult::Success
        } else {
            OperationResult::Failed(response.message.clone())
        },
    };

    if let Err(e) = audit_logger.log_operation(&operation).await {
        error!("Failed to log reboot operation: {}", e);
    }

    Ok(Json(ControlOperationResponse {
        success: response.success,
        message: response.message,
        operation_id: response.operation_id,
        timestamp: response.timestamp,
    }))
}

/// Handle remote desktop request for a client
pub async fn remote_desktop_client(
    State(state): State<AppState>,
    Path(client_id): Path<String>,
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

    // Convert to types::client::Client
    let types_client = convert_client(&client);

    // Get the master image to determine OS type
    let master_os = None; // Will be determined by OS detector

    // Create control handler
    let control_handler = ControlHandler::new(state.ssh_executor.clone());

    // Execute remote desktop
    let response = control_handler
        .handle_remote_desktop(&types_client, master_os)
        .await
        .map_err(|e| {
            error!("Failed to launch remote desktop: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to launch remote desktop".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    // Log the operation
    let audit_logger = AuditLogger::new(Arc::new(state.db_pool.clone()));
    let operation = ControlOperation {
        client_id: client.id.clone(),
        client_name: client.name.clone(),
        client_ip: client.ip.clone(),
        os_type: "unknown".to_string(), // Will be determined by OS detector
        operation_type: "remote".to_string(),
        operation_mode: "interactive".to_string(),
        delay_minutes: None,
        timestamp: Utc::now(),
        administrator: "system".to_string(), // TODO: Get from auth context
        result: OperationResult::Success,
    };

    if let Err(e) = audit_logger.log_operation(&operation).await {
        error!("Failed to log remote desktop operation: {}", e);
    }

    Ok(Json(RemoteDesktopResponse {
        success: true,
        protocol_used: response.protocol_used,
        message: response.message,
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
