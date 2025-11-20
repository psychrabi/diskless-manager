//! Authentication module for JWT-based authentication
use crate::types::User;
use crate::utils::append_log;
use bcrypt::{ hash, verify, DEFAULT_COST };
use chrono::{ Duration, Utc };
use jsonwebtoken::{ decode, encode, DecodingKey, EncodingKey, Header, Validation };
use std::collections::HashMap;

use crate::types::{ Claims, LoginRequest, LoginResponse, UserResponse, AuthError };

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
    static ref SECRET_ENCODING_KEY: EncodingKey = EncodingKey::from_secret(SECRET_KEY.as_ref());
    static ref SECRET_DECODING_KEY: DecodingKey = DecodingKey::from_secret(SECRET_KEY.as_ref());
}

const SECRET_KEY: &str = "diskless_manager_secret_key_2025"; // In production, use a more secure secret

pub fn authenticate_user(username: &str, password: &str) -> Result<LoginResponse, AuthError> {
    // Find user by username in the in-memory store (base data)
    let user = USERS.get(username).ok_or_else(|| AuthError {
        message: "Invalid username or password".to_string(),
    })?;

    // Determine which password hash to use:
    // - If username is "admin" and a hashed admin_password exists in config.json, use that.
    // - Otherwise use the password_hash from the USERS map.
    let mut password_hash = user.password_hash.clone();
    if username == "admin" {
        let cfg = crate::config::read_config();
        if let Some(obj) = cfg.settings.as_object() {
            if let Some(val) = obj.get("admin_password") {
                if let Some(s) = val.as_str() {
                    // use configured admin password hash
                    password_hash = s.to_string();
                }
            }
        }
    }

    // Verify password against the selected hash
    let valid = verify(password, &password_hash).map_err(|_| AuthError {
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

    let token = encode(&Header::default(), &claims, &SECRET_ENCODING_KEY).map_err(|_| AuthError {
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
    let decoded = decode::<Claims>(token, &SECRET_DECODING_KEY, &validation).map_err(|_| AuthError {
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

    // gate login behind activated license
    // ensure_license_valid()?;

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

// Tauri command for updating admin password
#[tauri::command]
pub fn update_admin_password(
    token: &str,
    old_password: &str,
    new_password: &str
) -> Result<String, AuthError> {
    // validate token and ensure caller is admin
    let claims = validate_token(token)?;
    if claims.role != "admin" {
        return Err(AuthError {
            message: "Forbidden: admin role required".to_string(),
        });
    }

    // determine current admin password hash (config override or default USERS)
    let mut current_hash = None;
    let cfg = crate::config::read_config();
    if let Some(obj) = cfg.settings.as_object() {
        if let Some(val) = obj.get("admin_password") {
            if let Some(s) = val.as_str() {
                current_hash = Some(s.to_string());
            }
        }
    }
    if current_hash.is_none() {
        if let Some(u) = USERS.get("admin") {
            current_hash = Some(u.password_hash.clone());
        }
    }
    let current_hash = current_hash.ok_or_else(|| AuthError {
        message: "No admin password available to verify against".to_string(),
    })?;

    // verify provided old_password
    let valid = verify(old_password, &current_hash).map_err(|_| AuthError {
        message: "Failed to verify current admin password".to_string(),
    })?;
    if !valid {
        return Err(AuthError {
            message: "Old password is incorrect".to_string(),
        });
    }

    // hash the new password server-side
    let hashed = hash(new_password, DEFAULT_COST).map_err(|e| AuthError {
        message: format!("Failed to hash password: {}", e),
    })?;

    // update config with the hashed password
    let mut cfg = crate::config::read_config();
    let mut settings = cfg.settings.as_object().cloned().unwrap_or_default();
    settings.insert(
        "admin_password".to_string(),
        serde_json::to_value(&hashed).map_err(|e| AuthError {
            message: format!("Failed to serialize password: {}", e),
        })?
    );
    cfg.settings = serde_json::Value::Object(settings);
    crate::config::write_config(&cfg).map_err(|e| AuthError {
        message: format!("Failed to save admin password: {}", e),
    })?;

    append_log("INFO", "admin password updated");
    Ok("Admin password updated successfully".to_string())
}
