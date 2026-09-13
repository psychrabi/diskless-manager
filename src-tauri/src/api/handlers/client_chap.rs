use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct ChapRotationError {
    pub status: u16,
    pub error: String,
}

impl IntoResponse for ChapRotationError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (
            status,
            Json(serde_json::json!({
                "code": if status == StatusCode::NOT_FOUND { "not_found" } else { "internal_error" },
                "message": self.error,
                "details": {},
            })),
        )
            .into_response()
    }
}

/// Rotate a client's iSCSI CHAP secret without rebuilding storage.
pub async fn rotate_client_chap(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ChapRotationError> {
    let _client_guard = state.client_mutations.lock().await;

    let (client, credentials) = state
        .application
        .clients
        .rotate_chap(&id)
        .await
        .map_err(|error| {
            let not_found = error.to_string().contains("client not found");
            ChapRotationError {
                status: if not_found { 404 } else { 500 },
                error: if not_found {
                    format!("Client not found: {id}")
                } else {
                    format!("Failed to rotate CHAP credentials: {error}")
                },
            }
        })?;

    if let Some(target_iqn) = client
        .target_iqn
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        state
            .application
            .storage
            .set_target_chap(target_iqn, Some(&credentials))
            .map_err(|error| ChapRotationError {
                status: 500,
                error: format!("Failed to apply CHAP secret: {error}"),
            })?;

        let settings = state.settings.read().await;
        let next_server = settings.dhcp.next_server_ip.trim();
        let server_ip = if next_server.is_empty() {
            settings.server.ip_address.trim()
        } else {
            next_server
        };
        let reservation = crate::infrastructure::dhcp::BootReservation {
            client_name: client.name.clone(),
            mac: client.mac.to_string(),
            ip: client.ip.to_string(),
            target_iqn: target_iqn.to_string(),
            server_ip: server_ip.to_string(),
            chap: Some(credentials.clone()),
        };

        if let Err(error) = crate::infrastructure::dhcp::publish_client_ipxe(&reservation).await {
            tracing::warn!(client_id = %id, %error, "failed to republish boot menu after CHAP rotation");
        }
    }

    tracing::info!(
        client_id = %id,
        user = %credentials.username,
        "rotated iSCSI CHAP secret"
    );

    Ok(Json(serde_json::json!({
        "message": format!("CHAP secret rotated for '{}'", client.name),
        "username": credentials.username,
        "password": credentials.password,
        "chap_enabled": true,
    })))
}
