//! Service management types
//!
//! This module contains all service-related types and structures.

use serde::{Deserialize, Serialize};

/// Service control request
#[derive(Debug, Deserialize)]
pub struct ServiceControlRequest {
    pub action: String,
}

/// Package status information
#[derive(Debug, Serialize, Deserialize)]
pub struct PackageStatus {
    pub name: String,
    pub service: String,
    pub installed: bool,
    pub configured: bool,
    pub running: bool,
    pub version: Option<String>,
}

/// DHCP configuration structure
#[derive(Debug, Serialize, Deserialize)]
pub struct DHCPConfig {
    pub subnet_ip: String,
    pub start_ip: String,
    pub end_ip: String,
    pub subnet_mask: String,
    pub gateway_ip: String,
    pub dns_server1: String,
    pub dns_server2: String,
    pub broadcast_ip: String,
    pub next_server_ip: String,
    pub boot_server_ip: String,
    pub boot_script: String,
    pub boot_file_legacy: String,
    pub boot_file_uefi32: String,
    pub boot_file_uefi64: String,
}

/// TFTP configuration structure
#[derive(Debug, Serialize, Deserialize)]
pub struct TFTPConfig {
    pub tftp_root: String,
    pub tftp_server_ip: String,
    pub tftp_options: String,
}

/// HTTP configuration structure
#[derive(Debug, Serialize, Deserialize)]
pub struct HTTPConfig {
    pub http_root: String,
    pub http_server_ip: String,
    pub http_server_port: String,
}

/// Samba share configuration
#[derive(Debug, Serialize, Deserialize)]
pub struct SambaShare {
    pub name: String,
    pub path: String,
    pub read_only: bool,
    pub guest_ok: bool,
}