//! Windows image servicing API.
//!
//! These endpoints are admin-only because they mount and mutate offline Windows
//! images on the host. Dry-run preparation still invokes DISM and therefore
//! remains privileged.

use crate::{
    infrastructure::pxe::{
        windows_servicing_available, RemoteWindowsCapabilitiesRequest, RemoteWindowsServicer,
        RemoteWindowsServicingCapabilities, RemoteWindowsServicingRequest,
        WindowsImagePreparationRequest, WindowsImagePreparationResult, WindowsImagePreparer,
    },
    types::Claims,
};
use axum::{
    http::StatusCode,
    Extension, Json,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct WindowsServicingCapabilities {
    pub available: bool,
    pub host_platform: &'static str,
}

#[derive(Debug, Serialize)]
pub struct WindowsServicingError {
    pub error: String,
}

fn require_admin(
    claims: &Claims,
) -> Result<(), (StatusCode, Json<WindowsServicingError>)> {
    if claims.role != "admin" {
        return Err((
            StatusCode::FORBIDDEN,
            Json(WindowsServicingError {
                error: "Forbidden: admin role required".to_string(),
            }),
        ));
    }
    Ok(())
}

pub async fn capabilities(
    Extension(claims): Extension<Claims>,
) -> Result<Json<WindowsServicingCapabilities>, (StatusCode, Json<WindowsServicingError>)> {
    require_admin(&claims)?;

    let available = tokio::task::spawn_blocking(windows_servicing_available)
        .await
        .map_err(|error| internal_message(error.to_string()))?;

    Ok(Json(WindowsServicingCapabilities {
        available,
        host_platform: std::env::consts::OS,
    }))
}

pub async fn prepare_image(
    Extension(claims): Extension<Claims>,
    Json(request): Json<WindowsImagePreparationRequest>,
) -> Result<Json<WindowsImagePreparationResult>, (StatusCode, Json<WindowsServicingError>)> {
    require_admin(&claims)?;

    let result = tokio::task::spawn_blocking(move || {
        let preparer = WindowsImagePreparer::new()?;
        preparer.prepare(request)
    })
    .await
    .map_err(|error| internal_message(error.to_string()))?
    .map_err(operation_error)?;

    Ok(Json(result))
}

fn operation_error(error: anyhow::Error) -> (StatusCode, Json<WindowsServicingError>) {
    log::error!("Windows servicing operation failed: {error:#}");
    (
        StatusCode::BAD_REQUEST,
        Json(WindowsServicingError {
            error: error.to_string(),
        }),
    )
}

fn internal_message(message: String) -> (StatusCode, Json<WindowsServicingError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(WindowsServicingError { error: message }),
    )
}


pub async fn remote_capabilities(
    Extension(claims): Extension<Claims>,
    Json(request): Json<RemoteWindowsCapabilitiesRequest>,
) -> Result<Json<RemoteWindowsServicingCapabilities>, (StatusCode, Json<WindowsServicingError>)> {
    require_admin(&claims)?;

    RemoteWindowsServicer::new()
        .capabilities(request)
        .await
        .map(Json)
        .map_err(operation_error)
}

pub async fn prepare_image_remote(
    Extension(claims): Extension<Claims>,
    Json(request): Json<RemoteWindowsServicingRequest>,
) -> Result<Json<WindowsImagePreparationResult>, (StatusCode, Json<WindowsServicingError>)> {
    require_admin(&claims)?;

    RemoteWindowsServicer::new()
        .prepare(request)
        .await
        .map(Json)
        .map_err(operation_error)
}
