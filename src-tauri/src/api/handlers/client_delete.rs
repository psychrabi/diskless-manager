use axum::{
    extract::{Path, State},
    http::StatusCode,
};

use crate::state::AppState;

pub async fn delete_client(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(), StatusCode> {
    let _client_guard = state.client_mutations.lock().await;

    let recovering: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM client_offline_resets WHERE client_id = ? AND operation IS NOT NULL",
    )
    .bind(&id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if recovering != 0 {
        return Err(StatusCode::CONFLICT);
    }

    tracing::info!(client_id = %id, "deleting client");

    let client = state
        .application
        .clients
        .get_by_string(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let settings = state.settings.read().await.clone();

    match crate::application::client_storage_mapping::storage_from_client(&settings, &client) {
        Ok(storage) => {
            if let Err(error) = state.application.storage.destroy_client_storage(&storage) {
                tracing::warn!(client_id = %id, %error, "failed to completely remove client storage");
            }
        }
        Err(error) => {
            tracing::warn!(client_id = %id, %error, "could not reconstruct client storage during deletion");
        }
    }

    let client_id = client.id.to_string();
    if let Err(error) = state
        .application
        .storage
        .destroy_client_game_clones(&client_id, client.target_iqn.as_deref())
    {
        tracing::warn!(client_id = %id, %error, "failed to completely remove client game clones");
    }

    if let Err(error) =
        crate::infrastructure::dhcp::remove_client_ipxe_menu(client.mac.as_str()).await
    {
        tracing::warn!(client_id = %id, %error, "failed to remove boot menu for deleted client");
    }

    state
        .application
        .clients
        .delete(&client.id)
        .await
        .map_err(|error| {
            tracing::error!(client_id = %id, %error, "database deletion failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if let Err(error) = state.refresh_client_ips().await {
        tracing::warn!(%error, "failed to refresh client IP cache after deletion");
    }

    super::clients::refresh_dhcp(&state, &settings, "deleting client").await;
    Ok(())
}
