//! Input validation module
//!
//! Provides comprehensive validation for all user inputs before they are used
//! in system commands. This prevents command injection and ensures data integrity.

use regex::Regex;
use std::net::Ipv4Addr;
use std::str::FromStr;
use thiserror::Error;

lazy_static::lazy_static! {
    // Client ID: alphanumeric, dash, underscore (1-32 chars)
    static ref CLIENT_ID_RE: Regex = Regex::new(r"^[a-zA-Z0-9_-]{1,32}$").expect("Failed to compile CLIENT_ID regex");
    // MAC Address: standard format with colons or dashes
    static ref MAC_RE: Regex = Regex::new(r"^([0-9A-Fa-f]{2}[:-]){5}([0-9A-Fa-f]{2})$").expect("Failed to compile MAC regex");
    static ref DATASET_NAME_RE: Regex = Regex::new(r"^[a-zA-Z0-9/_-]+$").expect("Failed to compile DATASET_NAME regex");
    static ref POOL_NAME_RE: Regex = Regex::new(r"^[a-zA-Z0-9_.-]+$").expect("Failed to compile POOL_NAME regex");
    static ref IQN_RE: Regex = Regex::new(r"^iqn\.\d{4}-\d{2}\.[a-z0-9.-]+:[a-zA-Z0-9._-]+$").expect("Failed to compile IQN regex");
    static ref SNAPSHOT_NAME_RE: Regex = Regex::new(r"^[a-zA-Z0-9._-]+$").expect("Failed to compile SNAPSHOT_NAME regex");
    static ref ZFS_NAME_RE: Regex = Regex::new(r"^[a-zA-Z0-9_-]+$").expect("Failed to compile ZFS_NAME regex");
    static ref SIZE_RE: Regex = Regex::new(r"^\d+[KMGTP]?$").expect("Failed to compile SIZE regex");
}

#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("Invalid client ID: must be alphanumeric, dash, or underscore (1-32 characters)")]
    InvalidClientId,

    #[error("Invalid MAC address: must be in format XX:XX:XX:XX:XX:XX or XX-XX-XX-XX-XX-XX")]
    InvalidMacAddress,

    #[error("Invalid IP address: must be a valid IPv4 address")]
    InvalidIpAddress,

    #[error("Invalid dataset name: must contain only alphanumeric characters, slash, dash, or underscore")]
    InvalidDatasetName,

    #[error("Invalid ZFS pool name: must contain only alphanumeric characters, dash, underscore, or dot")]
    InvalidPoolName,

    #[error("Invalid IQN: must follow iSCSI Qualified Name format (RFC 3720)")]
    InvalidIqn,

    #[error("Invalid snapshot name: must contain only alphanumeric characters, dash, underscore, or dot")]
    InvalidSnapshotName,

    #[error("Invalid size format: must be a number optionally followed by K, M, G, or T")]
    InvalidSize,

    #[error("Invalid path: path traversal detected")]
    PathTraversal,

    #[error("Invalid name: must contain only alphanumeric characters, dash, or underscore")]
    InvalidName,

    #[error("Empty value not allowed")]
    EmptyValue,
}

/// Validates a client ID
///
/// # Rules
/// - 1-32 characters
/// - Only alphanumeric, dash, or underscore
///
/// # Examples
/// ```
/// # use app_lib::validation::validate_client_id;
/// assert!(validate_client_id("PC001").is_ok());
/// assert!(validate_client_id("client-123").is_ok());
/// assert!(validate_client_id("invalid@id").is_err());
/// ```
pub fn validate_client_id(id: &str) -> Result<(), ValidationError> {
    if id.is_empty() {
        return Err(ValidationError::EmptyValue);
    }

    if CLIENT_ID_RE.is_match(id) {
        Ok(())
    } else {
        Err(ValidationError::InvalidClientId)
    }
}

/// Validates a MAC address
///
/// # Rules
/// - Standard format: XX:XX:XX:XX:XX:XX or XX-XX-XX-XX-XX-XX
/// - Hexadecimal digits (0-9, A-F, case insensitive)
///
/// # Examples
/// ```
/// # use app_lib::validation::validate_mac_address;
/// assert!(validate_mac_address("00:11:22:33:44:55").is_ok());
/// assert!(validate_mac_address("AA-BB-CC-DD-EE-FF").is_ok());
/// assert!(validate_mac_address("invalid").is_err());
/// ```
pub fn validate_mac_address(mac: &str) -> Result<(), ValidationError> {
    if mac.is_empty() {
        return Err(ValidationError::EmptyValue);
    }

    if MAC_RE.is_match(mac) {
        Ok(())
    } else {
        Err(ValidationError::InvalidMacAddress)
    }
}

