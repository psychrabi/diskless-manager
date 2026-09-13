use axum::{extract::State, http::StatusCode, Json};

use crate::state::AppState;
use crate::types::AppConfig;

pub async fn get_config(State(_state): State<AppState>) -> Result<Json<AppConfig>, StatusCode> {
    let mut cfg = crate::config::get_config();
    if let Some(settings) = cfg.settings.as_object_mut() {
        settings.remove("license_key");
    }
    Ok(Json(cfg))
}
