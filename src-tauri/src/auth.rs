//! Authentication module for JWT-based authentication
use crate::types::User;
use bcrypt::{hash, verify, DEFAULT_COST};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use log::{info, warn};
use sqlx::SqlitePool;
use std::env;

use crate::types::{AuthError, Claims, LoginRequest, LoginResponse, UserResponse};

// Load JWT secret from environment variable
// SECURITY: Never commit the actual secret to version control
// Set JWT_SECRET environment variable before running the application
lazy_static::lazy_static! {
    static ref SECRET_KEY: Vec<u8> = {
        env::var("JWT_SECRET")
            .unwrap_or_else(|_| {
                eprintln!("WARNING: JWT_SECRET environment variable not set!");
                eprintln!("Using fallback secret for development only.");
                eprintln!("For production, set JWT_SECRET environment variable with a secure random string.");
                // Fallback for development only - generates a warning
                "d939af3c6a5b136c954e48de599dd57dd987032a5e9e32ae6caa9369087cfecb".to_string()
            })
            .into_bytes()
    };
    static ref SECRET_ENCODING_KEY: EncodingKey = EncodingKey::from_secret(&SECRET_KEY);
    static ref SECRET_DECODING_KEY: DecodingKey = DecodingKey::from_secret(&SECRET_KEY);
}

/// Fetch user from database by username
async fn get_user_by_username(pool: &SqlitePool, username: &str) -> Result<User, AuthError> {
    sqlx::query_as::<_, User>(
        r#"
        SELECT id, username, password_hash, role
        FROM users
        WHERE username = ?
        "#,
    )
    .bind(username)
    .fetch_one(pool)
    .await
    .map_err(|_| AuthError {
        message: "Invalid username or password".to_string(),
    })
}

/// Update last login timestamp for user
async fn update_last_login(pool: &SqlitePool, user_id: &str) -> Result<(), AuthError> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE users
        SET last_login = ?
        WHERE id = ?
        "#,
    )
    .bind(&now)
    .bind(user_id)
    .execute(pool)
    .await
    .map_err(|e| AuthError {
        message: format!("Failed to update last login: {}", e),
    })?;
    Ok(())
}

pub async fn authenticate_user(
    pool: &SqlitePool,
    username: &str,
    password: &str,
) -> Result<LoginResponse, AuthError> {
    // Fetch user from database
    let user = get_user_by_username(pool, username).await?;

    // Verify password against the hash from database
    let valid = verify(password, &user.password_hash).map_err(|_| AuthError {
        message: "Invalid username or password".to_string(),
    })?;

    if !valid {
        return Err(AuthError {
            message: "Invalid username or password".to_string(),
        });
    }

    // Update last login timestamp
    let _ = update_last_login(pool, &user.id).await;

    // Generate JWT token
    let expiration = Utc::now()
        .checked_add_signed(Duration::hours(24))
        .ok_or_else(|| AuthError {
            message: "Failed to calculate expiration time".to_string(),
        })?
        .timestamp();

    let claims = Claims {
        sub: user.id.clone(),
        username: user.username.clone(),
        role: user.role.clone(),
        exp: expiration,
        iat: Utc::now().timestamp() as usize,
    };

    let token =
        encode(&Header::default(), &claims, &SECRET_ENCODING_KEY).map_err(|_| AuthError {
            message: "Failed to generate token".to_string(),
        })?;

    Ok(LoginResponse {
        token,
        user: UserResponse {
            id: user.id.clone(),
            username: user.username.clone(),
            role: user.role.clone(),
        },
    })
}

pub fn validate_token(token: &str) -> Result<Claims, AuthError> {
    let validation = Validation::default();
    let decoded =
        decode::<Claims>(token, &SECRET_DECODING_KEY, &validation).map_err(|_| AuthError {
            message: "Invalid token".to_string(),
        })?;

    // Check if token is expired
    let now = Utc::now().timestamp();
    if decoded.claims.exp < now {
        return Err(AuthError {
            message: "Token expired".to_string(),
        });
    }

    Ok(decoded.claims)
}

// Tauri command for login
#[tauri::command]
pub async fn login(
    state: tauri::State<'_, crate::state::AppState>,
    request: LoginRequest,
) -> Result<LoginResponse, AuthError> {
    let username = request.username.clone();
    let password = request.password.clone();

    info!("login attempt: user={}", username);

    // gate login behind activated license
    // ensure_license_valid()?;

    let auth_result = authenticate_user(&state.db_pool, &username, &password).await;

    match &auth_result {
        Ok(response) => {
            info!("login success: user={}", username);
            Ok(response.clone())
        }
        Err(_) => {
            warn!("login failed: user={}", username);
            Err(AuthError {
                message: "Invalid username or password".to_string(),
            })
        }
    }
}

// Tauri command for token validation
#[tauri::command]
pub fn validate_auth_token(token: &str) -> Result<Claims, AuthError> {
    validate_token(token)
}

// Tauri command for updating admin password
#[tauri::command]
pub async fn update_admin_password(
    state: tauri::State<'_, crate::state::AppState>,
    token: &str,
    old_password: &str,
    new_password: &str,
) -> Result<String, AuthError> {
    // validate token and ensure caller is admin
    let claims = validate_token(token)?;
    if claims.role != "admin" {
        return Err(AuthError {
            message: "Forbidden: admin role required".to_string(),
        });
    }

    // Fetch current user from database
    let user = get_user_by_username(&state.db_pool, &claims.username).await?;

    // verify provided old_password
    let valid = verify(old_password, &user.password_hash).map_err(|_| AuthError {
        message: "Failed to verify current password".to_string(),
    })?;
    if !valid {
        return Err(AuthError {
            message: "Old password is incorrect".to_string(),
        });
    }

    // hash the new password
    let hashed = hash(new_password, DEFAULT_COST).map_err(|e| AuthError {
        message: format!("Failed to hash password: {}", e),
    })?;

    // update password in database
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
    .bind(&user.id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AuthError {
        message: format!("Failed to update password: {}", e),
    })?;

    info!("password updated for user: {}", claims.username);
    Ok("Password updated successfully".to_string())
}
