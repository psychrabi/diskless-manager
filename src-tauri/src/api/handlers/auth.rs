use axum::{
    extract::{ConnectInfo, State},
    http::StatusCode,
    Extension, Json,
};
use log::info;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;

use crate::{
    audit_logger::{AuditLogger, NewAuditEvent},
    auth::{authenticate_user, bootstrap_admin, validate_token, BootstrapAdminError},
    state::AppState,
    types::{Claims, LoginRequest, LoginResponse},
};

async fn record_login_event(
    state: &AppState,
    username: &str,
    role: Option<&str>,
    ip: Option<String>,
    status_code: StatusCode,
) {
    let event = NewAuditEvent {
        actor: username.to_string(),
        actor_role: role.map(str::to_string),
        ip,
        module: "auth".to_string(),
        action: "login".to_string(),
        method: "POST".to_string(),
        path: "/api/auth/login".to_string(),
        status: if status_code.is_success() {
            "success".to_string()
        } else {
            "failed".to_string()
        },
        status_code: i64::from(status_code.as_u16()),
        details: (!status_code.is_success()).then(|| {
            status_code
                .canonical_reason()
                .unwrap_or("failed")
                .to_string()
        }),
    };

    if let Err(error) = AuditLogger::new(Arc::new(state.db_pool.clone()))
        .log_event(&event)
        .await
    {
        tracing::error!(%error, "failed to record login audit event");
    }
}

pub async fn login(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    let username = request.username.clone();
    let password = request.password.clone();
    let ip = Some(peer.ip().to_string());

    info!("login attempt: user={}", username);

    let auth_result = authenticate_user(&state.db_pool, &username, &password).await;

    match auth_result {
        Ok(response) => {
            info!("login success: user={}", username);
            record_login_event(
                &state,
                &username,
                Some(&response.user.role),
                ip,
                StatusCode::OK,
            )
            .await;
            Ok(Json(response))
        }
        Err(_) => {
            record_login_event(&state, &username, None, ip, StatusCode::UNAUTHORIZED).await;
            Err(StatusCode::UNAUTHORIZED)
        }
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
    State(state): State<AppState>,
    Json(request): Json<ValidateTokenRequest>,
) -> Result<Json<ValidateTokenResponse>, StatusCode> {
    match validate_token(&state.db_pool, &request.token).await {
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
    use crate::auth::{hash_password, verify_password};
    use chrono::Utc;

    if claims.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }

    // Fetch the authenticated administrator from database
    let admin_user = sqlx::query_as::<_, crate::types::User>(
        r#"
        SELECT id, username, password_hash, role, session_version
        FROM users
        WHERE id = ?
        "#,
    )
    .bind(&claims.sub)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Verify old password
    let valid = verify_password(&request.old_password, &admin_user.password_hash)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !valid {
        return Err(StatusCode::UNAUTHORIZED);
    }

    if !crate::auth::valid_password(&request.new_password) {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Hash new password
    let hashed = hash_password(&request.new_password)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Update password in database
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE users
        SET password_hash = ?, updated_at = ?, session_version = session_version + 1
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
