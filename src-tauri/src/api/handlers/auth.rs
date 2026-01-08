use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use log::info;
use serde::{Deserialize, Serialize};

use crate::{
    auth::{authenticate_user, validate_token},
    types::{LoginRequest, LoginResponse},
    state::AppState,
};

pub async fn login(Json(request): Json<LoginRequest>) -> Result<Json<LoginResponse>, StatusCode> {
    let username = request.username.clone();
    let password = request.password.clone();

    info!("login attempt: user={}", username);

    let auth_result = authenticate_user(&username, &password);

    match auth_result {
        Ok(response) => {
            info!("login success: user={}", username);
            Ok(Json(response))
        }
        Err(_) => Err(StatusCode::UNAUTHORIZED),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ValidateTokenRequest {
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ValidateTokenResponse {
    pub valid: bool,
    pub username: Option<String>,
    pub role: Option<String>,
}

pub async fn validate_auth_token(
    Json(request): Json<ValidateTokenRequest>,
) -> Result<Json<ValidateTokenResponse>, StatusCode> {
    match validate_token(&request.token) {
        Ok(claims) => {
            info!("token validation success: user={}", claims.username);
            Ok(Json(ValidateTokenResponse {
                valid: true,
                username: Some(claims.username),
                role: Some(claims.role),
            }))
        }
        Err(_) => {
            info!("token validation failed");
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateAdminPasswordRequest {
    pub old_password: String,
    pub new_password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateAdminPasswordResponse {
    pub message: String,
}

pub async fn update_admin_password(
    State(state): State<AppState>,
    Json(request): Json<UpdateAdminPasswordRequest>,
) -> Result<Json<UpdateAdminPasswordResponse>, StatusCode> {
    use bcrypt::{hash, verify, DEFAULT_COST};

    // Get current admin password hash from config
    let cfg = crate::config::get_config();
    let mut current_hash = None;
    
    if let Some(obj) = cfg.settings.as_object() {
        if let Some(val) = obj.get("admin_password") {
            if let Some(s) = val.as_str() {
                current_hash = Some(s.to_string());
            }
        }
    }

    // Fallback to default admin user hash if not in config
    if current_hash.is_none() {
        if let Some(u) = crate::auth::USERS.get("admin") {
            current_hash = Some(u.password_hash.clone());
        }
    }

    let current_hash = current_hash.ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Verify old password
    let valid = verify(&request.old_password, &current_hash)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !valid {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Hash new password
    let hashed = hash(&request.new_password, DEFAULT_COST)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Update config with new password hash
    let mut cfg = crate::config::get_config();
    let mut settings = cfg.settings.as_object().cloned().unwrap_or_default();
    settings.insert(
        "admin_password".to_string(),
        serde_json::to_value(&hashed).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    );
    cfg.settings = serde_json::Value::Object(settings);

    // Write config to database
    crate::config::write_config(&state.db_pool, &cfg)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    info!("admin password updated");
    Ok(Json(UpdateAdminPasswordResponse {
        message: "Admin password updated successfully".to_string(),
    }))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AdminExistsResponse {
    pub exists: bool,
}

pub async fn check_admin_exists() -> Result<Json<AdminExistsResponse>, StatusCode> {
    // Check if admin user exists in the system
    let cfg = crate::config::get_config();
    let has_admin_password = cfg
        .settings
        .as_object()
        .and_then(|obj| obj.get("admin_password"))
        .and_then(|val| val.as_str())
        .is_some();

    // Admin exists if either configured password exists or default admin user exists
    let exists = has_admin_password || crate::auth::USERS.contains_key("admin");

    Ok(Json(AdminExistsResponse { exists }))
}
