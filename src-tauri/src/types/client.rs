//! Client management types
//!
//! This module contains client-related request types and structures.

use serde::{Deserialize, Serialize};

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
