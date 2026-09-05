use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{domain::ClientId, state::AppState};

#[derive(Debug, Deserialize)]
pub struct PrepareNvmeOfBootRequest {
    pub server_ip: String,
}

#[derive(Debug, Serialize)]
pub struct NvmeOfApiError {
    pub error: String,
}

impl IntoResponse for NvmeOfApiError {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::BAD_REQUEST, Json(self)).into_response()
    }
}

fn client_id(value: String) -> Result<ClientId, NvmeOfApiError> {
    ClientId::from_string(value).map_err(|error| NvmeOfApiError {
        error: error.to_string(),
    })
}

/// POST /api/clients/{id}/nvmeof/prepare
///
/// Creates an unauthenticated experimental NVMe/TCP export for the client's
/// existing ZVOL and returns the NQN, boot URI and iPXE boot fragment.
pub async fn prepare_nvmeof_boot(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<PrepareNvmeOfBootRequest>,
) -> Result<Json<crate::application::NvmeOfBootPreparation>, NvmeOfApiError> {
    let _client_guard = state.client_mutations.lock().await;
    let id = client_id(id)?;
    let server_ip = request.server_ip.trim();
    if server_ip.is_empty() {
        return Err(NvmeOfApiError {
            error: "server_ip cannot be empty".to_string(),
        });
    }

    state
        .application
        .nvmeof_boot
        .prepare(&id, server_ip)
        .await
        .map(Json)
        .map_err(|error| {
            tracing::error!(client_id = %id, error = %error, "failed to prepare experimental NVMe-oF boot");
            NvmeOfApiError {
                error: error.to_string(),
            }
        })
}

/// GET /api/clients/{id}/nvmeof
pub async fn inspect_nvmeof_boot(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<crate::infrastructure::nvmeof::NvmeOfExportStatus>, NvmeOfApiError> {
    let id = client_id(id)?;
    state
        .application
        .nvmeof_boot
        .inspect(&id)
        .await
        .map(Json)
        .map_err(|error| NvmeOfApiError {
            error: error.to_string(),
        })
}

/// DELETE /api/clients/{id}/nvmeof
///
/// Removes only the experimental NVMe/TCP export. The ZVOL and normal iSCSI
/// target remain untouched.
pub async fn remove_nvmeof_boot(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, NvmeOfApiError> {
    let id = client_id(id)?;
    state
        .application
        .nvmeof_boot
        .remove(&id)
        .await
        .map(|()| StatusCode::NO_CONTENT)
        .map_err(|error| NvmeOfApiError {
            error: error.to_string(),
        })
}
