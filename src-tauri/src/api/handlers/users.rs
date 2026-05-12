//! User management API handlers
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use bcrypt::{hash, DEFAULT_COST};
use chrono::Utc;
use log::info;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::validate_token,
    state::AppState,
    types::{AuthError, User, UserResponse},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub role: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateUserRequest {
    pub username: Option<String>,
    pub role: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateUserPasswordRequest {
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

impl From<AuthError> for ErrorResponse {
    fn from(err: AuthError) -> Self {
        ErrorResponse { error: err.message }
    }
}

/// Extract and validate JWT token from Authorization header
fn extract_token(headers: &axum::http::HeaderMap) -> Result<String, StatusCode> {
    let auth_header = headers
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if let Some(token) = auth_header.strip_prefix("Bearer ") {
        Ok(token.to_string())
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

/// Validate token and ensure user is admin
fn validate_admin_token(token: &str) -> Result<(), StatusCode> {
    let claims = validate_token(token).map_err(|_| StatusCode::UNAUTHORIZED)?;

    if claims.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(())
}

/// List all users
pub async fn list_users(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<UserResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let token = extract_token(&headers).map_err(|e| {
        (
            e,
            Json(ErrorResponse {
                error: "Unauthorized".to_string(),
            }),
        )
    })?;

    validate_admin_token(&token).map_err(|e| {
        (
            e,
            Json(ErrorResponse {
                error: "Forbidden: admin role required".to_string(),
            }),
        )
    })?;

    let users = sqlx::query_as::<_, User>(
        r#"
        SELECT id, username, password_hash, role
        FROM users
        ORDER BY username
        "#,
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to fetch users: {}", e),
            }),
        )
    })?;

    let response: Vec<UserResponse> = users
        .into_iter()
        .map(|u| UserResponse {
            id: u.id,
            username: u.username,
            role: u.role,
        })
        .collect();

    Ok(Json(response))
}

/// Get a specific user by ID
pub async fn get_user(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(user_id): Path<String>,
) -> Result<Json<UserResponse>, (StatusCode, Json<ErrorResponse>)> {
    let token = extract_token(&headers).map_err(|e| {
        (
            e,
            Json(ErrorResponse {
                error: "Unauthorized".to_string(),
            }),
        )
    })?;

    validate_admin_token(&token).map_err(|e| {
        (
            e,
            Json(ErrorResponse {
                error: "Forbidden: admin role required".to_string(),
            }),
        )
    })?;

    let user = sqlx::query_as::<_, User>(
        r#"
        SELECT id, username, password_hash, role
        FROM users
        WHERE id = ?
        "#,
    )
    .bind(&user_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|_| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "User not found".to_string(),
            }),
        )
    })?;

    Ok(Json(UserResponse {
        id: user.id,
        username: user.username,
        role: user.role,
    }))
}

/// Create a new user
pub async fn create_user(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(request): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<UserResponse>), (StatusCode, Json<ErrorResponse>)> {
    let token = extract_token(&headers).map_err(|e| {
        (
            e,
            Json(ErrorResponse {
                error: "Unauthorized".to_string(),
            }),
        )
    })?;

    validate_admin_token(&token).map_err(|e| {
        (
            e,
            Json(ErrorResponse {
                error: "Forbidden: admin role required".to_string(),
            }),
        )
    })?;

    // Validate role
    if request.role != "admin" && request.role != "user" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid role. Must be 'admin' or 'user'".to_string(),
            }),
        ));
    }

    // Hash password
    let password_hash = hash(&request.password, DEFAULT_COST).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to hash password: {}", e),
            }),
        )
    })?;

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO users (id, username, password_hash, role, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(&request.username)
    .bind(&password_hash)
    .bind(&request.role)
    .bind(&now)
    .bind(&now)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to create user: {}", e),
            }),
        )
    })?;

    info!(
        "user created: {} (role: {})",
        request.username, request.role
    );

    Ok((
        StatusCode::CREATED,
        Json(UserResponse {
            id,
            username: request.username,
            role: request.role,
        }),
    ))
}

