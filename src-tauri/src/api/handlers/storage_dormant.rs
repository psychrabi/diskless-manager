//! Admin-only dormant writeback cleanup.
//!
//! `GET` is a read-only plan; `POST` records the plan and only destroys
//! datasets when the configuration is enabled, `dry_run` is off and the caller
//! confirms.

use axum::{
    extract::{Extension, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    api::error::api_error,
    application::dormant_writeback_service::{
        DormantCleanupError, DormantCleanupService, DormantPlan, DormantRun,
    },
    state::AppState,
    types::Claims,
};

type ApiResult<T> = Result<T, (StatusCode, Json<Value>)>;

fn service(state: &AppState) -> DormantCleanupService {
    DormantCleanupService::new(
        state.db_pool.clone(),
        state.settings.clone(),
        state.application.image_backend.clone(),
        state.application.iscsi.clone(),
        crate::config::get_zpool_name(&state.config),
    )
}

fn require_admin(claims: &Claims) -> ApiResult<()> {
    if !claims.is_admin() {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "Administrator access required",
            Value::Null,
        ));
    }
    Ok(())
}

fn internal(message: &str, source: impl std::fmt::Display) -> (StatusCode, Json<Value>) {
    tracing::error!(%source, "dormant writeback cleanup request failed");
    api_error(StatusCode::INTERNAL_SERVER_ERROR, message, Value::Null)
}

#[derive(Debug, Default, Deserialize)]
pub struct DormantCleanupRunRequest {
    /// Required for a destructive run; ignored while `dry_run` is on.
    #[serde(default)]
    pub confirm: bool,
}

pub async fn get_dormant_cleanup_plan(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> ApiResult<Json<DormantPlan>> {
    require_admin(&claims)?;
    service(&state)
        .plan()
        .await
        .map(Json)
        .map_err(|error| internal("Failed to plan dormant writeback cleanup", error))
}

pub async fn run_dormant_cleanup(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<DormantCleanupRunRequest>,
) -> ApiResult<Json<DormantRun>> {
    require_admin(&claims)?;
    service(&state)
        .run(claims.sub.as_str(), request.confirm)
        .await
        .map(Json)
        .map_err(|error| match error {
            DormantCleanupError::Rejected(message) => {
                api_error(StatusCode::CONFLICT, message, Value::Null)
            }
            DormantCleanupError::Internal(source) => {
                internal("Failed to run dormant writeback cleanup", source)
            }
        })
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn dormant_cleanup_endpoints_are_admin_only() {
        let (state, app, admin, user) = crate::api::security_tests::setup().await;

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/system/storage/dormant")
                    .header("authorization", format!("Bearer {user}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/system/storage/dormant")
                    .header("authorization", format!("Bearer {admin}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // The plan itself is read-only; the listing may fail on a host without
        // ZFS, which is reported as an internal error and never as a mutation.
        assert!(matches!(
            response.status(),
            StatusCode::OK | StatusCode::INTERNAL_SERVER_ERROR
        ));

        // Disabled cleanup refuses even a confirmed run.
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/system/storage/dormant/run")
                    .header("authorization", format!("Bearer {admin}"))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"confirm":true}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
        assert_eq!(body["code"], "state_conflict");
        assert!(body["operation_id"].as_str().is_some());
        let _ = state;
    }
}
