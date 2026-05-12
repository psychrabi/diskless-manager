use axum::{extract::State, http::StatusCode, Json};

use crate::state::AppState;
use crate::types::AppConfig;

pub async fn get_config(State(_state): State<AppState>) -> Result<Json<AppConfig>, StatusCode> {
    let cfg = crate::config::get_config();
    Ok(Json(cfg))
}
