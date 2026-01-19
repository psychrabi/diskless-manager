//! ZFS-related logic for dataset, snapshot, and pool management.

use chrono::Local;

use log::info;

use serde_json::{json, Value};
use std::collections::HashSet;
use std::process::Command;
use tracing::debug;

use crate::cmd::{run_command, run_command_check, run_command_output_no_sudo};
use crate::types::image::CreateImageRequest;
use crate::types::{CreateZpoolRequest, Master, MasterData, Snapshot};
use crate::{
    client::get_clients,
    config::{get_config, get_zpool_name, write_config},
    error::AppError,
    middleware::validate_auth,
    state::AppState,
    types::image::{ArcstatInfo, ZpoolInfo},
    validation::{validate_size, validate_zfs_name},
};
use tauri::State;

// Import the timed execution macro
use crate::timed_execution;

// Check if a ZFS dataset/snapshot exists (returns 0 if exists)
pub fn zfs_exists(dataset: &str) -> bool {
    run_command_check(&["zfs", "list", "-H", dataset]) == 0
}

// Get the OS type of a master image from ZFS property
pub fn get_master_os(master_name: &str) -> Option<String> {
    let output = run_command_output_no_sudo(&[
        "zfs",
        "get",
        "-H",
        "-o",
        "value",
        "org.diskless:os",
        master_name,
    ])
    .ok()?;

    let val = output.trim().to_string();
    if val == "-" || val.is_empty() {
        None
    } else {
        Some(val)
    }
}

// Destroy a ZFS dataset/snapshot
pub fn zfs_destroy(dataset: &str) -> Result<(), AppError> {
    run_command(&["zfs", "destroy", dataset])
}

// Clone a ZFS snapshot to a new dataset
pub fn zfs_clone(snapshot: &str, clone: &str) -> Result<(), AppError> {
    run_command(&["zfs", "clone", snapshot, clone])
}

// Get the writeback dataset path if one exists, otherwise return the default zpool path
pub fn get_writeback_or_default_dataset(client_name: &str) -> String {
    let zpool = get_zpool_name();
    let mut writeback_path = format!("{}/{}-disk", zpool, client_name.to_uppercase()); // Default writeback path if Writeback disk is not set.

    if let Ok(pool_list) =
        run_command_output_no_sudo(&["zfs", "list", "-H", "-o", "name", "-r", &zpool])
    {
        if let Some(parent) = pool_list.lines().filter(|l| !l.is_empty()).find_map(|ds| {
            match run_command_output_no_sudo(&[
                "zfs",
                "get",
                "-H",
                "-o",
                "value",
                "org.diskless:type",
                ds,
            ]) {
                Ok(v) if v.trim() == "writeback" => Some(ds.to_string()),
                _ => None,
            }
        }) {
            // Use found writeback dataset as parent for the clone path (preserve existing naming convention)
            writeback_path = format!("{}/{}-disk", parent, client_name.to_uppercase());
        }
    }

    writeback_path
}

// Parse output of 'zfs list -H -o name,creation,used' (tab-separated)
pub fn parse_zfs_list(output: &str) -> Vec<Snapshot> {
    output
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() == 3 {
                let used_str = parts[2].trim().to_string();
                Some(Snapshot {
                    name: parts[0].trim().to_string(),
                    created: parts[1].trim().to_string(),
                    used: used_str.clone(),
                    size: Some(used_str),
                })
            } else {
                None
            }
        })
        .collect()
}

// Parse property get output (name\tvalue)
fn parse_property_output(output: &str) -> Vec<(String, String)> {
    output
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() == 2 {
                Some((parts[0].trim().to_string(), parts[1].trim().to_string()))
            } else {
                None
            }
        })
        .collect()
}

// Check for dependent clients (returns Ok(Vec<String>) of names if any, Err if fetch fails)
async fn check_dependent_clients(
    state: &State<'_, AppState>,
    base: &str,
    is_snapshot: bool,
    token: &str,
) -> Result<Vec<String>, AppError> {
    let key = if is_snapshot { "snapshot" } else { "master" };
    let clients_result = get_clients(state.clone(), token.to_string(), None).await;
    match clients_result {
        Ok(clients_json) => {
            if let Some(clients) = clients_json.as_array() {
                Ok(clients
                    .iter()
                    .filter(|client| client.get(key) == Some(&json!(base)))
                    .filter_map(|client| {
                        client
                            .get("name")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    })
                    .collect())
            } else {
                Ok(vec![])
            }
        }

        Err(e) => Err(AppError::Internal(format!("Failed to get clients: {}", e))),
    }
}

