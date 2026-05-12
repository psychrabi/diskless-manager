#![allow(dead_code)]

//! User management commands
use crate::types::{AuthError, User, UserResponse};
use bcrypt::{hash, DEFAULT_COST};
use chrono::Utc;
use log::info;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub role: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateUserRequest {
    pub id: String,
    pub username: Option<String>,
    pub role: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateUserPasswordRequest {
    pub id: String,
    pub password: String,
}

/// List all users (without password hashes)
pub async fn list_users(
    state: tauri::State<'_, crate::state::AppState>,
    token: &str,
) -> Result<Vec<UserResponse>, AuthError> {
    // Validate token and ensure caller is admin
    let claims = crate::auth::validate_token(token)?;
    if claims.role != "admin" {
        return Err(AuthError {
            message: "Forbidden: admin role required".to_string(),
        });
    }

    let users = sqlx::query_as::<_, User>(
        r#"
        SELECT id, username, password_hash, role
        FROM users
        ORDER BY username
        "#,
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AuthError {
        message: format!("Failed to fetch users: {}", e),
    })?;

    Ok(users
        .into_iter()
        .map(|u| UserResponse {
            id: u.id,
            username: u.username,
            role: u.role,
        })
        .collect())
}

/// Create a new user
pub async fn create_user(
    state: tauri::State<'_, crate::state::AppState>,
    token: &str,
    request: CreateUserRequest,
) -> Result<UserResponse, AuthError> {
    // Validate token and ensure caller is admin
    let claims = crate::auth::validate_token(token)?;
    if claims.role != "admin" {
        return Err(AuthError {
            message: "Forbidden: admin role required".to_string(),
        });
    }

    // Validate role
    if request.role != "admin" && request.role != "user" {
        return Err(AuthError {
            message: "Invalid role. Must be 'admin' or 'user'".to_string(),
        });
    }

    // Hash password
    let password_hash = hash(&request.password, DEFAULT_COST).map_err(|e| AuthError {
        message: format!("Failed to hash password: {}", e),
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
    .map_err(|e| AuthError {
        message: format!("Failed to create user: {}", e),
    })?;

    info!(
        "user created: {} (role: {})",
        request.username, request.role
    );

    Ok(UserResponse {
        id,
        username: request.username,
        role: request.role,
    })
}

/// Update user details (username and/or role)
pub async fn update_user(
    state: tauri::State<'_, crate::state::AppState>,
    token: &str,
    request: UpdateUserRequest,
) -> Result<UserResponse, AuthError> {
    // Validate token and ensure caller is admin
    let claims = crate::auth::validate_token(token)?;
    if claims.role != "admin" {
        return Err(AuthError {
            message: "Forbidden: admin role required".to_string(),
        });
    }

    // Fetch current user
    let user = sqlx::query_as::<_, User>(
        r#"
        SELECT id, username, password_hash, role
        FROM users
        WHERE id = ?
        "#,
    )
    .bind(&request.id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|_| AuthError {
        message: "User not found".to_string(),
    })?;

    let username = request.username.unwrap_or(user.username);
    let role = request.role.unwrap_or(user.role);

    // Validate role
    if role != "admin" && role != "user" {
        return Err(AuthError {
            message: "Invalid role. Must be 'admin' or 'user'".to_string(),
        });
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
    .bind(&request.id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AuthError {
        message: format!("Failed to update user: {}", e),
    })?;

    info!("user updated: {} (role: {})", username, role);

    Ok(UserResponse {
        id: request.id,
        username,
        role,
    })
}

/// Update user password (admin can change any user's password)
pub async fn update_user_password(
    state: tauri::State<'_, crate::state::AppState>,
    token: &str,
    request: UpdateUserPasswordRequest,
) -> Result<String, AuthError> {
    // Validate token and ensure caller is admin
    let claims = crate::auth::validate_token(token)?;
    if claims.role != "admin" {
        return Err(AuthError {
            message: "Forbidden: admin role required".to_string(),
        });
    }

    // Hash new password
    let password_hash = hash(&request.password, DEFAULT_COST).map_err(|e| AuthError {
        message: format!("Failed to hash password: {}", e),
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
    .bind(&request.id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AuthError {
        message: format!("Failed to update password: {}", e),
    })?;

    if result.rows_affected() == 0 {
        return Err(AuthError {
            message: "User not found".to_string(),
        });
    }

    info!("password updated for user id: {}", request.id);

    Ok("Password updated successfully".to_string())
}

/// Delete a user
pub async fn delete_user(
    state: tauri::State<'_, crate::state::AppState>,
    token: &str,
    user_id: &str,
) -> Result<String, AuthError> {
    // Validate token and ensure caller is admin
    let claims = crate::auth::validate_token(token)?;
    if claims.role != "admin" {
        return Err(AuthError {
            message: "Forbidden: admin role required".to_string(),
        });
    }

    // Prevent deleting yourself
    if claims.sub == user_id {
        return Err(AuthError {
            message: "Cannot delete your own account".to_string(),
        });
    }

    // Fetch user to get username for logging
    let user = sqlx::query_as::<_, User>(
        r#"
        SELECT id, username, password_hash, role
        FROM users
        WHERE id = ?
        "#,
    )
    .bind(user_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|_| AuthError {
        message: "User not found".to_string(),
    })?;

    sqlx::query(
        r#"
        DELETE FROM users
        WHERE id = ?
        "#,
    )
    .bind(user_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AuthError {
        message: format!("Failed to delete user: {}", e),
    })?;

    info!("user deleted: {}", user.username);

    Ok("User deleted successfully".to_string())
}
