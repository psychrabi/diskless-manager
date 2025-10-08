// New ZFS management commands: list_zpools, list_datasets, create_zfs_dataset

use crate::utils::{run_command, run_command_output};
use serde::Serialize;

#[derive(Serialize)]
pub struct DatasetInfo {
    pub name: String,
    pub disk_type: Option<String>,
}

#[tauri::command]
pub fn list_zpools() -> Result<Vec<String>, String> {
    // returns names, one per line
    let out = run_command_output(&["zpool", "list", "-H", "-o", "name"])?;
    let pools = out
        .lines()
        .filter(|l| !l.is_empty())
        .map(|s| s.to_string())
        .collect();
    Ok(pools)
}

#[tauri::command]
pub fn list_datasets(zpool: &str) -> Result<Vec<DatasetInfo>, String> {
    // list datasets under the zpool and fetch org.diskless:type if set
    let out = run_command_output(&["zfs", "list", "-H", "-o", "name", "-r", zpool])?;
    let mut res = Vec::new();
    for line in out.lines().filter(|l| !l.is_empty()) {
        let name = line.to_string();
        // try to read the custom property; if missing/failed, keep None
        let disk_type = match run_command_output(&["zfs", "get", "-H", "-o", "value", "org.diskless:type", &name]) {
            Ok(v) => {
                let v = v.trim();
                if v.is_empty() { None } else { Some(v.to_string()) }
            }
            Err(_) => None,
        };
        res.push(DatasetInfo { name, disk_type });
    }
    Ok(res)
}

#[tauri::command]
pub fn create_zfs_dataset(zpool: &str, name: &str, usage_type: &str) -> Result<String, String> {
    // usage_type must be one of these
    let allowed = ["image", "writeback", "game"];
    if !allowed.contains(&usage_type) {
        return Err("usage_type must be one of: image, writeback, game".into());
    }

    if zpool.trim().is_empty() || name.trim().is_empty() {
        return Err("zpool and name are required".into());
    }

    let dataset = format!("{}/{}", zpool, name);

    // create dataset
    run_command(&["zfs", "create", &dataset])?;
    // tag it with our custom property
    run_command(&["zfs", "set", &format!("org.diskless:type={}", usage_type), &dataset])?;

    // sensible default for image datasets
    if usage_type == "image" {
        let _ = run_command(&["zfs", "set", "compression=lz4", &dataset]);
    }

    Ok(format!("Created dataset {}", dataset))
}

// New: delete dataset (recursive)
#[tauri::command]
pub fn delete_zfs_dataset(dataset: &str, recursive: bool) -> Result<String, String> {
    if dataset.trim().is_empty() {
        return Err("dataset is required".into());
    }
    let args = if recursive {
        vec!["zfs", "destroy", "-r", dataset]
    } else {
        vec!["zfs", "destroy", dataset]
    };
    // convert to slice of &str
    let args_ref: Vec<&str> = args.iter().map(|s| *s).collect();
    run_command(&args_ref)?;
    Ok(format!("Destroyed dataset {}", dataset))
}

// New: rename dataset (zfs rename old new)
#[tauri::command]
pub fn rename_zfs_dataset(old: &str, new: &str) -> Result<String, String> {
    if old.trim().is_empty() || new.trim().is_empty() {
        return Err("old and new dataset names are required".into());
    }
    // zfs rename <old> <new>
    run_command(&["zfs", "rename", old, new])?;
    Ok(format!("Renamed {} -> {}", old, new))
}