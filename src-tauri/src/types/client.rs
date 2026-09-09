//! Client management types
//!
//! This module contains client-related request types and structures.

use serde::Deserialize;

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
