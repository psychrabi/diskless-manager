//! Image management types
//!
//! This module contains all image/master-related types and structures.

use serde::{Deserialize, Serialize};

/// Master image structure
#[derive(Serialize, Deserialize, Clone)]
pub struct Master {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub size: String,
    pub os: Option<String>,
    pub snapshots: Vec<Snapshot>,
}

/// Snapshot structure
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Snapshot {
    pub name: String,
    pub created: String,
    pub used: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
}

/// Master data structure (legacy format)
#[derive(Serialize, Deserialize, Clone)]
pub struct MasterData {
    pub name: String,
    pub size: String,
    pub snapshots: Vec<String>,
    pub created_at: String,
    pub last_modified: String,
}

/// Image creation request
#[derive(Debug, Deserialize)]
pub struct CreateImageRequest {
    pub token: String,
    pub name: String,
    pub size: String,
    pub os: Option<String>,
}

/// Snapshot creation request
#[derive(Debug, Deserialize)]
pub struct CreateSnapshotRequest {
    pub token: String,
    pub master_name: String,
    pub snapshot_name: Option<String>,
}

/// Image operations response
#[derive(Debug, Serialize)]
pub struct ImageOperationResponse {
    pub success: bool,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

impl ImageOperationResponse {
    /// Create a success response
    pub fn success<T: Serialize + std::fmt::Display>(
        message: T,
        data: Option<serde_json::Value>,
    ) -> Self {
        Self {
            success: true,
            message: message.to_string(),
            data,
        }
    }

    /// Create an error response
    pub fn error<T: Serialize + std::fmt::Display>(message: T) -> Self {
        Self {
            success: false,
            message: message.to_string(),
            data: None,
        }
    }
}

/// ZFS pool information
#[derive(Debug, Serialize)]
pub struct ZpoolInfo {
    pub name: String,
    pub size: String,
    pub alloc: String,
    pub free: String,
    pub health: String,
}

/// ZFS arc statistics
#[derive(Debug, Serialize)]
pub struct ArcstatInfo {
    pub size: u64,
    pub hit_percent: f64,
}
