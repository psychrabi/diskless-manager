//! ZFS-related logic for dataset, snapshot, and pool management.

use chrono::Local;

use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::process::Command;
use std::sync::{Arc, RwLock};
use tracing::debug;

use crate::types::image::CreateImageRequest;
use crate::types::{CreateZpoolRequest, Master, MasterData, Snapshot};
use crate::utils::{append_log, run_command, run_command_check, run_command_output_no_sudo};
use crate::{
    client::get_clients,
    config::{get_config, get_zpool_name, write_config},
    error::AppError,
    middleware::validate_auth_token_for_command,
    types::image::{ArcstatInfo, ZpoolInfo},
};

// Import the timed execution macro
use crate::timed_execution;

// Cache for ZFS datasets and snapshots to reduce system calls
use once_cell::sync::Lazy;

static ZFS_CACHE: Lazy<Arc<RwLock<ZfsCache>>> =
    Lazy::new(|| Arc::new(RwLock::new(ZfsCache::new())));

#[derive(Debug, Clone)]
struct ZfsCache {
    datasets: Vec<Snapshot>,
    snapshots: Vec<Snapshot>,
    last_updated: std::time::SystemTime,
    ttl: std::time::Duration,
}

impl ZfsCache {
    fn new() -> Self {
        ZfsCache {
            datasets: Vec::new(),
            snapshots: Vec::new(),
            last_updated: std::time::SystemTime::UNIX_EPOCH,
            ttl: std::time::Duration::from_secs(30), // 30 second cache TTL
        }
    }

    fn is_fresh(&self) -> bool {
        self.last_updated
            .elapsed()
            .map(|elapsed| elapsed < self.ttl)
            .unwrap_or(false)
    }

    fn needs_refresh(&self) -> bool {
        !self.is_fresh()
    }
}

// Helper to validate auth token, returning Err if invalid
fn validate_auth(token: &str) -> Result<(), AppError> {
    validate_auth_token_for_command(token)
        .map(|_| ())
        .map_err(|e| AppError::Auth(e.message))
}

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

