use serde::Serialize;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Command execution failed: {0}")]
    Command(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Authentication failed: {0}")]
    Auth(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("SSH connection error: {0}")]
    SshConnection(String),

    #[error("SSH command timeout")]
    SshTimeout,

    #[error("SSH command failed: {0}")]
    SshCommand(String),

    #[error("SSH authentication failed: {0}")]
    SshAuth(String),
}

impl From<crate::validation::ValidationError> for AppError {
    fn from(err: crate::validation::ValidationError) -> Self {
        AppError::Validation(err.to_string())
    }
}

// Implement Serialize manually or via a helper to support Tauri return types
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
