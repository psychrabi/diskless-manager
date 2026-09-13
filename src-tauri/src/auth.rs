//! Authentication module for JWT-based authentication
use crate::types::User;
use bcrypt::{hash, verify, DEFAULT_COST};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use log::{info, warn};
use sqlx::SqlitePool;
use std::{env, fs::OpenOptions, io::Write, path::Path, sync::OnceLock};
use uuid::Uuid;

use crate::types::{AuthError, Claims, LoginResponse, UserResponse};

#[derive(Debug, thiserror::Error)]
pub enum BootstrapAdminError {
    #[error("username or password does not meet the security requirements")]
    InvalidCredentials,
    #[error("the application has already been initialized")]
    AlreadyInitialized,
    #[error("failed to secure the administrator password")]
    PasswordHash,
    #[error("failed to create the administrator account")]
    Database(#[source] sqlx::Error),
}

pub async fn bootstrap_admin(
    pool: &SqlitePool,
    username: &str,
    password: &str,
) -> Result<(), BootstrapAdminError> {
    let valid_username = (3..=50).contains(&username.len())
        && username
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'));
    let valid_password = password.len() >= 8
        && password
            .chars()
            .any(|character| character.is_ascii_lowercase())
        && password
            .chars()
            .any(|character| character.is_ascii_uppercase())
        && password.chars().any(|character| character.is_ascii_digit());

    if !valid_username || !valid_password {
        return Err(BootstrapAdminError::InvalidCredentials);
    }

    let password_hash =
        hash(password, DEFAULT_COST).map_err(|_| BootstrapAdminError::PasswordHash)?;
    let now = Utc::now().to_rfc3339();
    let result = sqlx::query(
        r#"
        INSERT INTO users (id, username, password_hash, role, created_at, updated_at)
        SELECT ?, ?, ?, 'admin', ?, ?
        WHERE NOT EXISTS (SELECT 1 FROM users)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(username)
    .bind(password_hash)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(BootstrapAdminError::Database)?;

    if result.rows_affected() == 0 {
        return Err(BootstrapAdminError::AlreadyInitialized);
    }

    Ok(())
}

// Load JWT secret from environment variable
// SECURITY: Never commit the actual secret to version control
// Set JWT_SECRET environment variable before running the application
static SECRET_KEY: OnceLock<Vec<u8>> = OnceLock::new();

pub fn initialize_jwt_secret(config_dir: &Path) -> anyhow::Result<()> {
    let configured = env::var("JWT_SECRET").ok();
    let secret = load_or_create_jwt_secret(&config_dir.join("jwt-secret"), configured.as_deref())?;
    if let Some(existing) = SECRET_KEY.get() {
        anyhow::ensure!(
            existing == &secret,
            "JWT signing material changed after initialization"
        );
        return Ok(());
    }
    SECRET_KEY.set(secret).map_err(|_| {
        anyhow::anyhow!("JWT signing material changed concurrently during initialization")
    })
}

fn load_or_create_jwt_secret(path: &Path, configured: Option<&str>) -> anyhow::Result<Vec<u8>> {
    if let Some(secret) = configured {
        if secret.len() < 32 {
            anyhow::bail!("JWT_SECRET must contain at least 32 bytes");
        }
        return Ok(secret.as_bytes().to_vec());
    }

    if let Ok(secret) = std::fs::read(path) {
        if secret.len() < 32 {
            anyhow::bail!("persisted JWT signing material is invalid");
        }
        return Ok(secret);
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let secret = format!("{}{}", Uuid::new_v4(), Uuid::new_v4()).into_bytes();
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    match options.open(path) {
        Ok(mut file) => {
            file.write_all(&secret)?;
            file.sync_all()?;
            info!("Created persistent JWT signing material");
            Ok(secret)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = std::fs::read(path)?;
            if existing.len() < 32 {
                anyhow::bail!("persisted JWT signing material is invalid");
            }
            Ok(existing)
        }
        Err(error) => Err(error.into()),
    }
}

pub fn jwt_secret() -> &'static [u8] {
    SECRET_KEY
        .get_or_init(|| {
            warn!("JWT signing material used before application initialization");
            format!("{}{}", Uuid::new_v4(), Uuid::new_v4()).into_bytes()
        })
        .as_slice()
}

/// Fetch user from database by username
async fn get_user_by_username(pool: &SqlitePool, username: &str) -> Result<User, AuthError> {
    sqlx::query_as::<_, User>(
        r#"
        SELECT id, username, password_hash, role, session_version
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
        session_version: user.session_version,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret()),
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

pub async fn validate_token(pool: &SqlitePool, token: &str) -> Result<Claims, AuthError> {
    let validation = Validation::default();
    let decoded = decode::<Claims>(token, &DecodingKey::from_secret(jwt_secret()), &validation)
        .map_err(|_| AuthError {
            message: "Invalid token".to_string(),
        })?;

    validate_session(pool, &decoded.claims).await
}

/// Recheck account state for both HTTP requests and established WebSockets.
pub async fn validate_session(pool: &SqlitePool, claims: &Claims) -> Result<Claims, AuthError> {
    if claims.exp <= Utc::now().timestamp() {
        return Err(AuthError {
            message: "Token expired".to_string(),
        });
    }

    let account: Option<(String, String, i64)> =
        sqlx::query_as("SELECT username, role, session_version FROM users WHERE id = ?")
            .bind(&claims.sub)
            .fetch_optional(pool)
            .await
            .map_err(|_| AuthError {
                message: "Session validation failed".to_string(),
            })?;
    let (username, role, version) = account.ok_or_else(|| AuthError {
        message: "Session revoked".to_string(),
    })?;
    if version != claims.session_version
        || role != claims.role
        || !matches!(role.as_str(), "admin" | "user")
    {
        return Err(AuthError {
            message: "Session revoked".to_string(),
        });
    }
    Ok(Claims {
        username,
        role,
        ..claims.clone()
    })
}

#[cfg(test)]
mod bootstrap_tests {
    use super::{
        authenticate_user, bootstrap_admin, load_or_create_jwt_secret, BootstrapAdminError,
    };
    use sqlx::sqlite::SqlitePoolOptions;
    use std::path::PathBuf;

    async fn users_pool() -> sqlx::SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory database should open");

        sqlx::query(
            r#"
            CREATE TABLE users (
                id TEXT PRIMARY KEY,
                username TEXT NOT NULL UNIQUE,
                password_hash TEXT NOT NULL,
                role TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                last_login TEXT,
                session_version INTEGER NOT NULL DEFAULT 0
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("users table should be created");

        pool
    }

    #[tokio::test]
    async fn bootstrap_creates_the_requested_first_admin() {
        let pool = users_pool().await;

        bootstrap_admin(&pool, "operator", "StrongPass1")
            .await
            .expect("first administrator should be created");

        let login = authenticate_user(&pool, "operator", "StrongPass1")
            .await
            .expect("new administrator should be able to log in");
        assert_eq!(login.user.username, "operator");
        assert_eq!(login.user.role, "admin");
    }

    #[tokio::test]
    async fn bootstrap_refuses_to_replace_an_existing_installation() {
        let pool = users_pool().await;
        bootstrap_admin(&pool, "first-admin", "StrongPass1")
            .await
            .expect("first administrator should be created");

        let result = bootstrap_admin(&pool, "replacement", "OtherPass2").await;

        assert!(matches!(
            result,
            Err(BootstrapAdminError::AlreadyInitialized)
        ));
        let usernames: Vec<String> =
            sqlx::query_scalar("SELECT username FROM users ORDER BY username")
                .fetch_all(&pool)
                .await
                .expect("users should remain readable");
        assert_eq!(usernames, vec!["first-admin"]);
    }

    #[tokio::test]
    async fn bootstrap_rejects_weak_credentials_without_creating_a_user() {
        let pool = users_pool().await;

        let result = bootstrap_admin(&pool, "ad", "password").await;

        assert!(matches!(
            result,
            Err(BootstrapAdminError::InvalidCredentials)
        ));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&pool)
            .await
            .expect("user count should be readable");
        assert_eq!(count, 0);
    }

    #[test]
    fn generated_jwt_secret_survives_restart() {
        let directory =
            std::env::temp_dir().join(format!("diskless-auth-{}", uuid::Uuid::new_v4()));
        let path = directory.join("jwt-secret");

        let first = load_or_create_jwt_secret(&path, None).expect("secret should be created");
        let second = load_or_create_jwt_secret(&path, None).expect("secret should be reloaded");

        assert_eq!(first, second);
        assert!(first.len() >= 32);
        std::fs::remove_dir_all(directory).expect("test directory should be removable");
    }

    #[test]
    fn configured_jwt_secret_does_not_write_a_local_copy() {
        let path = PathBuf::from("/path/that/must/not/be/written");

        let configured = "configured-production-secret-over-32-bytes";
        let secret = load_or_create_jwt_secret(&path, Some(configured))
            .expect("configured secret should be accepted");

        assert_eq!(secret, configured.as_bytes());
        assert!(!path.exists());
    }
}