// Parse output of 'zfs list -H -o name,creation,used' (tab-separated)
pub fn parse_zfs_list(output: &str) -> Vec<Snapshot> {
    output
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() == 3 {
                Some(Snapshot {
                    name: parts[0].trim().to_string(),
                    created: parts[1].trim().to_string(),
                    used: parts[2].trim().to_string(),
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
    base: &str,
    is_snapshot: bool,
    token: &str,
) -> Result<Vec<String>, AppError> {
    let key = if is_snapshot { "snapshot" } else { "master" };
    let clients_result = get_clients(token.to_string(), None).await;
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

// Validate dataset name format
fn validate_name(name: &str) -> Result<(), AppError> {
    let re = Regex::new(r"^[\w-]+$").unwrap();
    if !re.is_match(name) || name.contains(' ') || name.contains('/') {
        Err(AppError::Validation(
            "Invalid name format (alphanumeric, _, -; no spaces or /)".to_string(),
        ))
    } else {
        Ok(())
    }
}

// Validate size format (e.g., 50G)
fn validate_size(size: &str) -> Result<(), AppError> {
    let upper = size.to_uppercase();
    let re = Regex::new(r"^\d+[KMGTP]$").unwrap();
    if re.is_match(&upper) {
        Ok(())
    } else {
        Err(AppError::Validation(
            "Invalid size format (e.g., '50G')".to_string(),
        ))
    }
}

// Generic ZVOL creation (used by image and game disk)
fn create_zvol(
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
    if !save_master_config(&master_data) {
        return Err(AppError::Config("Failed to update config.json".to_string()));
    }

    append_log(
        "INFO",
        &format!("create_{} start: {}", zvol_type, full_name),
    );
    Ok(full_name)
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

#[tauri::command]
pub fn create_image(request: CreateImageRequest) -> Result<Value, AppError> {
    validate_auth(&request.token)?;
    validate_name(&request.name)?;
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
            debug!(
                "No top-level image datasets found, using default: {}",
                parent_dataset
            );
            eprintln!(
                "=== CREATE_IMAGE: No top-level image datasets found, using default: {}",
                parent_dataset
            );
        }
    }

    eprintln!(
        "=== CREATE_IMAGE: Final parent_dataset = {}",
        parent_dataset
    );

    // Ensure parent exists
    ensure_parent_dataset(&parent_dataset, "org.diskless:type", "image")?;

    let full_name = create_zvol(
        &parent_dataset,
        &request.name,
        &request.size,
        "image",
        request.os.as_deref(),
    )?;
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

#[tauri::command]
pub fn create_game_disk(token: String, name: String, size: String) -> Result<Value, AppError> {
    validate_auth(&token)?;
    validate_name(&name)?;
    validate_size(&size)?;

    let zpool = get_zpool_name();
    let games_parent = format!("{}/games", zpool);
    ensure_parent_dataset(&games_parent, "org.diskless:type", "games")?;

    let basename = format!("{}-games", name); // Append suffix as in original
    let full_name = create_zvol(&games_parent, &basename, &size, "game_disk", None)?;
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

#[tauri::command]
pub async fn get_images(token: String) -> Result<Vec<Master>, AppError> {
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

        // Get datasets and snapshots using cache
        let (all_datasets, all_snaps) = get_cached_zfs_data(&zpool)?;

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
        if let Err(e) = write_config(&config) {
            eprintln!("Error writing masters to config: {}", e);
        }

        Ok(masters_data)
    })
}

// Function to get ZFS data with caching
fn get_cached_zfs_data(zpool: &str) -> Result<(Vec<Snapshot>, Vec<Snapshot>), AppError> {
    let cache = ZFS_CACHE.read().unwrap();

    if !cache.needs_refresh() {
        // Return cached data
        return Ok((cache.datasets.clone(), cache.snapshots.clone()));
    }

    // Drop read lock before acquiring write lock
    drop(cache);

    let mut cache = ZFS_CACHE.write().unwrap();

    // Double-check after acquiring write lock
    if !cache.needs_refresh() {
        // Another thread may have updated the cache while we were waiting
        return Ok((cache.datasets.clone(), cache.snapshots.clone()));
    }

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

    // Update cache
    cache.datasets = all_datasets;
    cache.snapshots = all_snaps;
    cache.last_updated = std::time::SystemTime::now();

    Ok((cache.datasets.clone(), cache.snapshots.clone()))
}

pub fn save_master_config(master_data: &MasterData) -> bool {
    let mut config = get_config();
    if !config.masters.is_object() {
        config.masters = json!({});
    }
    config.masters[&master_data.name] = serde_json::to_value(master_data).unwrap();
    match write_config(&config) {
        Ok(_) => true,
        Err(e) => {
            println!("Error saving master config: {}", e);
            false
        }
    }
}

async fn common_delete(token: String, entity: &str, is_snapshot: bool) -> Result<Value, AppError> {
    eprintln!("=== common_delete AUTH START: {}", entity);
    validate_auth(&token)?;
    eprintln!("=== common_delete AUTH OK");

    eprintln!("=== common_delete DEPENDENTS START");
    let dependents = check_dependent_clients(entity, is_snapshot, &token)
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
                let _ = delete_image_config(entity);
            }
            append_log(
                "INFO",
                &format!(
                    "delete_{}: {}",
                    if is_snapshot { "snapshot" } else { "image" },
                    entity
                ),
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
pub fn delete_image_config(master_name: &str) -> bool {
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

            if let Err(e) = write_config(&config) {
                eprintln!("Error writing config file: {}", e);
                return false;
            }
            return true;
        }
    }
    true // Nothing deleted, but success
}

#[tauri::command]
pub async fn delete_image(
    token: String,
    master_name: String,
) -> Result<serde_json::Value, AppError> {
    eprintln!("=== delete_image START: {}", master_name); // Terminal log
    let result = common_delete(token, &master_name, false).await;
    eprintln!("=== delete_image END: {:?}", result); // Will show if it completes
    result
}

// Create a ZFS snapshot

#[tauri::command]
pub fn create_snapshot(token: String, snapshot_name: String) -> Result<Value, String> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;
    let zpool_name = get_zpool_name();
    if !snapshot_name.contains('@') || !snapshot_name.starts_with(&format!("{}/", zpool_name)) {
        return Err(format!(
            "Invalid snapshot name. Expected {}/master@snapname",
            zpool_name
        ));
    }
    let master_name = snapshot_name.split('@').next().unwrap();
    let status_code = run_command_check(&["zfs", "list", "-H", master_name]);
    if status_code != 0 {
        return Err(format!("Master '{}' not found.", master_name));
    }
    let output = Command::new("sudo")
        .args(["zfs", "snapshot", &snapshot_name])
        .output()
        .map_err(|e| format!("Failed to run zfs snapshot: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("dataset already exists") {
            return Err(format!("Snapshot '{}' already exists.", snapshot_name));
        } else {
            return Err(format!("Failed creating snapshot: {}", stderr));
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
            write_config(&config).map_err(|e| format!("Failed to write config: {}", e))?;
        }
    }
    Ok(json!({
        "message": format!("Snapshot {} created", snapshot_name)
    }))
}

#[tauri::command]
pub async fn delete_snapshot(token: String, snapshot_name: String) -> Result<Value, AppError> {
    validate_auth(&token)?;

    let zpool = get_zpool_name();
    if !snapshot_name.contains('@') || !snapshot_name.starts_with(&format!("{}/", &zpool)) {
        return Err(AppError::Validation(
            "Invalid snapshot name format.".to_string(),
        ));
    }

    common_delete(token, &snapshot_name, true).await
}

#[tauri::command]
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

#[tauri::command]
pub fn zfs_pool_exists(pool_name: Option<String>) -> Result<bool, AppError> {
    if pool_name.is_none() {
        let output = Command::new("zpool")
            .args(["list", "-H"])
            .output()
            .map_err(|e| AppError::Io(e))?;
        Ok(output.status.success() && !output.stdout.is_empty())
    } else {
        let pool = pool_name.unwrap();
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
            let _ = write_config(&config);
        }
        Ok(exists)
    }
}

