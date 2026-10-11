use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Serialize;
use sqlx::FromRow;

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
    let _client_guard = state.client_mutations.lock().await;
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM client_offline_resets WHERE client_id = ? AND operation IS NOT NULL",
    )
    .bind(&id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|error| ReconciliationApiError {
        error: error.to_string(),
    })?;
    if pending != 0 {
        return Err(ReconciliationApiError {
            error: "Automatic storage recovery is pending".into(),
        });
    }
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

#[derive(Debug, Serialize, FromRow)]
pub struct PendingClientRecovery {
    pub client_id: String,
    pub client_name: String,
    pub failures: i64,
    pub retry_after: i64,
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PendingClientRecoveriesResponse {
    pub recoveries: Vec<PendingClientRecovery>,
}

/// List interrupted per-client storage recovery journals without exposing their contents.
pub async fn list_pending_client_recoveries(
    State(state): State<AppState>,
) -> Result<Json<PendingClientRecoveriesResponse>, ReconciliationApiError> {
    let recoveries = sqlx::query_as::<_, PendingClientRecovery>(
        "SELECT c.id AS client_id, c.name AS client_name, r.failures, r.retry_after, r.last_error FROM client_offline_resets r JOIN clients c ON c.id = r.client_id WHERE r.operation IS NOT NULL ORDER BY r.retry_after, c.name COLLATE NOCASE",
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|error| ReconciliationApiError {
        error: error.to_string(),
    })?;
    Ok(Json(PendingClientRecoveriesResponse { recoveries }))
}

/// Retry one interrupted storage recovery after confirming its client is offline.
pub async fn retry_pending_client_recovery(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ReconciliationApiError>)> {
    let _client_guard = state.client_mutations.lock().await;
    match crate::application::client_lifecycle::retry_pending_recovery(&state, &id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err((
            StatusCode::NOT_FOUND,
            Json(ReconciliationApiError {
                error: "No pending storage recovery exists for this client".into(),
            }),
        )),
        Err(error) => {
            tracing::error!(client_id = %id, error = %error, "manual client storage recovery failed");
            let status = if error
                .downcast_ref::<crate::application::client_lifecycle::PendingRecoverySafetyError>()
                .is_some()
            {
                StatusCode::CONFLICT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            Err((
                status,
                Json(ReconciliationApiError {
                    error: error.to_string(),
                }),
            ))
        }
    }
}
