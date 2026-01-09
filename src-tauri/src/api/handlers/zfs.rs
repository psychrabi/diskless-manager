use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use log::info;
use serde::{Deserialize, Serialize};
use std::process::Command;

use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct ZpoolStats {
    pub name: String,
    pub size: String,
    pub allocated: String,
    pub free: String,
    pub health: String,
}

#[derive(Debug, Deserialize)]
pub struct DatasetQuery {
    zpool: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Dataset {
    pub name: String,
    pub used: String,
    pub available: String,
    pub referenced: String,
    pub mountpoint: String,
    pub disk_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateDatasetRequest {
    pub zpool: String,
    pub name: String,
    pub usage_type: String,
    pub size: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteDatasetRequest {
    pub recursive: bool,
}

pub async fn list_zpools(State(_state): State<AppState>) -> Result<Json<Vec<String>>, StatusCode> {
    let output = Command::new("zpool")
        .args(&["list", "-H", "-o", "name"])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let content = String::from_utf8_lossy(&output.stdout);
            let pools: Vec<String> = content
                .lines()
                .map(|line| line.trim().to_string())
                .filter(|line| !line.is_empty())
                .collect();
            Ok(Json(pools))
        }
        _ => Ok(Json(vec![])),
    }
}

pub async fn get_zpool_stats(
    State(_state): State<AppState>,
) -> Result<Json<Vec<ZpoolStats>>, StatusCode> {
    let output = Command::new("zpool").args(&["list", "-H"]).output();

    match output {
        Ok(output) if output.status.success() => {
            let content = String::from_utf8_lossy(&output.stdout);
            let stats: Vec<ZpoolStats> = content
                .lines()
                .filter_map(|line| {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 5 {
                        Some(ZpoolStats {
                            name: parts[0].to_string(),
                            size: parts[1].to_string(),
                            allocated: parts[2].to_string(),
                            free: parts[3].to_string(),
                            health: parts[4].to_string(),
                        })
                    } else {
                        None
                    }
                })
                .collect();
            info!("Zpool stats: {:?}", stats);
            Ok(Json(stats))
        }
        _ => Ok(Json(vec![])),
    }
}

pub async fn list_datasets(
    Query(params): Query<DatasetQuery>,
) -> Result<Json<Vec<Dataset>>, StatusCode> {
    use std::process::Command;

    let output = Command::new("zfs")
        .args(&[
            "list",
            "-H",
            "-o",
            "name,used,avail,refer,mountpoint",
            "-r",
            &params.zpool,
        ])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let content = String::from_utf8_lossy(&output.stdout);
            let mut datasets: Vec<Dataset> = content
                .lines()
                .filter(|l| !l.is_empty())
                .filter_map(|line| {
                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.len() >= 5 {
                        let name = parts[0];
                        // Only include datasets that have a '-' in their name
                        if name.contains('-') {
                            // Get the custom property org.diskless:type
                            let disk_type = match Command::new("zfs")
                                .args(&["get", "-H", "-o", "value", "org.diskless:type", name])
                                .output()
                            {
                                Ok(output) if output.status.success() => {
                                    let v = String::from_utf8_lossy(&output.stdout);
                                    let v = v.trim();
                                    // treat '-' or 'none' (zfs placeholder) as not set
                                    if v.is_empty() || v == "-" || v.eq_ignore_ascii_case("none") {
                                        None
                                    } else {
                                        Some(v.to_string())
                                    }
                                }
                                _ => None,
                            };

                            Some(Dataset {
                                name: name.to_string(),
                                used: parts[1].to_string(),
                                available: parts[2].to_string(),
                                referenced: parts[3].to_string(),
                                mountpoint: parts[4].to_string(),
                                disk_type,
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect();

            // Exclude any dataset that is a child of another dataset in the list
            let names: Vec<String> = datasets.iter().map(|d| d.name.clone()).collect();
            let mut result: Vec<Dataset> = Vec::new();

            'outer: for ds in datasets.into_iter() {
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

            Ok(Json(result))
        }
        _ => Ok(Json(vec![])),
    }
}

pub async fn create_dataset(
    Json(request): Json<CreateDatasetRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let dataset_name = format!("{}/{}", request.zpool, request.name);

    let mut cmd = Command::new("zfs");
    cmd.args(&["create"]);

    // Add size quota if provided
    if let Some(size) = request.size {
        cmd.args(&["-o", &format!("quota={}", size)]);
    }

    cmd.arg(&dataset_name);

    match cmd.output() {
        Ok(output) if output.status.success() => Ok(Json(serde_json::json!({
            "success": true,
            "message": format!("Dataset {} created successfully", dataset_name)
        }))),
        Ok(output) => {
            let _error = String::from_utf8_lossy(&output.stderr);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn delete_dataset(
    Path(dataset): Path<String>,
    Json(request): Json<DeleteDatasetRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut cmd = Command::new("zfs");
    cmd.args(&["destroy"]);

    if request.recursive {
        cmd.arg("-r");
    }

    cmd.arg(&dataset);

    match cmd.output() {
        Ok(output) if output.status.success() => Ok(Json(serde_json::json!({
            "success": true,
            "message": format!("Dataset {} deleted successfully", dataset)
        }))),
        Ok(_output) => Err(StatusCode::INTERNAL_SERVER_ERROR),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}
