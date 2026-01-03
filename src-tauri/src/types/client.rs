//! Client management types
//!
//! This module contains all client-related types and structures.

use serde::{Deserialize, Serialize};

/// Main client structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Client {
    pub id: String,
    pub name: String,
    pub mac: String,
    pub ip: String,
    pub master: String,
    pub snapshot: Option<String>,
    pub block_store: Option<String>,
    pub target_iqn: Option<String>,
    pub writeback: Option<String>,
    pub created_at: Option<String>,
    pub last_modified: Option<String>,
    pub block_device: Option<String>,
    pub status: Option<String>,
    pub mode: Option<String>,
    pub pxe_mode: Option<String>,
    pub keep_writeback: Option<bool>, // If false, writeback is deleted on shutdown (non-persistent mode)
    pub use_game_disk: Option<bool>,
}

impl Client {
    /// Check if client is online
    pub fn is_online(&self) -> bool {
        matches!(self.status.as_deref(), Some("Online"))
    }

    /// Check if client is offline
    pub fn is_offline(&self) -> bool {
        matches!(self.status.as_deref(), Some("Offline"))
    }

    /// Check if client is in super mode
    pub fn is_super_mode(&self) -> bool {
        matches!(self.mode.as_deref(), Some("super"))
    }

    /// Get normalized MAC address (uppercase, colon-separated)
    pub fn normalized_mac(&self) -> String {
        self.mac.to_uppercase()
    }

    /// Check if client has a master image assigned
    pub fn has_master(&self) -> bool {
        !self.master.is_empty()
    }

    /// Check if client has a snapshot assigned
    pub fn has_snapshot(&self) -> bool {
        self.snapshot.is_some() && !self.snapshot.as_ref().unwrap().is_empty()
    }
}

/// Request to add a new client
#[derive(Debug, Deserialize)]
pub struct AddClientRequest {
    pub name: String,
    pub mac: String,
    pub ip: String,
    pub master: String,
    pub snapshot: Option<String>,
    pub keep_writeback: Option<bool>, // Default: true for backward compatibility
    pub use_game_disk: Option<bool>,
}

impl AddClientRequest {
    /// Validate the request
    pub fn validate(&self) -> Result<(), crate::error::AppError> {
        // Name is optional (auto-generated if empty)
        if !self.name.trim().is_empty() {
            crate::validation::validate_client_id(&self.name)?;
        }

        crate::validation::validate_mac_address(&self.mac)?;
        crate::validation::validate_ip_address(&self.ip)?;

        Ok(())
    }
}

/// Request to edit an existing client
#[derive(Debug, Deserialize)]
pub struct EditClientRequest {
    pub name: String,
    pub mac: String,
    pub ip: String,
    pub master: String,
    pub snapshot: Option<String>,
    pub keep_writeback: Option<bool>,
    pub use_game_disk: Option<bool>,
}

impl EditClientRequest {
    /// Validate the request
    pub fn validate(&self) -> Result<(), crate::error::AppError> {
        crate::validation::validate_client_id(&self.name)?;
        crate::validation::validate_mac_address(&self.mac)?;
        crate::validation::validate_ip_address(&self.ip)?;
        Ok(())
    }
}

/// Request to control a client (wake, reboot, shutdown, etc.)
#[derive(Debug, Deserialize)]
pub struct ControlRequest {
    pub action: String,
    pub make_super: Option<bool>,
}

/// Client overview information
#[derive(Debug, Serialize, Deserialize)]
pub struct ClientOverview {
    pub total_clients: usize,
    pub active_clients: usize,
    pub offline_clients: usize,
}