// Generic ZVOL creation (used by image and game disk)
async fn create_zvol(
    pool: &sqlx::SqlitePool,
    parent: &str,
    basename: &str,
    size: &str,
    zvol_type: &str, // for logging
    os: Option<&str>,
) -> Result<String, AppError> {
    let full_name = format!("{}/{}", parent, basename);
    if zfs_exists(&full_name) {
        return Err(AppError::Validation(format!(
            "ZVOL '{}' already exists.",
            full_name
        )));
    }

    run_command(&[
        "zfs",
        "create",
        "-s",
        "-V",
        size,
        "-o",
        "volblocksize=128K",
        &full_name,
    ])?;

    if let Some(os_type) = os {
        run_command(&[
            "zfs",
            "set",
            &format!("org.diskless:os={}", os_type),
            &full_name,
        ])?;
    }

    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let master_data = MasterData {
        name: full_name.clone(),
        size: size.to_string(),
        snapshots: vec![],
        created_at: now.clone(),
        last_modified: now,
    };
    if !save_master_config(pool, &master_data).await {
        return Err(AppError::Config("Failed to update config.json".to_string()));
    }

    info!("create_{} start: {}", zvol_type, full_name);
    Ok(full_name)
}

pub async fn save_master_config(pool: &sqlx::SqlitePool, master_data: &MasterData) -> bool {
    let mut config = get_config();
    if !config.masters.is_object() {
        config.masters = json!({});
    }
    config.masters[&master_data.name] =
        serde_json::to_value(master_data).expect("Failed to serialize master data");
    match crate::config::write_config(pool, &config).await {
        Ok(_) => true,
        Err(e) => {
            println!("Error saving master config: {}", e);
            false
        }
    }
}

// Ensure parent dataset exists with property
fn ensure_parent_dataset(parent: &str, prop: &str, prop_val: &str) -> Result<(), AppError> {
    if run_command_check(&["zfs", "list", "-H", parent]) != 0 {
        run_command(&[
            "zfs",
            "create",
            "-o",
            &format!("{}={}", prop, prop_val),
            parent,
        ])?;
    }
    Ok(())
}