/// Update user details (username and/or role)
pub async fn update_user(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(user_id): Path<String>,
    Json(request): Json<UpdateUserRequest>,
) -> Result<Json<UserResponse>, (StatusCode, Json<ErrorResponse>)> {
    let token = extract_token(&headers).map_err(|e| {
        (
            e,
            Json(ErrorResponse {
                error: "Unauthorized".to_string(),
            }),
        )
    })?;

    validate_admin_token(&token).map_err(|e| {
        (
            e,
            Json(ErrorResponse {
                error: "Forbidden: admin role required".to_string(),
            }),
        )
    })?;

    // Fetch current user
    let user = sqlx::query_as::<_, User>(
        r#"
        SELECT id, username, password_hash, role
        FROM users
        WHERE id = ?
        "#,
    )
    .bind(&user_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|_| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "User not found".to_string(),
            }),
        )
    })?;

    let username = request.username.unwrap_or(user.username);
    let role = request.role.unwrap_or(user.role);

    // Validate role
    if role != "admin" && role != "user" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid role. Must be 'admin' or 'user'".to_string(),
            }),
        ));
    }

    let now = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        UPDATE users
        SET username = ?, role = ?, updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(&username)
    .bind(&role)
    .bind(&now)
    .bind(&user_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to update user: {}", e),
            }),
        )
    })?;

    info!("user updated: {} (role: {})", username, role);

    Ok(Json(UserResponse {
        id: user_id,
        username,
        role,
    }))
}

/// Update user password (admin can change any user's password)
pub async fn update_user_password(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(user_id): Path<String>,
    Json(request): Json<UpdateUserPasswordRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let token = extract_token(&headers).map_err(|e| {
        (
            e,
            Json(ErrorResponse {
                error: "Unauthorized".to_string(),
            }),
        )
    })?;

    validate_admin_token(&token).map_err(|e| {
        (
            e,
            Json(ErrorResponse {
                error: "Forbidden: admin role required".to_string(),
            }),
        )
    })?;

    // Hash new password
    let password_hash = hash(&request.password, DEFAULT_COST).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to hash password: {}", e),
            }),
        )
    })?;

    let now = Utc::now().to_rfc3339();

    let result = sqlx::query(
        r#"
        UPDATE users
        SET password_hash = ?, updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(&password_hash)
    .bind(&now)
    .bind(&user_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to update password: {}", e),
            }),
        )
    })?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "User not found".to_string(),
            }),
        ));
    }

    info!("password updated for user id: {}", user_id);

    Ok(Json(serde_json::json!({
        "message": "Password updated successfully"
    })))
}

/// Delete a user
pub async fn delete_user(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let token = extract_token(&headers).map_err(|e| {
        (
            e,
            Json(ErrorResponse {
                error: "Unauthorized".to_string(),
            }),
        )
    })?;

    let claims = validate_token(&token).map_err(|_| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Invalid token".to_string(),
            }),
        )
    })?;

    if claims.role != "admin" {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "Forbidden: admin role required".to_string(),
            }),
        ));
    }

    // Prevent deleting yourself
    if claims.sub == user_id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Cannot delete your own account".to_string(),
            }),
        ));
    }

    // Fetch user to get username for logging
    let user = sqlx::query_as::<_, User>(
        r#"
        SELECT id, username, password_hash, role
        FROM users
        WHERE id = ?
        "#,
    )
    .bind(&user_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|_| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "User not found".to_string(),
            }),
        )
    })?;

    sqlx::query(
        r#"
        DELETE FROM users
        WHERE id = ?
        "#,
    )
    .bind(&user_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to delete user: {}", e),
            }),
        )
    })?;

    info!("user deleted: {}", user.username);

    Ok(Json(serde_json::json!({
        "message": "User deleted successfully"
    })))
}
