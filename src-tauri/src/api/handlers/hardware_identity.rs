use crate::{
    api::handlers::clients::ErrorResponse, application::hardware_identity_service::IdentityStatus,
    state::AppState, types::Claims,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Extension, Json, Router,
};
type ApiResult = Result<Json<IdentityStatus>, (StatusCode, Json<ErrorResponse>)>;
fn admin(claims: &Claims) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if claims.role == "admin" {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                status: 403,
                error: "Administrator access required".into(),
            }),
        ))
    }
}
fn error(error: anyhow::Error) -> (StatusCode, Json<ErrorResponse>) {
    log::warn!("Identity cache operation failed: {error:#}");
    (
        StatusCode::CONFLICT,
        Json(ErrorResponse {
            status: 409,
            error: error.to_string(),
        }),
    )
}
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/clients/{id}/identity", get(status).delete(delete))
        .route("/api/clients/{id}/identity/capture", post(capture))
        .route("/api/clients/{id}/identity/apply", post(apply))
}
pub async fn status(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(claims): Extension<Claims>,
) -> ApiResult {
    admin(&claims)?;
    state
        .hardware_identity_service()
        .status(&id)
        .await
        .map(Json)
        .map_err(error)
}
pub async fn capture(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(claims): Extension<Claims>,
) -> ApiResult {
    admin(&claims)?;
    state
        .hardware_identity_service()
        .capture(&id)
        .await
        .map(Json)
        .map_err(error)
}
pub async fn apply(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(claims): Extension<Claims>,
) -> ApiResult {
    admin(&claims)?;
    state
        .hardware_identity_service()
        .apply(&id)
        .await
        .map(Json)
        .map_err(error)
}
pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(claims): Extension<Claims>,
) -> ApiResult {
    admin(&claims)?;
    state
        .hardware_identity_service()
        .delete(&id)
        .await
        .map(Json)
        .map_err(error)
}
