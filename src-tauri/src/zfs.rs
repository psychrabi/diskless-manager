//! ZFS-related logic for dataset, snapshot, and pool management.

use chrono::Local;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::process::Command;

use crate::utils::{append_log, run_command, run_command_check, run_command_output};
use crate::{
    client::get_clients,
    config::{get_config, get_zpool_name, write_config},
    middleware::validate_auth_token_for_command,
};

#[derive(Serialize, Deserialize, Clone)]
pub struct Master {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub size: String,
    pub snapshots: Vec<Snapshot>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Snapshot {
    pub name: String,
    pub created: String,
    pub used: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct MasterData {
    name: String,
    size: String,
    snapshots: Vec<String>,
    created_at: String,
    last_modified: String,
}

// Helper to validate auth token, returning Err if invalid
fn validate_auth(token: &str) -> Result<(), String> {
    validate_auth_token_for_command(token)
        .map(|_| ())
        .map_err(|e| format!("Authentication failed: {}", e.message))
}

// Check if a ZFS dataset/snapshot exists (returns 0 if exists)
pub fn zfs_exists(dataset: &str) -> bool {
    run_command_check(&["zfs", "list", "-H", dataset]) == 0
}

// Destroy a ZFS dataset/snapshot
pub fn zfs_destroy(dataset: &str) -> Result<(), String> {
    run_command(&["zfs", "destroy", dataset])
}

// Clone a ZFS snapshot to a new dataset
pub fn zfs_clone(snapshot: &str, clone: &str) -> Result<(), String> {
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
                Some((
                    parts[0].trim().to_string(),
                    parts[1].trim().to_string(),
                ))
            } else {
                None
            }
        })
        .collect()
}

// Check for dependent clients (returns Ok(Vec<String>) of names if any, Err if fetch fails)
async fn check_dependent_clients(base: &str, is_snapshot: bool) -> Result<Vec<String>, String> {
    let key = if is_snapshot { "snapshot" } else { "master" };
    let clients_result = get_clients("".to_string(), None).await;
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
        Err(e) => Err(format!("Failed to get clients: {}", e)),
    }
}

// Validate dataset name format
fn validate_name(name: &str) -> Result<(), String> {
    let re = Regex::new(r"^[\w-]+$").unwrap();
    if !re.is_match(name) || name.contains(' ') || name.contains('/') {
        Err("Invalid name format (alphanumeric, _, -; no spaces or /)".to_string())
    } else {
        Ok(())
    }
}

// Validate size format (e.g., 50G)
fn validate_size(size: &str) -> Result<(), String> {
    let upper = size.to_uppercase();
    let re = Regex::new(r"^\d+[KMGTP]$").unwrap();
    if re.is_match(&upper) {
        Ok(())
    } else {
        Err("Invalid size format (e.g., '50G')".to_string())
    }
}

