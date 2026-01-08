use axum::Json;
use log::info;
use serde::{Deserialize, Serialize};

use crate::license::get_license_info;

#[derive(Debug, Serialize, Deserialize)]
pub struct LicenseInfoResponse {
    pub license_key: Option<String>,
    pub license_status: Option<String>,
    pub license_expires: Option<String>,
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
