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

/// GET /api/clients
///
/// V2 read path.
///
/// This endpoint is deliberately read-only at this stage.
/// Provisioning, iSCSI, DHCP, and ZFS operations remain in the
/// legacy mutation path until their infrastructure adapters are migrated.
pub async fn list_clients(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ClientApiError> {
    match state.application.clients.list().await {
        Ok(clients) => serde_json::to_value(clients)
            .map(Json)
            .map_err(|error| ClientApiError {
                error: format!("Failed to serialize clients: {error}"),
            }),
        Err(error) => {
            tracing::warn!(
                error = %error,
                "strict client decoding failed; serving legacy client rows"
            );
            let rows = sqlx::query_scalar::<_, String>(
                r#"
                SELECT json_object(
                    'id', id,
                    'name', name,
                    'mac', mac,
                    'ip', ip,
                    'master', master,
                    'enabled', json(CASE WHEN enabled <> 0 THEN 'true' ELSE 'false' END),
                    'created_at', created_at,
                    'updated_at', updated_at,
                    'snapshot', snapshot,
                    'block_store', block_store,
                    'target_iqn', target_iqn,
                    'writeback', writeback,
                    'last_modified', last_modified,
                    'block_device', block_device,
                    'status', status,
                    'mode', mode,
                    'pxe_mode', pxe_mode,
                    'boot_image', boot_image,
                    'keep_writeback', json(CASE
                        WHEN keep_writeback IS NULL THEN 'null'
                        WHEN keep_writeback <> 0 THEN 'true'
                        ELSE 'false' END),
                    'use_game_disk', json(CASE
                        WHEN use_game_disk IS NULL THEN 'null'
                        WHEN use_game_disk <> 0 THEN 'true'
                        ELSE 'false' END),
                    'chap_enabled', json(CASE
                        WHEN chap_enabled IS NULL THEN 'null'
                        WHEN chap_enabled <> 0 THEN 'true'
                        ELSE 'false' END),
                    'game_disks', json(COALESCE((
                        SELECT json_group_array(master_dataset)
                        FROM client_game_disks
                        WHERE client_id = clients.id
                    ), '[]'))
                )
                FROM clients
                ORDER BY name
                "#,
            )
            .fetch_all(&state.db_pool)
            .await
            .map_err(|error| {
                tracing::error!(error = %error, "failed to read raw client rows");
                ClientApiError {
                    error: "Failed to load clients".to_string(),
                }
            })?;
            let clients = rows
                .into_iter()
                .map(|row| {
                    serde_json::from_str(&row).map_err(|error| ClientApiError {
                        error: format!("Failed to decode client rows: {error}"),
                    })
                })
                .collect::<Result<Vec<serde_json::Value>, _>>()?;
            Ok(Json(serde_json::Value::Array(clients)))
        }
    }
}

/// GET /api/clients/{id}
///
/// V2 read path.
pub async fn get_client(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<crate::domain::Client>, ClientApiError> {
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
        Some(client) => Ok(Json(client)),

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
    use super::*;
    use crate::state::AppState;
    use std::sync::Arc;
    use tokio::sync::{Mutex, RwLock};

    #[tokio::test]
    async fn list_keeps_legacy_clients_with_invalid_network_metadata_visible() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory database should open");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("database should migrate");
        sqlx::query("INSERT INTO clients (id,name,mac,ip,master,enabled,created_at,updated_at) VALUES ('legacy','PC-legacy','unknown','invalid','pending',2,'2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')")
            .execute(&pool)
            .await
            .expect("legacy client should be inserted");
        let state = AppState {
            client_mutations: Arc::new(Mutex::new(())),
            settings: Arc::new(RwLock::new(crate::core::config::Settings::default())),
            db_pool: pool.clone(),
            config_path: std::path::PathBuf::new(),
            client_ips: Arc::new(RwLock::new(Vec::new())),
            metrics_collector: Arc::new(crate::metrics::MetricsCollector::default()),
            ssh_executor: Arc::new(crate::ssh_executor::SshExecutor::new()),
            application: Arc::new(crate::application::ApplicationServices::new(pool)),
        };

        let response = list_clients(State(state)).await;
        assert!(response.is_ok(), "legacy client should remain visible");
        let clients = serde_json::to_value(response.expect("response should be present").0)
            .expect("clients should serialize");
        assert_eq!(clients.as_array().map(|rows| rows.len()), Some(1));
        assert_eq!(clients[0]["id"], "legacy");
        assert_eq!(clients[0]["mac"], "unknown");
    }
}
