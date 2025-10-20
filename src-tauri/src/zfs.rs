//! ZFS-related logic for dataset, snapshot, and pool management.

use chrono::Local;
use regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Command;

use crate::utils::{append_log, run_command, run_command_check, run_command_output};
use crate::{
    client::get_clients,
    config::{get_config, get_zpool_name, write_config, Config},
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

#[derive(Serialize, Deserialize, Default)]
struct Settings {
    #[serde(default)]
    default_master: String,
}

// Check if a ZFS dataset exists
pub fn zfs_exists(dataset: &str) -> bool {
    run_command_check(&["zfs", "list", "-H", dataset]) == 0
}

// Destroy a ZFS dataset
pub fn zfs_destroy(dataset: &str) -> Result<(), String> {
    run_command(&["zfs", "destroy", dataset])
}

// Clone a ZFS snapshot to a new dataset
pub fn zfs_clone(snapshot: &str, clone: &str) -> Result<(), String> {
    run_command(&["zfs", "clone", snapshot, clone])
}

// Helper: Parse output of 'zfs list -H -o name,creation,used'
fn parse_zfs_list(output: &str) -> Vec<Snapshot> {
    output
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() == 3 {
                Some(Snapshot {
                    name: parts[0].to_string(),
                    created: parts[1].to_string(),
                    used: parts[2].to_string(),
                })
            } else {
                None
            }
        })
        .collect()
}

#[tauri::command]
pub fn create_image(token: String, name: String, size: String) -> Result<Value, String> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;
    if !regex::Regex::new(r"^[\w-]+$").unwrap().is_match(&name) {
        return Err("Invalid master base name format (use alphanumeric, _, -).".to_string());
    }
    if name.contains(' ') {
        return Err("Master base name cannot contain spaces.".to_string());
    }
    if !regex::Regex::new(r"^\d+[KMGTP]$")
        .unwrap()
        .is_match(&size.to_uppercase())
    {
        return Err("Invalid size format (e.g., '50G')".to_string());
    }



      // Determine parent dataset for image zvols:
    // If any existing dataset has property org.diskless:type=image, use that dataset as parent.
    // Otherwise fallback to <zpool>/images and create it if missing.
    let zpool = get_zpool_name();
    let mut parent_dataset = format!("{}/images", zpool);

    if let Ok(list_out) = run_command_output(&["zfs", "list", "-H", "-o", "name", "-r", &zpool]) {
        for line in list_out.lines().map(|l| l.trim()).filter(|l| !l.is_empty()) {
            if let Ok(prop) =
                run_command_output(&["zfs", "get", "-H", "-o", "value", "org.diskless:type", line])
            {
                if prop.trim() == "image" {
                    parent_dataset = line.to_string();
                    break;
                }
            }
        }
    }

    // Ensure parent exists (if we fell back to <zpool>/images)
    if parent_dataset == format!("{}/images", zpool) && run_command_check(&["zfs", "list", "-H", &parent_dataset]) != 0 {
        run_command(&["zfs", "create", &parent_dataset])?;
    }

    let master_zvol_name = format!("{}/{}", parent_dataset, name);
    let status_code = run_command_check(&["zfs", "list", "-H", &master_zvol_name]);
    if status_code == 0 {
        return Err(format!("Image '{}' already exists.", master_zvol_name));
    }


    run_command(&[
        "zfs",
        "create",
        "-s",
        "-V",
        &size,
        "-o",
        "volblocksize=128K",
        &master_zvol_name,
    ])?;
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let master_data = MasterData {
        name: master_zvol_name.clone(),
        size: size.clone(),
        snapshots: vec![],
        created_at: now.clone(),
        last_modified: now,
    };
    if !save_master_config(&master_data) {
        return Err("Failed to update config.json".to_string());
    }
    append_log("INFO", &format!("create_image start: {}", name));
    Ok(json!({
        "message": format!("Master ZVOL '{}' created successfully.", master_zvol_name),
        "master": {
            "id": master_zvol_name,
            "name": master_zvol_name,
            "snapshots": []
        }
    }))
}


