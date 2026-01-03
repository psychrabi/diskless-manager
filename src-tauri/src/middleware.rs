//! Middleware for authentication and authorization
use crate::auth::validate_token;
use crate::types::{AuthError, Claims};

/// Middleware for Tauri commands that require authentication
#[tauri::command]
pub fn authenticate(token: String) -> Result<Claims, AuthError> {
    validate_token(&token)
}

/// Function to validate authentication token for Tauri commands
pub fn validate_auth_token_for_command(token: &str) -> Result<Claims, AuthError> {
    validate_token(token)
}

/// Helper for Tauri commands that return AppError
pub fn validate_auth(token: &str) -> Result<(), crate::error::AppError> {
    validate_auth_token_for_command(token)
        .map(|_| ())
        .map_err(|e| crate::error::AppError::Auth(e.message))
}
