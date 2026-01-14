//! Authentication types
//!
//! This module contains all authentication-related types and structures.

use serde::{Deserialize, Serialize};

/// User structure
#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct User {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub role: String, // admin, user
}

/// JWT claims structure
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,      // subject (user id)
    pub username: String, // username
    pub role: String,     // user role
    pub exp: i64,         // expiration time
    pub iat: usize,       // issued at time
}

/// Login request structure
#[derive(Debug, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Login response structure
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserResponse,
}

/// User response structure (without sensitive data)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserResponse {
    pub id: String,
    pub username: String,
    pub role: String,
}

/// Authentication error structure
#[derive(Debug, Serialize, Deserialize)]
pub struct AuthError {
    pub message: String,
}

impl From<String> for AuthError {
    fn from(s: String) -> Self {
        AuthError { message: s }
    }
}

impl From<&str> for AuthError {
    fn from(s: &str) -> Self {
        AuthError {
            message: s.to_string(),
        }
    }
}
