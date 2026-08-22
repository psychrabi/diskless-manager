use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::process::Command;

use crate::{
    core::client::{BootLogEntry, Client, ClientManager, CreateClientRequest, UpdateClientRequest},
    domain::storage::{ClientStorage, ClientStorageSpec, StorageSource, StorageVolume},
    state::AppState,
    validation::{validate_ip_address, validate_mac_address},
};

/// Helper function to determine the master OS.
///
/// This is currently based on the master name. It can later be replaced
/// with a proper image repository lookup.
fn get_master_os(master_name: &str) -> Option<String> {
    if master_name.to_lowercase().contains("windows") {
        Some("windows".to_string())
    } else if master_name.to_lowercase().contains("linux") {
        Some("linux".to_string())
    } else {
        None
    }
}

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
        (StatusCode::INTERNAL_SERVER_ERROR, Json(self)).into_response()
    }
}

// ============================================================================
// Storage helpers
// ============================================================================

/// Build the desired storage specification for a client.
///
/// When a snapshot is configured:
///
/// ```text
/// snapshot
///     │
///     ▼
/// ZFS clone
///     │
///     ▼
/// iSCSI LUN
/// ```
///
/// When no snapshot is configured, the client references the existing
/// master ZVOL instead:
///
/// ```text
/// existing master ZVOL
///     │
///     ▼
/// iSCSI LUN
/// ```
fn build_storage_spec(
    settings: &crate::core::config::Settings,
    client_id: &str,
    client_name: &str,
    master: &str,
    snapshot: Option<&str>,
    use_game_disk: bool,
) -> Result<ClientStorageSpec, String> {
    let client_name = client_name.trim();

    if client_name.is_empty() {
        return Err("Client name cannot be empty".to_string());
    }

    if master.trim().is_empty() {
        return Err("Master image cannot be empty".to_string());
    }

    let target_iqn = format!(
        "{}:client.{}",
        settings.iscsi.target_prefix,
        client_name.to_lowercase()
    );

    let backstore = format!("block_{}", client_name.to_lowercase());

    match snapshot {
        Some(snapshot) if !snapshot.trim().is_empty() => {
            let dataset = crate::zfs::get_writeback_or_default_dataset(client_name);

            Ok(ClientStorageSpec {
                client_id: client_id.to_string(),
                source: StorageSource::Snapshot(snapshot.to_string()),
                dataset,
                backstore,
                target_iqn,
                lun: 0,
                use_game_disk,
            })
        }

        _ => Ok(ClientStorageSpec {
            client_id: client_id.to_string(),
            source: StorageSource::ExistingVolume(master.to_string()),
            dataset: master.to_string(),
            backstore,
            target_iqn,
            lun: 0,
            use_game_disk,
        }),
    }
}

/// Replace a generated target IQN with the persisted IQN for an existing client.
///
/// New clients receive an IQN generated from the configured prefix. Existing
/// clients keep their persisted IQN so legacy targets are never renamed by a
/// reset, update, or delete operation.
fn preserve_persisted_target_iqn(spec: &mut ClientStorageSpec, client: &Client) {
    if let Some(target_iqn) = client
        .target_iqn
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        spec.target_iqn = target_iqn.to_string();
    }
}

/// Convert application storage information into the legacy client fields
/// stored in the clients table.
fn apply_storage_to_request(request: &mut CreateClientRequest, storage: &ClientStorage) {
    request.block_store = Some(format!("/dev/zvol/{}", storage.dataset()));

    request.block_device = Some(storage.block_device().display().to_string());

    request.target_iqn = Some(storage.target_iqn().to_string());
}

/// Convert application storage information into the legacy client fields
/// stored in the clients table.
fn apply_storage_to_update_request(request: &mut UpdateClientRequest, storage: &ClientStorage) {
    request.block_store = Some(format!("/dev/zvol/{}", storage.dataset()));

    request.block_device = Some(storage.block_device().display().to_string());

    request.target_iqn = Some(storage.target_iqn().to_string());
}

/// Reconstruct the application-level storage object from the persisted
/// client record.
///
/// This is required when deleting/resetting a client because the actual
/// application storage object is not persisted separately.
fn storage_from_client(
    settings: &crate::core::config::Settings,
    client: &Client,
) -> Result<ClientStorage, String> {
    let mut spec = build_storage_spec(
        settings,
        &client.id,
        &client.name,
        &client.master,
        client.snapshot.as_deref(),
        client.use_game_disk.unwrap_or(false),
    )?;

    preserve_persisted_target_iqn(&mut spec, client);

    Ok(ClientStorage {
        client_id: spec.client_id.clone(),
        source: spec.source.clone(),
        volume: StorageVolume::new(
            spec.dataset.clone(),
            spec.block_device(),
            spec.backstore.clone(),
            spec.target_iqn.clone(),
            spec.lun,
        ),
        use_game_disk: spec.use_game_disk,
    })
}