#[tauri::command]
pub fn create_zfs_pool(req: CreateZpoolRequest) -> Result<(), AppError> {
    let status = run_command(&["zpool", "create", &req.name, &format!("/dev/{}", req.disk)]);
    if status.is_err() {
        return Err(AppError::Command("Failed to create ZFS pool".to_string()));
    }

    let mut config = get_config();
    let mut settings = config.settings.as_object().cloned().unwrap_or_default();
    settings.insert("zpool_name".to_string(), json!(req.name.clone()));
    settings.insert("zfsPool".to_string(), json!(req.name));
    config.settings = json!(settings);
    write_config(&config).map_err(|e| {
        AppError::Config(format!(
            "ZFS pool created, but failed to update config: {}",
            e
        ))
    })?;

    Ok(())
}

#[tauri::command]
pub fn set_default_image(token: String, name: &str) -> Result<bool, AppError> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| AppError::Auth(e.message))?;
    let mut config = get_config();
    if !config.settings.is_object() {
        config.settings = json!({});
    }
    config.settings["default_master"] = Value::String(name.to_string());
    match write_config(&config) {
        Ok(_) => {
            // Invalidate cache since we've changed the default image
            invalidate_default_image_cache();
            Ok(true)
        }
        Err(e) => {
            println!("Error saving default master: {}", e);
            Err(AppError::Config(format!(
                "Error saving default master: {}",
                e
            )))
        }
    }
}

