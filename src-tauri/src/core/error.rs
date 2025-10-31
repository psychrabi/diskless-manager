//! Centralized error handling for the Diskless Manager
//!
//! This module provides a unified error type system that combines
//! domain-specific errors with infrastructure errors.

use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

/// Unified error type for the entire application
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum DisklessError {
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("Authentication error: {0}")]
    Auth(#[from] AuthError),

    #[error("Client management error: {0}")]
    Client(#[from] ClientError),

    #[error("Image management error: {0}")]
    Image(#[from] ImageError),

    #[error("Disk management error: {0}")]
    Disk(#[from] DiskError),

    #[error("Service management error: {0}")]
    Service(#[from] ServiceError),

    #[error("License error: {0}")]
    License(#[from] LicenseError),

    #[error("ZFS operation failed: {0}")]
    Zfs(#[from] ZfsError),

    #[error("DHCP configuration error: {0}")]
    Dhcp(#[from] DhcpError),

    #[error("iSCSI operation failed: {0}")]
    Iscsi(#[from] IscsiError),

    #[error("File system operation failed: {0}")]
    Filesystem(#[from] FilesystemError),

    #[error("Process execution failed: {0}")]
    Process(#[from] ProcessError),

    #[error("Network operation failed: {0}")]
    Network(#[from] NetworkError),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Input error: {0}")]
    Input(String),

    #[error("System resource error: {0}")]
    Resource(String),

    #[error("Timeout error: {0}")]
    Timeout(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl DisklessError {
    /// Create a validation error
    pub fn validation<T: fmt::Display>(message: T) -> Self {
        DisklessError::Validation(message.to_string())
    }

    /// Create an input validation error
    pub fn invalid_input<T: fmt::Display>(message: T) -> Self {
        DisklessError::Input(message.to_string())
    }

    /// Create an internal error
    pub fn internal<T: fmt::Display>(message: T) -> Self {
        DisklessError::Internal(message.to_string())
    }

    /// Create a timeout error
    pub fn timeout<T: fmt::Display>(message: T) -> Self {
        DisklessError::Timeout(message.to_string())
    }

    /// Check if this is a user-facing error (not internal debugging info)
    pub fn is_user_facing(&self) -> bool {
        !matches!(self, DisklessError::Internal(_))
    }
}

/// Result type alias for the application
pub type Result<T> = std::result::Result<T, DisklessError>;

/// Configuration-related errors
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum ConfigError {
    #[error("Config file not found: {0}")]
    FileNotFound(String),

    #[error("Invalid config format: {0}")]
    InvalidFormat(String),

    #[error("Failed to read config: {0}")]
    ReadError(String),

    #[error("Failed to write config: {0}")]
    WriteError(String),

    #[error("Config validation failed: {0}")]
    ValidationError(String),

    #[error("Missing required config key: {0}")]
    MissingKey(String),
}

/// Authentication-related errors
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum AuthError {
    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Token expired")]
    TokenExpired,

    #[error("Invalid token format")]
    InvalidToken,

    #[error("Insufficient permissions")]
    InsufficientPermissions,

    #[error("License validation failed: {0}")]
    LicenseValidation(String),
}

/// Client management errors
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum ClientError {
    #[error("Client not found: {0}")]
    NotFound(String),

    #[error("Client already exists: {0}")]
    AlreadyExists(String),

    #[error("Invalid client data: {0}")]
    InvalidData(String),

    #[error("Client operation failed: {0}")]
    OperationFailed(String),

    #[error("Client dependency error: {0}")]
    DependencyError(String),
}

/// Image management errors
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum ImageError {
    #[error("Image not found: {0}")]
    NotFound(String),

    #[error("Image already exists: {0}")]
    AlreadyExists(String),

    #[error("Invalid image configuration: {0}")]
    InvalidConfig(String),

    #[error("Snapshot operation failed: {0}")]
    SnapshotFailed(String),

    #[error("Clone operation failed: {0}")]
    CloneFailed(String),
}

/// Disk management errors
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum DiskError {
    #[error("Disk not found: {0}")]
    NotFound(String),

    #[error("Invalid disk configuration: {0}")]
    InvalidConfig(String),

    #[error("Disk operation failed: {0}")]
    OperationFailed(String),

    #[error("Insufficient disk space")]
    InsufficientSpace,
}

/// Service management errors
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum ServiceError {
    #[error("Service not found: {0}")]
    NotFound(String),

    #[error("Service operation failed: {0}")]
    OperationFailed(String),

    #[error("Service configuration error: {0}")]
    ConfigError(String),
}

/// License management errors
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum LicenseError {
    #[error("License not activated")]
    NotActivated,

    #[error("License expired")]
    Expired,

    #[error("Invalid license key")]
    InvalidKey,

    #[error("License server error: {0}")]
    ServerError(String),
}

/// ZFS operation errors
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum ZfsError {
    #[error("Dataset not found: {0}")]
    DatasetNotFound(String),

    #[error("Pool not found: {0}")]
    PoolNotFound(String),

    #[error("Snapshot not found: {0}")]
    SnapshotNotFound(String),

    #[error("ZFS operation failed: {0}")]
    OperationFailed(String),

    #[error("Permission denied")]
    PermissionDenied,
}

/// DHCP configuration errors
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum DhcpError {
    #[error("Config file error: {0}")]
    ConfigFile(String),

    #[error("Invalid DHCP entry: {0}")]
    InvalidEntry(String),

    #[error("Service restart failed")]
    ServiceRestartFailed,
}

/// iSCSI operation errors
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum IscsiError {
    #[error("Target not found: {0}")]
    TargetNotFound(String),

    #[error("LUN not found: {0}")]
    LunNotFound(String),

    #[error("Backstore error: {0}")]
    BackstoreError(String),

    #[error("iSCSI operation failed: {0}")]
    OperationFailed(String),
}

/// Filesystem operation errors
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum FilesystemError {
    #[error("Path not found: {0}")]
    PathNotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Disk full")]
    DiskFull,

    #[error("IO error: {0}")]
    Io(String),
}

/// Process execution errors
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum ProcessError {
    #[error("Command not found: {0}")]
    CommandNotFound(String),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Exit code {code}: {output}")]
    NonZeroExit { code: i32, output: String },
}

/// Network operation errors
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum NetworkError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("DNS resolution failed: {0}")]
    DnsResolutionFailed(String),
}

/// Helper function to convert string errors to internal errors
impl From<String> for DisklessError {
    fn from(s: String) -> Self {
        DisklessError::internal(s)
    }
}

/// Helper function to convert &str errors to internal errors
impl From<&str> for DisklessError {
    fn from(s: &str) -> Self {
        DisklessError::internal(s.to_string())
    }
}

/// Helper function to convert std::io::Error
impl From<std::io::Error> for DisklessError {
    fn from(error: std::io::Error) -> Self {
        DisklessError::Filesystem(FilesystemError::Io(error.to_string()))
    }
}

/// Helper function to convert reqwest errors
impl From<reqwest::Error> for DisklessError {
    fn from(error: reqwest::Error) -> Self {
        DisklessError::Network(NetworkError::Http(error.to_string()))
    }
}