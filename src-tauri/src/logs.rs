use crate::cmd;
use crate::error::AppError;
use crate::middleware::validate_auth;
use log::info;

/// Return entire log content as string
#[expect(dead_code, reason = "Old Tauri command - log viewing handled by Axum")]
pub fn get_logs(token: String) -> Result<serde_json::Value, AppError> {
    validate_auth(&token)?;
    let text = cmd::read_logs();
    Ok(serde_json::json!({ "text": text }))
}

/// Clear log file
#[expect(dead_code, reason = "Old Tauri command - log clearing handled by Axum")]
pub fn clear_logs(token: String) -> Result<serde_json::Value, AppError> {
    validate_auth(&token)?;
    info!("Log cleared by user");
    cmd::clear_logs().map_err(|e| AppError::Command(e.to_string()))?;
    Ok(serde_json::json!({ "message": "Logs cleared" }))
}