#[tauri::command]
pub async fn rollback_image_snapshot(
    token: String,
    _master_name: String,
    snapshot_name: String,
) -> Result<serde_json::Value, AppError> {
    validate_auth(&token)?;

    // Rollback the snapshot (destroys newer snapshots and their clones)
    if let Err(e) = run_command(["zfs", "rollback", "-r", &snapshot_name]) {
        return Err(AppError::Command(format!(
            "Failed to rollback snapshot: {}",
            e
        )));
    }

    // Get clients and recreate clones for those using exactly this snapshot
    let clients_result = get_clients("".to_string(), None).await;
    let mut recreated = vec![];
    if let Ok(clients_json) = clients_result {
        if let Some(clients) = clients_json.as_array() {
            let zpool = get_zpool_name();
            for client in clients {
                let client_snapshot = client
                    .get("snapshot")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let client_id = client.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let client_name = client.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let client_clone = format!("{}/{}-disk", zpool, client_name.to_uppercase());
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

#[tauri::command]
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

static DEFAULT_IMAGE_CACHE: Lazy<Arc<RwLock<DefaultImageCache>>> =
    Lazy::new(|| Arc::new(RwLock::new(DefaultImageCache::new())));

#[derive(Debug, Clone)]
struct DefaultImageCache {
    overview: serde_json::Value,
    last_updated: std::time::SystemTime,
    ttl: std::time::Duration,
}

impl DefaultImageCache {
    fn new() -> Self {
        DefaultImageCache {
            overview: serde_json::json!({
                "name": null,
                "creation_date": null,
                "clones": null,
                "message": "No default master image set"
            }),
            last_updated: std::time::SystemTime::UNIX_EPOCH,
            ttl: std::time::Duration::from_secs(30), // 30 second cache TTL
        }
    }

    fn is_fresh(&self) -> bool {
        self.last_updated
            .elapsed()
            .map(|elapsed| elapsed < self.ttl)
            .unwrap_or(false)
    }

    fn needs_refresh(&self) -> bool {
        !self.is_fresh()
    }
}

#[tauri::command]
pub async fn get_default_image_overview() -> Result<serde_json::Value, AppError> {
    let config = get_config(); // Use cached config
    let master_dataset = config
        .settings
        .get("default_master")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();

    // For empty dataset, return immediately without caching
    if master_dataset.is_empty() {
        return Ok(json!({
            "name": null,
            "creation_date": null,
            "clones": null,
            "message": "No default master image set"
        }));
    }

    // Try to get cached data first
    {
        let cache = DEFAULT_IMAGE_CACHE.read().unwrap();

        if !cache.needs_refresh() {
            // Return cached data
            return Ok(cache.overview.clone());
        }
    } // Drop read lock

    let mut cache = DEFAULT_IMAGE_CACHE.write().unwrap();

    // Double-check after acquiring write lock
    if !cache.needs_refresh() {
        // Another thread may have updated the cache while we were waiting
        return Ok(cache.overview.clone());
    }

    // Check if the dataset exists
    if !zfs_exists(&master_dataset) {
        // Clear the invalid default_master from config
        let mut config = get_config();
        if !config.settings.is_object() {
            config.settings = json!({});
        }
        config.settings["default_master"] = json!(null);
        let _ = write_config(&config);

        let overview = json!({
            "name": null,
            "creation_date": null,
            "clones": null,
            "message": format!("Default master image '{}' no longer exists and has been cleared from config", master_dataset)
        });

        // Update cache
        cache.overview = overview.clone();
        cache.last_updated = std::time::SystemTime::now();

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

    // Update cache
    cache.overview = overview.clone();
    cache.last_updated = std::time::SystemTime::now();

    Ok(overview)
}

// Function to invalidate the default image cache (to be called when operations change the default image)
pub fn invalidate_default_image_cache() {
    if let Ok(mut cache) = DEFAULT_IMAGE_CACHE.try_write() {
        cache.last_updated = std::time::SystemTime::UNIX_EPOCH; // Force refresh next time
    }
}

#[tauri::command]
pub async fn rename_image(
    token: String,
    old_name: String,
    new_name: String,
) -> Result<serde_json::Value, AppError> {
    validate_auth(&token)?;
    validate_name(&new_name)?;

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

    let dependents = check_dependent_clients(&old_name, false, &token).await?;
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

    // Update config if default
    let mut config = get_config();
    if config.settings.get("default_master") == Some(&json!(old_name)) {
        config.settings["default_master"] = json!(new_master_zvol_name.clone());
        let _ = write_config(&config);
    }

    // Update master entry
    if let Some(masters) = config.masters.as_object_mut() {
        if let Some(master_data) = masters.remove(&old_name) {
            masters.insert(new_master_zvol_name.clone(), master_data);
            let _ = write_config(&config);
        }
    }

    Ok(json!({
        "message": format!("Master renamed from '{}' to '{}' successfully", old_name, new_master_zvol_name)
    }))
}
