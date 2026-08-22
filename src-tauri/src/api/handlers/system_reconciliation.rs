use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Serialize;

use crate::{
    core::system_reconciliation::{inspect_system_reconciliation, SystemReconciliationSummary},
    state::AppState,
};

#[derive(Debug, Serialize)]
pub struct SystemReconciliationApiError {
    pub error: String,
}

impl IntoResponse for SystemReconciliationApiError {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(self)).into_response()
    }
}

/// Inspect storage and DHCP state without changing infrastructure.
///
/// GET /api/system/reconciliation
pub async fn inspect_system_reconciliation_handler(
    State(state): State<AppState>,
) -> Result<Json<SystemReconciliationSummary>, SystemReconciliationApiError> {
    inspect_system_reconciliation(&state)
        .await
        .map(Json)
        .map_err(|error| {
            tracing::error!(error = %error, "system reconciliation inspection failed");

            SystemReconciliationApiError {
                error: error.to_string(),
            }
        })
}