pub async fn create_image(
    state: tauri::State<'_, crate::state::AppState>,
    request: CreateImageRequest,
) -> Result<Value, AppError> {
    validate_auth(&request.token)?;
    validate_zfs_name(&request.name)?;
    validate_size(&request.size)?;

    let zpool = get_zpool_name();
    // Validate that the ZFS pool exists before querying datasets
    if std::process::Command::new("zpool")
        .args(["list", &zpool])
        .status()
        .map(|s| !s.success())
        .unwrap_or(true)
    {
        return Err(AppError::Config(format!(
            "ZFS pool '{}' not found. Create it (e.g., 'zpool create {} <disk>') or update zpool_name in settings.",
            zpool, zpool
        )));
    }
    let mut parent_dataset = format!("{}/images", zpool);
    eprintln!(
        "=== CREATE_IMAGE: Initial parent_dataset = {}",
        parent_dataset
    );

    // Batch find all parents with org.diskless:type=image
    if let Ok(get_out) = run_command_output_no_sudo(&[
        "zfs",
        "get",
        "-H",
        "-o",
        "name,value",
        "-r",
        "org.diskless:type",
        &zpool,
    ]) {
        let mut image_datasets = vec![];
        for (dataset, val) in parse_property_output(&get_out) {
            debug!("{}: {}", dataset, val);
            eprintln!(
                "=== CREATE_IMAGE: Found dataset {} with type {}",
                dataset, val
            );
            if val == "image" {
                // Only consider datasets that are direct children of zpool
                // e.g., "diskless/images" or "diskless/image-disk"
                // NOT "diskless/images/win11" (that's an image itself, not a parent)
                let parts: Vec<&str> = dataset.split('/').collect();
                eprintln!(
                    "=== CREATE_IMAGE: Dataset {} has {} parts",
                    dataset,
                    parts.len()
                );
                if parts.len() == 2 {
                    // This is a direct child of zpool (zpool/dataset)
                    eprintln!("=== CREATE_IMAGE: Adding {} to image_datasets", dataset);
                    image_datasets.push(dataset);
                }
            }
        }

        debug!("Found {} top-level image datasets", image_datasets.len());
        eprintln!(
            "=== CREATE_IMAGE: Found {} top-level image datasets",
            image_datasets.len()
        );

        // If we found image datasets, use the most recently created one
        if !image_datasets.is_empty() {
            // Get creation times for all image datasets
            if let Ok(creation_out) = run_command_output_no_sudo(
                &["zfs", "get", "-H", "-o", "name,value", "creation"]
                    .iter()
                    .chain(
                        &image_datasets
                            .iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>(),
                    )
                    .cloned()
                    .collect::<Vec<_>>(),
            ) {
                let mut datasets_with_time: Vec<(String, String)> = vec![];
                for (dataset, creation_time) in parse_property_output(&creation_out) {
                    datasets_with_time.push((dataset.clone(), creation_time.clone()));
                    debug!("Dataset {} created at {}", dataset, creation_time);
                    eprintln!(
                        "=== CREATE_IMAGE: Dataset {} created at {}",
                        dataset, creation_time
                    );
                }

                // Sort by creation time (newest first) - ZFS times are sortable as strings
                datasets_with_time.sort_by(|a, b| b.1.cmp(&a.1));

                // Use the most recently created image dataset
                if let Some((newest_dataset, _)) = datasets_with_time.first() {
                    debug!("Selected parent dataset: {}", newest_dataset);
                    eprintln!(
                        "=== CREATE_IMAGE: Selected parent dataset: {}",
                        newest_dataset
                    );
                    parent_dataset = newest_dataset.clone();
                }
            } else {
                // Fallback: if we can't get creation times, use the first one found
                debug!(
                    "Failed to get creation times, using first dataset: {}",
                    image_datasets[0]
                );
                eprintln!(
                    "=== CREATE_IMAGE: Failed to get creation times, using first dataset: {}",
                    image_datasets[0]
                );
                parent_dataset = image_datasets[0].clone();
            }
        } else {
            // No image datasets found - fail the operation instead of using default
            return Err(AppError::Config(
                "No image-disk found. Please create a ZFS dataset with org.diskless:type=image property before creating images.".to_string()
            ));
        }
    }

    eprintln!(
        "=== CREATE_IMAGE: Final parent_dataset = {}",
        parent_dataset
    );

    // Ensure parent exists
    ensure_parent_dataset(&parent_dataset, "org.diskless:type", "image")?;

    let full_name = create_zvol(
        &state.db_pool,
        &parent_dataset,
        &request.name,
        &request.size,
        "image",
        request.os.as_deref(),
    )
    .await?;

    Ok(json!({
        "message": format!("Master ZVOL '{}' created successfully.", full_name),
        "master": {
            "id": full_name.clone(),
            "name": full_name,
            "os": request.os,
            "snapshots": []
        }
    }))
}


pub async fn create_game_disk(
    state: State<'_, AppState>,
    token: String,
    name: String,
    size: String,
) -> Result<Value, AppError> {
    validate_auth(&token)?;
    validate_zfs_name(&name)?;
    validate_size(&size)?;

    let zpool = get_zpool_name();
    let games_parent = format!("{}/games", zpool);
    ensure_parent_dataset(&games_parent, "org.diskless:type", "games")?;

    let basename = format!("{}-games", name); // Append suffix as in original
    let full_name = create_zvol(
        &state.db_pool,
        &games_parent,
        &basename,
        &size,
        "game_disk",
        None,
    )
    .await?;
    Ok(json!({
        "message": format!("Game Disk '{}' created successfully.", full_name),
        "master": {
            "id": full_name.clone(),
            "name": full_name,
            "os": null,
            "snapshots": []
        }
    }))
}


