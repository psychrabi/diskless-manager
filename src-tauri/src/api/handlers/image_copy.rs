//! Image copy endpoints (Phase 4C).
//!
//! Copying is asynchronous: the request reserves the destination and returns a
//! job, and the caller polls the job list for progress. A destination becomes a
//! managed image only after the transfer is verified.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Extension, Json, Router,
};
use serde::Deserialize;
use serde_json::Value;

use crate::api::error::api_error;
use crate::application::image_copy_service::{
    CopyJob, ImageCopyError, ImageCopyRegistry, ImageCopyService,
};
use crate::state::AppState;
use crate::types::Claims;

type ApiResult<T> = Result<T, (StatusCode, Json<Value>)>;

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

fn service(state: &AppState) -> ImageCopyService {
    ImageCopyService::new(
        state.db_pool.clone(),
        state.application.image_backend.clone(),
        state.application.image_copies.clone(),
    )
}

fn map_error(error: ImageCopyError, context: &str) -> (StatusCode, Json<Value>) {
    match error {
        ImageCopyError::NotFound(message) => api_error(StatusCode::NOT_FOUND, message, Value::Null),
        ImageCopyError::Rejected(message) => api_error(StatusCode::CONFLICT, message, Value::Null),
        ImageCopyError::Internal(source) => {
            tracing::error!(%source, "{context}");
            api_error(StatusCode::INTERNAL_SERVER_ERROR, context, Value::Null)
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct CopyImageRequest {
    /// Destination image name inside the image dataset.
    #[serde(default)]
    pub name: Option<String>,
}

pub async fn copy_image(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    body: Option<Json<CopyImageRequest>>,
) -> ApiResult<(StatusCode, Json<CopyJob>)> {
    require_admin(&claims)?;
    let request = body.map(|Json(request)| request).unwrap_or_default();
    let job = service(&state)
        .start(&id, request.name.as_deref())
        .await
        .map_err(|error| map_error(error, "Failed to start the image copy"))?;
    Ok((StatusCode::ACCEPTED, Json(job)))
}

pub async fn list_copies(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> ApiResult<Json<Vec<CopyJob>>> {
    require_admin(&claims)?;
    let registry: &ImageCopyRegistry = &state.application.image_copies;
    Ok(Json(registry.list()))
}

pub async fn cancel_copy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(job_id): Path<String>,
) -> ApiResult<StatusCode> {
    require_admin(&claims)?;
    let registry: &ImageCopyRegistry = &state.application.image_copies;
    registry
        .cancel(&job_id)
        .map_err(|error| map_error(error, "Failed to cancel the image copy"))?;
    Ok(StatusCode::ACCEPTED)
}

pub fn image_copy_routes() -> Router<AppState> {
    Router::new()
        .route("/api/images/copies", get(list_copies))
        .route("/api/images/copies/{job_id}/cancel", post(cancel_copy))
        .route("/api/images/{id}/copy", post(copy_image))
}
