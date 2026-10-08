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
        .args(["list", "-H", "-o", "name"])
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
    let output = Command::new("zpool").args(["list", "-H"]).output();

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
        .args([
            "list",
            "-H",
            "-o",
            "name,used,avail,refer,mountpoint,origin",
            "-r",
            &params.zpool,
        ])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let content = String::from_utf8_lossy(&output.stdout);
            let datasets: Vec<Dataset> = content
                .lines()
                .filter(|l| !l.is_empty())
                .filter_map(|line| {
                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.len() >= 6 {
                        let name = parts[0];
                        if should_include_managed_disk(name, parts[5]) {
                            // Get the custom property org.diskless:type
                            let disk_type = match Command::new("zfs")
                                .args(["get", "-H", "-o", "value", "org.diskless:type", name])
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

fn should_include_managed_disk(name: &str, origin: &str) -> bool {
    let origin = origin.trim();
    name.contains('-') && (origin.is_empty() || origin == "-")
}

pub async fn create_dataset(
    Json(request): Json<CreateDatasetRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let is_game_disk = request.usage_type == "game" || request.usage_type == "game_disk";
    // Strip a pre-existing `-games` suffix so retries and explicit names
    // never double it (`steam-games` stays `steam-games`).
    let base_name = request.name.trim();
    let base_name = if is_game_disk {
        base_name.strip_suffix("-games").unwrap_or(base_name)
    } else {
        base_name
    };
    let dataset_name = if is_game_disk {
        format!("{}/games/{}-games", request.zpool, base_name)
    } else {
        format!("{}/{}", request.zpool, base_name)
    };

    // Validate before creating anything so a 400 leaves no parent dataset
    // behind as a side effect.
    let game_size: Option<&str> = if is_game_disk {
        match request
            .size
            .as_deref()
            .map(str::trim)
            .filter(|size| !size.is_empty())
        {
            Some(size) => Some(size),
            None => {
                tracing::error!("Game disks require a size");
                return Err(StatusCode::BAD_REQUEST);
            }
        }
    } else {
        None
    };

    // zfs create requires root
    let mut cmd = Command::new("sudo");
    cmd.args(["-n", "zfs", "create"]);

    if is_game_disk {
        let games_parent = format!("{}/games", request.zpool);
        let _ = Command::new("sudo")
            .args([
                "-n",
                "zfs",
                "create",
                "-p",
                "-o",
                "org.diskless:type=games",
                &games_parent,
            ])
            .output();
        cmd.args(["-V", game_size.unwrap_or_default()]);
    }

    if !is_game_disk {
        if let Some(size) = request.size {
            let size = size.trim();
            if !size.is_empty() {
                cmd.args(["-o", &format!("quota={}", size)]);
            }
        }
    }

    cmd.arg(&dataset_name);

    match cmd.output() {
        Ok(output) if output.status.success() => {
            // Set the org.diskless:type property so list_datasets can find it
            let _ = Command::new("sudo")
                .args([
                    "-n",
                    "zfs",
                    "set",
                    &format!("org.diskless:type={}", request.usage_type),
                    &dataset_name,
                ])
                .output();

            // Enable compression for image datasets
            if request.usage_type == "image" {
                let _ = Command::new("sudo")
                    .args(["-n", "zfs", "set", "compression=lz4", &dataset_name])
                    .output();
            }

            Ok(Json(serde_json::json!({
                "success": true,
                "dataset": dataset_name,
                "message": format!("Dataset {} created successfully", dataset_name)
            })))
        }
        Ok(output) => {
            let error = String::from_utf8_lossy(&output.stderr);
            tracing::error!("Failed to create dataset {}: {}", dataset_name, error);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
        Err(e) => {
            tracing::error!("Failed to execute zfs create for {}: {}", dataset_name, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn delete_dataset(
    State(state): State<AppState>,
    Path(dataset): Path<String>,
    Json(request): Json<DeleteDatasetRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    // Game masters must not be destroyed while clients depend on them:
    // per-client clones would be left pointing at a dead zvol, breaking
    // every opted-in target until manual targetcli cleanup. This applies
    // to recursive deletes too: remove the clients' selections (or the
    // clients) first.
    {
        let mut dependents: Vec<String> = Vec::new();
        if let Ok(clones) =
            crate::application::storage_service::StorageService::list_game_clones()
        {
            dependents.extend(
                clones
                    .into_iter()
                    .filter(|clone| clone.master_dataset == dataset)
                    .map(|clone| clone.client_id),
            );
        }
        if let Ok(rows) = sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT client_id FROM client_game_disks WHERE master_dataset = ?",
        )
        .bind(&dataset)
        .fetch_all(&state.db_pool)
        .await
        {
            dependents.extend(rows);
        }
        dependents.sort();
        dependents.dedup();
        if !dependents.is_empty() {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "success": false,
                    "dataset": dataset,
                    "dependents": dependents,
                    "message": format!(
                        "Dataset {} is used by game selections of clients: {}. Remove the selections or delete the clients first.",
                        dataset,
                        dependents.join(", ")
                    )
                })),
            );
        }
    }

    let mut cmd = Command::new("sudo");
    cmd.args(["-n", "zfs", "destroy"]);

    if request.recursive {
        cmd.arg("-r");
    }

    cmd.arg(&dataset);

    match cmd.output() {
        Ok(output) if output.status.success() => (
            StatusCode::OK,
            Json(serde_json::json!({
                "success": true,
                "message": format!("Dataset {} deleted successfully", dataset)
            })),
        ),
        Ok(output) => {
            let error = String::from_utf8_lossy(&output.stderr);
            tracing::error!("Failed to delete dataset {}: {}", dataset, error);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "success": false,
                    "message": format!("Failed to delete dataset {}: {}", dataset, error.trim())
                })),
            )
        }
        Err(e) => {
            tracing::error!("Failed to execute zfs destroy for {}: {}", dataset, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "success": false,
                    "message": format!("Failed to execute zfs destroy for {}: {}", dataset, e)
                })),
            )
        }
    }
}