pub async fn get_images(
    state: tauri::State<'_, crate::state::AppState>,
    token: String,
) -> Result<Vec<Master>, AppError> {
    validate_auth(&token)?;

    timed_execution!("get_images", {
        let zpool = get_zpool_name();
        let mut config = get_config();
        let default_master = config
            .settings
            .get("default_master")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();

        // Get fresh datasets and snapshots
        let (all_datasets, all_snaps) = get_fresh_zfs_data(&zpool)?;

        // Collect master_names: non -disk datasets + snapshots of -disk that end with -master
        let mut master_names = vec![];
        for ds in &all_datasets {
            if !ds.name.to_lowercase().ends_with("-disk") {
                master_names.push(ds.name.clone());
            }
        }
        for snap in &all_snaps {
            if let Some(ds_part) = snap.name.split_once('@') {
                if ds_part.0.to_lowercase().ends_with("-disk") {
                    master_names.push(snap.name.clone());
                }
            }
        }
        master_names.sort();
        master_names.dedup();

        // Collect unique parents for batch property check
        let mut unique_parents = HashSet::new();
        for master_name in &master_names {
            if let Some(p) = master_name.rfind('/') {
                unique_parents.insert(master_name[..p].to_string());
            }
        }
        let parent_vec: Vec<&str> = unique_parents.iter().map(|s| s.as_str()).collect();
        let mut image_filter = HashSet::new();
        if !parent_vec.is_empty() {
            if let Ok(get_out) = run_command_output_no_sudo(
                &["zfs", "get", "-H", "-o", "name,value", "org.diskless:type"]
                    .iter()
                    .chain(&parent_vec)
                    .cloned()
                    .collect::<Vec<_>>(),
            ) {
                for (name, val) in parse_property_output(&get_out) {
                    if val == "image" {
                        image_filter.insert(name);
                    }
                }
            }
        }

        // Batch fetch OS type for all masters
        let mut master_os_map = std::collections::HashMap::new();
        if !master_names.is_empty() {
            if let Ok(get_out) = run_command_output_no_sudo(
                &["zfs", "get", "-H", "-o", "name,value", "org.diskless:os"]
                    .iter()
                    .chain(&master_names.iter().map(|s| s.as_str()).collect::<Vec<_>>())
                    .cloned()
                    .collect::<Vec<_>>(),
            ) {
                for (name, val) in parse_property_output(&get_out) {
                    if val != "-" {
                        master_os_map.insert(name, val);
                    }
                }
            }
        }

        // For each master, get snapshots and build Master
        let mut masters_data = vec![];
        for master_name_ref in &master_names {
            let master_name = master_name_ref.clone();
            let is_default = master_name_ref == &default_master;
            let parent = if let Some(p) = master_name_ref.rfind('/') {
                &master_name_ref[..p]
            } else {
                continue;
            };
            if !image_filter.contains(parent) {
                continue;
            }

            // Get snapshots of this master by filtering global list
            let snapshots: Vec<Snapshot> = all_snaps
                .iter()
                .filter(|s| s.name.starts_with(&format!("{}@", master_name_ref)))
                .cloned()
                .collect();

            // Get size from all_datasets ( "-" if not found, e.g., for snapshot masters)
            let size = all_datasets
                .iter()
                .find(|ds| ds.name == *master_name_ref)
                .map(|ds| ds.used.clone())
                .unwrap_or_else(|| "-".to_string());

            let os = master_os_map.get(master_name_ref).cloned();

            masters_data.push(Master {
                id: master_name_ref.clone(),
                name: master_name,
                is_default,
                size,
                os,
                snapshots,
            });
        }

        // Update config.json with the current masters list
        config.masters = serde_json::to_value(&masters_data).unwrap_or(json!({}));
        if let Err(e) = write_config(&state.db_pool, &config).await {
            eprintln!("Error writing masters to config: {}", e);
        }

        Ok(masters_data)
    })
}

// Function to get fresh ZFS data
fn get_fresh_zfs_data(zpool: &str) -> Result<(Vec<Snapshot>, Vec<Snapshot>), AppError> {
    // List all datasets (fs/vol)
    let ds_out = run_command_output_no_sudo(&[
        "zfs",
        "list",
        "-H",
        "-t",
        "filesystem,volume",
        "-o",
        "name,creation,used",
        "-r",
        zpool,
    ])
    .map_err(|e| AppError::Command(format!("Failed to run zfs list: {}", e)))?;
    let all_datasets = parse_zfs_list(&ds_out);

    // List all snapshots once
    let snap_out = run_command_output_no_sudo(&[
        "zfs",
        "list",
        "-H",
        "-t",
        "snapshot",
        "-o",
        "name,creation,used",
        "-r",
        zpool,
    ])
    .map_err(|e| AppError::Command(format!("Failed to list snapshots: {}", e)))?;
    let all_snaps = parse_zfs_list(&snap_out);

    Ok((all_datasets, all_snaps))
}