/// Validates an IPv4 address
///
/// # Rules
/// - Valid IPv4 format (0-255.0-255.0-255.0-255)
///
/// # Examples
/// ```
/// # use app_lib::validation::validate_ip_address;
/// assert!(validate_ip_address("192.168.1.100").is_ok());
/// assert!(validate_ip_address("10.0.0.1").is_ok());
/// assert!(validate_ip_address("256.1.1.1").is_err());
/// ```
pub fn validate_ip_address(ip: &str) -> Result<(), ValidationError> {
    if ip.is_empty() {
        return Err(ValidationError::EmptyValue);
    }

    Ipv4Addr::from_str(ip)
        .map(|_| ())
        .map_err(|_| ValidationError::InvalidIpAddress)
}

/// Validates a ZFS dataset name
///
/// # Rules
/// - Only alphanumeric, slash, dash, or underscore
/// - No path traversal (../)
///
/// # Examples
/// ```
/// # use app_lib::validation::validate_dataset_name;
/// assert!(validate_dataset_name("tank/images/ubuntu").is_ok());
/// assert!(validate_dataset_name("pool/data-set_01").is_ok());
/// assert!(validate_dataset_name("../etc/passwd").is_err());
/// ```
pub fn validate_dataset_name(name: &str) -> Result<(), ValidationError> {
    if name.is_empty() {
        return Err(ValidationError::EmptyValue);
    }

    // Check for path traversal
    if name.contains("..") {
        return Err(ValidationError::PathTraversal);
    }

    if DATASET_NAME_RE.is_match(name) {
        Ok(())
    } else {
        Err(ValidationError::InvalidDatasetName)
    }
}

/// Validates a ZFS pool name
///
/// # Rules
/// - Only alphanumeric, dash, underscore, or dot
///
/// # Examples
/// ```
/// # use app_lib::validation::validate_pool_name;
/// assert!(validate_pool_name("tank").is_ok());
/// assert!(validate_pool_name("my-pool_01").is_ok());
/// assert!(validate_pool_name("pool/invalid").is_err());
/// ```
pub fn validate_pool_name(name: &str) -> Result<(), ValidationError> {
    if name.is_empty() {
        return Err(ValidationError::EmptyValue);
    }

    if POOL_NAME_RE.is_match(name) {
        Ok(())
    } else {
        Err(ValidationError::InvalidPoolName)
    }
}

/// Validates an iSCSI Qualified Name (IQN)
///
/// # Rules
/// - Follows RFC 3720 format
/// - Format: iqn.YYYY-MM.reversed.domain.name:identifier
///
/// # Examples
/// ```
/// # use app_lib::validation::validate_iqn;
/// assert!(validate_iqn("iqn.2025-04.local.diskless:client001").is_ok());
/// assert!(validate_iqn("invalid-iqn").is_err());
/// ```
pub fn validate_iqn(iqn: &str) -> Result<(), ValidationError> {
    if iqn.is_empty() {
        return Err(ValidationError::EmptyValue);
    }

    if IQN_RE.is_match(iqn) {
        Ok(())
    } else {
        Err(ValidationError::InvalidIqn)
    }
}

/// Validates a ZFS name (non-hierarchical)
///
/// # Rules
/// - Only alphanumeric, dash, or underscore
///
/// # Examples
/// ```
/// # use app_lib::validation::validate_zfs_name;
/// assert!(validate_zfs_name("myimage").is_ok());
/// assert!(validate_zfs_name("image-01").is_ok());
/// assert!(validate_zfs_name("path/to/image").is_err());
/// ```
pub fn validate_zfs_name(name: &str) -> Result<(), ValidationError> {
    if name.is_empty() {
        return Err(ValidationError::EmptyValue);
    }

    if ZFS_NAME_RE.is_match(name) {
        Ok(())
    } else {
        Err(ValidationError::InvalidName)
    }
}

/// Validates a ZFS snapshot name
///
/// # Rules
/// - Only alphanumeric, dash, underscore, or dot
///
/// # Examples
/// ```
/// # use app_lib::validation::validate_snapshot_name;
/// assert!(validate_snapshot_name("snap-2025-01-01").is_ok());
/// assert!(validate_snapshot_name("backup.001").is_ok());
/// assert!(validate_snapshot_name("snap/invalid").is_err());
/// ```
pub fn validate_snapshot_name(name: &str) -> Result<(), ValidationError> {
    if name.is_empty() {
        return Err(ValidationError::EmptyValue);
    }

    if SNAPSHOT_NAME_RE.is_match(name) {
        Ok(())
    } else {
        Err(ValidationError::InvalidSnapshotName)
    }
}

