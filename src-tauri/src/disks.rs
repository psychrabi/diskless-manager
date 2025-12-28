// New ZFS management commands: list_zpools, list_datasets, create_zfs_dataset

use crate::{
    cmd::{run_command, run_command_check, run_command_output, run_command_output_no_sudo},
    types::{CreateDatasetRequest, DatasetInfo},
};
use regex::Regex;
use std::sync::{Arc, RwLock};

// Cache for ZFS pool and dataset information to reduce system calls
use once_cell::sync::Lazy;

static DISK_CACHE: Lazy<Arc<RwLock<DiskCache>>> =
    Lazy::new(|| Arc::new(RwLock::new(DiskCache::new())));

#[derive(Debug, Clone)]
struct DiskCache {
    zpools: Vec<String>,
    datasets: std::collections::HashMap<String, Vec<DatasetInfo>>, // zpool -> datasets
    last_updated: std::time::SystemTime,
    ttl: std::time::Duration,
}

impl DiskCache {
    fn new() -> Self {
        DiskCache {
            zpools: Vec::new(),
            datasets: std::collections::HashMap::new(),
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
pub fn list_zpools() -> Result<Vec<String>, String> {
    // Try to get cached data first
    let cache = DISK_CACHE.read().unwrap();

    if !cache.needs_refresh() {
        // Return cached data
        return Ok(cache.zpools.clone());
    }

    // Drop read lock before acquiring write lock
    drop(cache);

    let mut cache = DISK_CACHE.write().unwrap();

    // Double-check after acquiring write lock
    if !cache.needs_refresh() {
        // Another thread may have updated the cache while we were waiting
        return Ok(cache.zpools.clone());
    }

    // Get fresh data
    let out = run_command_output_no_sudo(&["zpool", "list", "-H", "-o", "name"])
        .map_err(|e| e.to_string())?;
    let pools: Vec<String> = out
        .lines()
        .filter(|l| !l.is_empty())
        .map(|s| s.to_string())
        .collect();

    // Update cache
    cache.zpools = pools.clone();
    cache.last_updated = std::time::SystemTime::now();

    Ok(pools)
}

#[tauri::command]
pub fn list_datasets(zpool: &str) -> Result<Vec<DatasetInfo>, String> {
    // Try to get cached data first
    let cache = DISK_CACHE.read().unwrap();

    if !cache.needs_refresh() {
        if let Some(cached_datasets) = cache.datasets.get(zpool) {
            return Ok(cached_datasets.clone());
        }
    }

    // Drop read lock before acquiring write lock
    drop(cache);

    let mut cache = DISK_CACHE.write().unwrap();

    // Double-check after acquiring write lock
    if !cache.needs_refresh() {
        if let Some(cached_datasets) = cache.datasets.get(zpool) {
            return Ok(cached_datasets.clone());
        }
    }

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
    .map_err(|e| e.to_string())?;

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

    // Update cache for this specific zpool
    cache.datasets.insert(zpool.to_string(), result.clone());
    cache.last_updated = std::time::SystemTime::now();

    Ok(result)
}

#[tauri::command]
pub fn create_zfs_dataset(req: CreateDatasetRequest) -> Result<String, String> {
    // usage_type must be one of these
    let allowed = ["image", "writeback", "game"];
    if !allowed.contains(&req.usage_type.as_str()) {
        return Err("usage_type must be one of: image, writeback, game".into());
    }

    if req.zpool.trim().is_empty() || req.name.trim().is_empty() {
        return Err("zpool and name are required".into());
    }

    let dataset = format!("{}/{}-disk", req.zpool, req.name);

    if req.usage_type == "game" {
        // size is required for game (zvol)
        let size_trim = req.size.as_ref().map(|s| s.trim()).unwrap_or("");
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
        let games_parent = format!("{}/games", req.zpool);
        if run_command_check(&["zfs", "list", "-H", &games_parent]) != 0 {
            // create the parent dataset if missing
            run_command(&["zfs", "create", &games_parent]).map_err(|e| e.to_string())?;
        }

        // Use given name for the zvol under <zpool>/games/<name>
        let zvol_name = format!("{}/{}", games_parent, req.name);
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
        ])
        .map_err(|e| e.to_string())?;

        // tag it with our custom property
        let _ = run_command(&[
            "zfs",
            "set",
            &format!("org.diskless:type={}", req.usage_type),
            &zvol_name,
        ]);

        // Invalidate cache since we've modified datasets
        if let Ok(mut cache) = DISK_CACHE.try_write() {
            cache.datasets.remove(&req.zpool);
            cache.last_updated = std::time::SystemTime::UNIX_EPOCH; // Force refresh next time
        }

        return Ok(format!("Created zvol {}", zvol_name));
    }

    // create dataset
    run_command(&["zfs", "create", &dataset]).map_err(|e| e.to_string())?;
    // tag it with our custom property
    run_command(&[
        "zfs",
        "set",
        &format!("org.diskless:type={}", req.usage_type),
        &dataset,
    ])
    .map_err(|e| e.to_string())?;

    // sensible default for image datasets
    if req.usage_type == "image" {
        let _ = run_command(&["zfs", "set", "compression=lz4", &dataset]);
    }

    // Invalidate cache since we've modified datasets
    if let Ok(mut cache) = DISK_CACHE.try_write() {
        cache.datasets.remove(&req.zpool);
        cache.last_updated = std::time::SystemTime::UNIX_EPOCH; // Force refresh next time
    }

    Ok(format!("Created dataset {}", dataset))
}

// New: delete dataset (recursive)
#[tauri::command]
pub fn delete_zfs_dataset(dataset: &str, recursive: bool) -> Result<String, String> {
    if dataset.trim().is_empty() {
        return Err("dataset is required".into());
    }

    // Extract zpool name from dataset path (format: zpool/dataset)
    let zpool = dataset.split('/').next().unwrap_or("").to_string();

    let args = if recursive {
        vec!["zfs", "destroy", "-r", dataset]
    } else {
        vec!["zfs", "destroy", dataset]
    };
    // convert to slice of &str
    let args_ref: Vec<&str> = args.to_vec();
    run_command(&args_ref).map_err(|e| e.to_string())?;

    // Invalidate cache since we've modified datasets
    if let Ok(mut cache) = DISK_CACHE.try_write() {
        cache.datasets.remove(&zpool);
        cache.last_updated = std::time::SystemTime::UNIX_EPOCH; // Force refresh next time
    }

    Ok(format!("Destroyed dataset {}", dataset))
}

// New: rename dataset (zfs rename old new)
#[tauri::command]
pub fn rename_zfs_dataset(old: &str, new: &str) -> Result<String, String> {
    if old.trim().is_empty() || new.trim().is_empty() {
        return Err("old and new dataset names are required".into());
    }

    // Extract zpool name from old dataset path (format: zpool/dataset)
    let zpool = old.split('/').next().unwrap_or("").to_string();

    // zfs rename <old> <new>
    run_command(&["zfs", "rename", old, new]).map_err(|e| e.to_string())?;

    // Invalidate cache since we've modified datasets
    if let Ok(mut cache) = DISK_CACHE.try_write() {
        cache.datasets.remove(&zpool);
        cache.last_updated = std::time::SystemTime::UNIX_EPOCH; // Force refresh next time
    }

    Ok(format!("Renamed {} -> {}", old, new))
}