async fn common_delete(
    state: &State<'_, AppState>,
    token: String,
    entity: &str,
    is_snapshot: bool,
) -> Result<Value, AppError> {
    eprintln!("=== common_delete AUTH START: {}", entity);
    validate_auth(&token)?;
    eprintln!("=== common_delete AUTH OK");

    eprintln!("=== common_delete DEPENDENTS START");
    let dependents = check_dependent_clients(state, entity, is_snapshot, &token)
        .await
        .map_err(|e| {
            eprintln!("=== common_delete DEPENDENTS ERR: {}", e);
            AppError::Internal(format!("Failed to check clients: {e}"))
        })?;
    eprintln!(
        "=== common_delete DEPENDENTS OK: {} found",
        dependents.len()
    );

    if !dependents.is_empty() {
        eprintln!("=== common_delete BLOCKED BY DEPENDENTS");
        return Ok(json!({
            "error": "Entity has dependent clients",
            "message": format!("Cannot delete – used by: {}", dependents.join(", ")),
            "dependent_clients": dependents
        }));
    }

    eprintln!("=== common_delete ZFS DESTROY START");
    match run_command(&["zfs", "destroy", entity]) {
        Ok(()) => {
            eprintln!("=== common_delete ZFS DESTROY OK");
            if !is_snapshot {
                let _ = delete_image_config(&state.db_pool, entity).await;
            }
            info!(
                "delete_{}: {}",
                if is_snapshot { "snapshot" } else { "image" },
                entity
            );
            Ok(json!({
                "message": format!("{} {} deleted successfully", if is_snapshot { "Snapshot" } else { "Master" }, entity)
            }))
        }
        Err(err) => {
            let stderr = err.to_string();
            eprintln!("=== common_delete ZFS DESTROY ERR: {}", stderr);
            if stderr.contains("has dependent clones") {
                Ok(json!({
                    "error": "Entity has dependent clones",
                    "message": format!("Cannot delete '{}': dependent clones exist.", entity)
                }))
            } else {
                Ok(json!({
                    "error": "ZFS destroy failed",
                    "message": format!("Failed for '{}': {}", entity, stderr.trim())
                }))
            }
        }
    }
}

// Enhanced: Clear default if deleted master was default
pub async fn delete_image_config(pool: &sqlx::SqlitePool, master_name: &str) -> bool {
    let mut config = get_config();
    let was_default = config
        .settings
        .get("default_master")
        .and_then(|s| s.as_str())
        == Some(master_name);

    if let Some(masters) = config.masters.as_object_mut() {
        if masters.remove(master_name).is_some() {
            // NEW: Clear default if this was the default
            if was_default {
                if !config.settings.is_object() {
                    config.settings = json!({});
                }
                config.settings["default_master"] = json!(null); // Or json!("")
                eprintln!("=== delete_image_config: Cleared default_master after delete");
            }

            if let Err(e) = write_config(pool, &config).await {
                eprintln!("Error writing config file: {}", e);
                return false;
            }
            return true;
        }
    }
    true // Nothing deleted, but success
}


pub async fn delete_image(
    state: State<'_, AppState>,
    token: String,
    master_name: String,
) -> Result<serde_json::Value, AppError> {
    eprintln!("=== delete_image START: {}", master_name); // Terminal log
    let result = common_delete(&state, token, &master_name, false).await;

    eprintln!("=== delete_image END: {:?}", result); // Will show if it completes
    result
}

// Create a ZFS snapshot


pub async fn create_snapshot(
    state: State<'_, AppState>,
    token: String,
    snapshot_name: String,
) -> Result<Value, AppError> {
    // Validate authentication token
    validate_auth(&token)?;
    let zpool_name = get_zpool_name();

    // Basic format check
    if !snapshot_name.contains('@') || !snapshot_name.starts_with(&format!("{}/", zpool_name)) {
        return Err(AppError::Validation(format!(
            "Invalid snapshot name. Expected {}/master@snapname",
            zpool_name
        )));
    }

    // Validate the snapshot part after @
    if let Some(snap_part) = snapshot_name.split('@').nth(1) {
        crate::validation::validate_snapshot_name(snap_part)?;
    }

    let master_name = snapshot_name
        .split('@')
        .next()
        .expect("Invalid snapshot name format: missing '@'");
    let status_code = run_command_check(&["zfs", "list", "-H", master_name]);
    if status_code != 0 {
        return Err(AppError::NotFound(format!(
            "Master '{}' not found.",
            master_name
        )));
    }
    let output = Command::new("sudo")
        .args(["zfs", "snapshot", &snapshot_name])
        .output()
        .map_err(|e| AppError::Command(format!("Failed to run zfs snapshot: {}", e)))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("dataset already exists") {
            return Err(AppError::Validation(format!(
                "Snapshot '{}' already exists.",
                snapshot_name
            )));
        } else {
            return Err(AppError::Command(format!(
                "Failed creating snapshot: {}",
                stderr
            )));
        }
    }
    let mut config = get_config();
    if let Some(masters) = config.masters.as_object_mut() {
        if let Some(master) = masters.get_mut(master_name) {
            // Avoid double mutable borrow by splitting the logic
            if master.get("snapshots").and_then(|s| s.as_array()).is_none() {
                master["snapshots"] = json!([]);
            }
            let snapshots = master
                .get_mut("snapshots")
                .and_then(|s| s.as_array_mut())
                .expect("snapshots should be an array after initialization");
            if !snapshots.iter().any(|v| v == &json!(snapshot_name)) {
                snapshots.push(json!(snapshot_name));
            }
            write_config(&state.db_pool, &config)
                .await
                .map_err(AppError::Config)?;
        }
    }

    Ok(json!({
        "message": format!("Snapshot {} created", snapshot_name)
    }))
}


