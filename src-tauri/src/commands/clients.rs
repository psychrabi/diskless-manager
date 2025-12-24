use crate::core::client::{
    BootLogEntry, Client, ClientManager, CreateClientRequest, UpdateClientRequest,
};
use crate::services::DhcpService;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn list_clients(state: State<'_, AppState>) -> Result<Vec<Client>, String> {
    let manager = ClientManager::new(state.db_pool.clone());
    manager.list().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_client(state: State<'_, AppState>, id: String) -> Result<Client, String> {
    let manager = ClientManager::new(state.db_pool.clone());
    manager.get(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn add_client_command(
    state: State<'_, AppState>,
    request: CreateClientRequest,
) -> Result<Client, String> {
    let manager = ClientManager::new(state.db_pool.clone());
    let client = manager.create(request).await.map_err(|e| e.to_string())?;

    // Regenerate DHCP configuration with new client
    let settings = state.settings.read().await;
    if settings.dhcp.enabled {
        let dhcp_service = DhcpService::new(settings.clone(), state.db_pool.clone());
        dhcp_service.generate_config().await.map_err(|e| {
            tracing::warn!("Failed to regenerate DHCP config: {}", e);
            format!("Client added but DHCP config update failed: {}", e)
        })?;
        tracing::info!("DHCP configuration regenerated after adding client");

        // Reload DHCP service to apply changes
        if let Err(e) = dhcp_service.reload().await {
            tracing::warn!("Failed to reload DHCP service: {}", e);
        } else {
            tracing::info!("DHCP service reloaded successfully after adding client");
        }
    }

    Ok(client)
}

#[tauri::command]
pub async fn update_client_command(
    state: State<'_, AppState>,
    id: String,
    request: UpdateClientRequest,
) -> Result<Client, String> {
    let manager = ClientManager::new(state.db_pool.clone());
    let client = manager
        .update(&id, request)
        .await
        .map_err(|e| e.to_string())?;

    // Regenerate DHCP configuration with updated client
    let settings = state.settings.read().await;
    if settings.dhcp.enabled {
        let dhcp_service = DhcpService::new(settings.clone(), state.db_pool.clone());
        dhcp_service.generate_config().await.map_err(|e| {
            tracing::warn!("Failed to regenerate DHCP config: {}", e);
            format!("Client updated but DHCP config update failed: {}", e)
        })?;
        tracing::info!("DHCP configuration regenerated after updating client");

        // Reload DHCP service to apply changes
        if let Err(e) = dhcp_service.reload().await {
            tracing::warn!("Failed to reload DHCP service: {}", e);
        } else {
            tracing::info!("DHCP service reloaded successfully after updating client");
        }
    }

    Ok(client)
}

#[tauri::command]
pub async fn delete_client_command(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let manager = ClientManager::new(state.db_pool.clone());
    manager.delete(&id).await.map_err(|e| e.to_string())?;

    // Regenerate DHCP configuration without deleted client
    let settings = state.settings.read().await;
    if settings.dhcp.enabled {
        let dhcp_service = DhcpService::new(settings.clone(), state.db_pool.clone());
        dhcp_service.generate_config().await.map_err(|e| {
            tracing::warn!("Failed to regenerate DHCP config: {}", e);
            format!("Client deleted but DHCP config update failed: {}", e)
        })?;
        tracing::info!("DHCP configuration regenerated after deleting client");

        // Reload DHCP service to apply changes
        if let Err(e) = dhcp_service.reload().await {
            tracing::warn!("Failed to reload DHCP service: {}", e);
        } else {
            tracing::info!("DHCP service reloaded successfully after deleting client");
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn get_client_boot_history(
    state: State<'_, AppState>,
    client_id: String,
    limit: Option<i32>,
) -> Result<Vec<BootLogEntry>, String> {
    let manager = ClientManager::new(state.db_pool.clone());
    manager
        .get_boot_history(&client_id, limit.unwrap_or(50))
        .await
        .map_err(|e| e.to_string())
}
