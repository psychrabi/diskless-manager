use crate::{
    core::config::Settings,
    domain::{
        storage::{ClientStorage, ClientStorageSpec, StorageSource, StorageVolume},
        Client,
    },
};

fn target_iqn(settings: &Settings, client_name: &str, persisted: Option<&str>) -> String {
    persisted
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            format!(
                "{}:client.{}",
                settings.iscsi.target_prefix,
                client_name.trim().to_lowercase()
            )
        })
}

pub fn storage_spec_for_values(
    settings: &Settings,
    client_id: impl Into<String>,
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

    let backstore = format!("block_{}", client_name.to_lowercase());
    let target_iqn = format!(
        "{}:client.{}",
        settings.iscsi.target_prefix,
        client_name.to_lowercase()
    );

    let (source, dataset) = match snapshot.map(str::trim).filter(|value| !value.is_empty()) {
        Some(snapshot) => (
            StorageSource::Snapshot(snapshot.to_string()),
            crate::infrastructure::zfs::legacy::get_writeback_or_default_dataset(client_name),
        ),
        None => {
            crate::validation::validate_managed_dataset(master, &crate::config::get_zpool_name())
                .map_err(|error| format!("Invalid master dataset: {error}"))?;
            (
                StorageSource::ExistingVolume(master.to_string()),
                master.to_string(),
            )
        }
    };

    Ok(ClientStorageSpec {
        client_id: client_id.into(),
        source,
        dataset,
        backstore,
        target_iqn,
        lun: 0,
        use_game_disk,
        game_disks: Vec::new(),
        chap: None,
    })
}

pub fn storage_from_client(settings: &Settings, client: &Client) -> Result<ClientStorage, String> {
    let mut spec = storage_spec_for_values(
        settings,
        client.id.to_string(),
        &client.name,
        &client.master,
        client.snapshot.as_deref(),
        client.use_game_disk,
    )?;

    spec.target_iqn = target_iqn(settings, &client.name, client.target_iqn.as_deref());

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
