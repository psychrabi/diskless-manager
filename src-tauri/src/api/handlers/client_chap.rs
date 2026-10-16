use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use super::clients::ErrorResponse;
use crate::state::AppState;

/// Rotate a client's iSCSI CHAP secret without rebuilding storage.
pub async fn rotate_client_chap(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let _client_guard = state.client_mutations.lock_client(&id).await;

    let (client, credentials) =
        state
            .application
            .clients
            .rotate_chap(&id)
            .await
            .map_err(|error| {
                let not_found = error.to_string().contains("client not found");
                let status = if not_found {
                    StatusCode::NOT_FOUND
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                };
                (
                    status,
                    Json(ErrorResponse {
                        status: status.as_u16(),
                        error: if not_found {
                            format!("Client not found: {id}")
                        } else {
                            format!("Failed to rotate CHAP credentials: {error}")
                        },
                    }),
                )
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
            .map_err(|error| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                        error: format!("Failed to apply CHAP secret: {error}"),
                    }),
                )
            })?;

        if let Err(error) = state.refresh_client_boot_menu(&id).await {
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
