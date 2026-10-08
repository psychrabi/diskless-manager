use crate::config::get_config;
use crate::core::provisioning::{
    add_client_provisioning, check_duplicate_client, AddClientProvisioningRequest,
};
use crate::domain::storage::{ClientStorageSpec, StorageSource};
use crate::error::AppError;
use crate::state::AppState;
use crate::types::AddClientRequest;
use log::info;

/// Add a client from the command-line entry point.
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

    // Preserve inventory-only mode when no image is selected.
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
        game_disks: Vec::new(),
        chap: None,
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
                game_disks: Vec::new(),
                chap_enabled: false,
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
