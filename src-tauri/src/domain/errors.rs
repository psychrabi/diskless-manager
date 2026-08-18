use thiserror::Error;

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("invalid client name: {0}")]
    InvalidClientName(String),

    #[error("invalid MAC address: {0}")]
    InvalidMacAddress(String),

    #[error("invalid IP address: {0}")]
    InvalidIpAddress(String),

    #[error("invalid client ID")]
    InvalidClientId,

    #[error("invalid image ID")]
    InvalidImageId,

    #[error("client name cannot be empty")]
    EmptyClientName,

    #[error("client master image cannot be empty")]
    EmptyMasterImage,
}
