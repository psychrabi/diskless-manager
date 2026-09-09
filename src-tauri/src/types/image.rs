//! Image management types
//!
//! This module contains all image/master-related types and structures.

use serde::{Deserialize, Serialize};

/// Snapshot structure
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Snapshot {
    pub name: String,
    pub created: String,
    pub used: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
}