#[tauri::command]
pub fn create_game_disk(token: String, name: String, size: String) -> Result<Value, String> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;

    // Validate provided name (no slashes, only alnum, underscore, hyphen)
    if !regex::Regex::new(r"^[\w-]+$").unwrap().is_match(&name) {
        return Err("Invalid game disk name format (use alphanumeric, _, -).".to_string());
    }
    if name.contains(' ') || name.contains('/') {
        return Err("Game disk name cannot contain spaces or '/'.".to_string());
    }

    // Validate size format (e.g., 50G)
    if !regex::Regex::new(r"^\d+[KMGTP]$")
        .unwrap()
        .is_match(&size.to_uppercase())
    {
        return Err("Invalid disk size format (e.g., '50G')".to_string());
    }

    // Ensure the games parent dataset exists: <zpool>/games
    let games_parent = format!("{}/games", get_zpool_name());
    if run_command_check(&["zfs", "list", "-H", &games_parent]) != 0 {
        // create the parent dataset if missing
        run_command(&["zfs", "create", &games_parent])?;
    }

    // Use given name for the zvol under <zpool>/games/<name>
    let disk_name = format!("{}/{}-games", games_parent, name);
    let status_code = run_command_check(&["zfs", "list", "-H", &disk_name]);
    if status_code == 0 {
        return Err(format!("Disk with the name '{}' already exists.", disk_name));
    }

    // Create the zvol
    run_command(&[
        "zfs",
        "create",
        "-s",
        "-V",
        &size,
        "-o",
        "volblocksize=128K",
        &disk_name,
    ])?;

    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let master_data = MasterData {
        name: disk_name.clone(),
        size: size.clone(),
        snapshots: vec![],
        created_at: now.clone(),
        last_modified: now,
    };

    // Save to config (same behavior as create_image)
    if !save_master_config(&master_data) {
        return Err("Failed to update config.json".to_string());
    }

    append_log("INFO", &format!("create_game_disk start: {}", disk_name));
    Ok(json!({
        "message": format!("Game Disk '{}' created successfully.", disk_name),
        "master": {
            "id": disk_name,
            "name": disk_name,
            "snapshots": []
        }
    }))
}

