use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{domain::ClientId, state::AppState};

#[derive(Debug, Serialize)]
pub struct ClientApiError {
    pub error: String,
}

impl IntoResponse for ClientApiError {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(self)).into_response()
    }
}

#[derive(Debug, Deserialize)]
pub struct BootHistoryQuery {
    pub limit: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct ClientWithLegacySuperState {
    #[serde(flatten)]
    pub client: crate::domain::Client,
    /// `legacy` or `recovery_required`; null for normal and managed-session clients.
    pub legacy_super_state: Option<String>,
    /// Snapshot selected for a pending exit, to make recovery retries deterministic.
    pub legacy_super_snapshot: Option<String>,
}

async fn with_legacy_super_state(
    state: &AppState,
    client: crate::domain::Client,
) -> Result<ClientWithLegacySuperState, sqlx::Error> {
    let (status, snapshot): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT CASE WHEN EXISTS (SELECT 1 FROM legacy_super_exits e WHERE e.client_id = c.id) THEN 'recovery_required' WHEN EXISTS (SELECT 1 FROM super_sessions s WHERE s.client_id = c.id AND s.state != 'completed') THEN NULL WHEN c.mode = 'super' THEN 'legacy' ELSE NULL END, (SELECT selected_snapshot FROM legacy_super_exits e WHERE e.client_id = c.id) FROM clients c WHERE c.id = ?",
    )
    .bind(client.id.as_str())
    .fetch_one(&state.db_pool)
    .await?;
    Ok(ClientWithLegacySuperState {
        client,
        legacy_super_state: status,
        legacy_super_snapshot: snapshot,
    })
}

/// GET /api/clients
///
/// V2 read path.
///
/// This endpoint is deliberately read-only at this stage.
/// Provisioning, iSCSI, DHCP, and ZFS operations remain in the
/// legacy mutation path until their infrastructure adapters are migrated.
pub async fn list_clients(
    State(state): State<AppState>,
) -> Result<Json<Vec<ClientWithLegacySuperState>>, ClientApiError> {
    let clients = state.application.clients.list().await.map_err(|error| {
        tracing::error!(
            error = %error,
            "failed to list clients"
        );

        ClientApiError {
            error: "Failed to load clients".to_string(),
        }
    })?;
    let mut result = Vec::with_capacity(clients.len());
    for client in clients {
        result.push(
            with_legacy_super_state(&state, client)
                .await
                .map_err(|error| {
                    tracing::error!(%error, "failed to classify legacy Super clients");
                    ClientApiError {
                        error: "Failed to load clients".into(),
                    }
                })?,
        );
    }
    Ok(Json(result))
}

/// GET /api/clients/{id}
///
/// V2 read path.
pub async fn get_client(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ClientWithLegacySuperState>, ClientApiError> {
    let client_id = ClientId::from_string(id).map_err(|error| ClientApiError {
        error: error.to_string(),
    })?;

    match state
        .application
        .clients
        .get(&client_id)
        .await
        .map_err(|error| ClientApiError {
            error: error.to_string(),
        })? {
        Some(client) => with_legacy_super_state(&state, client)
            .await
            .map(Json)
            .map_err(|error| {
                tracing::error!(%error, "failed to classify legacy Super client");
                ClientApiError {
                    error: "Failed to load client".into(),
                }
            }),

        None => Err(ClientApiError {
            error: "Client not found".to_string(),
        }),
    }
}

/// GET /api/clients/{id}/boot-history
///
/// Read-only boot-history path backed by the application service rather than
/// the legacy client manager.
pub async fn get_client_boot_history(
    State(state): State<AppState>,
    Path(client_id): Path<String>,
    Query(params): Query<BootHistoryQuery>,
) -> Result<Json<Vec<crate::domain::BootLogEntry>>, ClientApiError> {
    let limit = params.limit.unwrap_or(50);

    state
        .application
        .boot_history
        .list_for_client(&client_id, limit)
        .await
        .map(Json)
        .map_err(|error| {
            tracing::error!(client_id = %client_id, %error, "failed to load client boot history");
            ClientApiError {
                error: "Failed to load client boot history".to_string(),
            }
        })
}

#[cfg(test)]
mod tests {
    use super::with_legacy_super_state;

    #[tokio::test]
    async fn client_response_exposes_legacy_and_pending_exit_state() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO clients(id, name, mac, ip, master, mode, created_at, updated_at) VALUES ('legacy', 'Legacy', '00:11:22:33:44:55', '192.0.2.1', 'pool/master', 'super', '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z'), ('normal', 'Normal', '00:11:22:33:44:56', '192.0.2.2', 'pool/master', NULL, '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO legacy_super_exits(client_id, prior_master, prior_snapshot, selected_snapshot, state, created_at, updated_at) VALUES ('legacy', 'pool/master', NULL, 'pool/master@chosen', 'recovery_required', '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z')")
            .execute(&pool)
            .await
            .unwrap();
        let repo = crate::persistence::ClientRepository::new(pool.clone());
        let client = repo
            .find_by_id(&crate::domain::ClientId::from_string("legacy").unwrap())
            .await
            .unwrap()
            .unwrap();
        let state = crate::state::AppState {
            config: crate::config::RuntimeConfig::default(),
            client_mutations: crate::state::ClientMutationLocks::default(),
            settings: std::sync::Arc::new(tokio::sync::RwLock::new(
                crate::core::config::Settings::default(),
            )),
            db_pool: pool,
            config_path: std::path::PathBuf::new(),
            client_ips: std::sync::Arc::new(tokio::sync::RwLock::new(Vec::new())),
            metrics_collector: std::sync::Arc::new(crate::metrics::MetricsCollector::default()),
            ssh_executor: std::sync::Arc::new(crate::ssh_executor::SshExecutor::new()),
            application: std::sync::Arc::new(crate::application::ApplicationServices::new(
                sqlx::sqlite::SqlitePoolOptions::new()
                    .max_connections(1)
                    .connect("sqlite::memory:")
                    .await
                    .unwrap(),
            )),
        };
        let response = with_legacy_super_state(&state, client).await.unwrap();
        assert_eq!(
            response.legacy_super_state.as_deref(),
            Some("recovery_required")
        );
        assert_eq!(
            response.legacy_super_snapshot.as_deref(),
            Some("pool/master@chosen")
        );
    }
}
