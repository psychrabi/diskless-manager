use axum::{
    extract::{Extension, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;

use crate::{
    application::writeback_spare_service::{list_spares, SpareStatus},
    persistence::ClientRepository,
    state::AppState,
    types::Claims,
};

#[derive(Serialize)]
pub struct SpareView {
    pub client_id: String,
    pub client_name: Option<String>,
    pub dataset: String,
    pub status: &'static str,
    pub source_snapshot: String,
    pub age_seconds: i64,
    pub ready: bool,
    pub last_error: Option<String>,
}

/// Read-only prewarming status: which spares exist, their age, and the limits
/// currently in force. No storage is inspected or changed here.
#[derive(Serialize)]
pub struct SparePrewarmView {
    pub enabled: bool,
    pub max_ready_spares: u32,
    pub ttl_hours: u32,
    pub min_free_bytes: u64,
    pub ready_count: usize,
    pub spares: Vec<SpareView>,
}

pub async fn list_storage_spares(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<SparePrewarmView>, (StatusCode, Json<super::clients::ErrorResponse>)> {
    if claims.role != "admin" {
        return Err((
            StatusCode::FORBIDDEN,
            Json(super::clients::ErrorResponse {
                status: StatusCode::FORBIDDEN.as_u16(),
                error: "Administrator access required".into(),
            }),
        ));
    }
    let config = state.settings.read().await.storage.spare_prewarm.clone();
    let names: std::collections::HashMap<String, String> =
        ClientRepository::new(state.db_pool.clone())
            .find_all()
            .await
            .map_err(internal)?
            .into_iter()
            .map(|client| (client.id.to_string(), client.name))
            .collect();
    let now = chrono::Utc::now().timestamp();
    let spares = list_spares(&state.db_pool).await.map_err(internal)?;
    let ready_count = spares
        .iter()
        .filter(|spare| spare.status == SpareStatus::Ready)
        .count();
    Ok(Json(SparePrewarmView {
        enabled: config.enabled,
        max_ready_spares: config.max_ready_spares,
        ttl_hours: config.ttl_hours,
        min_free_bytes: config.min_free_bytes,
        ready_count,
        spares: spares
            .into_iter()
            .map(|spare| SpareView {
                client_name: names.get(&spare.client_id).cloned(),
                client_id: spare.client_id,
                dataset: spare.dataset,
                status: spare.status.as_str(),
                source_snapshot: spare.source_snapshot,
                age_seconds: now.saturating_sub(spare.created_at),
                ready: spare.status == SpareStatus::Ready,
                last_error: spare.last_error,
            })
            .collect(),
    }))
}

fn internal(error: anyhow::Error) -> (StatusCode, Json<super::clients::ErrorResponse>) {
    tracing::error!(%error, "failed to read writeback spares");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(super::clients::ErrorResponse {
            status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
            error: "Failed to read writeback spares".into(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn spare_status_requires_admin_and_reports_stored_spares() {
        let (state, app, admin, user) = crate::api::security_tests::setup().await;
        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, snapshot, writeback, block_device, block_store, target_iqn, keep_writeback, use_game_disk, enabled, created_at, updated_at) VALUES ('client', 'PC001', '02:00:00:00:00:01', '192.0.2.10', 'pool/master', 'pool/master@ready', 'pool/clients/pc001', '/dev/zvol/pool/clients/pc001', 'block_pc001', 'iqn.test:pc001', 0, 0, 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
            .execute(&state.db_pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO client_writeback_spares(client_id, dataset, source_snapshot, fingerprint, status, created_at, retry_after) VALUES ('client', 'pool/clients/pc001-spare-abc123', 'pool/master@ready', 'fp', 'ready', ?, 0)")
            .bind(chrono::Utc::now().timestamp() - 30)
            .execute(&state.db_pool)
            .await
            .unwrap();

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/system/storage/spares")
                    .header("authorization", format!("Bearer {user}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/system/storage/spares")
                    .header("authorization", format!("Bearer {admin}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
        assert_eq!(body["enabled"], false);
        assert_eq!(body["ready_count"], 1);
        assert_eq!(body["spares"][0]["client_name"], "PC001");
        assert_eq!(body["spares"][0]["status"], "ready");
        assert!(body["spares"][0]["age_seconds"].as_i64().unwrap() >= 30);
    }
}
