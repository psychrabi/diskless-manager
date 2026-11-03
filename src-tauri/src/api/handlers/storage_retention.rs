//! Admin-only image-snapshot retention (Phase 4A).
//!
//! `GET` is a read-only plan; `POST` records the plan and only deletes when the
//! configuration is enabled, `dry_run` is off and the caller confirms.

use axum::{
    extract::{Extension, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    api::error::api_error,
    application::retention_service::{
        RetentionError, RetentionPlan, RetentionRun, RetentionService,
    },
    state::AppState,
    types::Claims,
};

type ApiResult<T> = Result<T, (StatusCode, Json<Value>)>;

fn service(state: &AppState) -> RetentionService {
    RetentionService::new(
        state.db_pool.clone(),
        state.settings.clone(),
        state.application.images.clone(),
    )
}

fn require_admin(claims: &Claims) -> ApiResult<()> {
    if claims.role != "admin" {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "Administrator access required",
            Value::Null,
        ));
    }
    Ok(())
}

fn internal(message: &str, source: impl std::fmt::Display) -> (StatusCode, Json<Value>) {
    tracing::error!(%source, "retention request failed");
    api_error(StatusCode::INTERNAL_SERVER_ERROR, message, Value::Null)
}

#[derive(Debug, Default, Deserialize)]
pub struct RetentionRunRequest {
    /// Required for a destructive run; ignored while `dry_run` is on.
    #[serde(default)]
    pub confirm: bool,
}

pub async fn get_retention_plan(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> ApiResult<Json<RetentionPlan>> {
    require_admin(&claims)?;
    service(&state)
        .plan()
        .await
        .map(Json)
        .map_err(|error| internal("Failed to plan image retention", error))
}

pub async fn run_retention(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<RetentionRunRequest>,
) -> ApiResult<Json<RetentionRun>> {
    require_admin(&claims)?;
    service(&state)
        .run(claims.sub.as_str(), request.confirm)
        .await
        .map(Json)
        .map_err(|error| match error {
            RetentionError::Rejected(message) => {
                api_error(StatusCode::CONFLICT, message, Value::Null)
            }
            RetentionError::Internal(source) => internal("Failed to run image retention", source),
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
    async fn retention_endpoints_are_admin_only() {
        let (state, app, admin, user) = crate::api::security_tests::setup().await;

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/system/storage/retention")
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
                    .uri("/api/system/storage/retention")
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
        assert_eq!(body["dry_run"], true);
        assert!(body["candidates"].as_array().is_some_and(Vec::is_empty));

        // Disabled retention refuses even a confirmed run.
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/system/storage/retention/run")
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
