use crate::infrastructure::pxe::{
    select_drivers, NetworkDriverInjectionPlugin, NetworkDriverSelectorInput, SelectedNetworkDriver,
};
use crate::state::AppState;
use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;

async fn plugin_from_state(state: &AppState) -> NetworkDriverInjectionPlugin {
    let settings = state.settings.read().await;
    NetworkDriverInjectionPlugin::new(settings.http.root_dir.clone())
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
