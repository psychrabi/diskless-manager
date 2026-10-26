//! Authenticated management and separately scoped preboot installation endpoints.
use crate::{
    application::installation_service::{
        CreateInstallation, InstallationReport, InstallationService,
    },
    state::AppState,
    types::Claims,
};
use axum::{
    body::Body,
    extract::{ConnectInfo, Path, State},
    http::{header, StatusCode},
    response::Response,
    Extension, Json,
};
use serde_json::{json, Value};
use std::net::SocketAddr;
type ApiError = (StatusCode, Json<Value>);
fn error(status: StatusCode, message: &str) -> ApiError {
    (
        status,
        Json(
            json!({"code":if status==StatusCode::FORBIDDEN {"forbidden"} else {"state_conflict"},"message":message,"operation_id":uuid::Uuid::new_v4().to_string(),"details":null}),
        ),
    )
}
fn admin(claims: &Claims) -> Result<(), ApiError> {
    if claims.role == "admin" {
        Ok(())
    } else {
        Err(error(
            StatusCode::FORBIDDEN,
            "Administrator access required",
        ))
    }
}
fn service_error(e: anyhow::Error) -> ApiError {
    // Service errors have intentionally bounded, secret-free messages; nested process stderr is never included.
    error(StatusCode::CONFLICT, &e.to_string())
}
pub async fn list(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    admin(&claims)?;
    let service = InstallationService::new(state);
    let sessions = service.repository.list(&id).await.map_err(service_error)?;
    let mut result = Vec::new();
    for session in sessions {
        let events = service
            .repository
            .events(&session.id)
            .await
            .map_err(service_error)?;
        result.push(json!({"session":session,"events":events}));
    }
    Ok(Json(json!(result)))
}
pub async fn create(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Json(request): Json<CreateInstallation>,
) -> Result<Json<Value>, ApiError> {
    admin(&claims)?;
    Ok(Json(json!(InstallationService::new(state)
        .create(id, request)
        .await
        .map_err(service_error)?)))
}
pub async fn cancel(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    admin(&claims)?;
    InstallationService::new(state)
        .cancel(&id)
        .await
        .map_err(service_error)?;
    Ok(Json(
        json!({"status":"cancelled","message":"Boot credentials revoked; power off the client before closing recovery"}),
    ))
}
pub async fn close_recovery(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    admin(&claims)?;
    InstallationService::new(state)
        .close_recovery(&id)
        .await
        .map_err(service_error)?;
    Ok(Json(json!({"status":"completed"})))
}
pub async fn report(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path((id, token)): Path<(String, String)>,
    Json(request): Json<InstallationReport>,
) -> Result<Json<Value>, ApiError> {
    InstallationService::new(state).report(&id,&token,peer.ip(),request).await.map_err(|_|error(StatusCode::FORBIDDEN,"Installation report denied; verify client address, session state and boot credential"))?;
    Ok(Json(json!({"status":"accepted"})))
}
pub async fn payload(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path((id, token, asset)): Path<(String, String, String)>,
) -> Result<Response<Body>, ApiError> {
    let body=InstallationService::new(state).payload(&id,&token,peer.ip(),&asset).await.map_err(|_|error(StatusCode::FORBIDDEN,"Installation payload denied; verify session state, prerequisites and assigned client address"))?;
    let content_type = match asset.as_str() {
        "user-data" | "meta-data" | "ks.cfg" | "disk-gate.sh" | "install.ps1" | "startnet.cmd" => {
            "text/plain; charset=utf-8"
        }
        _ => "application/octet-stream",
    };
    Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "no-store")
        .header("Referrer-Policy", "no-referrer")
        .body(body)
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not build installation payload",
            )
        })
}