#[tauri::command]
pub async fn get_images(token: String) -> Result<Vec<Master>, String> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;
    // 1. Get default master from config
    let mut config = get_config();
    let default_master = config
        .settings
        .get("default_master")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();

    // 2. List all datasets
    let zfs_pool = get_zpool_name();
    let output = Command::new("sudo")
        .args([
            "zfs",
            "list",
            "-H",
            "-t",
            "filesystem,volume",
            "-o",
            "name,creation,used",
            "-r",
            &zfs_pool,
        ])
        .output()
        .map_err(|e| format!("Failed to run zfs list: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    let all_datasets = parse_zfs_list(&String::from_utf8_lossy(&output.stdout));

    // 3. Find master datasets and master snapshots
    let mut master_names = vec![];
    for ds in &all_datasets {
        if !ds.name.to_lowercase().ends_with("-disk") {
            master_names.push(ds.name.clone());
            continue;
        }
        // Check snapshots of this dataset
        let snap_out = Command::new("sudo")
            .args([
                "zfs", "list", "-H", "-t", "snapshot", "-o", "name", "-r", &ds.name,
            ])
            .output();
        if let Ok(snap_out) = snap_out {
            if snap_out.status.success() {
                for snap in String::from_utf8_lossy(&snap_out.stdout).lines() {
                    if snap.to_lowercase().ends_with("-master") {
                        master_names.push(snap.to_string());
                    }
                }
            }
        }
    }
    master_names.sort();
    master_names.dedup();

    // 4. For each master, get its snapshots
    let mut masters_data = vec![];
    for master_name in &master_names {
        // Only include masters whose parent dataset has org.diskless:type=image
        let parent = if let Some(p) = master_name.rfind('/') {
            &master_name[..p]
        } else {
            // no parent -> skip
            continue;
        };
        // try to read the custom property; if missing or not "image", skip this master
        match run_command_output(&["zfs", "get", "-H", "-o", "value", "org.diskless:type", parent]) {
            Ok(val) if val.trim() == "image" => { /* ok, include */ }
            _ => continue,
        }

        let mut snapshots = vec![];
        let snap_out = Command::new("sudo")
            .args([
                "zfs",
                "list",
                "-H",
                "-t",
                "snapshot",
                "-o",
                "name,creation,used",
                "-r",
                master_name,
            ])
            .output();
        if let Ok(snap_out) = snap_out {
            if snap_out.status.success() {
                snapshots = parse_zfs_list(&String::from_utf8_lossy(&snap_out.stdout));
            }
        }

        // Find the master dataset to get its size
        let size = all_datasets
            .iter()
            .find(|ds| &ds.name == master_name)
            .map(|ds| ds.used.clone())
            .unwrap_or_else(|| "-".to_string());

        masters_data.push(Master {
            id: master_name.clone(),
            name: master_name.clone(),
            is_default: master_name == &default_master,
            size,
            snapshots,
        });
    }

    // --- Update config.json with the current masters list ---
    config.masters = serde_json::to_value(&masters_data).unwrap_or(json!({}));
    if let Err(e) = write_config(&config) {
        println!("Error writing masters to config: {}", e);
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

#[tauri::command]
pub async fn delete_image(
    token: String,
    master_name: String,
) -> Result<serde_json::Value, String> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;
    let clients_result = get_clients("".to_string(), None).await;
    if let Ok(clients_json) = clients_result {
        if let Some(clients) = clients_json.as_array() {
            let dependent_clients: Vec<String> = clients
                .iter()
                .filter(|client| client.get("master") == Some(&json!(master_name)))
                .filter_map(|client| {
                    client
                        .get("name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .collect();
            if !dependent_clients.is_empty() {
                return Ok(json!({
                    "error": "Master has dependent clients",
                    "message": format!(
                        "Cannot delete master: It is being used by the following clients: {}",
                        dependent_clients.join(", ")
                    ),
                    "dependent_clients": dependent_clients
                }));
            }
        }
    }
    // Always treat master as ZFS and attempt destroy
    let output = Command::new("sudo")
        .args(["zfs", "destroy", &master_name])
        .output()
        .map_err(|e| format!("Failed to run zfs destroy: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("has dependent clones") {
            return Ok(json!({
                "error": "Master has dependent clones",
                "message": format!("Cannot delete master '{}': It has dependent clones.", master_name)
            }));
        } else {
            return Ok(json!({
                "error": format!("Failed to delete master: {}", stderr)
            }));
        }
    }
    if !delete_image_config(&master_name) {
        print!("Failed to remove master from config.json");
    }
    append_log("INFO", &format!("delete_image start: {}", master_name));
    Ok(json!({
        "message": format!("Master {} deleted successfully", master_name)
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
            if !master.get("snapshots").and_then(|s| s.as_array()).is_some() {
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
pub async fn delete_snapshot(token: String, snapshot_name: String) -> Result<Value, String> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;
    let zpool_name = get_zpool_name();
    if !snapshot_name.contains('@') || !snapshot_name.starts_with(&format!("{}/", zpool_name)) {
        return Err("Invalid snapshot name format.".to_string());
    }
    let clients_result = get_clients("".to_string(), None).await;
    if let Ok(clients_json) = clients_result {
        if let Some(clients) = clients_json.as_array() {
            let dependent_clients: Vec<String> = clients
                .iter()
                .filter(|client| client.get("snapshot") == Some(&json!(snapshot_name)))
                .filter_map(|client| {
                    client
                        .get("name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .collect();
            if !dependent_clients.is_empty() {
                return Ok(json!({
                    "error": "Snapshot has dependent clients",
                    "message": format!(
                        "Cannot delete snapshot: It is being used by the following clients: {}",
                        dependent_clients.join(", ")
                    ),
                    "dependent_clients": dependent_clients
                }));
            }
        }
    }
    let output = Command::new("sudo")
        .args(["zfs", "destroy", &snapshot_name])
        .output()
        .map_err(|e| format!("Failed to run zfs destroy: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("has dependent clones") {
            return Ok(json!({
                "error": "Snapshot has dependent clones",
                "message": format!("Cannot delete snapshot '{}': It has dependent clones.", snapshot_name)
            }));
        } else {
            return Ok(json!({
                "error": format!("Failed to delete snapshot: {}", stderr)
            }));
        }
    }
    let mut config = get_config();
    if let Some(masters) = config.masters.as_object_mut() {
        for (_master_name, master) in masters.iter_mut() {
            if let Some(snapshots) = master.get_mut("snapshots").and_then(|s| s.as_array_mut()) {
                let before = snapshots.len();
                snapshots.retain(|s| s != &json!(snapshot_name));
                if snapshots.len() != before {
                    write_config(&config).map_err(|e| format!("Failed to write config: {}", e))?;
                    break;
                }
            }
        }
    }
    Ok(json!({
        "message": format!("Snapshot {} deleted successfully", snapshot_name)
    }))
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
    let mut pools: Vec<ZpoolInfo> = Vec::new();
    if Command::new("which").arg("zpool").output().is_ok() {
        if let Ok(out) = Command::new("zpool")
            .args(["list", "-H", "-o", "name,size,alloc,free,health"])
            .output()
        {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout);
                for line in s.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
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
    }
    pools
}

#[tauri::command]
pub fn zfs_pool_exists(pool_name: Option<String>) -> Result<bool, String> {
    if pool_name.is_none() {
        let output = Command::new("zpool")
            .args(["list", "-H"])
            .output()
            .map_err(|e| format!("Failed to list ZFS pools: {}", e))?;

        return Ok(output.status.success() && !output.stdout.is_empty());
    }

    // If pool name is provided, check that specific pool
    let pool = pool_name.unwrap(); // Safe to unwrap since we checked is_none
    let status = match Command::new("zpool").args(["list", &pool]).status() {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "Warning: 'zpool' not available when checking pool '{}': {}",
                pool, e
            );
            return Ok(false);
        }
    };

    let exists = status.success();

    if exists {
        // Update config.json settings with both keys for compatibility
        let mut config: Config = get_config();
        if !config.settings.is_object() {
            config.settings = json!({});
        }
        config.settings["zpool_name"] = json!(pool.clone());
        config.settings["zfsPool"] = json!(pool.clone());
        if let Err(e) = write_config(&config) {
            println!("Error updating zpool_name/zfsPool in config: {}", e);
        }
    }

    Ok(exists)
}

#[tauri::command]
pub fn create_zfs_pool(name: String, disk: String) -> Result<(), String> {
    // WARNING: This will destroy data on the disk!
    let status = run_command(&["zpool", "create", &name, &format!("/dev/{}", disk)]);
    if let Ok(_) = status {
        let mut cfg = get_config();
        let mut settings = cfg.settings.as_object().cloned().unwrap_or_default();
        settings.insert("zpool_name".to_string(), json!(name.clone()));
        settings.insert("zfsPool".to_string(), json!(name));
        cfg.settings = json!(settings);
        if let Err(e) = write_config(&cfg) {
            return Err(format!(
                "ZFS pool created, but failed to update config: {}",
                e
            ));
        }
        Ok(())
    } else {
        Err("Failed to create ZFS pool".to_string())
    }
}

#[tauri::command]
pub fn set_default_image(token: String, name: &str) -> Result<bool, String> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;
    let mut config = get_config();
    if !config.settings.is_object() {
        config.settings = json!({});
    }
    config.settings["default_master"] = Value::String(name.to_string());
    match write_config(&config) {
        Ok(_) => Ok(true),
        Err(e) => {
            println!("Error saving default master: {}", e);
            Err(format!("Error saving default master: {}", e))
        }
    }
}

#[tauri::command]
pub async fn rollback_image_snapshot(
    token: String,
    _master_name: String,
    snapshot_name: String,
) -> Result<serde_json::Value, String> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;
    // 1. Rollback the snapshot
    let rollback_output = Command::new("sudo")
        .args(["zfs", "rollback", "-r", &snapshot_name])
        .output();
    if let Err(e) = rollback_output {
        return Err(format!("Failed to execute zfs rollback: {}", e));
    }
    let rollback_output = rollback_output.unwrap();
    if !rollback_output.status.success() {
        return Err(format!(
            "Failed to rollback snapshot: {}",
            String::from_utf8_lossy(&rollback_output.stderr)
        ));
    }

    // 2. Find all clients that were using this snapshot as their base
    let clients_result = get_clients("".to_string(), None)
        .await
        .map_err(|e| format!("Failed to get clients: {}", e))?;
    let mut recreated = vec![];
    if let Some(clients) = clients_result.as_array() {
        for client in clients {
            let client_snapshot = client
                .get("snapshot")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let client_id = client.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let client_name = client.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let client_clone = format!("{}/{}-disk", get_zpool_name(), client_name.to_uppercase());
            if client_snapshot == snapshot_name {
                // Destroy the old clone if it exists
                let _ = run_command(&["zfs", "destroy", &client_clone]);
                // Re-create the clone from the rolled-back snapshot
                let clone_output = run_command(&["zfs", "clone", &snapshot_name, &client_clone]);
                if clone_output.is_ok() {
                        recreated.push(client_id.to_string());
                        // Optionally update the client config's block_device
                        // (Assumes block_device is /dev/zvol/{clone})
                        // You may want to reload and update the client config here
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
        if line.starts_with("hits ") {
            hits = line
                .split_whitespace()
                .nth(2)
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
        }
        if line.starts_with("misses ") {
            misses = line
                .split_whitespace()
                .nth(2)
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
        }
        if line.starts_with("size ") {
            size = line
                .split_whitespace()
                .nth(2)
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
        }
    }
    let hit_percent = if hits + misses > 0 {
        (hits as f64 / (hits + misses) as f64) * 100.0
    } else {
        0.0
    };
    Ok(serde_json::json!({
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
        .unwrap_or("");

    if master_dataset.is_empty() {
        return Err("Default master image not set in config".to_string());
    }

    let output = Command::new("sudo")
        .args([
            "zfs",
            "get",
            "creation,clones",
            "-o",
            "value",
            "-H",
            master_dataset,
        ])
        .output()
        .map_err(|e| format!("Failed to get master image info: {}", e))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    if lines.len() < 2 {
        return Err("Unexpected output from zfs get".to_string());
    }

    Ok(json!({
        "name": master_dataset,
        "creation_date": lines[0],
        "clones": lines[1]
    }))
}

#[tauri::command]
pub async fn rename_image(
    token: String,
    old_name: String,
    new_name: String,
) -> Result<serde_json::Value, String> {
    // Validate authentication token
    crate::middleware::validate_auth_token_for_command(&token)
        .map_err(|e| format!("Authentication failed: {}", e.message))?;
    // Validate new name format
    if !regex::Regex::new(r"^[\w-]+$").unwrap().is_match(&new_name) {
        return Err("Invalid master base name format (use alphanumeric, _, -).".to_string());
    }
    if new_name.contains(' ') {
        return Err("Master base name cannot contain spaces.".to_string());
    }

    // Check if old master exists
    if !zfs_exists(&old_name) {
        return Err(format!("Master '{}' not found.", old_name));
    }

    // Check if new master name already exists
    let new_master_zvol_name = format!("{}/{}-master", get_zpool_name(), new_name);
    if zfs_exists(&new_master_zvol_name) {
        return Err(format!("Master '{}' already exists.", new_master_zvol_name));
    }

    // Check for dependent clients
    let clients_result = get_clients("".to_string(), None).await;
    if let Ok(clients_json) = clients_result {
        if let Some(clients) = clients_json.as_array() {
            let dependent_clients: Vec<String> = clients
                .iter()
                .filter(|client| client.get("master") == Some(&json!(old_name)))
                .filter_map(|client| {
                    client
                        .get("name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .collect();
            if !dependent_clients.is_empty() {
                return Ok(json!({
                    "error": "Master has dependent clients",
                    "message": format!(
                        "Cannot rename master: It is being used by the following clients: {}",
                        dependent_clients.join(", ")
                    ),
                    "dependent_clients": dependent_clients
                }));
            }
        }
    }

    // Perform the rename
    let output = Command::new("sudo")
        .args(["zfs", "rename", &old_name, &new_master_zvol_name])
        .output()
        .map_err(|e| format!("Failed to run zfs rename: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
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

    // Update config if this was the default master
    let mut config = get_config();
    if config.settings.get("default_master") == Some(&json!(old_name)) {
        config.settings["default_master"] = json!(new_master_zvol_name);
        if let Err(e) = write_config(&config) {
            println!("Warning: Failed to update default master in config: {}", e);
        }
    }

    // Update master config entry
    if let Some(masters) = config.masters.as_object_mut() {
        if let Some(master_data) = masters.remove(&old_name) {
            masters.insert(new_master_zvol_name.clone(), master_data);
            if let Err(e) = write_config(&config) {
                println!("Warning: Failed to update master config: {}", e);
            }
        }
    }

    Ok(json!({
        "message": format!("Master renamed from '{}' to '{}' successfully", old_name, new_master_zvol_name)
    }))
}