/// List game master volumes with their per-client clones and selection
/// usage. Powers the client game-disk picker.
///
/// `GET /api/zfs/game-disks` returns
/// `{ disks: [{ dataset, disk_type, size_bytes, used_by, clones }] }`
/// where `used_by` is the sorted union of clone owners and selection
/// rows, and `clones` lists `{ client_id, clone_dataset }`.
pub async fn list_game_disks(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    use crate::application::storage_service::StorageService;

    let masters = StorageService::discover_game_masters().map_err(|error| {
        tracing::error!("Failed to list game disks: {}", error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let clones = StorageService::list_game_clones().map_err(|error| {
        tracing::error!("Failed to list game clones: {}", error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let selections: Vec<(String, String)> =
        sqlx::query_as::<_, (String, String)>(
            "SELECT client_id, master_dataset FROM client_game_disks",
        )
        .fetch_all(&state.db_pool)
        .await
        .map_err(|error| {
            tracing::error!("Failed to list game selections: {}", error);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let disks: Vec<serde_json::Value> = masters
        .into_iter()
        .map(|master| {
            let mut used_by: Vec<String> = clones
                .iter()
                .filter(|clone| clone.master_dataset == master.dataset)
                .map(|clone| clone.client_id.clone())
                .chain(
                    selections
                        .iter()
                        .filter(|(_, selected)| selected == &master.dataset)
                        .map(|(client_id, _)| client_id.clone()),
                )
                .collect();
            used_by.sort();
            used_by.dedup();
            let master_clones: Vec<serde_json::Value> = clones
                .iter()
                .filter(|clone| clone.master_dataset == master.dataset)
                .map(|clone| {
                    serde_json::json!({
                        "client_id": clone.client_id,
                        "clone_dataset": clone.clone_dataset,
                    })
                })
                .collect();
            serde_json::json!({
                "dataset": master.dataset,
                "disk_type": master.disk_type,
                "size_bytes": master.size_bytes,
                "used_by": used_by,
                "clones": master_clones,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "disks": disks })))
}

#[cfg(test)]
mod disk_listing_tests {
    use super::should_include_managed_disk;

    #[test]
    fn client_snapshot_clones_are_not_managed_disks() {
        assert!(!should_include_managed_disk(
            "diskless/PC001-disk",
            "diskless/image-disk/windows11@ready"
        ));
    }

    #[test]
    fn independent_managed_disks_remain_visible() {
        assert!(should_include_managed_disk("diskless/image-disk", "-"));
    }
}
