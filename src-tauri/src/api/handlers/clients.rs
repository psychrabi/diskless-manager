use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use log::{error, info};
use serde::{Deserialize, Serialize};

use crate::core::client::{
    BootLogEntry, Client, ClientManager, CreateClientRequest, UpdateClientRequest,
};
use crate::state::AppState;
use crate::zfs::{get_writeback_or_default_dataset, zfs_clone, zfs_destroy, zfs_exists};

#[derive(Deserialize)]
pub struct Pagination {
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(self),
        ).into_response()
    }
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

    info!("Creating client: {:?}", request);

    request.target_iqn = Some(format!(
        "{}:client.{}",
        settings.iscsi.target_prefix,
        request.name.to_lowercase()
    ));
    request.block_store = Some(format!("/dev/zvol/{}", request.master));
    request.block_device = Some(format!("block_{}", request.name.to_lowercase()));

    let client_name: &str = &request.name;

    let clone_dataset = get_writeback_or_default_dataset(client_name);

    // Check if a clone already exists and destroy it first
    if zfs_exists(&clone_dataset) {
        if let Err(e) = zfs_destroy(&clone_dataset) {
            error!(
                "Failed to destroy existing ZFS clone for client '{}': {}",
                client_name, e
            );
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        info!(
            "Successfully destroyed existing ZFS clone for client '{}' before creating new one",
            client_name
        );
    }

    // Generate iSCSI target details based on whether snapshot is provided
    if let Some(snapshot) = &request.snapshot {
        // If snapshot is provided, create ZFS clone for the snapshot
        let block_store_path = format!("/dev/zvol/{}", clone_dataset);
        request.block_store = Some(block_store_path);
        // Create the ZFS clone from the snapshot
        if let Err(e) = zfs_clone(snapshot, &clone_dataset) {
            error!(
                "Failed to create ZFS clone for client '{}': {}",
                client_name, e
            );
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        info!(
            "Successfully created ZFS clone for client '{}' from snapshot '{}'",
            client_name, snapshot
        );
    }

    info!(
        "Generated iSCSI details for client '{}': block_store={}\n, block_device={}\n, target_iqn={}",
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

    // Refresh client IPs cache
    if let Err(e) = state.refresh_client_ips().await {
        tracing::warn!("Failed to refresh client IPs cache: {}", e);
    }

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
) -> Result<Json<Client>, (StatusCode, Json<ErrorResponse>)> {
    info!("Updating client: {:?}", request);

    let manager = ClientManager::new(state.db_pool.clone());
    // Get existing client to determine the name for iSCSI details
    let existing_client = manager.get(&id).await.map_err(|_| (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "Client not found".to_string() })))?;

    // Handle action-based requests (wake, reboot, shutdown, etc.)
    if let Some(action) = &request.action {
        match action.as_str() {
            "wake" => {
                // Send WOL packet to wake the client
                use std::process::Command;
                let mac = &existing_client.mac;
                
                info!("Attempting to send WOL packet to client {} ({})", existing_client.name, mac);
                
                // Try wakeonlan first
                let result = Command::new("wakeonlan")
                    .arg(mac)
                    .output();

                match result {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        
                        if output.status.success() {
                            info!("Successfully sent WOL packet using wakeonlan to client {} ({}). Output: {}", existing_client.name, mac, stdout);
                            return Ok(Json(existing_client));
                        } else {
                            error!("wakeonlan failed for client {} ({}). Status: {:?}, Stderr: {}", existing_client.name, mac, output.status, stderr);
                            
                            // Try etherwake as fallback
                            let result2 = Command::new("etherwake")
                                .arg("-b")
                                .arg(mac)
                                .output();
                            
                            match result2 {
                                Ok(output2) => {
                                    let stdout2 = String::from_utf8_lossy(&output2.stdout);
                                    let stderr2 = String::from_utf8_lossy(&output2.stderr);
                                    
                                    if output2.status.success() {
                                        info!("Successfully sent WOL packet using etherwake to client {} ({}). Output: {}", existing_client.name, mac, stdout2);
                                        return Ok(Json(existing_client));
                                    } else {
                                        let error_msg = format!("etherwake failed for client {} ({}): {}", existing_client.name, mac, stderr2);
                                        error!("{}", error_msg);
                                        return Err((
                                            StatusCode::INTERNAL_SERVER_ERROR,
                                            Json(ErrorResponse { error: error_msg }),
                                        ).into());
                                    }
                                }
                                Err(e) => {
                                    let error_msg = format!("Failed to execute etherwake for client {} ({}): {}", existing_client.name, mac, e);
                                    error!("{}", error_msg);
                                    return Err((
                                        StatusCode::INTERNAL_SERVER_ERROR,
                                        Json(ErrorResponse { error: error_msg }),
                                    ).into());
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("wakeonlan command not found or failed: {}. Trying etherwake...", e);
                        
                        // Try etherwake directly
                        let result2 = Command::new("etherwake")
                            .arg("-b")
                            .arg(mac)
                            .output();
                        
                        match result2 {
                            Ok(output2) => {
                                let stdout2 = String::from_utf8_lossy(&output2.stdout);
                                let stderr2 = String::from_utf8_lossy(&output2.stderr);
                                
                                if output2.status.success() {
                                    info!("Successfully sent WOL packet using etherwake to client {} ({}). Output: {}", existing_client.name, mac, stdout2);
                                    return Ok(Json(existing_client));
                                } else {
                                    let error_msg = format!("etherwake failed for client {} ({}): {}", existing_client.name, mac, stderr2);
                                    error!("{}", error_msg);
                                    return Err((
                                        StatusCode::INTERNAL_SERVER_ERROR,
                                        Json(ErrorResponse { error: error_msg }),
                                    ).into());
                                }
                            }
                            Err(e) => {
                                let error_msg = format!("Failed to execute etherwake for client {} ({}): {}", existing_client.name, mac, e);
                                error!("{}", error_msg);
                                return Err((
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(ErrorResponse { error: error_msg }),
                                ).into());
                            }
                        }
                    }
                }
            }
            "reboot" => {
                // Reboot is handled by sending a command to the client
                info!("Reboot command for client: {}", existing_client.name);
                return Ok(Json(existing_client));
            }
            "shutdown" => {
                // Shutdown is handled by sending a command to the client
                info!("Shutdown command for client: {}", existing_client.name);
                return Ok(Json(existing_client));
            }
            "remote" => {
                // Remote control is handled by the frontend
                info!("Remote control for client: {}", existing_client.name);
                return Ok(Json(existing_client));
            }
            "super" => {
                // Handle super mode toggle
                if let Some(make_super) = request.make_super {
                    let mut client = existing_client.clone();
                    client.mode = if make_super { Some("super".to_string()) } else { None };
                    client.updated_at = Utc::now();
                    client.last_modified = Some(client.updated_at.format("%Y-%m-%d %H:%M:%S").to_string());

                    sqlx::query(
                        r#"
                        UPDATE clients
                        SET mode = ?, last_modified = ?, updated_at = ?
                        WHERE id = ?
                        "#,
                    )
                    .bind(&client.mode)
                    .bind(&client.last_modified)
                    .bind(client.updated_at.to_rfc3339())
                    .bind(&client.id)
                    .execute(&state.db_pool)
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("Failed to update client mode: {}", e) })))?;

                    info!("Client '{}' super mode set to: {}", client.name, make_super);
                    return Ok(Json(client));
                }
            }
            "reset" => {
                // Reset writeback
                info!("Reset writeback for client: {}", existing_client.name);
                return Ok(Json(existing_client));
            }
            "reset_clean" => {
                // Reset to clean state
                info!("Reset to clean state for client: {}", existing_client.name);
                return Ok(Json(existing_client));
            }
            _ => {
                log::warn!("Unknown action: {}", action);
            }
        }
    }

    let settings = state.settings.read().await;

    // Regenerate iSCSI target if iSCSI details were updated
    let iscsi_service = crate::services::IscsiService::new(settings.clone());
    // First remove the old target if it existed
    info!(
        "Removing old iSCSI target for client: {}",
        existing_client.name
    );
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
        info!(
            "Removed old iSCSI target for client: {}",
            existing_client.name
        );
    }

    // Generate iSCSI target details based on snapshot presence
    // Use new name if provided, otherwise use existing name
    let client_name: &str = request.name.as_deref().unwrap_or(&existing_client.name);

    // Ensure required iSCSI fields are set
    if request.target_iqn.is_none() {
        request.target_iqn = Some(format!(
            "{}:client.{}",
            settings.iscsi.target_prefix,
            client_name.to_lowercase()
        ));
    }

    if request.block_device.is_none() {
        request.block_device = Some(format!("block_{}", client_name.to_lowercase()));
    }

    // Handle ZFS clone based on snapshot value in request
    if let Some(snapshot) = &request.snapshot {
        // If snapshot is provided, create ZFS clone for the snapshot
        let clone_dataset = get_writeback_or_default_dataset(client_name);

        // Check if a clone already exists and destroy it first
        if zfs_exists(&clone_dataset) {
            if let Err(e) = zfs_destroy(&clone_dataset) {
                let error_msg = format!("Failed to destroy existing ZFS clone for client '{}': {}", client_name, e);
                error!("{}", error_msg);
                return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: error_msg })));
            }
            info!(
                "Successfully destroyed existing ZFS clone for client '{}' before creating new one",
                client_name
            );
        }

        let block_store_path = format!("/dev/zvol/{}", clone_dataset);
        request.block_store = Some(block_store_path);
        // Create the ZFS clone from the snapshot
        if let Err(e) = zfs_clone(snapshot, &clone_dataset) {
            let error_msg = format!("Failed to create ZFS clone for client '{}': {}", client_name, e);
            error!("{}", error_msg);
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: error_msg })));
        }
        info!(
            "Successfully created ZFS clone for client '{}' from snapshot '{}'",
            client_name, snapshot
        );
    } else {
        // Previous client had a snapshot, but now it's being removed
        let clone_dataset = get_writeback_or_default_dataset(client_name);
        if zfs_exists(&clone_dataset) {
            if let Err(e) = zfs_destroy(&clone_dataset) {
                let error_msg = format!("Failed to destroy ZFS clone for client '{}': {}", client_name, e);
                error!("{}", error_msg);
                return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: error_msg })));
            }
            info!(
                "Successfully destroyed ZFS clone for client '{}' after snapshot removal",
                client_name
            );
        }

        // Since previous client had a snapshot but it's not provided in request,
        // we should clear the snapshot in the database to indicate no snapshot is used
        request.snapshot = None;

        // Use master image as block store
        let master_dataset = &existing_client.master; // Use the master dataset name directly
        let block_store_path = format!("/dev/zvol/{}", master_dataset);
        request.block_store = Some(block_store_path);
        info!(
            "Using master image as block store for client '{}'",
            client_name
        );
    }

    let manager = ClientManager::new(state.db_pool.clone());
    info!("Updating client in database: {:?}", request);
    let client = manager
        .update(&id, request)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("Failed to update client: {}", e) })))?;

    info!("Updated client: {}", client.name);

    // Refresh client IPs cache
    if let Err(e) = state.refresh_client_ips().await {
        tracing::warn!("Failed to refresh client IPs cache: {}", e);
    }

    let iscsi_service = crate::services::IscsiService::new(settings.clone());
    let _ = iscsi_service.create_target(&client).await.inspect_err(|e| {
        tracing::error!("Failed to create iSCSI target for client: {}", e);
    });
    info!("Created iSCSI target for client: {}", client.name);

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
    let iscsi_service = crate::services::IscsiService::new(settings.clone());
    let _ = iscsi_service
        .remove_target(&client.name)
        .await
        .inspect_err(|e| {
            tracing::warn!(
                "Failed to remove iSCSI target for client '{}': {}",
                client.name,
                e
            );
        });
    if let Some(_snapshot) = client.snapshot {
        let block_store = client.block_store.as_ref().unwrap();
        if block_store.starts_with("/dev/zvol/") {
            // Extract the dataset name from the block_store path
            let dataset_name = block_store
                .strip_prefix("/dev/zvol/")
                .unwrap_or(block_store);
            info!(
                "Found potential ZFS dataset in block_store: {} (extracted: {})",
                block_store, dataset_name
            );

            if zfs_exists(dataset_name) {
                info!("ZFS dataset {} exists, attempting to destroy", dataset_name);
                let result = zfs_destroy(dataset_name);
                match result {
                    Ok(_) => info!("Successfully destroyed ZFS dataset: {}", dataset_name),
                    Err(e) => tracing::warn!(
                        "Failed to destroy ZFS dataset for client '{}': {}",
                        client.name,
                        e
                    ),
                }
            } else {
                info!("ZFS dataset {} does not exist", dataset_name);
            }
        }
    }

    // Delete the client from the database
    manager
        .delete(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Refresh client IPs cache
    if let Err(e) = state.refresh_client_ips().await {
        tracing::warn!("Failed to refresh client IPs cache: {}", e);
    }

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
