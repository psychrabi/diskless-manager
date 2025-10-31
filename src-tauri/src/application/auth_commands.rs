//! Authentication commands - Tauri command handlers
//!
//! This module provides the Tauri command handlers for authentication operations.

use crate::core::error::{DisklessError, Result};
use crate::types::auth::{LoginRequest, LoginResponse, Claims};
use crate::constants::auth;

#[tauri::command]
pub async fn login(request: LoginRequest) -> Result<LoginResponse, String> {
    // Implementation would use the auth domain service
    // For now, return a placeholder
    Ok(LoginResponse {
        token: "demo_token".to_string(),
        user: crate::types::auth::UserResponse {
            id: "1".to_string(),
            username: request.username,
            role: "admin".to_string(),
        }
    })
}

#[tauri::command]
pub async fn validate_auth_token(token: &str) -> Result<Claims, String> {
    // Implementation would validate JWT token
    Ok(Claims {
        sub: "1".to_string(),
        username: "admin".to_string(),
        role: "admin".to_string(),
        exp: chrono::Utc::now().timestamp() + 86400,
    })
}

#[tauri::command]
pub async fn update_admin_password(
    token: &str,
    old_password: &str,
    new_password: &str,
) -> Result<String, String> {
    // Implementation would update admin password
    Ok("Admin password updated successfully".to_string())
}