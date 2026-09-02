use crate::config::get_config;
use crate::core::client::ClientManager;
use crate::core::provisioning::{
    add_client_provisioning, check_duplicate_client, delete_client_config, get_client_by_id,
    get_client_paths, get_client_paths_with_master, save_client_config,
    AddClientProvisioningRequest,
};
use crate::domain::storage::{ClientStorageSpec, StorageSource};
use crate::error::AppError;
use crate::infrastructure::command::{run_command, run_command_async, run_command_output_no_sudo};
use crate::infrastructure::dhcp::{create_dhcp_entry_for_server, update_dhcp_config};
use crate::infrastructure::zfs::legacy::{
    get_latest_snapshot, get_master_os, zfs_clone, zfs_destroy, zfs_exists,
};
use crate::middleware::validate_auth;
use crate::state::AppState;
use crate::timed_execution;
use crate::types::{AddClientRequest, ControlRequest, EditClientRequest};
use crate::utils::network::{get_client_status_realtime, ping_host};
use crate::utils::remote::{launch_remote_desktop, launch_vnc_viewer};
use crate::validation::validate_client_id;
use chrono::Local;
use log::{debug, error, info, warn};
use std::process::Command;
use tauri::State;

pub async fn get_clients(
    state: State<'_, AppState>,
    token: String,
    client_id: Option<String>,
) -> Result<serde_json::Value, AppError> {
    validate_auth(&token)?;

    timed_execution!("get_clients", {
        let mut config = crate::config::read_config(state)
            .await
            .map_err(|e| AppError::Config(e.to_string()))?;

        if let Some(id) = client_id {
            let client = config
                .clients
                .iter()
                .find(|client| client.id.eq_ignore_ascii_case(&id));

            Ok(serde_json::json!(client))
        } else {
            let client_count = config.clients.len();

            if client_count > 0 {
                let max_concurrent = client_count.clamp(50, 200);
                let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(max_concurrent));

                let mut futures = Vec::new();

                for (index, client) in config.clients.iter().enumerate() {
                    if !client.ip.is_empty() && client.ip != "N/A" {
                        let semaphore = semaphore.clone();
                        let ip = client.ip.clone();

                        let future = tokio::spawn(async move {
                            if let Ok(_permit) = semaphore.acquire().await {
                                ping_host(ip).await
                            } else {
                                "Offline".to_string()
                            }
                        });

                        futures.push((index, future));
                    }
                }

                for (index, future) in futures {
                    if let Ok(status) = future.await {
                        if let Some(client) = config.clients.get_mut(index) {
                            client.status = Some(status);
                        }
                    }
                }
            }

            Ok(serde_json::json!(config.clients))
        }
    })
}

async fn try_get_client(state: &AppState, id: &str) -> Option<crate::core::client::Client> {
    if let Some(client) = get_client_by_id(id) {
        return Some(client);
    }

    let manager = ClientManager::new(state.db_pool.clone());

    if let Ok(client) = manager.get(id).await {
        return Some(client);
    }

    None
}

pub async fn remote_client(
    state: State<'_, AppState>,
    token: String,
    client_id: String,
) -> Result<serde_json::Value, AppError> {
    validate_auth(&token)?;

    info!("Remote client: {}", client_id);

    let client = try_get_client(&state, &client_id)
        .await
        .ok_or_else(|| AppError::NotFound("Client not found".to_string()))?;

    let client_ip = client.ip.clone();

    if client_ip.is_empty() {
        return Err(AppError::Validation("Client IP not found".to_string()));
    }

    let status = get_client_status_realtime(client_ip.clone());

    if status != "Online" {
        return Err(AppError::Validation("Client is not online".to_string()));
    }

    let master_os = get_master_os(&client.master)
        .unwrap_or_default()
        .to_lowercase();

    if master_os.contains("linux") {
        match launch_vnc_viewer(&client_ip) {
            Ok(_) => Ok(serde_json::json!({
                "message": format!("VNC viewer initiated to {}", client_id),
                "ip": client_ip
            })),
            Err(error) => Err(AppError::Command(format!(
                "Failed to launch VNC viewer: {}",
                error
            ))),
        }
    } else {
        match launch_remote_desktop(&client_ip, "diskless") {
            Ok(_) => Ok(serde_json::json!({
                "message": format!(
                    "Remote desktop connection initiated to {}",
                    client_id
                ),
                "ip": client_ip
            })),
            Err(error) => Err(AppError::Command(format!(
                "Failed to launch remote desktop: {}",
                error
            ))),
        }
    }
}

