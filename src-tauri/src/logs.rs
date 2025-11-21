use crate::middleware;
use crate::utils;
use crate::types::AuthError;

/// Return entire log content as string
#[tauri::command]
pub fn get_logs(token: String) -> Result<serde_json::Value, String> {
    // validate token (reuse middleware)
    middleware::validate_auth_token_for_command(&token)
        .map_err(|e: AuthError| format!("Authentication failed: {}", e.message))?;
    let text = utils::read_logs();
    Ok(serde_json::json!({ "text": text }))
}

/// Clear log file
#[tauri::command]
pub fn clear_logs(token: String) -> Result<serde_json::Value, String> {
    middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;
    utils::append_log("INFO", "Log cleared by user");
    utils::clear_logs().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "message": "Logs cleared" }))
}