/// Validates a size specification
///
/// # Rules
/// - Number followed by optional unit (K, M, G, T)
/// - Examples: "100M", "10G", "1024"
///
/// # Examples
/// ```
/// # use app_lib::validation::validate_size;
/// assert!(validate_size("100M").is_ok());
/// assert!(validate_size("10G").is_ok());
/// assert!(validate_size("1024").is_ok());
/// assert!(validate_size("10GB").is_err());
/// ```
pub fn validate_size(size: &str) -> Result<(), ValidationError> {
    if size.is_empty() {
        return Err(ValidationError::EmptyValue);
    }

    if SIZE_RE.is_match(size) {
        Ok(())
    } else {
        Err(ValidationError::InvalidSize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_id_validation() {
        assert!(validate_client_id("PC001").is_ok());
        assert!(validate_client_id("client-123").is_ok());
        assert!(validate_client_id("my_client_01").is_ok());

        assert!(validate_client_id("").is_err());
        assert!(validate_client_id("client@test").is_err());
        assert!(validate_client_id("client with spaces").is_err());
        assert!(validate_client_id("x".repeat(33).as_str()).is_err()); // too long
    }

    #[test]
    fn test_mac_address_validation() {
        assert!(validate_mac_address("00:11:22:33:44:55").is_ok());
        assert!(validate_mac_address("AA:BB:CC:DD:EE:FF").is_ok());
        assert!(validate_mac_address("aa-bb-cc-dd-ee-ff").is_ok());

        assert!(validate_mac_address("").is_err());
        assert!(validate_mac_address("invalid").is_err());
        assert!(validate_mac_address("00:11:22:33:44").is_err()); // too short
        assert!(validate_mac_address("GG:11:22:33:44:55").is_err()); // invalid hex
    }

    #[test]
    fn test_ip_validation() {
        assert!(validate_ip_address("192.168.1.1").is_ok());
        assert!(validate_ip_address("10.0.0.1").is_ok());
        assert!(validate_ip_address("0.0.0.0").is_ok());

        assert!(validate_ip_address("").is_err());
        assert!(validate_ip_address("256.1.1.1").is_err());
        assert!(validate_ip_address("192.168.1").is_err());
        assert!(validate_ip_address("not-an-ip").is_err());
    }

    #[test]
    fn test_dataset_name_validation() {
        assert!(validate_dataset_name("tank/images/ubuntu").is_ok());
        assert!(validate_dataset_name("pool/data-set_01").is_ok());

        assert!(validate_dataset_name("").is_err());
        assert!(validate_dataset_name("../etc/passwd").is_err()); // path traversal
        assert!(validate_dataset_name("pool/../other").is_err()); // path traversal
        assert!(validate_dataset_name("pool with spaces").is_err());
    }

    #[test]
    fn test_pool_name_validation() {
        assert!(validate_pool_name("tank").is_ok());
        assert!(validate_pool_name("my-pool_01").is_ok());
        assert!(validate_pool_name("pool.backup").is_ok());

        assert!(validate_pool_name("").is_err());
        assert!(validate_pool_name("pool/invalid").is_err());
        assert!(validate_pool_name("pool with spaces").is_err());
    }

    #[test]
    fn test_iqn_validation() {
        assert!(validate_iqn("iqn.2025-04.local.diskless:client001").is_ok());
        assert!(validate_iqn("iqn.2024-12.com.example:target_01").is_ok());

        assert!(validate_iqn("").is_err());
        assert!(validate_iqn("invalid-iqn").is_err());
        assert!(validate_iqn("iqn.invalid").is_err());
    }

    #[test]
    fn test_snapshot_name_validation() {
        assert!(validate_snapshot_name("snap-2025-01-01").is_ok());
        assert!(validate_snapshot_name("backup.001").is_ok());
        assert!(validate_snapshot_name("my_snapshot-01").is_ok());

        assert!(validate_snapshot_name("").is_err());
        assert!(validate_snapshot_name("snap/invalid").is_err());
        assert!(validate_snapshot_name("snap with spaces").is_err());
    }

    #[test]
    fn test_size_validation() {
        assert!(validate_size("100M").is_ok());
        assert!(validate_size("10G").is_ok());
        assert!(validate_size("1024").is_ok());
        assert!(validate_size("5T").is_ok());

        assert!(validate_size("").is_err());
        assert!(validate_size("10GB").is_err()); // wrong format
        assert!(validate_size("abc").is_err());
        assert!(validate_size("-10M").is_err());
    }
}
