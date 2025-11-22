//! Disk management types
//!
//! This module contains all disk and storage-related types and structures.

use serde::{ Deserialize, Serialize };

/// Dataset information structure
#[derive(Serialize, Deserialize, Debug)]
pub struct DatasetInfo {
    pub name: String,
    pub disk_type: Option<String>,
    pub used: String,
    pub available: String,
    pub referenced: String,
    pub mountpoint: String,
}

/// Disk information structure
#[derive(Serialize, Deserialize)]
pub struct Disk {
    pub name: String,
    pub size: String,
}

/// RAM usage statistics
#[derive(Serialize, Deserialize)]
pub struct MemoryStats {
    pub total: String,
    pub used: String,
    pub free: String,
    pub shared: String,
    pub buff_cache: String,
    pub available: String,
}

/// RAM usage structure
#[derive(Serialize, Deserialize)]
pub struct RamUsage {
    pub memory: MemoryStats,
}

/// ZFS pool creation request
#[derive(Debug, Deserialize)]
pub struct CreateZpoolRequest {
    pub name: String,
    pub disk: String,
}

/// ZFS dataset creation request
#[derive(Debug, Deserialize)]
pub struct CreateDatasetRequest {
    pub zpool: String,
    pub name: String,
    pub usage_type: String,
    pub size: Option<String>,
}

/// Dataset operation response
#[derive(Debug, Serialize)]
pub struct DatasetOperationResponse {
    pub success: bool,
    pub message: String,
    pub dataset_name: Option<String>,
}

impl DatasetOperationResponse {
    /// Create a success response
    pub fn success(message: &str, dataset_name: Option<&str>) -> Self {
        Self {
            success: true,
            message: message.to_string(),
            dataset_name: dataset_name.map(|s| s.to_string()),
        }
    }

    /// Create an error response
    pub fn error(message: &str) -> Self {
        Self {
            success: false,
            message: message.to_string(),
            dataset_name: None,
        }
    }
}