/// Regenerate DHCP configuration after a client change.
async fn refresh_dhcp(state: &AppState, settings: &crate::core::config::Settings, operation: &str) {
    if !settings.dhcp.enabled {
        return;
    }

    let dhcp_service = crate::services::DhcpService::new(settings.clone(), state.db_pool.clone());

    if let Err(e) = dhcp_service.generate_client_configs().await {
        tracing::warn!(
            "Failed to regenerate DHCP client configuration after {}: {}",
            operation,
            e
        );
    } else {
        info!("DHCP client configuration regenerated after {}", operation);
    }

    if let Err(e) = dhcp_service.reload().await {
        tracing::warn!("Failed to reload DHCP service after {}: {}", operation, e);
    } else {
        info!("DHCP service reloaded successfully after {}", operation);
    }
}

// ============================================================================
// Client CRUD
// ============================================================================

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
    if validate_ip_address(&request.ip).is_err() || validate_mac_address(&request.mac).is_err() {
        return Err(StatusCode::BAD_REQUEST);
    }

    info!("Creating client: {:?}", request);

    let settings = state.settings.read().await;

    // ------------------------------------------------------------------------
    // Build desired storage state.
    // ------------------------------------------------------------------------

    let storage_spec = build_storage_spec(
        &settings,
        &request.name,
        &request.name,
        &request.master,
        request.snapshot.as_deref(),
        request.use_game_disk.unwrap_or(false),
    )
    .map_err(|error| {
        error!(
            "Invalid storage configuration for client '{}': {}",
            request.name, error
        );

        StatusCode::BAD_REQUEST
    })?;

    // ------------------------------------------------------------------------
    // Provision storage through the application service.
    // ------------------------------------------------------------------------

    let storage = state
        .application
        .storage
        .create_client_storage(&storage_spec)
        .map_err(|error| {
            error!(
                "Failed to provision storage for client '{}': {}",
                request.name, error
            );

            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    apply_storage_to_request(&mut request, &storage);

    info!(
        "Provisioned storage for client '{}': dataset={}, block_device={}, target_iqn={}",
        request.name,
        storage.dataset(),
        storage.block_device().display(),
        storage.target_iqn()
    );

    // ------------------------------------------------------------------------
    // Persist client.
    //
    // If persistence fails, rollback storage so that we don't leave orphaned
    // ZFS/iSCSI resources behind.
    // ------------------------------------------------------------------------

    let manager = ClientManager::new(state.db_pool.clone());

    let client = match manager.create(request).await {
        Ok(client) => client,

        Err(error) => {
            error!(
                "Failed to persist client after storage provisioning: {}",
                error
            );

            if let Err(cleanup_error) = state.application.storage.destroy_client_storage(&storage) {
                error!(
                    "Failed to rollback storage after client creation failure: {}",
                    cleanup_error
                );
            }

            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    info!("Created client: {}", client.name);

    // Refresh client IP cache.
    if let Err(e) = state.refresh_client_ips().await {
        tracing::warn!("Failed to refresh client IPs cache: {}", e);
    }

    // DHCP configuration.
    refresh_dhcp(&state, &settings, "adding client").await;

    Ok(Json(client))
}

pub async fn update_client(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(mut request): Json<UpdateClientRequest>,
) -> Result<Json<Client>, (StatusCode, Json<ErrorResponse>)> {
    info!("Updating client: {:?}", request);

    // ------------------------------------------------------------------------
    // Validate request.
    // ------------------------------------------------------------------------

    if let Some(ip) = &request.ip {
        if validate_ip_address(ip).is_err() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Invalid IPv4 address".to_string(),
                }),
            ));
        }
    }

    if let Some(mac) = &request.mac {
        if validate_mac_address(mac).is_err() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Invalid MAC address".to_string(),
                }),
            ));
        }
    }

    let manager = ClientManager::new(state.db_pool.clone());

    let existing_client = manager.get(&id).await.map_err(|_| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Client not found".to_string(),
            }),
        )
    })?;

    // ========================================================================
    // Action-based requests
    // ========================================================================

    if let Some(action) = &request.action {
        match action.as_str() {
            // ----------------------------------------------------------------
            // Wake
            // ----------------------------------------------------------------
            "wake" => {
                let mac = &existing_client.mac;

                info!(
                    "Attempting to send WOL packet to client {} ({})",
                    existing_client.name, mac
                );

                let result = Command::new("wakeonlan").arg(mac).output();

                match result {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout);

                        let stderr = String::from_utf8_lossy(&output.stderr);

                        if output.status.success() {
                            info!(
                                "Successfully sent WOL packet using wakeonlan to client {} ({}). Output: {}",
                                existing_client.name,
                                mac,
                                stdout
                            );

                            return Ok(Json(existing_client));
                        }

                        error!(
                            "wakeonlan failed for client {} ({}). Status: {:?}, Stderr: {}",
                            existing_client.name, mac, output.status, stderr
                        );
                    }

                    Err(e) => {
                        error!("wakeonlan command not found or failed: {}", e);
                    }
                }

                // Fallback to etherwake.
                match Command::new("etherwake").arg("-b").arg(mac).output() {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout);

                        let stderr = String::from_utf8_lossy(&output.stderr);

                        if output.status.success() {
                            info!(
                                "Successfully sent WOL packet using etherwake to client {} ({}). Output: {}",
                                existing_client.name,
                                mac,
                                stdout
                            );

                            return Ok(Json(existing_client));
                        }

                        let error_msg = format!(
                            "etherwake failed for client {} ({}): {}",
                            existing_client.name, mac, stderr
                        );

                        error!("{}", error_msg);

                        return Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse { error: error_msg }),
                        ));
                    }

                    Err(e) => {
                        let error_msg = format!(
                            "Failed to execute etherwake for client {} ({}): {}",
                            existing_client.name, mac, e
                        );

                        error!("{}", error_msg);

                        return Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse { error: error_msg }),
                        ));
                    }
                }
            }

            // ----------------------------------------------------------------
            // Reboot
            // ----------------------------------------------------------------
            "reboot" => {
                let ip = &existing_client.ip;

                if ip.is_empty() {
                    let error_msg = format!("IP address not found for '{}'", existing_client.name);

                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse { error: error_msg }),
                    ));
                }

                let master_os = get_master_os(&existing_client.master)
                    .unwrap_or_default()
                    .to_lowercase();

                if master_os.contains("linux") {
                    let output = Command::new("ssh")
                        .args([
                            "-o",
                            "StrictHostKeyChecking=no",
                            "-o",
                            "ConnectTimeout=5",
                            &format!("root@{}", ip),
                            "reboot",
                        ])
                        .output()
                        .map_err(|e| {
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(ErrorResponse {
                                    error: format!("Failed to execute SSH: {}", e),
                                }),
                            )
                        })?;

                    if !output.status.success() {
                        let error_msg = format!(
                            "Failed to reboot Linux client (SSH): {}",
                            String::from_utf8_lossy(&output.stderr)
                        );

                        return Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse { error: error_msg }),
                        ));
                    }
                } else {
                    let output = Command::new("net")
                        .args([
                            "rpc",
                            "shutdown",
                            "-r",
                            "-I",
                            ip,
                            "-U",
                            "diskless%1",
                            "-f",
                            "-t",
                            "0",
                        ])
                        .output()
                        .map_err(|e| {
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(ErrorResponse {
                                    error: format!("Failed to execute net rpc: {}", e),
                                }),
                            )
                        })?;

                    if !output.status.success() {
                        let error_msg = format!(
                            "Failed to reboot client: {}",
                            String::from_utf8_lossy(&output.stderr)
                        );

                        return Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse { error: error_msg }),
                        ));
                    }
                }

                info!("Reboot command sent to {} ({})", existing_client.name, ip);

                return Ok(Json(existing_client));
            }

            // ----------------------------------------------------------------
            // Shutdown
            // ----------------------------------------------------------------
            "shutdown" => {
                let ip = &existing_client.ip;

                if ip.is_empty() {
                    let error_msg = format!("IP address not found for '{}'", existing_client.name);

                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse { error: error_msg }),
                    ));
                }

                let master_os = get_master_os(&existing_client.master)
                    .unwrap_or_default()
                    .to_lowercase();

                if master_os.contains("linux") {
                    let output = Command::new("ssh")
                        .args([
                            "-o",
                            "StrictHostKeyChecking=no",
                            "-o",
                            "ConnectTimeout=5",
                            &format!("root@{}", ip),
                            "poweroff",
                        ])
                        .output()
                        .map_err(|e| {
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(ErrorResponse {
                                    error: format!("Failed to execute SSH: {}", e),
                                }),
                            )
                        })?;

                    if !output.status.success() {
                        let error_msg = format!(
                            "Failed to shutdown Linux client (SSH): {}",
                            String::from_utf8_lossy(&output.stderr)
                        );

                        return Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse { error: error_msg }),
                        ));
                    }
                } else {
                    let output = Command::new("net")
                        .args([
                            "rpc",
                            "shutdown",
                            "-I",
                            ip,
                            "-U",
                            "diskless%1",
                            "-f",
                            "-t",
                            "0",
                        ])
                        .output()
                        .map_err(|e| {
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(ErrorResponse {
                                    error: format!("Failed to execute net rpc: {}", e),
                                }),
                            )
                        })?;

                    if !output.status.success() {
                        let error_msg = format!(
                            "Failed to shutdown client: {}",
                            String::from_utf8_lossy(&output.stderr)
                        );

                        return Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse { error: error_msg }),
                        ));
                    }
                }

                info!("Shutdown command sent to {} ({})", existing_client.name, ip);

                return Ok(Json(existing_client));
            }

            // ----------------------------------------------------------------
            // Remote
            // ----------------------------------------------------------------
            "remote" => {
                info!("Remote control for client: {}", existing_client.name);

                return Ok(Json(existing_client));
            }

            // ----------------------------------------------------------------
            // Super mode
            // ----------------------------------------------------------------
            "super" => {
                if let Some(make_super) = request.make_super {
                    let mut client = existing_client.clone();

                    client.mode = if make_super {
                        Some("super".to_string())
                    } else {
                        None
                    };

                    client.updated_at = Utc::now();

                    client.last_modified =
                        Some(client.updated_at.format("%Y-%m-%d %H:%M:%S").to_string());

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
                    .map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Failed to update client mode: {}", e),
                            }),
                        )
                    })?;

                    info!("Client '{}' super mode set to: {}", client.name, make_super);

                    return Ok(Json(client));
                }
            }

            // ----------------------------------------------------------------
            // Reset writeback
            // ----------------------------------------------------------------
            "reset" => {
                info!("Reset writeback for client: {}", existing_client.name);

                let snapshot = match existing_client.snapshot.as_deref() {
                    Some(value) if !value.trim().is_empty() => value,

                    _ => {
                        return Err((
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse {
                                error: "Cannot reset writeback: client has no snapshot configured"
                                    .to_string(),
                            }),
                        ));
                    }
                };

                let settings = state.settings.read().await;

                let mut spec = build_storage_spec(
                    &settings,
                    &existing_client.id,
                    &existing_client.name,
                    &existing_client.master,
                    Some(snapshot),
                    existing_client.use_game_disk.unwrap_or(false),
                )
                .map_err(|error| (StatusCode::BAD_REQUEST, Json(ErrorResponse { error })))?;

                preserve_persisted_target_iqn(&mut spec, &existing_client);

                let current =
                    storage_from_client(&settings, &existing_client).map_err(|error| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse { error }),
                        )
                    })?;

                state
                    .application
                    .storage
                    .reset_client_storage(&current, &spec)
                    .map_err(|error| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Failed to reset client storage: {}", error),
                            }),
                        )
                    })?;

                info!(
                    "Successfully reset writeback for client '{}'",
                    existing_client.name
                );

                return Ok(Json(existing_client));
            }

            // ----------------------------------------------------------------
            // Reset clean
            // ----------------------------------------------------------------
            "reset_clean" => {
                info!("Reset to clean state for client: {}", existing_client.name);

                let settings = state.settings.read().await;

                let source = existing_client
                    .snapshot
                    .as_deref()
                    .filter(|value| !value.trim().is_empty());

                let mut spec = build_storage_spec(
                    &settings,
                    &existing_client.id,
                    &existing_client.name,
                    &existing_client.master,
                    source,
                    existing_client.use_game_disk.unwrap_or(false),
                )
                .map_err(|error| (StatusCode::BAD_REQUEST, Json(ErrorResponse { error })))?;

                preserve_persisted_target_iqn(&mut spec, &existing_client);

                let current =
                    storage_from_client(&settings, &existing_client).map_err(|error| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse { error }),
                        )
                    })?;

                state
                    .application
                    .storage
                    .reset_client_storage(&current, &spec)
                    .map_err(|error| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Failed to reset client storage: {}", error),
                            }),
                        )
                    })?;

                info!(
                    "Successfully reset client '{}' to clean state",
                    existing_client.name
                );

                return Ok(Json(existing_client));
            }

            _ => {
                tracing::warn!("Unknown client action: {}", action);
            }
        }
    }

    // ========================================================================
    // Normal client update
    // ========================================================================

    let settings = state.settings.read().await;

    let new_name = request.name.as_deref().unwrap_or(&existing_client.name);

    let new_master = request.master.as_deref().unwrap_or(&existing_client.master);

    let new_snapshot = request.snapshot.as_deref();

    // ------------------------------------------------------------------------
    // Remove current storage first.
    //
    // This allows us to safely replace:
    //
    //     old snapshot clone
    //
    // with:
    //
    //     new snapshot clone
    //
    // or:
    //
    //     master volume
    // ------------------------------------------------------------------------

    let current_storage = storage_from_client(&settings, &existing_client).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error }),
        )
    })?;

    if let Err(error) = state
        .application
        .storage
        .destroy_client_storage(&current_storage)
    {
        tracing::warn!(
            "Failed to remove existing storage for client '{}': {}",
            existing_client.name,
            error
        );
    }

    // ------------------------------------------------------------------------
    // Create desired new storage.
    // ------------------------------------------------------------------------

    let mut storage_spec = build_storage_spec(
        &settings,
        &existing_client.id,
        new_name,
        new_master,
        new_snapshot,
        request
            .use_game_disk
            .or(existing_client.use_game_disk)
            .unwrap_or(false),
    )
    .map_err(|error| (StatusCode::BAD_REQUEST, Json(ErrorResponse { error })))?;

    preserve_persisted_target_iqn(&mut storage_spec, &existing_client);

    let storage = state
        .application
        .storage
        .create_client_storage(&storage_spec)
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to provision new client storage: {}", error),
                }),
            )
        })?;

    apply_storage_to_update_request(&mut request, &storage);

    // ------------------------------------------------------------------------
    // Persist the new client configuration.
    // ------------------------------------------------------------------------

    let client = manager.update(&id, request).await.map_err(|e| {
        // Database update failed. Remove newly created storage.
        if let Err(cleanup_error) = state.application.storage.destroy_client_storage(&storage) {
            error!(
                "Failed to rollback newly created storage after database update failure: {}",
                cleanup_error
            );
        }

        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to update client: {}", e),
            }),
        )
    })?;

    info!("Updated client: {}", client.name);

    // Refresh IP cache.
    if let Err(e) = state.refresh_client_ips().await {
        tracing::warn!("Failed to refresh client IPs cache: {}", e);
    }

    // Regenerate DHCP.
    refresh_dhcp(&state, &settings, "updating client").await;

    Ok(Json(client))
}

