use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Serialize;

use crate::{
    core::reconciliation::{inspect_storage, repair_client_storage, ReconciliationEntry},
    state::AppState,
};

#[derive(Debug, Serialize)]
pub struct ReconciliationApiError {
    pub error: String,
}

impl IntoResponse for ReconciliationApiError {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(self)).into_response()
    }
}

/// Inspect all enabled clients without changing infrastructure.
///
/// GET /api/system/reconciliation/storage
pub async fn inspect_storage_reconciliation(
    State(state): State<AppState>,
) -> Result<Json<crate::core::reconciliation::ReconciliationSummary>, ReconciliationApiError> {
    inspect_storage(&state).await.map(Json).map_err(|error| {
        tracing::error!(error = %error, "storage reconciliation inspection failed");

        ReconciliationApiError {
            error: error.to_string(),
        }
    })
}

/// Repair one client using the persisted desired storage configuration.
///
/// POST /api/system/reconciliation/storage/{id}
pub async fn repair_storage_reconciliation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ReconciliationEntry>, ReconciliationApiError> {
    repair_client_storage(&state, &id)
        .await
        .map(Json)
        .map_err(|error| {
            tracing::error!(
                client_id = %id,
                error = %error,
                "storage reconciliation repair failed"
            );

            ReconciliationApiError {
                error: error.to_string(),
            }
        })
}
