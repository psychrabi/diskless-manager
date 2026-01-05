use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::state::AppState;
use crate::core::client::{ClientManager, Client, CreateClientRequest, UpdateClientRequest, BootLogEntry};

#[derive(Deserialize)]
pub struct Pagination {
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

pub async fn list_clients(
    State(state): State<AppState>,
) -> Result<Json<Vec<Client>>, StatusCode> {
    let manager = ClientManager::new(state.db_pool.clone());
    let clients = manager.list().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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
    Json(request): Json<CreateClientRequest>,
) -> Result<Json<Client>, StatusCode> {
    let manager = ClientManager::new(state.db_pool.clone());
    let client = manager.create(request).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Regenerate DHCP configuration with new client
    let settings = state.settings.read().await;
    if settings.dhcp.enabled {
        let dhcp_service = crate::services::DhcpService::new(settings.clone(), state.db_pool.clone());
        if let Err(e) = dhcp_service.generate_config().await {
            tracing::warn!("Failed to regenerate DHCP config: {}", e);
        } else {
            tracing::info!("DHCP configuration regenerated after adding client");
        }

        // Reload DHCP service to apply changes
        if let Err(e) = dhcp_service.reload().await {
            tracing::warn!("Failed to reload DHCP service: {}", e);
        } else {
            tracing::info!("DHCP service reloaded successfully after adding client");
        }
    }

    Ok(Json(client))
}

pub async fn update_client(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateClientRequest>,
) -> Result<Json<Client>, StatusCode> {
    let manager = ClientManager::new(state.db_pool.clone());
    let client = manager
        .update(&id, request)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Regenerate DHCP configuration with updated client
    let settings = state.settings.read().await;
    if settings.dhcp.enabled {
        let dhcp_service = crate::services::DhcpService::new(settings.clone(), state.db_pool.clone());
        if let Err(e) = dhcp_service.generate_config().await {
            tracing::warn!("Failed to regenerate DHCP config: {}", e);
        } else {
            tracing::info!("DHCP configuration regenerated after updating client");
        }

        // Reload DHCP service to apply changes
        if let Err(e) = dhcp_service.reload().await {
            tracing::warn!("Failed to reload DHCP service: {}", e);
        } else {
            tracing::info!("DHCP service reloaded successfully after updating client");
        }
    }

    Ok(Json(client))
}

pub async fn delete_client(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(), StatusCode> {
    let manager = ClientManager::new(state.db_pool.clone());
    manager.delete(&id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Regenerate DHCP configuration without deleted client
    let settings = state.settings.read().await;
    if settings.dhcp.enabled {
        let dhcp_service = crate::services::DhcpService::new(settings.clone(), state.db_pool.clone());
        if let Err(e) = dhcp_service.generate_config().await {
            tracing::warn!("Failed to regenerate DHCP config: {}", e);
        } else {
            tracing::info!("DHCP configuration regenerated after deleting client");
        }

        // Reload DHCP service to apply changes
        if let Err(e) = dhcp_service.reload().await {
            tracing::warn!("Failed to reload DHCP service: {}", e);
        } else {
            tracing::info!("DHCP service reloaded successfully after deleting client");
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