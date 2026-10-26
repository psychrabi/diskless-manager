use axum::{extract::State, http::StatusCode, Json};

use crate::state::AppState;
use crate::types::AppConfig;

pub async fn get_config(State(state): State<AppState>) -> Result<Json<AppConfig>, StatusCode> {
    let mut cfg = crate::config::read_config_db(&state.db_pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Some(settings) = cfg.settings.as_object_mut() {
        settings.remove("license_key");
    }
    Ok(Json(cfg))
}