// Generic ZVOL creation (used by image and game disk)
fn create_zvol(
    parent: &str,
    basename: &str,
    size: &str,
    zvol_type: &str, // for logging
) -> Result<String, String> {
    let full_name = format!("{}/{}", parent, basename);
    if zfs_exists(&full_name) {
        return Err(format!("ZVOL '{}' already exists.", full_name));
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

    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let master_data = MasterData {
        name: full_name.clone(),
        size: size.to_string(),
        snapshots: vec![],
        created_at: now.clone(),
        last_modified: now,
    };
    if !save_master_config(&master_data) {
        return Err("Failed to update config.json".to_string());
    }

    append_log("INFO", &format!("create_{} start: {}", zvol_type, full_name));
    Ok(full_name)
}

// Ensure parent dataset exists with property
fn ensure_parent_dataset(parent: &str, prop: &str, prop_val: &str) -> Result<(), String> {
    if run_command_check(&["zfs", "list", "-H", parent]) != 0 {
        run_command(&["zfs", "create", "-o", &format!("{}={}", prop, prop_val), parent])?;
    }
    Ok(())
}

#[tauri::command]
pub fn create_image(token: String, name: String, size: String) -> Result<Value, String> {
    validate_auth(&token)?;
    validate_name(&name)?;
    validate_size(&size)?;

    let zpool = get_zpool_name();
    let mut parent_dataset = format!("{}/images", zpool);

    // Batch find parent with org.diskless:type=image
    if let Ok(get_out) = run_command_output(&[
        "zfs",
        "get",
        "-H",
        "-o",
        "name,value",
        "-r",
        &zpool,
        "org.diskless:type",
    ]) {
        for (dataset, val) in parse_property_output(&get_out) {
            if val == "image" {
                parent_dataset = dataset;
                break;
            }
        }
    }

    // Ensure parent exists
    ensure_parent_dataset(&parent_dataset, "org.diskless:type", "image")?;

    let full_name = create_zvol(&parent_dataset, &name, &size, "image")?;
    Ok(json!({
        "message": format!("Master ZVOL '{}' created successfully.", full_name),
        "master": {
            "id": full_name.clone(),
            "name": full_name,
            "snapshots": []
        }
    }))
}

#[tauri::command]
pub fn create_game_disk(token: String, name: String, size: String) -> Result<Value, String> {
    validate_auth(&token)?;
    validate_name(&name)?;
    validate_size(&size)?;

    let zpool = get_zpool_name();
    let games_parent = format!("{}/games", zpool);
    ensure_parent_dataset(&games_parent, "org.diskless:type", "games")?;

    let basename = format!("{}-games", name); // Append suffix as in original
    let full_name = create_zvol(&games_parent, &basename, &size, "game_disk")?;
    Ok(json!({
        "message": format!("Game Disk '{}' created successfully.", full_name),
        "master": {
            "id": full_name.clone(),
            "name": full_name,
            "snapshots": []
        }
    }))
}

#[tauri::command]
pub async fn get_images(token: String) -> Result<Vec<Master>, String> {
    validate_auth(&token)?;

    let zpool = get_zpool_name();
    let mut config = get_config();
    let default_master = config
        .settings
        .get("default_master")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();

    // List all datasets (fs/vol)
    let ds_out = run_command_output(&[
        "zfs",
        "list",
        "-H",
        "-t",
        "filesystem,volume",
        "-o",
        "name,creation,used",
        "-r",
        &zpool,
    ])
    .map_err(|e| format!("Failed to run zfs list: {}", e))?;
    let all_datasets = parse_zfs_list(&ds_out);

    // List all snapshots once
    let snap_out = run_command_output(&[
        "zfs",
        "list",
        "-H",
        "-t",
        "snapshot",
        "-o",
        "name,creation,used",
        "-r",
        &zpool,
    ])
    .map_err(|e| format!("Failed to list snapshots: {}", e))?;
    let all_snaps = parse_zfs_list(&snap_out);

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
        if let Ok(get_out) = run_command_output(&[
            "zfs",
            "get",
            "-H",
            "-o",
            "name,value",
            "org.diskless:type",
        ].iter().chain(&parent_vec).cloned().collect::<Vec<_>>()) {
            for (name, val) in parse_property_output(&get_out) {
                if val == "image" {
                    image_filter.insert(name);
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

        masters_data.push(Master {
            id: master_name_ref.clone(),
            name: master_name,
            is_default,
            size,
            snapshots,
        });
    }

    // Update config.json with the current masters list
    config.masters = serde_json::to_value(&masters_data).unwrap_or(json!({}));
    if let Err(e) = write_config(&config) {
        eprintln!("Error writing masters to config: {}", e);
    }

    Ok(masters_data)
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
// Common delete logic for image/snapshot
async fn common_delete(
    token: String,
    entity: &str,
    is_snapshot: bool,
) -> Result<Value, String> {
    validate_auth(&token)?;

    let dependents = check_dependent_clients(entity, is_snapshot).await?;
    if !dependents.is_empty() {
        return Ok(json!({
            "error": "Entity has dependent clients",
            "message": format!(
                "Cannot delete entity: It is being used by the following clients: {}",
                dependents.join(", ")
            ),
            "dependent_clients": dependents
        }));
    }

    if let Err(stderr) = run_command(["zfs", "destroy", entity]) {
        if stderr.contains("has dependent clones") {
            return Ok(json!({
                "error": "Entity has dependent clones",
                "message": format!("Cannot delete entity '{}': It has dependent clones.", entity)
            }));
        } else {
            return Ok(json!({
                "error": format!("Failed to delete entity: {}", stderr)
            }));
        }
    }

    if !is_snapshot {
        let _ = delete_image_config(entity);
    }
    append_log("INFO", &format!("delete_{}: {}", if is_snapshot { "snapshot" } else { "image" }, entity));
    Ok(json!({
        "message": format!("{} {} deleted successfully", if is_snapshot { "Snapshot" } else { "Master" }, entity)
    }))
}

pub fn delete_image_config(master_name: &str) -> bool {
    let mut config = get_config();
    if let Some(masters) = config.masters.as_object_mut() {
        if masters.remove(master_name).is_some() {
            if let Err(e) = write_config(&config) {
                println!("Error writing config file: {}", e);
                return false;
            }
        }
    }
    true
}

#[tauri::command]
pub async fn delete_image(token: String, master_name: String) -> Result<serde_json::Value, String> {
    common_delete(token, &master_name, false).await
}

#[tauri::command]
pub async fn delete_snapshot(token: String, snapshot_name: String) -> Result<Value, String> {
    validate_auth(&token)?;

    let zpool = get_zpool_name();
    if !snapshot_name.contains('@') || !snapshot_name.starts_with(&format!("{}/", &zpool)) {
        return Err("Invalid snapshot name format.".to_string());
    }

    common_delete(token, &snapshot_name, true).await
}

#[derive(Debug, Serialize)]
pub struct ZpoolInfo {
    name: String,
    size: String,
    alloc: String,
    free: String,
    health: String,
}

#[tauri::command]
pub fn get_zpool_list() -> Vec<ZpoolInfo> {
    let mut pools = vec![];
    if run_command_check(&["which", "zpool"]) == 0 {
        if let Ok(out) = run_command_output(&["zpool", "list", "-H", "-o", "name,size,alloc,free,health"]) {
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
    }
    pools
}

#[tauri::command]
pub fn zfs_pool_exists(pool_name: Option<String>) -> Result<bool, String> {
    if pool_name.is_none() {
        let output = Command::new("zpool")
            .args(["list", "-H"])
            .output()
            .map_err(|e| format!("Failed to list ZFS pools: {e}"))?;
        Ok(output.status.success() && !output.stdout.is_empty())
    } else {
        let pool = pool_name.unwrap();
        let status = Command::new("zpool").args(["list", &pool]).status().map_err(|e| {
            format!("Failed to check pool '{}': {}", pool, e)
        })?;

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
pub fn create_zfs_pool(name: String, disk: String) -> Result<(), String> {
    let status = run_command(&["zpool", "create", &name, &format!("/dev/{}", disk)]);
    if status.is_err() {
        return Err("Failed to create ZFS pool".to_string());
    }

    let mut config = get_config();
    let mut settings = config.settings.as_object().cloned().unwrap_or_default();
    settings.insert("zpool_name".to_string(), json!(name.clone()));
    settings.insert("zfsPool".to_string(), json!(name));
    config.settings = json!(settings);
    write_config(&config).map_err(|e| {
        format!("ZFS pool created, but failed to update config: {}", e)
    })?;

    Ok(())
}

#[tauri::command]
pub fn set_default_image(token: String, name: String) -> Result<bool, String> {
    validate_auth(&token)?;
    let mut config = get_config();
    if !config.settings.is_object() {
        config.settings = json!({});
    }
    config.settings["default_master"] = json!(name);
    match write_config(&config) {
        Ok(_) => Ok(true),
        Err(e) => Err(format!("Error saving default master: {}", e)),
    }
}

#[tauri::command]
pub async fn rollback_image_snapshot(
    token: String,
    _master_name: String,
    snapshot_name: String,
) -> Result<serde_json::Value, String> {
    validate_auth(&token)?;

    // Rollback the snapshot (destroys newer snapshots and their clones)
    if let Err(e) = run_command(["zfs", "rollback", "-r", &snapshot_name]) {
        return Err(format!("Failed to rollback snapshot: {}", e));
    }

    // Get clients and recreate clones for those using exactly this snapshot
    let clients_result = get_clients("".to_string(), None).await;
    let mut recreated = vec![];
    if let Ok(clients_json) = clients_result {
        if let Some(clients) = clients_json.as_array() {
            let zpool = get_zpool_name();
            for client in clients {
                let client_snapshot = client.get("snapshot").and_then(|v| v.as_str()).unwrap_or("");
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
pub async fn get_zfs_arcstat() -> Result<serde_json::Value, String> {
    use std::fs;
    let arcstat_path = "/proc/spl/kstat/zfs/arcstats";
    let content = fs::read_to_string(arcstat_path).map_err(|e| e.to_string())?;
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
    Ok(json!({
        "size": size,
        "hit_percent": hit_percent
    }))
}

#[tauri::command]
pub async fn get_default_image_overview() -> Result<serde_json::Value, String> {
    let config = get_config();
    let master_dataset = config
        .settings
        .get("default_master")
        .and_then(|s| s.as_str())
        .ok_or("Default master image not set in config".to_string())?;

    let stdout = run_command_output(&[
            "zfs",
            "get",
            "creation,clones",
            "-o",
            "value",
            "-H",
            master_dataset,
        ]).map_err(|e| format!("Failed to get master image info: {}", e))?;

    let lines: Vec<&str> = stdout.lines().collect();
    if lines.len() < 2 {
        return Err("Invalid master dataset output format".to_string());
    }

    Ok(json!({
        "name": master_dataset,
        "creation_date": lines[0].trim(),
        "clones": lines[1].trim()
    }))
}

#[tauri::command]
pub async fn rename_image(
    token: String,
    old_name: String,
    new_name: String,
) -> Result<serde_json::Value, String> {
    validate_auth(&token)?;
    validate_name(&new_name)?;

    if !zfs_exists(&old_name) {
        return Err(format!("Master '{}' not found.", old_name));
    }

    let zpool = get_zpool_name();
    let parent = if let Some(pos) = old_name.rfind('/') {
        old_name[..pos].to_string()
    } else {
        zpool.clone()
    };
    let new_master_zvol_name = format!("{}/{}", parent, new_name);

    if zfs_exists(&new_master_zvol_name) {
        return Err(format!("Master '{}' already exists.", new_master_zvol_name));
    }

    let dependents = check_dependent_clients(&old_name, false).await?;
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

    if let Err(stderr) = run_command(["zfs", "rename", &old_name, &new_master_zvol_name]) {
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