pub async fn delete_snapshot(
    state: State<'_, AppState>,
    token: String,
    snapshot_name: String,
) -> Result<serde_json::Value, AppError> {
    eprintln!("=== delete_snapshot START: {}", snapshot_name);
    let zpool = get_zpool_name();
    if !snapshot_name.contains('@') || !snapshot_name.starts_with(&format!("{}/", &zpool)) {
        return Err(AppError::Validation(
            "Invalid snapshot name format.".to_string(),
        ));
    }
    let result = common_delete(&state, token, &snapshot_name, true).await;

    eprintln!("=== delete_snapshot END: {:?}", result);
    result
}


pub fn get_zpool_list() -> Vec<ZpoolInfo> {
    let mut pools = vec![];

    if let Ok(out) =
        run_command_output_no_sudo(&["zpool", "list", "-H", "-o", "name,size,alloc,free,health"])
    {
        for line in out.lines() {
            let parts: Vec<&str> = line.split_whitespace().map(|s| s.trim()).collect();
            if parts.len() >= 5 {
                pools.push(ZpoolInfo {
                    name: parts[0].to_string(),
                    size: parts[1].to_string(),
                    alloc: parts[2].to_string(),
                    free: parts[3].to_string(),
                    health: parts[4].to_string(),
                });
            }
        }
    }

    pools
}


pub async fn zfs_pool_exists(
    state: State<'_, AppState>,
    pool_name: Option<String>,
) -> Result<bool, AppError> {
    if let Some(pool) = pool_name {
        let status = Command::new("zpool")
            .args(["list", &pool])
            .status()
            .map_err(AppError::Io)?;

        let exists = status.success();
        if exists {
            let mut config = get_config();
            if !config.settings.is_object() {
                config.settings = json!({});
            }
            config.settings["zpool_name"] = json!(pool.clone());
            config.settings["zfsPool"] = json!(pool);
            let _ = write_config(&state.db_pool, &config).await;
        }
        Ok(exists)
    } else {
        let output = Command::new("zpool")
            .args(["list", "-H"])
            .output()
            .map_err(AppError::Io)?;
        Ok(output.status.success() && !output.stdout.is_empty())
    }
}


pub async fn create_zfs_pool(
    state: State<'_, AppState>,
    req: CreateZpoolRequest,
) -> Result<(), AppError> {
    let status = run_command(&["zpool", "create", &req.name, &format!("/dev/{}", req.disk)]);
    if status.is_err() {
        return Err(AppError::Command("Failed to create ZFS pool".to_string()));
    }

    let mut config = get_config();
    let mut settings = config.settings.as_object().cloned().unwrap_or_default();
    settings.insert("zpool_name".to_string(), json!(req.name.clone()));
    settings.insert("zfsPool".to_string(), json!(req.name));
    config.settings = json!(settings);
    write_config(&state.db_pool, &config).await.map_err(|e| {
        AppError::Config(format!(
            "ZFS pool created, but failed to update config: {}",
            e
        ))
    })?;

    Ok(())
}


pub async fn set_default_image(
    state: State<'_, AppState>,
    token: String,
    master_name: String,
) -> Result<Value, AppError> {
    validate_auth(&token)?;
    let mut config = get_config();
    if !config.settings.is_object() {
        config.settings = json!({});
    }
    config.settings["default_master"] = json!(master_name);
    write_config(&state.db_pool, &config)
        .await
        .map_err(AppError::Config)?;
    Ok(json!({"message": "Successfully set default image"}))
}


