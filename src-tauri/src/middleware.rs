//! Middleware for authentication and authorization
use crate::auth::{validate_token, AuthError, Claims};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthRequest {
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthResponse {
    pub claims: Claims,
}

/// Middleware function to validate authentication token
pub fn validate_auth(request: AuthRequest) -> Result<AuthResponse, AuthError> {
    let claims = validate_token(&request.token)?;
    Ok(AuthResponse { claims })
}

/// Middleware for Tauri commands that require authentication
#[tauri::command]
pub fn authenticate(request: AuthRequest) -> Result<AuthResponse, AuthError> {
    validate_auth(request)
}

/// Function to validate authentication token for Tauri commands
pub fn validate_auth_token_for_command(token: &str) -> Result<Claims, AuthError> {
    validate_token(token)
}

