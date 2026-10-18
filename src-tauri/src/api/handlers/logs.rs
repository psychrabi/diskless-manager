use axum::{extract::Query, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
pub struct LogsQuery {
    unit: Option<String>,
    lines: Option<u32>,
}

pub async fn get_logs(
    Query(params): Query<LogsQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let unit = params.unit;
    let lines = params.lines.unwrap_or(50);

    // journalctl/file reads are synchronous; keep them off the request worker.
    let logs = tokio::task::spawn_blocking(move || match unit.as_deref() {
        // Fetch logs for a specific systemd unit
        Some(unit) => {
            crate::infrastructure::command::read_service_logs(unit, lines).unwrap_or_default()
        }
        // Fetch app logs
        None => crate::infrastructure::command::read_logs(),
    })
    .await
    .unwrap_or_default();

    Ok(Json(json!({ "text": logs })))
}

pub async fn clear_logs() -> Result<Json<serde_json::Value>, StatusCode> {
    match crate::infrastructure::command::clear_logs() {
        Ok(_) => {
            log::info!("Logs cleared by user");
            Ok(Json(json!({ "message": "Logs cleared successfully" })))
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}