// ============================================================================
// Delete client
// ============================================================================

pub async fn delete_client(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(), StatusCode> {
    tracing::info!(
        "DELETE CLIENT CALLED - Starting deletion for client: {}",
        id
    );

    let manager = ClientManager::new(state.db_pool.clone());

    let client = manager.get(&id).await.map_err(|_| StatusCode::NOT_FOUND)?;

    let settings = state.settings.read().await;

    // ------------------------------------------------------------------------
    // Remove client storage through application service.
    // ------------------------------------------------------------------------

    match storage_from_client(&settings, &client) {
        Ok(storage) => {
            if let Err(error) = state.application.storage.destroy_client_storage(&storage) {
                tracing::warn!(
                    "Failed to completely remove storage for client '{}': {}",
                    client.name,
                    error
                );
            }
        }

        Err(error) => {
            tracing::warn!(
                "Could not reconstruct storage for client '{}': {}",
                client.name,
                error
            );
        }
    }

    // ------------------------------------------------------------------------
    // Delete database record.
    // ------------------------------------------------------------------------

    tracing::info!("DELETE CLIENT - About to delete from database: {}", id);

    manager.delete(&id).await.map_err(|e| {
        tracing::error!("DELETE CLIENT - Database deletion failed: {}", e);

        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tracing::info!("DELETE CLIENT - Successfully deleted from database: {}", id);

    // Refresh client IP cache.
    if let Err(e) = state.refresh_client_ips().await {
        tracing::warn!("Failed to refresh client IPs cache: {}", e);
    }

    // Regenerate DHCP configuration.
    refresh_dhcp(&state, &settings, "deleting client").await;

    Ok(())
}

// ============================================================================
// Boot history
// ============================================================================

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
