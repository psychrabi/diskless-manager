//! ZFS-related logic for dataset, snapshot, and pool management.

use chrono::Local;
use regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::{
    client::get_clients,
    config::{read_config, write_config},
    CONFIG_PATH,
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

// Check if a ZFS dataset exists
pub fn zfs_exists(dataset: &str) -> bool {
    let output = Command::new("sudo")
        .args(["zfs", "list", "-H", dataset])
        .output();
    match output {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}

// Destroy a ZFS dataset
pub fn zfs_destroy(dataset: &str) -> Result<(), String> {
    let status = Command::new("sudo")
        .args(["zfs", "destroy", dataset])
        .status()
        .map_err(|e| format!("Failed to run zfs destroy: {}", e))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("Failed to destroy ZFS dataset: {}", dataset))
    }
}

// Clone a ZFS snapshot to a new dataset
pub fn zfs_clone(snapshot: &str, clone: &str) -> Result<(), String> {
    let status = Command::new("sudo")
        .args(["zfs", "clone", snapshot, clone])
        .status()
        .map_err(|e| format!("Failed to run zfs clone: {}", e))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "Failed to clone ZFS snapshot {} to {}",
            snapshot, clone
        ))
    }
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
pub fn create_master(name: String, size: String) -> Result<Value, String> {
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
    let master_zvol_name = format!("{}/{}-master", crate::ZFS_POOL, name);
    let status = Command::new("sudo")
        .args(["zfs", "list", "-H", &master_zvol_name])
        .status()
        .map_err(|e| format!("Failed to check ZFS volume: {}", e))?;
    if status.success() {
        return Err(format!("ZFS volume '{}' already exists.", master_zvol_name));
    }
    let create_status = Command::new("sudo")
        .args([
            "zfs",
            "create",
            "-s",
            "-V",
            &size,
            "-o",
            "volblocksize=64k",
            &master_zvol_name,
        ])
        .status()
        .map_err(|e| format!("Failed to create ZFS volume: {}", e))?;
    if !create_status.success() {
        return Err(format!(
            "Failed to create ZFS volume '{}'",
            master_zvol_name
        ));
    }
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
pub async fn get_masters(zfs_pool: String) -> Result<Vec<Master>, String> {
    // 1. Get default master from config
    let config: serde_json::Value = fs::read_to_string("config.json")
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let default_master = config
        .get("settings")
        .and_then(|s| s.get("default_master"))
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();

    // 2. List all datasets
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
        if ds.name.to_lowercase().ends_with("-master") {
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
    Ok(masters_data)
}

pub fn save_master_config(master_data: &MasterData) -> bool {
    let mut config: Value = if Path::new(crate::CONFIG_PATH).exists() {
        match fs::read_to_string(crate::CONFIG_PATH)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
        {
            Some(val) => val,
            None => json!({"clients": [], "masters": {}}),
        }
    } else {
        json!({"clients": [], "masters": {}})
    };
    if !config.get("masters").map_or(false, |v| v.is_object()) {
        config["masters"] = json!({});
    }
    config["masters"][&master_data.name] = serde_json::to_value(master_data).unwrap();
    match fs::write(
        crate::CONFIG_PATH,
        serde_json::to_string_pretty(&config).unwrap(),
    ) {
        Ok(_) => true,
        Err(e) => {
            println!("Error saving master config: {}", e);
            false
        }
    }
}

#[tauri::command]
pub async fn delete_master(master_name: String) -> Result<serde_json::Value, String> {
    let clients_result = get_clients(None).await;
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
    if !delete_master_config(&master_name) {
        print!("Failed to remove master from config.json");
    }
    Ok(json!({
        "message": format!("Master {} deleted successfully", master_name)
    }))
}

pub fn delete_master_config(master_name: &str) -> bool {
    if !Path::new(crate::CONFIG_PATH).exists() {
        return true;
    }
    let config_content = match fs::read_to_string(crate::CONFIG_PATH) {
        Ok(content) => content,
        Err(e) => {
            println!("Error reading config file: {}", e);
            return false;
        }
    };
    let mut config: Value = match serde_json::from_str(&config_content) {
        Ok(cfg) => cfg,
        Err(e) => {
            println!("Error parsing config file: {}", e);
            return false;
        }
    };
    if let Some(masters) = config.get_mut("masters") {
        if masters.get(master_name).is_some() {
            masters.as_object_mut().unwrap().remove(master_name);
            if let Err(e) = fs::write(
                crate::CONFIG_PATH,
                serde_json::to_string_pretty(&config).unwrap(),
            ) {
                println!("Error writing config file: {}", e);
                return false;
            }
        }
    }
    true
}

#[tauri::command]
pub fn create_snapshot(snapshot_name: String) -> Result<Value, String> {
    if !snapshot_name.contains('@') || !snapshot_name.starts_with(&format!("{}/", crate::ZFS_POOL))
    {
        return Err(format!(
            "Invalid snapshot name. Expected {}/master@snapname",
            crate::ZFS_POOL
        ));
    }
    let master_name = snapshot_name.split('@').next().unwrap();
    let status = Command::new("sudo")
        .args(["zfs", "list", "-H", master_name])
        .status()
        .map_err(|e| format!("Error validating master: {}", e))?;
    if !status.success() {
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
    if Path::new(crate::CONFIG_PATH).exists() {
        let config_content = fs::read_to_string(crate::CONFIG_PATH)
            .map_err(|e| format!("Failed to read config: {}", e))?;
        let mut config: Value = serde_json::from_str(&config_content)
            .map_err(|e| format!("Failed to parse config: {}", e))?;
        if let Some(masters) = config.get_mut("masters") {
            if let Some(master) = masters.get_mut(master_name) {
                let snapshots =
                    if let Some(arr) = master.get_mut("snapshots").and_then(|s| s.as_array_mut()) {
                        arr
                    } else {
                        master["snapshots"] = json!([]);
                        master.get_mut("snapshots").unwrap().as_array_mut().unwrap()
                    };
                if !snapshots.iter().any(|v| v == &json!(snapshot_name)) {
                    snapshots.push(json!(snapshot_name));
                }
                fs::write(
                    crate::CONFIG_PATH,
                    serde_json::to_string_pretty(&config).unwrap(),
                )
                .map_err(|e| format!("Failed to write config: {}", e))?;
            }
        }
    }
    Ok(json!({
        "message": format!("Snapshot {} created", snapshot_name)
    }))
}

#[tauri::command]
pub async fn delete_snapshot(snapshot_name: String) -> Result<Value, String> {
    if !snapshot_name.contains('@') || !snapshot_name.starts_with(&format!("{}/", crate::ZFS_POOL))
    {
        return Err("Invalid snapshot name format.".to_string());
    }
    let clients_result = get_clients(None).await;
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
    if Path::new(CONFIG_PATH).exists() {
        let config_content =
            fs::read_to_string(CONFIG_PATH).map_err(|e| format!("Failed to read config: {}", e))?;
        let mut config: Value = serde_json::from_str(&config_content)
            .map_err(|e| format!("Failed to parse config: {}", e))?;
        if let Some(masters) = config.get_mut("masters") {
            for (_master_name, master) in masters.as_object_mut().unwrap() {
                if let Some(snapshots) = master.get_mut("snapshots").and_then(|s| s.as_array_mut())
                {
                    let before = snapshots.len();
                    snapshots.retain(|s| s != &json!(snapshot_name));
                    if snapshots.len() != before {
                        fs::write(CONFIG_PATH, serde_json::to_string_pretty(&config).unwrap())
                            .map_err(|e| format!("Failed to write config: {}", e))?;
                        break;
                    }
                }
            }
        }
    }
    Ok(json!({
        "message": format!("Snapshot {} deleted successfully", snapshot_name)
    }))
}

#[tauri::command]
pub async fn get_zpool_stats() -> Result<serde_json::Value, String> {
    let output = Command::new("zpool")
        .args(["list", "-H", "-o", "name,size,alloc,free"])
        .output()
        .map_err(|e| format!("Failed to run zpool list: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next().ok_or("No zpool found")?;
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 4 {
        return Err("Unexpected zpool output".to_string());
    }
    Ok(json!({
        "name": parts[0],
        "size": parts[1],
        "used": parts[2],
        "available": parts[3],
    }))
}

#[tauri::command]
pub fn zfs_pool_exists(pool_name: String) -> Result<bool, String> {
    let status = Command::new("zpool")
        .args(&["list", pool_name.as_str()])
        .status()
        .map_err(|e| e.to_string())?;
    Ok(status.success())
}

#[tauri::command]
pub fn create_zfs_pool(name: String, disk: String) -> Result<(), String> {
    // WARNING: This will destroy data on the disk!
    let status = Command::new("sudo")
        .args(&["zpool", "create", &name, &format!("/dev/{}", disk)])
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        let mut cfg = read_config();
        let mut settings = cfg.settings.as_object().cloned().unwrap_or_default();
        settings.insert("zpool_name".to_string(), json!(name));
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
