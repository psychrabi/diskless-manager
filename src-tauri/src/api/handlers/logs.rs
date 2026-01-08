use axum::{
    extract::Query,
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
pub struct LogsQuery {
    unit: Option<String>,
    lines: Option<u32>,
}

pub async fn get_logs(Query(params): Query<LogsQuery>) -> Result<Json<serde_json::Value>, StatusCode> {
    let unit = params.unit.as_deref();
    let lines = params.lines.unwrap_or(50);
    
    let logs = if let Some(unit) = unit {
        // Fetch logs for a specific systemd unit
        match crate::cmd::read_service_logs(unit, lines) {
            Ok(logs) => logs,
            Err(_) => String::new(),
        }
    } else {
        // Fetch app logs
        crate::cmd::read_logs()
    };
    
    Ok(Json(json!({ "text": logs })))
}

pub async fn clear_logs() -> Result<Json<serde_json::Value>, StatusCode> {
    match crate::cmd::clear_logs() {
        Ok(_) => {
            log::info!("Logs cleared by user");
            Ok(Json(json!({ "message": "Logs cleared successfully" })))
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}
