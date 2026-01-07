use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use log::info;
use serde::Deserialize;

use crate::core::client::{
    BootLogEntry, Client, ClientManager, CreateClientRequest, UpdateClientRequest,
};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct Pagination {
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

pub async fn list_clients(State(state): State<AppState>) -> Result<Json<Vec<Client>>, StatusCode> {
    let manager = ClientManager::new(state.db_pool.clone());
    let clients = manager
        .list()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(clients))
}

pub async fn get_client(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Client>, StatusCode> {
    let manager = ClientManager::new(state.db_pool.clone());
    let client = manager.get(&id).await.map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(Json(client))
}

pub async fn create_client(
    State(state): State<AppState>,
    Json(mut request): Json<CreateClientRequest>,
) -> Result<Json<Client>, StatusCode> {
    let settings = state.settings.read().await;

    // Generate iSCSI target details based on whether snapshot is provided
    if let Some(snapshot) = &request.snapshot {
        if !snapshot.is_empty() {
            // Client with snapshot: use clone-based block store
            let clone_dataset = crate::zfs::get_writeback_or_default_dataset(&request.name);
            let block_store_path = format!("/dev/zvol/{}", clone_dataset);
            let target_iqn = format!(
                "{}:client.{}",
                settings.iscsi.target_prefix,
                request.name.to_lowercase()
            );

            request.block_store = Some(block_store_path);
            request.block_device = Some(format!("block_{}", request.name.to_lowercase()));
            request.target_iqn = Some(target_iqn);
        }
    } else {
        // Client without snapshot: use master image directly
        let block_store_path = format!("/dev/zvol/{}", request.master);
        let target_iqn = format!(
            "{}:client.{}",
            settings.iscsi.target_prefix,
            request.name.to_lowercase()
        );

        request.block_store = Some(block_store_path);
        request.block_device = Some(format!("block_{}", request.name.to_lowercase()));
        request.target_iqn = Some(target_iqn);
    }

    info!(
        "Generated iSCSI details for client '{}': block_store={}, block_device={}, target_iqn={}",
        request.name,
        request.block_store.clone().ok_or(StatusCode::BAD_REQUEST)?,
        request
            .block_device
            .clone()
            .ok_or(StatusCode::BAD_REQUEST)?,
        request.target_iqn.clone().ok_or(StatusCode::BAD_REQUEST)?
    );

    let manager = ClientManager::new(state.db_pool.clone());
    let client = manager
        .create(request)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    info!("Created client: {}", client.name);

    let iscsi_service = crate::services::IscsiService::new(settings.clone());
    let _ = iscsi_service.create_target(&client).await.inspect_err(|e| {
        tracing::error!("Failed to create iSCSI target for client: {}", e);
    });
    info!("Created iSCSI target for client: {}", client.name);

    // Regenerate DHCP configuration with new client
    if settings.dhcp.enabled {
        let dhcp_service =
            crate::services::DhcpService::new(settings.clone(), state.db_pool.clone());
        if let Err(e) = dhcp_service.generate_client_configs().await {
            tracing::warn!("Failed to regenerate DHCP client config: {}", e);
        } else {
            info!("DHCP client configuration regenerated after adding client");
        }

        // Reload DHCP service to apply changes
        if let Err(e) = dhcp_service.reload().await {
            tracing::warn!("Failed to reload DHCP service: {}", e);
        } else {
            info!("DHCP service reloaded successfully after adding client");
        }
    }

    Ok(Json(client))
}

pub async fn update_client(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(mut request): Json<UpdateClientRequest>,
) -> Result<Json<Client>, StatusCode> {
    let manager = ClientManager::new(state.db_pool.clone());

    // Get existing client to determine the name for iSCSI details
    let existing_client = manager.get(&id).await.map_err(|_| StatusCode::NOT_FOUND)?;

    let settings = state.settings.read().await;

    // Generate iSCSI target details based on snapshot presence
    // Use new name if provided, otherwise use existing name
    let client_name = request.name.as_deref().unwrap_or(&existing_client.name);

    if let Some(snapshot) = &request.snapshot {
        if !snapshot.is_empty() {
            // Client with snapshot: use clone-based block store
            let clone_dataset = crate::zfs::get_writeback_or_default_dataset(client_name);
            let block_store_path = format!("/dev/zvol/{}", clone_dataset);
            let target_iqn = format!(
                "{}:client.{}",
                settings.iscsi.target_prefix,
                client_name.to_lowercase()
            );

            request.block_store = Some(block_store_path);
            request.block_device = Some(format!("block_{}", client_name.to_lowercase()));
            request.target_iqn = Some(target_iqn);

            info!(
                "Generated iSCSI details for client '{}' with snapshot: block_store={}, block_device={}, target_iqn={}",
                client_name,
                request.block_store.as_ref().ok_or(StatusCode::BAD_REQUEST)?,
                request.block_device.as_ref().ok_or(StatusCode::BAD_REQUEST)?,
                request.target_iqn.as_ref().ok_or(StatusCode::BAD_REQUEST)?
            );
        } else {
            // Empty snapshot means remove snapshot and use master
            let master = request.master.as_deref().unwrap_or(&existing_client.master);
            let block_store_path = format!("/dev/zvol/{}", master);
            let target_iqn = format!(
                "{}:client.{}",
                settings.iscsi.target_prefix,
                client_name.to_lowercase()
            );

            request.block_store = Some(block_store_path);
            request.block_device = Some(format!("block_{}", client_name.to_lowercase()));
            request.target_iqn = Some(target_iqn);

            info!(
                "Generated iSCSI details for client '{}' using master: block_store={}, block_device={}, target_iqn={}",
                client_name,
                request.block_store.as_ref().ok_or(StatusCode::BAD_REQUEST)?,
                request.block_device.as_ref().ok_or(StatusCode::BAD_REQUEST)?,
                request.target_iqn.as_ref().ok_or(StatusCode::BAD_REQUEST)?
            );
        }
    } else if request.master.is_some() {
        // Master changed but no snapshot specified - use new master
        let master = request.master.as_ref().ok_or(StatusCode::BAD_REQUEST)?;
        let block_store_path = format!("/dev/zvol/{}", master);
        let target_iqn = format!(
            "{}:client.{}",
            settings.iscsi.target_prefix,
            client_name.to_lowercase()
        );

        request.block_store = Some(block_store_path);
        request.block_device = Some(format!("block_{}", client_name.to_lowercase()));
        request.target_iqn = Some(target_iqn);

        info!(
            "Generated iSCSI details for client '{}' with new master: block_store={}, block_device={}, target_iqn={}",
            client_name,
            request.block_store.as_ref().ok_or(StatusCode::BAD_REQUEST)?,
            request.block_device.as_ref().ok_or(StatusCode::BAD_REQUEST)?,
            request.target_iqn.as_ref().ok_or(StatusCode::BAD_REQUEST)?
        );
    }

    let client = manager
        .update(&id, request)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Regenerate iSCSI target if iSCSI details were updated
    let iscsi_service = crate::services::IscsiService::new(settings.clone());
    // First remove the old target if it existed
    if existing_client.target_iqn.is_some() {
        let _ = iscsi_service
            .remove_target(&existing_client.name)
            .await
            .inspect_err(|e| {
                tracing::warn!(
                    "Failed to remove old iSCSI target for client '{}': {}",
                    existing_client.name,
                    e
                );
            });
    }

    // Create the new target if all required iSCSI details are present
    if client.block_store.is_some() && client.block_device.is_some() && client.target_iqn.is_some()
    {
        let _ = iscsi_service.create_target(&client).await.inspect_err(|e| {
            tracing::error!("Failed to create updated iSCSI target for client: {}", e);
        });
        info!("Updated iSCSI target for client: {}", client.name);
    } else {
        info!(
            "Client {} does not have complete iSCSI details, skipping target creation",
            client.name
        );
    }

    // Regenerate DHCP configuration with updated client
    if settings.dhcp.enabled {
        let dhcp_service =
            crate::services::DhcpService::new(settings.clone(), state.db_pool.clone());
        if let Err(e) = dhcp_service.generate_client_configs().await {
            tracing::warn!("Failed to regenerate DHCP client config: {}", e);
        } else {
            info!("DHCP client configuration regenerated after updating client");
        }

        // Reload DHCP service to apply changes
        if let Err(e) = dhcp_service.reload().await {
            tracing::warn!("Failed to reload DHCP service: {}", e);
        } else {
            info!("DHCP service reloaded successfully after updating client");
        }
    }

    Ok(Json(client))
}

pub async fn delete_client(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(), StatusCode> {
    // Get the client first to access its iSCSI details
    let manager = ClientManager::new(state.db_pool.clone());
    let client = manager.get(&id).await.map_err(|_| StatusCode::NOT_FOUND)?;

    // Remove the iSCSI target if it exists
    let settings = state.settings.read().await;
    if let Some(ref target_iqn) = client.target_iqn {
        let iscsi_service = crate::services::IscsiService::new(settings.clone());
        let _ = iscsi_service
            .remove_target_by_iqn(target_iqn, &client.block_device)
            .await
            .inspect_err(|e| {
                tracing::warn!(
                    "Failed to remove iSCSI target for client '{}': {}",
                    client.name,
                    e
                );
            });
    }

    // Delete the client from the database
    manager
        .delete(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Regenerate DHCP configuration without deleted client
    if settings.dhcp.enabled {
        let dhcp_service =
            crate::services::DhcpService::new(settings.clone(), state.db_pool.clone());
        if let Err(e) = dhcp_service.generate_client_configs().await {
            tracing::warn!("Failed to regenerate DHCP client config: {}", e);
        } else {
            info!("DHCP client configuration regenerated after deleting client");
        }

        // Reload DHCP service to apply changes
        if let Err(e) = dhcp_service.reload().await {
            tracing::warn!("Failed to reload DHCP service: {}", e);
        } else {
            info!("DHCP service reloaded successfully after deleting client");
        }
    }

    Ok(())
}

pub async fn get_client_boot_history(
    State(state): State<AppState>,
    Path(client_id): Path<String>,
    Query(params): Query<Pagination>,
) -> Result<Json<Vec<BootLogEntry>>, StatusCode> {
    let limit = params.limit.unwrap_or(50);
    let manager = ClientManager::new(state.db_pool.clone());
    let history = manager
        .get_boot_history(&client_id, limit)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(history))
}
