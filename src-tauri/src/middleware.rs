//! Middleware for authentication and authorization
use crate::auth::{validate_token, AuthError, Claims};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthRequest {
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthResponse {
    pub claims: Claims,
}

/// Middleware function to validate authentication token
pub fn validate_auth(request: AuthRequest) -> Result<AuthResponse, AuthError> {
    let claims = validate_token(&request.token)?;
    Ok(AuthResponse { claims })
}

/// Check if user has required role
// pub fn check_role(claims: &Claims, required_role: &str) -> Result<(), AuthError> {
//     // Admin can access everything
//     if claims.role == "admin" {
//         return Ok(());
//     }
    
//     // Check if user has required role
//     if claims.role == required_role {
//         return Ok(());
//     }
    
//     Err(AuthError {
//         message: "Insufficient permissions".to_string(),
//     })
// }

/// Middleware for Tauri commands that require authentication
#[tauri::command]
pub fn authenticate(request: AuthRequest) -> Result<AuthResponse, AuthError> {
    validate_auth(request)
}

/// Example of how to protect a command with authentication
/// This would be used in command implementations
// pub fn protected_command_example(token: &str) -> Result<String, AuthError> {
//     // Validate token
//     let auth_request = AuthRequest {
//         token: token.to_string(),
//     };
//     let auth_response = validate_auth(auth_request)?;
    
//     // Check if user has required role (example: admin)
//     check_role(&auth_response.claims, "admin")?;
    
//     // Perform the actual command logic here
//     Ok("Command executed successfully".to_string())
// }

/// Function to validate authentication token for Tauri commands
pub fn validate_auth_token_for_command(token: &str) -> Result<Claims, AuthError> {
    validate_token(token)
}

/// Function to check if user has required role for command
// pub fn check_user_role(claims: &Claims, required_role: &str) -> Result<(), AuthError> {
//     // Admin can access everything
//     if claims.role == "admin" {
//         return Ok(());
//     }
    
//     // Check if user has required role
//     if claims.role == required_role {
//         return Ok(());
//     }
    
//     Err(AuthError {
//         message: "Insufficient permissions".to_string(),
//     })
// }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{authenticate_user, LoginRequest};

    #[test]
    fn test_validate_auth_valid_token() {
        // First, login to get a valid token
        let login_request = LoginRequest {
            username: "admin".to_string(),
            password: "admin123".to_string(),
        };
        let login_response = authenticate_user(&login_request.username, &login_request.password).unwrap();
        
        // Then validate the token
        let auth_request = AuthRequest {
            token: login_response.token,
        };
        let result = validate_auth(auth_request);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_auth_invalid_token() {
        let auth_request = AuthRequest {
            token: "invalid_token".to_string(),
        };
        let result = validate_auth(auth_request);
        assert!(result.is_err());
    }

    #[test]
    fn test_check_role_admin() {
        let claims = Claims {
            sub: "1".to_string(),
            username: "admin".to_string(),
            role: "admin".to_string(),
            exp: 10000000000, // Far in the future
        };
        let result = check_role(&claims, "user");
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_role_insufficient_permissions() {
        let claims = Claims {
            sub: "2".to_string(),
            username: "user".to_string(),
            role: "user".to_string(),
            exp: 10000000000, // Far in the future
        };
        let result = check_role(&claims, "admin");
        assert!(result.is_err());
    }
}