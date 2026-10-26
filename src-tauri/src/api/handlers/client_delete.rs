use axum::{
    extract::{Path, State},
    http::StatusCode,
};

use crate::state::AppState;

/// Remove owned storage before deleting metadata; retain the client on cleanup failure.
pub async fn delete_client(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(), StatusCode> {
    let _client_guard = state.client_mutations.lock_client(&id).await;

    if crate::application::client_storage_reservations::has_storage_reservation(&state.db_pool, &id)
        .await
        .map_err(|error| {
            tracing::error!(client_id = %id, %error, "failed to check client storage reservation");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    {
        return Err(StatusCode::CONFLICT);
    }

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

    // Enrolled pool clients have no storage yet; provisioned clients must be cleaned successfully.
    let has_storage = client.target_iqn.is_some()
        || client.block_store.is_some()
        || client.writeback.is_some()
        || client.block_device.is_some()
        || client.snapshot.is_some();
    if has_storage {
        let storage = crate::application::client_storage_mapping::storage_from_client(&state.config, &settings, &client)
            .await
            .map_err(|error| {
                tracing::error!(client_id = %id, %error, "could not reconstruct client storage during deletion");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        state.application.storage.destroy_client_storage(&storage).map_err(|error| {
            tracing::error!(client_id = %id, %error, "failed to remove client storage; retaining client record");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }

    let client_id = client.id.to_string();
    if has_storage || client.use_game_disk || !client.game_disks.is_empty() {
        state
            .application
            .storage
            .destroy_client_game_clones(&client_id, client.target_iqn.as_deref())
            .map_err(|error| {
                tracing::error!(client_id = %id, %error, "failed to remove client game clones; retaining client record");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    }

    crate::infrastructure::dhcp::remove_client_ipxe_menu(&state.settings.read().await.clone(), client.mac.as_str())
        .await
        .map_err(|error| {
            tracing::error!(client_id = %id, %error, "failed to remove boot menu; retaining client record");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

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