pub async fn rollback_image_snapshot(
    state: State<'_, AppState>,
    token: String,
    _master_name: String,
    snapshot_name: String,
) -> Result<serde_json::Value, AppError> {
    validate_auth(&token)?;

    // Rollback the snapshot (destroys newer snapshots and their clones)
    if let Err(e) = run_command(&["zfs", "rollback", "-r", &snapshot_name]) {
        return Err(AppError::Command(format!(
            "Failed to rollback snapshot: {}",
            e
        )));
    }

    // Get clients and recreate clones for those using exactly this snapshot
    let clients_result = get_clients(state, token, None).await;
    let mut recreated = vec![];
    if let Ok(clients_json) = clients_result {
        if let Some(clients) = clients_json.as_array() {
            for client in clients {
                let client_snapshot = client
                    .get("snapshot")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let client_id = client.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let client_name = client.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let client_clone = get_writeback_or_default_dataset(client_name);
                if client_snapshot == snapshot_name {
                    let _ = zfs_destroy(&client_clone);
                    if zfs_clone(&snapshot_name, &client_clone).is_ok() {
                        recreated.push(client_id.to_string());
                    }
                }
            }
        }
    }

    Ok(json!({
        "message": format!("Rolled back snapshot {} and re-created {} clones", snapshot_name, recreated.len()),
        "recreated_clones": recreated
    }))
}


pub async fn get_zfs_arcstat() -> Result<ArcstatInfo, AppError> {
    use std::fs;
    let arcstat_path = "/proc/spl/kstat/zfs/arcstats";
    let content = fs::read_to_string(arcstat_path).map_err(AppError::Io)?;
    let mut hits = 0u64;
    let mut misses = 0u64;
    let mut size = 0u64;
    for line in content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            match parts[0] {
                "hits" => hits = parts[1].parse().unwrap_or(0),
                "misses" => misses = parts[1].parse().unwrap_or(0),
                "size" => size = parts[1].parse().unwrap_or(0),
                _ => {}
            }
        }
    }
    let hit_percent = if hits + misses > 0 {
        let percent = (hits as f64 / (hits + misses) as f64) * 100.0;
        (percent * 100.0).round() / 100.0
    } else {
        0.0
    };
    Ok(ArcstatInfo { size, hit_percent })
}