pub async fn add_client(
    state: State<'_, AppState>,
    token: String,
    req: AddClientRequest,
) -> Result<serde_json::Value, AppError> {
    validate_auth(&token)?;

    add_client_impl(state.inner(), req).await
}

pub async fn add_client_impl(
    state: &AppState,
    req: AddClientRequest,
) -> Result<serde_json::Value, AppError> {
    req.validate()?;

    let mac = req.mac.trim().to_uppercase();
    let ip = req.ip.trim().to_string();

    let name = if req.name.trim().is_empty() {
        if let Some(last) = ip.split('.').next_back() {
            if let Ok(number) = last.parse::<u8>() {
                format!("PC{:03}", number)
            } else {
                format!("PC_{}", mac.replace(':', ""))
            }
        } else {
            format!("PC_{}", mac.replace(':', ""))
        }
    } else {
        req.name.trim().to_lowercase()
    };

    let mut master = req.master.trim().to_string();

    if master.is_empty() {
        let config = get_config();

        if let Some(default) = config
            .settings
            .get("default_master")
            .and_then(|value| value.as_str())
        {
            if !default.is_empty() {
                master = default.to_string();
                info!("Using default master image: {}", master);
            }
        }
    }

    let snapshot = req
        .snapshot
        .as_ref()
        .map(|value| value.trim().to_string())
        .unwrap_or_default();

    if let Some(duplicate) = check_duplicate_client(&name, &mac, &ip) {
        return Err(AppError::Validation(duplicate));
    }

    // Preserve the legacy "inventory only" mode when no image is selected.
    // No infrastructure is provisioned in this branch.
    if master.is_empty() {
        return add_client_provisioning(
            state,
            AddClientProvisioningRequest {
                name,
                mac,
                ip,
                master,
                snapshot,
                keep_writeback: req.keep_writeback,
                use_game_disk: req.use_game_disk,
            },
        )
        .await;
    }

    let settings = state.settings.read().await.clone();
    let source = if snapshot.is_empty() {
        StorageSource::ExistingVolume(master.clone())
    } else {
        StorageSource::Snapshot(snapshot.clone())
    };
    let dataset = if snapshot.is_empty() {
        master.clone()
    } else {
        crate::infrastructure::zfs::legacy::get_writeback_or_default_dataset(&name)
    };
    let storage_spec = ClientStorageSpec {
        client_id: name.clone(),
        source,
        dataset,
        backstore: format!("block_{}", name.to_lowercase()),
        target_iqn: crate::domain::provisioning::TargetIqn::for_client_name(
            &settings.iscsi.target_prefix,
            &name,
        )
        .as_str()
        .to_string(),
        lun: 0,
        use_game_disk: req.use_game_disk.unwrap_or(false),
    };
    let client = state
        .application
        .provisioning
        .create_client(
            crate::domain::CreateClient {
                name: name.clone(),
                mac,
                ip,
                master,
                snapshot: (!snapshot.is_empty()).then_some(snapshot),
                block_store: None,
                block_device: None,
                target_iqn: None,
                pxe_mode: crate::domain::PxeMode::Uefi,
                keep_writeback: req.keep_writeback.unwrap_or(true),
                use_game_disk: req.use_game_disk.unwrap_or(false),
            },
            storage_spec,
            &settings.dhcp.next_server_ip,
        )
        .await
        .map_err(|error| AppError::Config(error.to_string()))?;

    state
        .refresh_client_ips()
        .await
        .map_err(|error| AppError::Config(error.to_string()))?;

    Ok(serde_json::json!({
        "message": format!("Client {} added successfully", client.name),
        "client": client,
    }))
}

