use axum::{extract::State, http::StatusCode, Extension, Json};
use log::info;
use serde::{Deserialize, Serialize};

use crate::{
    auth::{authenticate_user, bootstrap_admin, validate_token, BootstrapAdminError},
    state::AppState,
    types::{Claims, LoginRequest, LoginResponse},
};

pub async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    let username = request.username.clone();
    let password = request.password.clone();

    info!("login attempt: user={}", username);

    let auth_result = authenticate_user(&state.db_pool, &username, &password).await;

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

#[derive(Debug, Serialize, Deserialize)]
pub struct BootstrapAdminRequest {
    pub username: String,
    pub password: String,
}

pub async fn bootstrap_first_admin(
    State(state): State<AppState>,
    Json(request): Json<BootstrapAdminRequest>,
) -> Result<StatusCode, StatusCode> {
    bootstrap_admin(&state.db_pool, &request.username, &request.password)
        .await
        .map_err(|error| match error {
            BootstrapAdminError::InvalidCredentials => StatusCode::BAD_REQUEST,
            BootstrapAdminError::AlreadyInitialized => StatusCode::CONFLICT,
            BootstrapAdminError::PasswordHash | BootstrapAdminError::Database(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;

    info!("first administrator created: user={}", request.username);
    Ok(StatusCode::CREATED)
}

pub async fn update_admin_password(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<UpdateAdminPasswordRequest>,
) -> Result<Json<UpdateAdminPasswordResponse>, StatusCode> {
    use bcrypt::{hash, verify, DEFAULT_COST};
    use chrono::Utc;

    if claims.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }

    // Fetch the authenticated administrator from database
    let admin_user = sqlx::query_as::<_, crate::types::User>(
        r#"
        SELECT id, username, password_hash, role
        FROM users
        WHERE username = ?
        "#,
    )
    .bind(&claims.username)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Verify old password
    let valid = verify(&request.old_password, &admin_user.password_hash)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !valid {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Hash new password
    let hashed =
        hash(&request.new_password, DEFAULT_COST).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Update password in database
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE users
        SET password_hash = ?, updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(&hashed)
    .bind(&now)
    .bind(&admin_user.id)
    .execute(&state.db_pool)
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

pub async fn check_admin_exists(
    State(state): State<AppState>,
) -> Result<Json<AdminExistsResponse>, StatusCode> {
    // Any existing user means bootstrap is closed, preventing account takeover
    // if an older installation has inconsistent role data.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db_pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(AdminExistsResponse { exists: count > 0 }))
}
