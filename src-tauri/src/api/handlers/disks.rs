use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::state::AppState;
use crate::types::disk::DatasetOperationResponse;

#[derive(Debug, Serialize, Deserialize)]
pub struct RenameDiskRequest {
    pub old_name: String,
    pub new_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreatePoolRequest {
    pub name: String,
    pub disk: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PoolExistsRequest {
    pub pool_name: Option<String>,
}

pub async fn list_disks(State(_state): State<AppState>) -> Result<Json<Vec<String>>, StatusCode> {
    match crate::disks::list_block_devices() {
        Ok(devices) => Ok(Json(devices)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn rename_disk(
    Path(name): Path<String>,
    State(_state): State<AppState>,
    Json(request): Json<RenameDiskRequest>,
) -> Result<Json<DatasetOperationResponse>, StatusCode> {
    // Validate that the path parameter matches the request
    if name != request.old_name {
        return Err(StatusCode::BAD_REQUEST);
    }

    match crate::disks::rename_zfs_dataset(&request.old_name, &request.new_name) {
        Ok(message) => Ok(Json(DatasetOperationResponse::success(
            &message,
            Some(&request.new_name),
        ))),
        Err(e) => Ok(Json(DatasetOperationResponse::error(&e.to_string()))),
    }
}

pub async fn create_pool(
    State(_state): State<AppState>,
    Json(request): Json<CreatePoolRequest>,
) -> Result<Json<DatasetOperationResponse>, StatusCode> {
    // Call the underlying zpool creation logic without the Tauri State wrapper
    use std::process::Command;

    let output = Command::new("zpool")
        .args(&["create", &request.name, &format!("/dev/{}", request.disk)])
        .output();

    match output {
        Ok(output) if output.status.success() => Ok(Json(DatasetOperationResponse::success(
            &format!("ZFS pool '{}' created successfully", request.name),
            Some(&request.name),
        ))),
        Ok(output) => {
            let error_msg = String::from_utf8_lossy(&output.stderr).to_string();
            Ok(Json(DatasetOperationResponse::error(&error_msg)))
        }
        Err(e) => Ok(Json(DatasetOperationResponse::error(&e.to_string()))),
    }
}

pub async fn pool_exists(State(_state): State<AppState>) -> Result<Json<bool>, StatusCode> {
    // Check if any ZFS pool exists (typically "diskless" or "rpool")
    use std::process::Command;

    let output = Command::new("zpool")
        .args(&["list", "-H", "-o", "name"])
        .output();

    match output {
        Ok(output) => {
            if output.status.success() {
                let pools = String::from_utf8_lossy(&output.stdout);
                // Check if any pool exists
                let has_pool = !pools.trim().is_empty();
                Ok(Json(has_pool))
            } else {
                Ok(Json(false))
            }
        }
        Err(_) => Ok(Json(false)),
    }
}