pub async fn get_default_image_overview(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, AppError> {
    let config = get_config(); // Use cached config
    let master_dataset = config
        .settings
        .get("default_master")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();

    // For empty dataset, return immediately
    if master_dataset.is_empty() {
        return Ok(json!({
            "name": null,
            "creation_date": null,
            "clones": null,
            "message": "No default master image set"
        }));
    }

    // Check if the dataset exists
    if !zfs_exists(&master_dataset) {
        // Clear the invalid default_master from config
        let mut config = get_config();
        if !config.settings.is_object() {
            config.settings = json!({});
        }
        config.settings["default_master"] = json!(null);
        let _ = write_config(&state.db_pool, &config).await;

        let overview = json!({
            "name": null,
            "creation_date": null,
            "clones": null,
            "message": format!("Default master image '{}' no longer exists and has been cleared from config", master_dataset)
        });

        return Ok(overview);
    }

    // Dataset exists, get its information
    let output = run_command_output_no_sudo(&[
        "zfs",
        "get",
        "creation,clones",
        "-o",
        "value",
        "-H",
        &master_dataset,
    ])?;

    let lines: Vec<&str> = output.lines().collect();
    if lines.len() < 2 {
        return Err(AppError::Internal(
            "Unexpected output from zfs get".to_string(),
        ));
    }

    let overview = json!({
        "name": master_dataset,
        "creation_date": lines[0],
        "clones": lines[1]
    });

    Ok(overview)
}


pub async fn rename_image(
    state: State<'_, AppState>,
    token: String,
    old_name: String,
    new_name: String,
) -> Result<serde_json::Value, AppError> {
    validate_auth(&token)?;
    validate_zfs_name(&new_name)?;

    if !zfs_exists(&old_name) {
        return Err(AppError::NotFound(format!(
            "Master '{}' not found.",
            old_name
        )));
    }

    let zpool = get_zpool_name();
    let parent = if let Some(pos) = old_name.rfind('/') {
        old_name[..pos].to_string()
    } else {
        zpool.clone()
    };
    let new_master_zvol_name = format!("{}/{}", parent, new_name);

    if zfs_exists(&new_master_zvol_name) {
        return Err(AppError::Validation(format!(
            "Master '{}' already exists.",
            new_master_zvol_name
        )));
    }

    let dependents = check_dependent_clients(&state, &old_name, false, &token).await?;
    if !dependents.is_empty() {
        return Ok(json!({
            "error": "Master has dependent clients",
            "message": format!(
                "Cannot rename master: It is being used by the following clients: {}",
                dependents.join(", ")
            ),
            "dependent_clients": dependents
        }));
    }

    if let Err(err) = run_command(["zfs", "rename", &old_name, &new_master_zvol_name]) {
        let stderr = err.to_string();
        if stderr.contains("has dependent clones") {
            return Ok(json!({
                "error": "Master has dependent clones",
                "message": format!("Cannot rename master '{}': It has dependent clones.", old_name)
            }));
        } else {
            return Ok(json!({
                "error": format!("Failed to rename master: {}", stderr)
            }));
        }
    }

    let mut config = get_config(); // Get config here
    if config.settings.get("default_master") == Some(&json!(old_name)) {
        config.settings["default_master"] = json!(new_master_zvol_name.clone());
        let _ = write_config(&state.db_pool, &config).await;
    }

    if let Some(masters) = config.masters.as_object_mut() {
        if let Some(master_data) = masters.remove(&old_name) {
            masters.insert(new_master_zvol_name.clone(), master_data);
            let _ = write_config(&state.db_pool, &config).await;
        }
    }

    Ok(json!({
        "message": format!("Master renamed from '{}' to '{}' successfully", old_name, new_master_zvol_name)
    }))
}

// Get latest snapshot for a master dataset
pub fn get_latest_snapshot(master_name: &str) -> Result<String, AppError> {
    debug!("Looking for snapshots of master: {}", master_name);

    // Get all snapshots for the master image, sorted by creation time
    // Try without -r flag first, then with it if needed
    let stdout = match run_command_output_no_sudo([
        "zfs",
        "list",
        "-H",
        "-t",
        "snapshot",
        "-o",
        "name,creation",
        master_name,
    ]) {
        Ok(output) => output,
        Err(e) => {
            debug!("First attempt failed: {}", e);

            // Try with -r flag as fallback
            match run_command_output_no_sudo([
                "zfs",
                "list",
                "-H",
                "-t",
                "snapshot",
                "-o",
                "name,creation",
                "-r",
                master_name,
            ]) {
                Ok(output) => output,
                Err(e) => {
                    return Err(AppError::Command(format!(
                        "Failed to list snapshots for {}: {}",
                        master_name, e
                    )));
                }
            }
        }
    };

    debug!("ZFS list output for {}: {}", master_name, stdout);

    let snapshots: Vec<(String, u64)> = stdout
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 2 {
                let name = parts[0].to_string();
                let creation = parts[1].parse::<u64>().ok()?;
                debug!("Found snapshot: {} (creation: {})", name, creation);
                Some((name, creation))
            } else {
                debug!("Skipping malformed line: {}", line);
                None
            }
        })
        .collect();

    if snapshots.is_empty() {
        return Err(AppError::NotFound(format!(
            "No snapshots found for master {}",
            master_name
        )));
    }

    // Find the snapshot with the highest creation timestamp (latest)
    let latest = snapshots
        .into_iter()
        .max_by_key(|(_, creation)| *creation)
        .ok_or_else(|| AppError::NotFound("No valid snapshots found".to_string()))?;

    debug!("Selected latest snapshot: {}", latest.0);
    Ok(latest.0)
}

// Get all snapshots for a dataset
pub fn get_snapshots_for_dataset(dataset: &str) -> Result<Vec<Snapshot>, AppError> {
    debug!("Getting snapshots for dataset: {}", dataset);

    let output = match run_command_output_no_sudo([
        "zfs",
        "list",
        "-H",
        "-t",
        "snapshot",
        "-o",
        "name,creation,used",
        "-r",
        dataset,
    ]) {
        Ok(output) => output,
        Err(_) => {
            // If the above fails, try listing snapshots of the dataset specifically
            run_command_output_no_sudo([
                "zfs",
                "list",
                "-H",
                "-t",
                "snapshot",
                "-o",
                "name,creation,used",
                dataset,
            ])?
        }
    };

    let snapshots = parse_zfs_list(&output);
    debug!(
        "Found {} snapshots for dataset {}",
        snapshots.len(),
        dataset
    );

    Ok(snapshots)
}
