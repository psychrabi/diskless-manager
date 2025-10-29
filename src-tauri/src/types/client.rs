//! Client management types
//!
//! This module contains all client-related types and structures.

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Local};

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
}

impl AddClientRequest {
    /// Validate the request
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("Client name cannot be empty".to_string());
        }

        if self.mac.trim().is_empty() {
            return Err("MAC address cannot be empty".to_string());
        }

        if self.ip.trim().is_empty() {
            return Err("IP address cannot be empty".to_string());
        }

        // Validate MAC format
        if !is_valid_mac(&self.mac) {
            return Err("Invalid MAC address format".to_string());
        }

        // Validate IP format
        if !is_valid_ip(&self.ip) {
            return Err("Invalid IP address format".to_string());
        }

        Ok(())
    }
}

/// Request to control a client (wake, reboot, shutdown, etc.)
#[derive(Debug, Deserialize)]
pub struct ControlRequest {
    pub action: String,
    pub make_super: Option<bool>,
}

/// Request to deprovision a client
#[derive(Debug, Deserialize)]
pub struct DeprovisionRequest {
    pub mac: String,
    pub force: Option<bool>,
    pub keep_zfs: Option<bool>,
    pub dry_run: Option<bool>,
}

/// Client overview information
#[derive(Debug, Serialize, Deserialize)]
pub struct ClientOverview {
    pub total_clients: usize,
    pub active_clients: usize,
    pub offline_clients: usize,
}

/// Helper function to validate MAC address format
fn is_valid_mac(mac: &str) -> bool {
    let mac_regex = regex::Regex::new(r"^([0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}$").unwrap();
    mac_regex.is_match(mac.trim())
}

/// Helper function to validate IP address format
fn is_valid_ip(ip: &str) -> bool {
    let ip_regex = regex::Regex::new(
        r"^(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)$"
    ).unwrap();
    ip_regex.is_match(ip.trim())
}