pub async fn edit_client(
    state: State<'_, AppState>,
    token: String,
    client_id: String,
    data: EditClientRequest,
) -> Result<serde_json::Value, AppError> {
    validate_auth(&token)?;

    validate_client_id(&client_id)?;

    data.validate()?;

    let mut client_info = match try_get_client(&state, &client_id).await {
        Some(info) => {
            info!("Client {} found", client_id);
            info
        }

        None => {
            info!("Client {} not found", client_id);

            return Err(AppError::NotFound(format!(
                "Client {} not found",
                client_id
            )));
        }
    };

    let current_paths = get_client_paths(&client_id, &client_info.mac);

    let new_name = data.name.trim().to_string();
    let new_mac = data.mac.trim().to_uppercase();
    let new_ip = data.ip.trim().to_string();
    let new_master = data.master.trim().to_string();
    let new_snapshot = data.snapshot.as_deref().unwrap_or("").trim().to_string();

    let new_keep_writeback = data.keep_writeback;
    let new_use_game_disk = data.use_game_disk;

    let name_changed = new_name != client_info.name;
    let mac_changed = new_mac != client_info.mac;
    let ip_changed = new_ip != client_info.ip;
    let master_changed = new_master != client_info.master;

    let snapshot_changed = new_snapshot != client_info.snapshot.clone().unwrap_or_default();

    let keep_wb_changed = new_keep_writeback != client_info.keep_writeback;

    let use_game_changed = new_use_game_disk != client_info.use_game_disk;

    if new_master.is_empty() {
        client_info.name = new_name.clone();
        client_info.mac = new_mac.clone();
        client_info.ip = new_ip.clone();
        client_info.master = new_master.clone();

        client_info.snapshot = if new_snapshot.is_empty() {
            None
        } else {
            Some(new_snapshot.clone())
        };

        client_info.keep_writeback = new_keep_writeback;
        client_info.use_game_disk = new_use_game_disk;
        client_info.last_modified = Some(Local::now().format("%Y-%m-%d %H:%M:%S").to_string());

        save_client_config(&state.db_pool, &client_info).await;

        return Ok(serde_json::json!({
            "message": format!("Client {} updated", client_id)
        }));
    }

    if (mac_changed || ip_changed || keep_wb_changed || use_game_changed)
        && !(name_changed || master_changed || snapshot_changed)
    {
        client_info.mac = new_mac.clone();
        client_info.ip = new_ip.clone();
        client_info.keep_writeback = new_keep_writeback;
        client_info.use_game_disk = new_use_game_disk;

        client_info.last_modified = Some(Local::now().format("%Y-%m-%d %H:%M:%S").to_string());

        let server_ip = state.settings.read().await.dhcp.next_server_ip.clone();
        let dhcp_entry = create_dhcp_entry_for_server(
            &new_name,
            &new_mac,
            &new_ip,
            &current_paths.target_iqn,
            &server_ip,
        );

        update_dhcp_config(&client_id, &dhcp_entry, false)
            .await
            .map_err(AppError::Config)?;

        save_client_config(&state.db_pool, &client_info).await;

        return Ok(serde_json::json!({
            "message": format!("Successfully updated client {}", client_id)
        }));
    }

    info!(
        "Cleaning up old resources for re-provisioning client {}",
        client_id
    );

    if let (Some(iqn), Some(store)) = (
        client_info.target_iqn.clone(),
        client_info.block_store.clone(),
    ) {
        if let Err(error) = state
            .application
            .storage
            .remove_client_target(&iqn, &[store])
        {
            warn!("Failed to cleanup old iSCSI target: {}", error);
        }
    }

    if let Some(ref writeback) = client_info.writeback {
        if let Err(error) = zfs_destroy(writeback) {
            warn!("Failed to destroy old ZFS clone {}: {}", writeback, error);
        }
    }

    crate::core::provisioning::add_client_provisioning(
        &state,
        AddClientProvisioningRequest {
            name: new_name,
            mac: new_mac,
            ip: new_ip,
            master: new_master,
            snapshot: new_snapshot,
            keep_writeback: new_keep_writeback,
            use_game_disk: new_use_game_disk,
        },
    )
    .await?;

    if name_changed {
        crate::core::provisioning::delete_client_config(&state.db_pool, &client_id).await;
    }

    Ok(serde_json::json!({
        "message": format!("Successfully updated client {}", client_id)
    }))
}

