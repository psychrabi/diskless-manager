use axum::{
    extract::{Path, State},
    http::StatusCode,
};

use crate::{
    domain::storage::{ClientStorage, StorageSource, StorageVolume},
    state::AppState,
};

fn storage_from_client(
    settings: &crate::core::config::Settings,
    client: &crate::domain::Client,
) -> Result<ClientStorage, String> {
    let client_name = client.name.trim();
    if client_name.is_empty() {
        return Err("Client name cannot be empty".to_string());
    }
    if client.master.trim().is_empty() {
        return Err("Master image cannot be empty".to_string());
    }

    let target_iqn = client
        .target_iqn
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            format!(
                "{}:client.{}",
                settings.iscsi.target_prefix,
                client_name.to_lowercase()
            )
        });
    let backstore = format!("block_{}", client_name.to_lowercase());

    let (source, dataset) = match client.snapshot.as_deref() {
        Some(snapshot) if !snapshot.trim().is_empty() => (
            StorageSource::Snapshot(snapshot.to_string()),
            crate::infrastructure::zfs::legacy::get_writeback_or_default_dataset(client_name),
        ),
        _ => {
            crate::validation::validate_managed_dataset(
                &client.master,
                &crate::config::get_zpool_name(),
            )
            .map_err(|error| format!("Invalid master dataset: {error}"))?;
            (
                StorageSource::ExistingVolume(client.master.clone()),
                client.master.clone(),
            )
        }
    };

    Ok(ClientStorage {
        client_id: client.id.to_string(),
        source,
        volume: StorageVolume::new(
            dataset.clone(),
            format!("/dev/zvol/{dataset}"),
            backstore,
            target_iqn,
            0,
        ),
        use_game_disk: client.use_game_disk,
    })
}

async fn refresh_dhcp(
    state: &AppState,
    settings: &crate::core::config::Settings,
) {
    if !settings.dhcp.enabled {
        return;
    }

    let service = crate::services::DhcpService::new(settings.clone(), state.db_pool.clone());
    if let Err(error) = service.generate_client_configs().await {
        tracing::warn!(%error, "failed to regenerate DHCP client configuration after deleting client");
        return;
    }
    if let Err(error) = service.validate_config().await {
        tracing::warn!(%error, "DHCP validation failed after deleting client; service was not reloaded");
        return;
    }
    if let Err(error) = service.reload().await {
        tracing::warn!(%error, "failed to reload DHCP service after deleting client");
    }
}

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

    match storage_from_client(&settings, &client) {
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

    refresh_dhcp(&state, &settings).await;
    Ok(())
}
