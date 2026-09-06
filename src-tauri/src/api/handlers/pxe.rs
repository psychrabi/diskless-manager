use crate::infrastructure::pxe::{
    select_drivers, DriverInjectionStatus, NetworkDriverInjectionPlugin, NetworkDriverPackage,
    NetworkDriverSelectorInput, SelectedNetworkDriver,
};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

async fn plugin_from_state(state: &AppState) -> NetworkDriverInjectionPlugin {
    let settings = state.settings.read().await;
    NetworkDriverInjectionPlugin::new(settings.http.root_dir.clone())
}

pub async fn list_network_drivers(
    State(state): State<AppState>,
) -> Result<Json<Vec<NetworkDriverPackage>>, StatusCode> {
    let plugin = plugin_from_state(&state).await;
    plugin.list().map(Json).map_err(|error| {
        log::error!("Failed to list network drivers: {error}");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

pub async fn get_network_driver_status(
    State(state): State<AppState>,
) -> Result<Json<DriverInjectionStatus>, StatusCode> {
    let plugin = plugin_from_state(&state).await;
    plugin.status().map(Json).map_err(|error| {
        log::error!("Failed to get network driver injection status: {error}");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

#[derive(Debug, Deserialize)]
pub struct ImportNetworkDriverRequest {
    pub source_path: String,
}

pub async fn import_network_driver(
    State(state): State<AppState>,
    Json(request): Json<ImportNetworkDriverRequest>,
) -> Result<Json<NetworkDriverPackage>, StatusCode> {
    if request.source_path.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let plugin = plugin_from_state(&state).await;
    plugin
        .import_zip(request.source_path)
        .map(Json)
        .map_err(|error| {
            log::error!("Failed to import network driver: {error}");
            StatusCode::BAD_REQUEST
        })
}

pub async fn delete_network_driver(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let plugin = plugin_from_state(&state).await;
    plugin.remove(&id).map(|_| StatusCode::NO_CONTENT).map_err(|error| {
        log::error!("Failed to delete network driver '{}': {error}", id);
        StatusCode::NOT_FOUND
    })
}

pub async fn rebuild_network_driver_media(
    State(state): State<AppState>,
) -> Result<Json<DriverInjectionStatus>, StatusCode> {
    let plugin = plugin_from_state(&state).await;
    plugin.rebuild().map(Json).map_err(|error| {
        log::error!("Failed to rebuild network driver media: {error}");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

#[derive(Debug, Deserialize)]
pub struct SelectNetworkDriversRequest {
    #[serde(flatten)]
    pub selector: NetworkDriverSelectorInput,
}

/// Select the best network-driver packages for a PXE client.
///
/// Matching is deterministic: explicit package IDs win first, followed by
/// PNP device ID, MAC address, and finally driver service name.
pub async fn select_network_drivers(
    State(state): State<AppState>,
    Json(request): Json<SelectNetworkDriversRequest>,
) -> Result<Json<Vec<SelectedNetworkDriver>>, StatusCode> {
    let plugin = plugin_from_state(&state).await;
    let packages = plugin.list().map_err(|error| {
        log::error!("Failed to load network driver catalog: {error}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(select_drivers(&packages, &request.selector)))
}
