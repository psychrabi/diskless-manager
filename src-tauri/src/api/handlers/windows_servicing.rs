//! Windows image servicing API.
//!
//! These endpoints are admin-only because they mount and mutate offline Windows
//! images on the host. Dry-run preparation still invokes DISM and therefore
//! remains privileged.

use crate::{
    infrastructure::pxe::{
        windows_servicing_available, NetworkDriverInjectionPlugin,
        RemoteWindowsCapabilitiesRequest, RemoteWindowsCatalogPreparationResult,
        RemoteWindowsServicer, RemoteWindowsServicingCapabilities, RemoteWindowsServicingRequest,
        StagedDriverPackage, WindowsBootArmConfig, WindowsImagePreparationRequest,
        WindowsImagePreparationResult, WindowsImagePreparer,
    },
    state::AppState,
    types::Claims,
};
use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
pub struct WindowsServicingCapabilities {
    pub available: bool,
    pub host_platform: &'static str,
}

#[derive(Debug, Serialize)]
pub struct WindowsServicingError {
    pub error: String,
}

fn default_image_index() -> u32 {
    1
}

#[derive(Debug, Deserialize)]
pub struct CatalogWindowsPreparationRequest {
    pub host: String,
    pub username: String,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub executable_path: Option<String>,
    pub image_path: PathBuf,
    #[serde(default = "default_image_index")]
    pub image_index: u32,
    #[serde(default)]
    pub commit: bool,
    #[serde(default)]
    pub boot_arm: WindowsBootArmConfig,
    pub driver_ids: Vec<String>,
}

fn require_admin(claims: &Claims) -> Result<(), (StatusCode, Json<WindowsServicingError>)> {
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

pub async fn prepare_image_remote_catalog(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<CatalogWindowsPreparationRequest>,
) -> Result<Json<RemoteWindowsCatalogPreparationResult>, (StatusCode, Json<WindowsServicingError>)>
{
    require_admin(&claims)?;

    if request.driver_ids.is_empty() {
        return Err(operation_error(anyhow::anyhow!(
            "select at least one imported network driver package"
        )));
    }
    if request.driver_ids.len() > 64 {
        return Err(operation_error(anyhow::anyhow!(
            "no more than 64 driver packages may be staged in one operation"
        )));
    }

    let unique_ids = request
        .driver_ids
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let root = state.settings.read().await.http.root_dir.clone();
    let packages =
        tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<StagedDriverPackage>> {
            let plugin = NetworkDriverInjectionPlugin::new(PathBuf::from(root));
            unique_ids
                .into_iter()
                .map(|id| {
                    let source_dir = plugin.package_directory(&id)?;
                    Ok(StagedDriverPackage { id, source_dir })
                })
                .collect()
        })
        .await
        .map_err(|error| internal_message(error.to_string()))?
        .map_err(operation_error)?;

    let preparation = WindowsImagePreparationRequest {
        image_path: request.image_path,
        driver_root: PathBuf::new(),
        mount_root: None,
        image_index: request.image_index,
        recursive: true,
        commit: request.commit,
        boot_arm: request.boot_arm,
    };

    RemoteWindowsServicer::new()
        .prepare_with_driver_packages(
            request.host,
            request.username,
            request.password,
            request.executable_path,
            preparation,
            packages,
        )
        .await
        .map(Json)
        .map_err(operation_error)
}
