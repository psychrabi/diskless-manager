use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Serialize;

use crate::{application::ClientService, domain::ClientId, state::AppState};

#[derive(Debug, Serialize)]
pub struct ClientApiError {
    pub error: String,
}

impl IntoResponse for ClientApiError {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(self)).into_response()
    }
}

fn service(state: &AppState) -> ClientService {
    ClientService::new(crate::persistence::ClientRepository::new(
        state.db_pool.clone(),
    ))
}

/// GET /api/clients
///
/// V2 read path.
///
/// This endpoint is deliberately read-only at this stage.
/// Provisioning, iSCSI, DHCP, and ZFS operations remain in the
/// legacy mutation path until their infrastructure adapters are migrated.
pub async fn list_clients(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::domain::Client>>, ClientApiError> {
    service(&state).list().await.map(Json).map_err(|error| {
        tracing::error!(
            error = %error,
            "failed to list clients"
        );

        ClientApiError {
            error: "Failed to load clients".to_string(),
        }
    })
}

/// GET /api/clients/{id}
///
/// V2 read path.
pub async fn get_client(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<crate::domain::Client>, ClientApiError> {
    let client_id = ClientId::from_string(id).map_err(|error| ClientApiError {
        error: error.to_string(),
    })?;

    match service(&state)
        .get(&client_id)
        .await
        .map_err(|error| ClientApiError {
            error: error.to_string(),
        })? {
        Some(client) => Ok(Json(client)),

        None => Err(ClientApiError {
            error: "Client not found".to_string(),
        }),
    }
}
