use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use serde_json::json;

use crate::{
    application::alarm_service::AlarmService, core::alarm::AlarmSummary, state::AppState,
    types::Claims,
};

type AlarmApiError = (StatusCode, Json<serde_json::Value>);

fn alarm_error(status: StatusCode, message: impl Into<String>) -> AlarmApiError {
    (status, Json(json!({ "error": message.into() })))
}

/// List active operational alarms and their acknowledgement state.
pub async fn list_alarms(
    State(state): State<AppState>,
) -> Result<Json<AlarmSummary>, AlarmApiError> {
    let settings = state.settings.read().await.clone();
    AlarmService::new(state.db_pool.clone())
        .list(settings)
        .await
        .map(Json)
        .map_err(|error| {
            tracing::error!(error = %error, "failed to derive alarms");
            alarm_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load operational alarms",
            )
        })
}

/// Acknowledge one currently active alarm.
///
/// The alarm id must still be present when the request is handled so a stale
/// UI cannot acknowledge a cleared occurrence.
pub async fn acknowledge_alarm(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<StatusCode, AlarmApiError> {
    let settings = state.settings.read().await.clone();
    let service = AlarmService::new(state.db_pool.clone());
    let summary = service.list(settings).await.map_err(|error| {
        tracing::error!(error = %error, "failed to derive alarms before acknowledgement");
        alarm_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to load operational alarms",
        )
    })?;

    if !summary.alarms.iter().any(|alarm| alarm.id == id) {
        return Err(alarm_error(
            StatusCode::NOT_FOUND,
            "Alarm is no longer active",
        ));
    }

    service
        .acknowledge(&id, &claims.username)
        .await
        .map_err(|error| {
            tracing::error!(alarm_id = %id, error = %error, "failed to acknowledge alarm");
            alarm_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to acknowledge alarm",
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}
