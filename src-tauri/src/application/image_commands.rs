//! Image management commands - Tauri command handlers
//!
//! This module provides the Tauri command handlers for image operations.

use crate::types::image::{CreateImageRequest, CreateSnapshotRequest};

#[tauri::command]
pub async fn create_image(token: String, name: String, size: String) -> Result<serde_json::Value, String> {
    // Implementation would use ImageCommands
    Ok(serde_json::json!({
        "message": format!("Master ZVOL '{}' created successfully.", name)
    }))
}

#[tauri::command]
pub async fn get_images(token: String) -> Result<serde_json::Value, String> {
    // Implementation would return images array
    Ok(serde_json::json!([]))
}

#[tauri::command]
pub async fn delete_image(token: String, image_name: String) -> Result<serde_json::Value, String> {
    // Implementation would delete image
    Ok(serde_json::json!({
        "message": format!("Image {} deleted successfully", image_name)
    }))
}