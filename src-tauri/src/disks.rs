// New ZFS management commands: list_zpools, list_datasets, create_zfs_dataset

use crate::{types::DatasetInfo, utils::{run_command, run_command_check, run_command_output, run_command_output_no_sudo}};
use regex::Regex;

#[tauri::command]
pub fn list_zpools() -> Result<Vec<String>, String> {
    // returns names, one per line
    let out = run_command_output_no_sudo(&["zpool", "list", "-H", "-o", "name"])?;
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
    let out = run_command_output_no_sudo(&["zfs", "list", "-H", "-o", "name", "-r", zpool])?;
    let mut with_type: Vec<DatasetInfo> = Vec::new();

    for line in out.lines().filter(|l| !l.is_empty()) {
        let name = line.to_string();
        // try to read the custom property; if missing/failed, treat as not set
        let disk_type = match run_command_output(&["zfs", "get", "-H", "-o", "value", "org.diskless:type", &name]) {
            Ok(v) => {
                let v = v.trim();
                // treat '-' or 'none' (zfs placeholder) as not set
                if v.is_empty() || v == "-" || v.eq_ignore_ascii_case("none") {
                    None
                } else {
                    Some(v.to_string())
                }
            }
            Err(_) => None,
        };

        if let Some(dt) = disk_type {
            with_type.push(DatasetInfo { name, disk_type: Some(dt) });
        }
    }

    // Exclude any dataset that is a child of another dataset in the list.
    // e.g. if "pool/images" is present, do not include "pool/images/foo"
    let names: Vec<String> = with_type.iter().map(|d| d.name.clone()).collect();
    let mut result: Vec<DatasetInfo> = Vec::new();

    'outer: for ds in with_type.into_iter() {
        for parent in &names {
            if parent == &ds.name {
                continue;
            }
            if ds.name.starts_with(&format!("{}/", parent)) {
                // ds is a child of `parent` which is also marked -> skip ds
                continue 'outer;
            }
        }
        result.push(ds);
    }

    Ok(result)
}

#[tauri::command]
pub fn create_zfs_dataset(zpool: &str, name: &str, usage_type: &str, size: &str) -> Result<String, String> {
    // usage_type must be one of these
    let allowed = ["image", "writeback", "game"];
    if !allowed.contains(&usage_type) {
        return Err("usage_type must be one of: image, writeback, game".into());
    }

    if zpool.trim().is_empty() || name.trim().is_empty() {
        return Err("zpool and name are required".into());
    }

    let dataset = format!("{}/{}-disk", zpool, name);

    if usage_type == "game" {
        // size is required for game (zvol)
        let size_trim = size.trim();
        if size_trim.is_empty() {
            return Err("size is required for game (zvol) disks, e.g. 20G".into());
        }

        // Validate size format (e.g., 50G)
        if !Regex::new(r"^\d+[KMGTP]$")
            .map_err(|e| format!("regex error: {}", e))?
            .is_match(&size_trim.to_uppercase())
        {
            return Err("Invalid size format (e.g., '50G')".into());
        }

        // Ensure the games parent dataset exists: <zpool>/games
        let games_parent = format!("{}/games", zpool);
        if run_command_check(&["zfs", "list", "-H", &games_parent]) != 0 {
            // create the parent dataset if missing
            run_command(&["zfs", "create", &games_parent])?;
        }

        // Use given name for the zvol under <zpool>/games/<name>
        let zvol_name = format!("{}/{}", games_parent, name);
        let status_code = run_command_check(&["zfs", "list", "-H", &zvol_name]);
        if status_code == 0 {
            return Err(format!("ZFS volume '{}' already exists.", zvol_name));
        }

        // Create the zvol
        run_command(&[
            "zfs",
            "create",
            "-s",
            "-V",
            size_trim,
            "-o",
            "volblocksize=4K",
            &zvol_name,
        ])?;

        // tag it with our custom property
        let _ = run_command(&["zfs", "set", &format!("org.diskless:type={}", usage_type), &zvol_name]);

        return Ok(format!("Created zvol {}", zvol_name));
    }
    
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