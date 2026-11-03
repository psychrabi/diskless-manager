//! Admin-only storage maintenance (Phase 5).
//!
//! `GET` reports pool state, scan progress, SMART and recent runs. `POST start`
//! and `POST stop` are the only mutating operations and both refuse to collide
//! with an active scan.

use axum::{
    extract::{Extension, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    api::error::api_error,
    application::maintenance_service::{
        MaintenanceError, MaintenanceOperation, MaintenanceRun, MaintenanceService,
        MaintenanceStatus,
    },
    state::AppState,
    types::Claims,
};

type ApiResult<T> = Result<T, (StatusCode, Json<Value>)>;

fn service(state: &AppState) -> MaintenanceService {
    MaintenanceService::new(state.db_pool.clone(), state.settings.clone())
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
    tracing::error!(%source, "storage maintenance request failed");
    api_error(StatusCode::INTERNAL_SERVER_ERROR, message, Value::Null)
}

#[derive(Debug, Default, Deserialize)]
pub struct StatusQuery {
    /// Bypass the SMART cache; still bounded by per-device timeouts.
    #[serde(default)]
    pub refresh: bool,
}

#[derive(Debug, Deserialize)]
pub struct MaintenanceRequest {
    /// `scrub` or `trim`.
    pub operation: String,
}

pub async fn get_maintenance_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<StatusQuery>,
) -> ApiResult<Json<MaintenanceStatus>> {
    require_admin(&claims)?;
    service(&state)
        .status(query.refresh)
        .await
        .map(Json)
        .map_err(|error| internal("Failed to read storage maintenance", error))
}

pub async fn start_maintenance(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<MaintenanceRequest>,
) -> ApiResult<Json<MaintenanceRun>> {
    require_admin(&claims)?;
    let operation = MaintenanceOperation::parse(&request.operation).ok_or_else(|| {
        api_error(
            StatusCode::BAD_REQUEST,
            "unknown maintenance operation",
            Value::Null,
        )
    })?;
    service(&state)
        .start(claims.sub.as_str(), operation)
        .await
        .map(Json)
        .map_err(|error| match error {
            MaintenanceError::Rejected(message) => {
                api_error(StatusCode::CONFLICT, message, Value::Null)
            }
            MaintenanceError::Internal(source) => {
                internal("Failed to start storage maintenance", source)
            }
        })
}

pub async fn stop_maintenance(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> ApiResult<Json<MaintenanceRun>> {
    require_admin(&claims)?;
    service(&state)
        .stop(claims.sub.as_str())
        .await
        .map(Json)
        .map_err(|error| match error {
            MaintenanceError::Rejected(message) => {
                api_error(StatusCode::CONFLICT, message, Value::Null)
            }
            MaintenanceError::Internal(source) => {
                internal("Failed to stop storage maintenance", source)
            }
        })
}
