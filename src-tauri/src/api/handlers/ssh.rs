use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

use crate::commands::system::{
    execute_ssh_command as execute_ssh_cmd, get_windows_system_info as get_windows_info,
    test_ssh_connection as test_ssh_conn, SshTestRequest, SshTestResult, WindowsSystemInfo,
};
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Debug, Deserialize)]
pub struct TestConnectionRequest {
    pub host: String,
    pub username: String,
    pub port: Option<u16>,
}

#[derive(Debug, Deserialize)]
pub struct ExecuteCommandRequest {
    pub host: String,
    pub username: String,
    pub command: String,
}

#[derive(Debug, Deserialize)]
pub struct GetSystemInfoRequest {
    pub host: String,
    pub username: String,
}

/// Test SSH connection to a remote host
pub async fn test_ssh_connection(
    State(_state): State<AppState>,
    Json(request): Json<TestConnectionRequest>,
) -> Result<Json<SshTestResult>, (StatusCode, Json<ErrorResponse>)> {
    let ssh_request = SshTestRequest {
        host: request.host,
        username: request.username,
        port: request.port,
    };

    match test_ssh_conn(ssh_request).await {
        Ok(result) => Ok(Json(result)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("SSH connection test failed: {}", e),
            }),
        )),
    }
}

/// Execute SSH command on a remote host
pub async fn execute_ssh_command(
    State(_state): State<AppState>,
    Json(request): Json<ExecuteCommandRequest>,
) -> Result<Json<SshTestResult>, (StatusCode, Json<ErrorResponse>)> {
    match execute_ssh_cmd(request.host, request.username, request.command).await {
        Ok(result) => Ok(Json(result)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("SSH command execution failed: {}", e),
            }),
        )),
    }
}

/// Get Windows system information via SSH
pub async fn get_windows_system_info(
    State(_state): State<AppState>,
    Json(request): Json<GetSystemInfoRequest>,
) -> Result<Json<WindowsSystemInfo>, (StatusCode, Json<ErrorResponse>)> {
    match get_windows_info(request.host, request.username).await {
        Ok(result) => Ok(Json(result)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to get system info: {}", e),
            }),
        )),
    }
}
