// New ZFS management commands: list_zpools, list_datasets, create_zfs_dataset

use crate::{
    cmd::{run_command, run_command_check, run_command_output, run_command_output_no_sudo},
    error::AppError,
    types::{CreateDatasetRequest, DatasetInfo},
    validation::{validate_pool_name, validate_size, validate_zfs_name},
};


pub fn list_block_devices() -> Result<Vec<String>, AppError> {
    let out = run_command_output_no_sudo(&[
        "lsblk",
        "-d",
        "-n",
        "-o",
        "NAME",
    ])
    .map_err(|e| AppError::Command(e.to_string()))?;

    let devices: Vec<String> = out
        .lines()
        .filter(|l| !l.is_empty())
        .map(|s| s.trim().to_string())
        .collect();

    Ok(devices)
}


#[allow(dead_code)]
pub fn list_zpools() -> Result<Vec<String>, AppError> {
    // Get fresh data
    let out = run_command_output_no_sudo(&["zpool", "list", "-H", "-o", "name"])
        .map_err(|e| AppError::Command(e.to_string()))?;
    let pools: Vec<String> = out
        .lines()
        .filter(|l| !l.is_empty())
        .map(|s| s.to_string())
        .collect();

    Ok(pools)
}


#[allow(dead_code)]
pub fn list_datasets(zpool: &str) -> Result<Vec<DatasetInfo>, AppError> {
    // Get fresh data
    // Get all datasets with their properties in one command
    let out = run_command_output_no_sudo(&[
        "zfs",
        "list",
        "-H",
        "-o",
        "name,used,avail,refer,mountpoint",
        "-r",
        zpool,
    ])
    .map_err(|e| AppError::Command(e.to_string()))?;

    let mut with_type: Vec<DatasetInfo> = Vec::new();

    for line in out.lines().filter(|l| !l.is_empty()) {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 5 {
            continue;
        }

        let name = parts[0].to_string();
        let used = parts[1].to_string();
        let available = parts[2].to_string();
        let referenced = parts[3].to_string();
        let mountpoint = parts[4].to_string();

        // Get the custom property org.diskless:type
        let disk_type = match run_command_output(&[
            "zfs",
            "get",
            "-H",
            "-o",
            "value",
            "org.diskless:type",
            &name,
        ]) {
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
            with_type.push(DatasetInfo {
                name,
                disk_type: Some(dt),
                used,
                available,
                referenced,
                mountpoint,
            });
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


#[allow(dead_code)]
pub fn create_zfs_dataset(req: CreateDatasetRequest) -> Result<String, AppError> {
    // Validate inputs
    validate_pool_name(&req.zpool)?;
    validate_zfs_name(&req.name)?;

    // usage_type must be one of these
    let allowed = ["image", "writeback", "game"];
    if !allowed.contains(&req.usage_type.as_str()) {
        return Err(AppError::Validation(
            "usage_type must be one of: image, writeback, game".into(),
        ));
    }

    let dataset = format!("{}/{}-disk", req.zpool, req.name);

    if req.usage_type == "game" {
        // size is required for game (zvol)
        let size_trim = req.size.as_ref().map(|s| s.trim()).unwrap_or("");
        if size_trim.is_empty() {
            return Err(AppError::Validation(
                "size is required for game (zvol) disks, e.g. 20G".into(),
            ));
        }

        // Validate size format
        validate_size(size_trim)?;

        // Ensure the games parent dataset exists: <zpool>/games
        let games_parent = format!("{}/games", req.zpool);
        if run_command_check(&["zfs", "list", "-H", &games_parent]) != 0 {
            // create the parent dataset if missing
            run_command(&["zfs", "create", &games_parent])
                .map_err(|e| AppError::Command(e.to_string()))?;
        }

        // Use given name for the zvol under <zpool>/games/<name>
        let zvol_name = format!("{}/{}", games_parent, req.name);
        let status_code = run_command_check(&["zfs", "list", "-H", &zvol_name]);
        if status_code == 0 {
            return Err(AppError::Validation(format!(
                "ZFS volume '{}' already exists.",
                zvol_name
            )));
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
        ])
        .map_err(|e| AppError::Command(e.to_string()))?;

        // tag it with our custom property
        let _ = run_command(&[
            "zfs",
            "set",
            &format!("org.diskless:type={}", req.usage_type),
            &zvol_name,
        ]);

        return Ok(format!("Created zvol {}", zvol_name));
    }

    // create dataset
    run_command(&["zfs", "create", &dataset]).map_err(|e| AppError::Command(e.to_string()))?;
    // tag it with our custom property
    run_command(&[
        "zfs",
        "set",
        &format!("org.diskless:type={}", req.usage_type),
        &dataset,
    ])
    .map_err(|e| AppError::Command(e.to_string()))?;

    // sensible default for image datasets
    if req.usage_type == "image" {
        let _ = run_command(&["zfs", "set", "compression=lz4", &dataset]);
    }

    Ok(format!("Created dataset {}", dataset))
}

// New: delete dataset (recursive)

#[allow(dead_code)]
pub fn delete_zfs_dataset(dataset: &str, recursive: bool) -> Result<String, AppError> {
    if dataset.trim().is_empty() {
        return Err(AppError::Validation("dataset is required".into()));
    }

    let args = if recursive {
        vec!["zfs", "destroy", "-r", dataset]
    } else {
        vec!["zfs", "destroy", dataset]
    };
    // convert to slice of &str
    let args_ref: Vec<&str> = args.to_vec();
    run_command(&args_ref).map_err(|e| AppError::Command(e.to_string()))?;

    Ok(format!("Destroyed dataset {}", dataset))
}

// New: rename dataset (zfs rename old new)

pub fn rename_zfs_dataset(old: &str, new: &str) -> Result<String, AppError> {
    if old.trim().is_empty() || new.trim().is_empty() {
        return Err(AppError::Validation(
            "old and new dataset names are required".into(),
        ));
    }

    // zfs rename <old> <new>
    run_command(&["zfs", "rename", old, new]).map_err(|e| AppError::Command(e.to_string()))?;

    Ok(format!("Renamed {} -> {}", old, new))
}