pub async fn delete_client(
    state: State<'_, AppState>,
    token: String,
    client_id: String,
) -> Result<serde_json::Value, AppError> {
    validate_auth(&token)?;

    let client = try_get_client(&state, &client_id)
        .await
        .ok_or_else(|| AppError::NotFound(format!("Client {} not found", client_id)))?;

    if let (Some(iqn), Some(store)) = (&client.target_iqn, &client.block_store) {
        if let Err(error) = state
            .application
            .storage
            .remove_client_target(iqn, std::slice::from_ref(store))
        {
            warn!(
                "Failed to cleanup iSCSI target for {}: {}",
                client_id, error
            );
        }
    }

    if let Some(ref writeback) = client.writeback {
        if client.mode.as_deref() != Some("super") && zfs_exists(writeback) {
            if let Err(error) = zfs_destroy(writeback) {
                warn!("Failed to destroy ZFS clone {}: {}", writeback, error);
            }
        }
    }

    let deleted_from_config = delete_client_config(&state.db_pool, &client_id).await;

    let _db_result = sqlx::query(
        r#"
        DELETE FROM clients
        WHERE id = ?1 OR name = ?2 OR mac = ?3
        "#,
    )
    .bind(&client_id)
    .bind(&client.name)
    .bind(&client.mac)
    .execute(&state.db_pool)
    .await;

    if !deleted_from_config {
        return Err(AppError::Config(format!(
            "Failed to delete client {} from configuration",
            client_id
        )));
    }

    Ok(serde_json::json!({
        "message": format!("Client {} deleted successfully", client_id)
    }))
}

