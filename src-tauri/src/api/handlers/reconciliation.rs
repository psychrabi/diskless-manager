use crate::api::util::spawn_client_locked;
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
    let _client_guard = state.client_mutations.lock_client(&id).await;
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
    pub kind: String,
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
    let recoveries = load_pending_client_recoveries(&state.db_pool).await?;
    Ok(Json(PendingClientRecoveriesResponse { recoveries }))
}

async fn load_pending_client_recoveries(
    pool: &sqlx::SqlitePool,
) -> Result<Vec<PendingClientRecovery>, ReconciliationApiError> {
    sqlx::query_as::<_, PendingClientRecovery>(
        "SELECT c.id AS client_id, c.name AS client_name, 'storage_recovery' AS kind, r.failures, r.retry_after, r.last_error FROM client_offline_resets r JOIN clients c ON c.id = r.client_id WHERE r.operation IS NOT NULL UNION ALL SELECT c.id AS client_id, c.name AS client_name, 'boot_failure' AS kind, b.failures, 0 AS retry_after, b.last_error FROM client_boot_failures b JOIN clients c ON c.id = b.client_id ORDER BY retry_after, client_name COLLATE NOCASE",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| ReconciliationApiError { error: error.to_string() })
}

/// Retry one interrupted storage recovery after confirming its client is offline.
pub async fn retry_pending_client_recovery(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ReconciliationApiError>)> {
    let recovery_id = id.clone();
    let recovery = spawn_client_locked(
        state.client_mutations.clone(),
        id.clone(),
        None,
        async move {
            crate::application::client_lifecycle::retry_pending_recovery(&state, &recovery_id).await
        },
    );
    match recovery
        .await
        .map(|result| result.and_then(std::convert::identity))
    {
        Ok(Ok(true)) => Ok(StatusCode::NO_CONTENT),
        Ok(Ok(false)) => Err((
            StatusCode::NOT_FOUND,
            Json(ReconciliationApiError {
                error: "No pending storage recovery exists for this client".into(),
            }),
        )),
        Ok(Err(error)) => {
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
        Err(error) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ReconciliationApiError {
                error: format!("manual client storage recovery task failed: {error}"),
            }),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::load_pending_client_recoveries;
    use crate::api::util::spawn_locked;
    use std::sync::Arc;
    use tokio::sync::{oneshot, Mutex};

    #[tokio::test]
    async fn cancelled_request_keeps_the_mutation_lock_until_recovery_finishes() {
        let lock = Arc::new(Mutex::new(()));
        let (started_tx, started_rx) = oneshot::channel();
        let (finish_tx, finish_rx) = oneshot::channel();
        let request = spawn_locked(lock.clone(), None, async move {
            let _ = started_tx.send(());
            let _ = finish_rx.await;
        });
        started_rx.await.unwrap();

        drop(request);

        assert!(lock.try_lock().is_err());
        finish_tx.send(()).unwrap();
        let _guard = tokio::time::timeout(std::time::Duration::from_secs(1), lock.lock())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn pending_list_includes_boot_failures_separately_from_storage_recovery() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, created_at, updated_at) VALUES ('client', 'client-one', '02:00:00:00:00:01', '192.0.2.1', 'pool/master', 'now', 'now')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO client_boot_failures (client_id, failures, last_failed_at, last_error) VALUES ('client', 2, 123, 'PXE target unavailable')")
            .execute(&pool).await.unwrap();

        let recoveries = load_pending_client_recoveries(&pool).await.unwrap();

        assert_eq!(recoveries.len(), 1);
        assert_eq!(recoveries[0].kind, "boot_failure");
        assert_eq!(recoveries[0].failures, 2);
        assert_eq!(
            recoveries[0].last_error.as_deref(),
            Some("PXE target unavailable")
        );
    }
}
