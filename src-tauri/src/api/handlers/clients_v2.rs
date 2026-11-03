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
) -> Result<Json<Vec<serde_json::Value>>, ClientApiError> {
    match state.application.clients.list().await {
        Ok(clients) => {
            let mut result = Vec::with_capacity(clients.len());
            for client in clients {
                let classified =
                    with_legacy_super_state(&state, client)
                        .await
                        .map_err(|error| {
                            tracing::error!(%error, "failed to classify legacy Super clients");
                            ClientApiError {
                                error: "Failed to load clients".into(),
                            }
                        })?;
                result.push(
                    serde_json::to_value(classified).map_err(|error| ClientApiError {
                        error: format!("Failed to serialize clients: {error}"),
                    })?,
                );
            }
            Ok(Json(result))
        }
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
                    'keep_writeback', json(CASE
                        WHEN keep_writeback IS NULL THEN 'null'
                        WHEN keep_writeback <> 0 THEN 'true'
                        ELSE 'false' END),
                    'use_game_disk', json(CASE
                        WHEN use_game_disk IS NULL THEN 'null'
                        WHEN use_game_disk <> 0 THEN 'true'
                        ELSE 'false' END),
                    'installer_media_id', installer_media_id,
                    'chap_enabled', json(CASE
                        WHEN chap_enabled IS NULL THEN 'null'
                        WHEN chap_enabled <> 0 THEN 'true'
                        ELSE 'false' END),
                    'legacy_super_state', CASE
                        WHEN EXISTS (SELECT 1 FROM legacy_super_exits e WHERE e.client_id = clients.id) THEN 'recovery_required'
                        WHEN EXISTS (SELECT 1 FROM super_sessions s WHERE s.client_id = clients.id AND s.state != 'completed') THEN NULL
                        WHEN clients.mode = 'super' THEN 'legacy'
                        ELSE NULL END,
                    'legacy_super_snapshot', (SELECT selected_snapshot FROM legacy_super_exits e WHERE e.client_id = clients.id),
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
            Ok(Json(clients))
        }
    }
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

/// Live iPXE-to-OS boot handoff for client detail diagnostics.
#[derive(Debug, Serialize)]
pub struct BootHandoffView {
    pub attempt_id: String,
    /// `script_served`, `session_observed` or `expired`.
    pub phase: &'static str,
    pub started_at: i64,
    pub expires_at: i64,
    pub age_seconds: i64,
    pub expires_in_seconds: i64,
    /// The window still suspends the non-persistent offline deadline.
    pub active: bool,
    pub expiry_reported: bool,
    /// Always false today: LIO reports the same initiator IQN for the iPXE and
    /// the OS session, so a session is inferred, never verified from a running
    /// operating system. Confirmed boots are recorded in the boot log instead.
    pub os_confirmed: bool,
}

/// GET /api/clients/{id}/boot-handoff
///
/// Read-only boot-attempt diagnostics: which attempt is open, how far it got,
/// and how long it has left. Nothing here changes storage or session state.
pub async fn get_client_boot_handoff(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Option<BootHandoffView>>, ClientApiError> {
    let client_id = ClientId::from_string(id).map_err(|error| ClientApiError {
        error: error.to_string(),
    })?;
    let exists = state
        .application
        .clients
        .get(&client_id)
        .await
        .map_err(|error| ClientApiError {
            error: error.to_string(),
        })?
        .is_some();
    if !exists {
        return Err(ClientApiError {
            error: "Client not found".to_string(),
        });
    }
    let now = chrono::Utc::now().timestamp();
    let handoff =
        crate::application::boot_handoff_service::load(&state.db_pool, client_id.as_str())
            .await
            .map_err(|error| {
                tracing::error!(client_id = %client_id, %error, "failed to load boot handoff");
                ClientApiError {
                    error: "Failed to load boot handoff".to_string(),
                }
            })?;
    Ok(Json(handoff.map(|handoff| {
        let active = handoff.is_live_at(now);
        BootHandoffView {
            attempt_id: handoff.attempt_id,
            phase: handoff.phase.as_str(),
            started_at: handoff.started_at,
            expires_at: handoff.expires_at,
            age_seconds: now.saturating_sub(handoff.started_at),
            expires_in_seconds: handoff.expires_at.saturating_sub(now).max(0),
            active,
            expiry_reported: handoff.expiry_reported,
            os_confirmed: false,
        }
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use std::sync::Arc;
    use tokio::sync::RwLock;

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
            config: crate::config::RuntimeConfig::default(),
            client_mutations: crate::state::ClientMutationLocks::default(),
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

    #[tokio::test]
    async fn boot_handoff_view_reports_the_live_window_and_its_expiry() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory database should open");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("database should migrate");
        sqlx::query("INSERT INTO clients (id,name,mac,ip,master,enabled,created_at,updated_at) VALUES ('client','PC001','02:00:00:00:00:01','192.0.2.10','pool/master',1,'2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')")
            .execute(&pool)
            .await
            .expect("client should be inserted");
        let now = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO client_boot_handoffs(client_id, attempt_id, fingerprint, phase, started_at, updated_at, expires_at, expiry_reported) VALUES ('client','boot-test','fp','script_served',?,?,?,0)")
            .bind(now - 20)
            .bind(now - 20)
            .bind(now + 280)
            .execute(&pool)
            .await
            .expect("handoff should be inserted");
        let state = AppState {
            config: crate::config::RuntimeConfig::default(),
            client_mutations: crate::state::ClientMutationLocks::default(),
            settings: Arc::new(RwLock::new(crate::core::config::Settings::default())),
            db_pool: pool.clone(),
            config_path: std::path::PathBuf::new(),
            client_ips: Arc::new(RwLock::new(Vec::new())),
            metrics_collector: Arc::new(crate::metrics::MetricsCollector::default()),
            ssh_executor: Arc::new(crate::ssh_executor::SshExecutor::new()),
            application: Arc::new(crate::application::ApplicationServices::new(pool.clone())),
        };

        let Json(view) = get_client_boot_handoff(State(state.clone()), Path("client".into()))
            .await
            .expect("handoff should load");
        let view = view.expect("a live attempt should be reported");
        assert_eq!(view.phase, "script_served");
        assert_eq!(view.attempt_id, "boot-test");
        assert!(view.active, "an open window suspends the offline deadline");
        assert!(view.age_seconds >= 20);
        assert!(view.expires_in_seconds > 0);
        assert!(
            !view.os_confirmed,
            "a session is inferred, never a verified OS boot"
        );

        // A window that has closed stops suspending the deadline but remains
        // readable until the next attempt replaces it.
        sqlx::query("UPDATE client_boot_handoffs SET expires_at = ? WHERE client_id = 'client'")
            .bind(now - 1)
            .execute(&state.db_pool)
            .await
            .expect("handoff should be expired");
        let Json(view) = get_client_boot_handoff(State(state.clone()), Path("client".into()))
            .await
            .expect("expired handoff should load");
        let view = view.expect("the expired attempt should still be reported");
        assert!(!view.active);
        assert_eq!(view.expires_in_seconds, 0);
    }
}
