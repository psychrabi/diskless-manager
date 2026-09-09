//! Disk management types
//!
//! This module contains all disk and storage-related types and structures.

use serde::{Deserialize, Serialize};

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
