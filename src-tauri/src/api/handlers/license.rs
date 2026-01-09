use axum::Json;
use log::info;
use serde::{Deserialize, Serialize};
use axum::extract::State;

use crate::license::get_license_info;
use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct LicenseInfoResponse {
    pub license_key: Option<String>,
    pub license_status: Option<String>,
    pub license_expires: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ActivateLicenseRequest {
    pub key: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ActivateLicenseResponse {
    pub message: String,
}

pub async fn get_license_info_handler() -> Result<Json<LicenseInfoResponse>, axum::http::StatusCode> {
    match get_license_info() {
        Ok(info) => {
            info!("license info retrieved");
            Ok(Json(LicenseInfoResponse {
                license_key: info.license_key,
                license_status: info.license_status,
                license_expires: info.license_expires,
            }))
        }
        Err(e) => {
            info!("failed to get license info: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn activate_license_handler(
    State(state): State<AppState>,
    Json(payload): Json<ActivateLicenseRequest>,
) -> Result<Json<ActivateLicenseResponse>, (axum::http::StatusCode, String)> {
    match crate::license::activate_license_http(state, &payload.key).await {
        Ok(message) => {
            info!("license activated successfully");
            Ok(Json(ActivateLicenseResponse { message }))
        }
        Err(e) => {
            info!("failed to activate license: {}", e);
            Err((
                axum::http::StatusCode::BAD_REQUEST,
                e,
            ))
        }
    }
}
