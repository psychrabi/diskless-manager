//! Authentication module for JWT-based authentication
use bcrypt::{hash, verify, DEFAULT_COST};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::utils::append_log;

#[derive(Debug, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub role: String, // admin, user
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,      // subject (user id)
    pub username: String, // username
    pub role: String,     // user role
    pub exp: i64,         // expiration time
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserResponse,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserResponse {
    pub id: String,
    pub username: String,
    pub role: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthError {
    pub message: String,
}

// In a real application, this would be stored in a database
lazy_static::lazy_static! {
    static ref USERS: HashMap<String, User> = {
        let mut m = HashMap::new();
        // Default admin user (password: admin123)
        let password_hash = hash("admin123", DEFAULT_COST).unwrap();
        m.insert(
            "admin".to_string(),
            User {
                id: "1".to_string(),
                username: "admin".to_string(),
                password_hash,
                role: "admin".to_string(),
            },
        );
        m
    };
}

const SECRET_KEY: &str = "diskless_manager_secret_key_2025"; // In production, use a more secure secret

pub fn authenticate_user(username: &str, password: &str) -> Result<LoginResponse, AuthError> {
    // Find user by username
    let user = USERS
        .get(username)
        .ok_or_else(|| AuthError {
            message: "Invalid username or password".to_string(),
        })?;

    // Verify password
    let valid = verify(password, &user.password_hash).map_err(|_| AuthError {
        message: "Invalid username or password".to_string(),
    })?;

    if !valid {
        return Err(AuthError {
            message: "Invalid username or password".to_string(),
        });
    }

    // Generate JWT token
    let expiration = Utc::now()
        .checked_add_signed(Duration::hours(24))
        .expect("Failed to calculate expiration time")
        .timestamp();

    let claims = Claims {
        sub: user.id.clone(),
        username: user.username.clone(),
        role: user.role.clone(),
        exp: expiration,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(SECRET_KEY.as_ref()),
    )
    .map_err(|_| AuthError {
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
    let decoded = decode::<Claims>(
        token,
        &DecodingKey::from_secret(SECRET_KEY.as_ref()),
        &validation,
    )
    .map_err(|_| AuthError {
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
pub fn login(request: LoginRequest) -> Result<LoginResponse, AuthError> {
    let username = request.username.clone();
    let password = request.password.clone();

    append_log("INFO", &format!("login attempt: user={}", username));

    let auth_result = authenticate_user(&username, &password);

    match &auth_result {
        Ok(response) => {
            append_log("INFO", &format!("login success: user={}", username));
            // response is a &LoginResponse — return an owned value
            Ok(response.clone())
        }
        Err(_) => {
            append_log("WARN", &format!("login failed: user={}", username));
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