pub async fn control_client(
    state: State<'_, AppState>,
    token: String,
    client_id: String,
    req: ControlRequest,
) -> Result<serde_json::Value, AppError> {
    validate_auth(&token)?;

    let client = try_get_client(&state, &client_id)
        .await
        .ok_or_else(|| AppError::NotFound(format!("Client {} not found", client_id)))?;

    let client_clone = client.clone();
    let mac = client.mac.clone();
    let ip = client.ip.clone();
    let name = client.name.clone();

    match req.action.as_str() {
        "wake" => {
            if mac.is_empty() {
                return Err(AppError::Validation(format!(
                    "MAC address not found for '{}'",
                    name
                )));
            }

            let output = Command::new("wakeonlan")
                .arg(&mac)
                .output()
                .map_err(AppError::Io)?;

            if !output.status.success() {
                return Err(AppError::Command(format!(
                    "Wake-on-LAN failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }

            Ok(serde_json::json!({
                "message": format!(
                    "Wake-on-LAN command sent to {} ({})",
                    name, ip
                )
            }))
        }

        "reboot" => {
            if ip.is_empty() {
                return Err(AppError::Validation(format!(
                    "IP address not found for '{}'",
                    client_id
                )));
            }

            let master_os = get_master_os(&client.master)
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
                    .map_err(AppError::Io)?;

                if !output.status.success() {
                    return Err(AppError::Command(format!(
                        "Failed to reboot Linux client (SSH): {}",
                        String::from_utf8_lossy(&output.stderr)
                    )));
                }
            } else {
                let output = Command::new("net")
                    .args([
                        "rpc",
                        "shutdown",
                        "-r",
                        "-I",
                        &ip,
                        "-U",
                        "diskless%1",
                        "-f",
                        "-t",
                        "0",
                    ])
                    .output()
                    .map_err(AppError::Io)?;

                if !output.status.success() {
                    return Err(AppError::Command(format!(
                        "Failed to reboot client: {}",
                        String::from_utf8_lossy(&output.stderr)
                    )));
                }
            }

            Ok(serde_json::json!({
                "message": format!(
                    "Reboot command sent to {} ({})",
                    name, ip
                )
            }))
        }

        "shutdown" => {
            if ip.is_empty() {
                return Err(AppError::Validation(format!(
                    "IP address not found for '{}'",
                    client_id
                )));
            }

            let master_os = get_master_os(&client.master)
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
                    .map_err(AppError::Io)?;

                if !output.status.success() {
                    return Err(AppError::Command(format!(
                        "Failed to shutdown Linux client (SSH): {}",
                        String::from_utf8_lossy(&output.stderr)
                    )));
                }
            } else {
                let output = Command::new("net")
                    .args(["rpc", "shutdown", "-S", &ip, "-U", "diskless%1"])
                    .output()
                    .map_err(AppError::Io)?;

                if !output.status.success() {
                    return Err(AppError::Command(format!(
                        "Failed to shutdown client: {}",
                        String::from_utf8_lossy(&output.stderr)
                    )));
                }
            }

            Ok(serde_json::json!({
                "message": format!(
                    "Shutdown command sent to {} ({})",
                    name, ip
                )
            }))
        }

        "super" => {
            let status = get_client_status_realtime(client.ip.clone());

            if status != "Offline" {
                return Err(AppError::Validation(
                    "Client must be offline to toggle Super mode".to_string(),
                ));
            }

            let paths = get_client_paths_with_master(&client.id, &client.mac, &client.master);

            if req.make_super.unwrap_or(false) {
                let block_device = format!("/dev/zvol/{}", client.master);

                let target_iqn = paths.target_iqn.clone();
                let block_store = paths.backstore.clone();

                if let Some(tiqn) = client.target_iqn.as_ref() {
                    if let Err(error) = state
                        .application
                        .storage
                        .remove_client_target(tiqn, std::slice::from_ref(&block_store))
                    {
                        warn!(
                            "Failed to remove existing iSCSI target before enabling super mode: {}",
                            error
                        );
                    }
                }

                if let Some(ref writeback) = client.writeback {
                    if client.mode.as_deref() != Some("super") && zfs_exists(writeback) {
                        if let Err(error) = zfs_destroy(writeback) {
                            warn!("Failed to destroy ZFS clone {}: {}", writeback, error);
                        }
                    }
                }

                let storage_spec = ClientStorageSpec {
                    client_id: client.id.clone(),
                    source: StorageSource::ExistingVolume(client.master.clone()),
                    dataset: client.master.clone(),
                    backstore: block_store.clone(),
                    target_iqn: target_iqn.clone(),
                    lun: 0,
                    use_game_disk: client.use_game_disk.unwrap_or(false),
                };

                state
                    .application
                    .storage
                    .create_client_storage(&storage_spec)
                    .map_err(|error| {
                        AppError::Command(format!(
                            "Failed to setup iSCSI target '{}': {}",
                            target_iqn, error
                        ))
                    })?;

                let mut updated = client_clone.clone();

                updated.mode = Some("super".to_string());
                updated.block_device = Some(block_device);
                updated.snapshot = None;
                updated.writeback = None;

                if !save_client_config(&state.db_pool, &updated).await {
                    warn!("Failed to persist client mode change for {}", client_id);
                }

                Ok(serde_json::json!({
                    "message": format!(
                        "Super Client enabled for {}",
                        client_id
                    )
                }))
            } else {
                let paths = get_client_paths_with_master(&client.id, &client.mac, &client.master);

                let clone_path = paths.dataset.clone();

                debug!("Testing ZFS list command for master: {}", client.master);

                if let Ok(stdout) =
                    run_command_output_no_sudo(["zfs", "list", "-t", "snapshot", &client.master])
                {
                    debug!("Simple ZFS list output: {}", stdout);
                }

                let latest_snapshot = match get_latest_snapshot(&client.master) {
                    Ok(snapshot) => {
                        debug!("Found existing snapshot: {}", snapshot);
                        snapshot
                    }

                    Err(error) => {
                        debug!("Failed to find existing snapshots: {}", error);

                        if let Ok(stdout) = run_command_output_no_sudo([
                            "zfs",
                            "list",
                            "-H",
                            "-t",
                            "snapshot",
                            "-o",
                            "name",
                            &client.master,
                        ]) {
                            debug!("Manual snapshot search output: {}", stdout);

                            if let Some(first_snapshot) = stdout
                                .lines()
                                .find(|line| line.contains(&client.master) && line.contains('@'))
                            {
                                debug!("Found snapshot manually: {}", first_snapshot);

                                first_snapshot.to_string()
                            } else {
                                return Ok(serde_json::json!({
                                    "message": format!(
                                        "Cannot disable super mode for {}: No snapshots found for master {}. Client will remain in super mode.",
                                        client_id,
                                        client.master
                                    )
                                }));
                            }
                        } else {
                            return Ok(serde_json::json!({
                                "message": format!(
                                    "Cannot disable super mode for {}: Unable to find snapshots for master {}. Client will remain in super mode.",
                                    client_id,
                                    client.master
                                )
                            }));
                        }
                    }
                };

                if !zfs_exists(&latest_snapshot) {
                    return Err(AppError::NotFound(format!(
                        "Snapshot {} does not exist",
                        latest_snapshot
                    )));
                }

                debug!("Snapshot {} exists, proceeding with clone", latest_snapshot);

                if zfs_exists(&clone_path) {
                    debug!(
                        "Target clone {} already exists, destroying it first",
                        clone_path
                    );

                    zfs_destroy(&clone_path).map_err(|error| {
                        AppError::Command(format!(
                            "Failed to destroy existing clone {}: {}",
                            clone_path, error
                        ))
                    })?;
                }

                if let Err(error) = run_command(["zfs", "clone", &latest_snapshot, &clone_path]) {
                    return Err(AppError::Command(format!(
                        "ZFS clone failed: {} -> {}: {}",
                        latest_snapshot, clone_path, error
                    )));
                }

                debug!(
                    "Successfully created ZFS clone from {} to {}",
                    latest_snapshot, clone_path
                );

                let block_device = format!("/dev/zvol/{}", clone_path);

                let target_iqn = paths.target_iqn.clone();
                let block_store = paths.backstore.clone();

                if let Some(tiqn) = client.target_iqn.as_ref() {
                    if let Err(error) = state
                        .application
                        .storage
                        .remove_client_target(tiqn, std::slice::from_ref(&block_store))
                    {
                        warn!(
                            "Failed to remove existing iSCSI target before demotion: {}",
                            error
                        );
                    }
                }

                let storage_spec = ClientStorageSpec {
                    client_id: client.id.clone(),
                    source: StorageSource::ExistingClientVolume(clone_path.clone()),
                    dataset: clone_path.clone(),
                    backstore: block_store.clone(),
                    target_iqn: target_iqn.clone(),
                    lun: 0,
                    use_game_disk: client.use_game_disk.unwrap_or(false),
                };

                state
                    .application
                    .storage
                    .create_client_storage(&storage_spec)
                    .map_err(|error| {
                        AppError::Command(format!(
                            "Failed to set iSCSI to client writeback: {}",
                            error
                        ))
                    })?;

                let mut updated = client_clone.clone();

                updated.mode = None;
                updated.block_device = Some(block_device);
                updated.snapshot = Some(latest_snapshot);
                updated.writeback = Some(clone_path);

                if !save_client_config(&state.db_pool, &updated).await {
                    warn!("Failed to persist client mode change for {}", client_id);
                }

                Ok(serde_json::json!({
                    "message": format!(
                        "Super Client disabled for {}",
                        client_id
                    )
                }))
            }
        }

        "edit" => Ok(serde_json::json!({
            "message": format!(
                "Placeholder: Edit Client {} not implemented.",
                client_id
            )
        })),

        _ => Err(AppError::Validation(format!(
            "Invalid action: {}",
            req.action
        ))),
    }
}

pub async fn reset_client(
    state: State<'_, AppState>,
    token: String,
    client_id: String,
) -> Result<serde_json::Value, AppError> {
    info!("reset_client start: client_id={}", client_id);

    validate_auth(&token)?;
    validate_client_id(&client_id)?;

    let mut client_info = match try_get_client(&state, &client_id).await {
        Some(info) => info,

        None => {
            return Err(AppError::NotFound(format!(
                "Client {} not found",
                client_id
            )));
        }
    };

    let current_paths = get_client_paths(&client_id, &client_info.mac);

    let target_iqn = current_paths.target_iqn.clone();
    let block_store = current_paths.backstore.clone();
    let clone = current_paths.dataset.clone();

    if let Err(error) = state
        .application
        .storage
        .remove_client_target(&target_iqn, std::slice::from_ref(&block_store))
    {
        warn!("Failed to clean up iSCSI target: {}", error);
    }

    if zfs_exists(&clone) {
        if let Err(error) = zfs_destroy(&clone) {
            return Err(AppError::Command(format!(
                "Failed to destroy existing ZFS clone: {}",
                error
            )));
        }
    }

    let snapshot = match &client_info.snapshot {
        Some(snapshot) if !snapshot.is_empty() => snapshot,

        _ => {
            return Err(AppError::Validation(
                "No snapshot found for client".to_string(),
            ));
        }
    };

    if let Err(error) = zfs_clone(snapshot, &clone) {
        return Err(AppError::Command(format!(
            "Failed to create ZFS clone: {}",
            error
        )));
    }

    let block_device = format!("/dev/zvol/{}", clone);

    let storage_spec = ClientStorageSpec {
        client_id: client_info.id.clone(),
        source: StorageSource::ExistingClientVolume(clone.clone()),
        dataset: clone.clone(),
        backstore: block_store.clone(),
        target_iqn: target_iqn.clone(),
        lun: 0,
        use_game_disk: client_info.use_game_disk.unwrap_or(false),
    };

    state
        .application
        .storage
        .create_client_storage(&storage_spec)
        .map_err(|error| AppError::Command(format!("Failed to set up iSCSI target: {}", error)))?;

    client_info.target_iqn = Some(target_iqn.clone());

    client_info.block_store = Some(block_store.clone());

    client_info.block_device = Some(block_device.clone());

    client_info.writeback = Some(clone.clone());

    client_info.last_modified = Some(Local::now().format("%Y-%m-%d %H:%M:%S").to_string());

    client_info.status = None;

    let server_ip = state.settings.read().await.dhcp.next_server_ip.clone();
    let dhcp_entry = create_dhcp_entry_for_server(
        &client_info.name,
        &client_info.mac,
        &client_info.ip,
        &target_iqn,
        &server_ip,
    );

    if let Err(error) = update_dhcp_config(&client_id, &dhcp_entry, false).await {
        warn!("Failed to update DHCP config after reset: {}", error);
    } else if let Err(error) =
        run_command_async(&["systemctl", "restart", "isc-dhcp-server.service"]).await
    {
        warn!("Failed to restart DHCP service: {}", error);
    }

    if !save_client_config(&state.db_pool, &client_info).await {
        error!("Failed to persist client after reset: {}", client_id);
    }

    info!("reset_client completed: client_id={}", client_id);

    Ok(serde_json::json!({
        "message": format!(
            "Client {} reset successfully",
            client_id.to_uppercase()
        )
    }))
}

/// Reset a non-persistent client to clean state by recreating
/// writeback from its configured snapshot.
pub async fn reset_client_to_clean(
    state: State<'_, AppState>,
    token: String,
    client_id: String,
) -> Result<serde_json::Value, AppError> {
    validate_auth(&token)?;

    info!("Resetting client {} to clean state", client_id);

    let client_info = try_get_client(&state, &client_id)
        .await
        .ok_or_else(|| AppError::NotFound(format!("Client {} not found", client_id)))?;

    if client_info.keep_writeback.unwrap_or(true) {
        return Err(AppError::Validation(
            "Client is in persistent mode. Cannot reset to clean state.".to_string(),
        ));
    }

    let snapshot = client_info
        .snapshot
        .as_ref()
        .ok_or_else(|| AppError::Validation("Client has no snapshot to reset from".to_string()))?;

    if snapshot.is_empty() {
        return Err(AppError::Validation("Client snapshot is empty".to_string()));
    }

    let writeback = client_info
        .writeback
        .as_ref()
        .ok_or_else(|| AppError::Validation("Client has no writeback to reset".to_string()))?;

    info!("Deleting writeback: {}", writeback);

    if zfs_exists(writeback) {
        zfs_destroy(writeback)?;
        info!("Writeback deleted successfully");
    }

    info!("Recreating writeback from snapshot: {}", snapshot);

    zfs_clone(snapshot, writeback)?;

    info!("Writeback recreated successfully");

    Ok(serde_json::json!({
        "message": format!(
            "Client {} reset to clean state successfully",
            client_id
        )
    }))
}

pub async fn get_client_overview() -> Result<crate::types::ClientOverview, AppError> {
    let config = get_config();

    let mut online_clients = 0;

    for client in &config.clients {
        if client.is_online() {
            online_clients += 1;
        }
    }

    let total_clients = config.clients.len();
    let active_clients = online_clients;
    let offline_clients = total_clients - active_clients;

    Ok(crate::types::ClientOverview {
        total_clients,
        active_clients,
        offline_clients,
    })
}

#[expect(
    dead_code,
    reason = "Utility function kept for reference, used by old Tauri commands"
)]
fn check_client_online_status(mac: &str) -> bool {
    if let Ok(output) = Command::new("dhcp-lease-list").output() {
        if String::from_utf8_lossy(&output.stdout).contains(mac) {
            return true;
        }
    }

    if let Ok(output) = Command::new("grep")
        .args(["-A", "10", mac, "/var/lib/dhcp/dhcpd.leases"])
        .output()
    {
        if let Ok(lease_content) = String::from_utf8(output.stdout) {
            if let Some(ip_line) = lease_content.lines().find(|line| line.contains("lease")) {
                if let Some(ip) = ip_line.split_whitespace().nth(1) {
                    let ip = ip.trim_end_matches(';');

                    if Command::new("ping")
                        .args(["-c", "1", "-W", "2", ip])
                        .output()
                        .map(|output| output.status.success())
                        .unwrap_or(false)
                    {
                        return true;
                    }
                }
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_client_paths() {
        let _paths = get_client_paths("client1", "00:11:22:33:44:55");

        // get_client_paths reads configuration in order to determine
        // the ZFS writeback dataset. The test intentionally only
        // verifies that the typed path API can be invoked.
    }
}
