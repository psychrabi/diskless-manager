use crate::core::image::{CreateImageRequest, Image, ImageInfo, ImageManager, ImportImageRequest};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

// Helper function to convert images to snapshots for a given parent_id
fn images_to_snapshots(images: &[Image], parent_id: &str) -> Vec<crate::types::image::Snapshot> {
    images
        .iter()
        .filter(|img| img.parent_id.as_ref() == Some(&parent_id.to_string()))
        .map(|snap| crate::types::image::Snapshot {
            name: snap.name.clone(),
            created: snap.created_at.to_rfc3339(),
            used: format!("{}GB", snap.size_gb),
            size: Some(format!("{}GB", snap.size_gb)),
        })
        .collect()
}

pub async fn list_images(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::core::image::Image>>, StatusCode> {
    let settings = state.settings.read().await;
    let manager = ImageManager::new(
        state.db_pool.clone(),
        settings.storage.images_dir.clone(),
        settings.storage.snapshots_dir.clone(),
    );

    let images = manager
        .list()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(images))
}

#[derive(Serialize)]
pub struct MasterWithSnapshots {
    #[serde(flatten)]
    pub image: crate::core::image::Image,
    pub snapshots: Vec<crate::types::image::Snapshot>,
}

pub async fn list_masters(
    State(state): State<AppState>,
) -> Result<Json<Vec<MasterWithSnapshots>>, StatusCode> {
    let settings = state.settings.read().await;
    let manager = ImageManager::new(
        state.db_pool.clone(),
        settings.storage.images_dir.clone(),
        settings.storage.snapshots_dir.clone(),
    );

    let images = manager
        .list()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    log::info!("list_masters: Found {} total images", images.len());
    for img in &images {
        log::info!(
            "  Image: id={}, name={}, parent_id={:?}",
            img.id,
            img.name,
            img.parent_id
        );
    }

    // Separate masters (no parent_id) from snapshots (have parent_id)
    let mut masters_with_snapshots = Vec::new();

    for image in images.iter() {
        // Only include images that are NOT snapshots (parent_id is None)
        if image.parent_id.is_none() {
            // Use helper function to get snapshots for this master
            let snapshots = images_to_snapshots(&images, &image.id);

            log::info!(
                "Master '{}' (id={}) has {} snapshots",
                image.name,
                image.id,
                snapshots.len()
            );

            masters_with_snapshots.push(MasterWithSnapshots {
                image: image.clone(),
                snapshots,
            });
        }
    }

    Ok(Json(masters_with_snapshots))
}

pub async fn get_image(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<crate::core::image::Image>, StatusCode> {
    let settings = state.settings.read().await;
    let manager = ImageManager::new(
        state.db_pool.clone(),
        settings.storage.images_dir.clone(),
        settings.storage.snapshots_dir.clone(),
    );

    let image = manager.get(&id).await.map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(Json(image))
}

pub async fn get_snapshots(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<crate::types::image::Snapshot>>, StatusCode> {
    let settings = state.settings.read().await;
    let manager = ImageManager::new(
        state.db_pool.clone(),
        settings.storage.images_dir.clone(),
        settings.storage.snapshots_dir.clone(),
    );

    let images = manager
        .list()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Use helper function to get snapshots for the given image id
    let snapshots = images_to_snapshots(&images, &id);

    Ok(Json(snapshots))
}

pub async fn create_image(
    State(state): State<AppState>,
    Json(request): Json<CreateImageRequest>,
) -> Result<Json<crate::core::image::Image>, StatusCode> {
    let settings = state.settings.read().await;
    let manager = ImageManager::new(
        state.db_pool.clone(),
        settings.storage.images_dir.clone(),
        settings.storage.snapshots_dir.clone(),
    );

    let image = manager
        .create(request)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(image))
}

pub async fn update_image(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::core::image::UpdateImageRequest>,
) -> Result<Json<crate::core::image::Image>, StatusCode> {
    log::info!(
        "Received update request for image id '{}', request: {:?}",
        id,
        request
    );
    let settings = state.settings.read().await;
    let manager = ImageManager::new(
        state.db_pool.clone(),
        settings.storage.images_dir.clone(),
        settings.storage.snapshots_dir.clone(),
    );

    let image = manager.update(&id, request).await.map_err(|e| {
        log::error!("Failed to update image '{}': {}", id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    log::info!("Successfully updated image '{}'", image.name);
    Ok(Json(image))
}

#[derive(Deserialize)]
pub struct RenameImageRequest {
    pub new_name: String,
}

pub async fn rename_image(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<RenameImageRequest>,
) -> Result<Json<crate::core::image::Image>, StatusCode> {
    log::info!(
        "Received rename request for image id '{}' to new name '{}'",
        id,
        request.new_name
    );
    let settings = state.settings.read().await;
    let manager = ImageManager::new(
        state.db_pool.clone(),
        settings.storage.images_dir.clone(),
        settings.storage.snapshots_dir.clone(),
    );

    let image = manager.rename(&id, &request.new_name).await.map_err(|e| {
        log::error!("Failed to rename image '{}': {}", id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    log::info!("Successfully renamed image to '{}'", image.name);
    Ok(Json(image))
}

pub async fn delete_image(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(), StatusCode> {
    let settings = state.settings.read().await;
    let manager = ImageManager::new(
        state.db_pool.clone(),
        settings.storage.images_dir.clone(),
        settings.storage.snapshots_dir.clone(),
    );

    manager
        .delete(&id, false)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(())
}

pub async fn import_image(
    State(state): State<AppState>,
    Json(request): Json<ImportImageRequest>,
) -> Result<Json<crate::core::image::Image>, StatusCode> {
    log::info!("Received import request for image '{}'", request.name);
    let settings = state.settings.read().await;
    let manager = ImageManager::new(
        state.db_pool.clone(),
        settings.storage.images_dir.clone(),
        settings.storage.snapshots_dir.clone(),
    );

    let image = manager.import(request).await.map_err(|e| {
        log::error!("Failed to import image: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    log::info!("Successfully imported image '{}'", image.name);
    Ok(Json(image))
}

#[derive(Deserialize)]
pub struct CloneImageRequest {
    pub new_name: String,
}

pub async fn clone_image(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<CloneImageRequest>,
) -> Result<Json<crate::core::image::Image>, StatusCode> {
    log::info!(
        "Received clone request for image id '{}' with new name '{}'",
        id,
        request.new_name
    );
    let settings = state.settings.read().await;
    let manager = ImageManager::new(
        state.db_pool.clone(),
        settings.storage.images_dir.clone(),
        settings.storage.snapshots_dir.clone(),
    );

    let image = manager
        .clone_image(&id, &request.new_name)
        .await
        .map_err(|e| {
            log::error!("Failed to clone image '{}': {}", id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    log::info!("Successfully cloned image to '{}'", image.name);
    Ok(Json(image))
}

#[derive(Deserialize)]
pub struct CreateSnapshotRequest {
    pub snapshot_name: String,
}

pub async fn create_snapshot(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<CreateSnapshotRequest>,
) -> Result<Json<crate::core::image::Image>, StatusCode> {
    log::info!(
        "Received snapshot request for image id '{}' with name '{}'",
        id,
        request.snapshot_name
    );
    let settings = state.settings.read().await;
    let manager = ImageManager::new(
        state.db_pool.clone(),
        settings.storage.images_dir.clone(),
        settings.storage.snapshots_dir.clone(),
    );

    let image = manager
        .create_snapshot(&id, &request.snapshot_name)
        .await
        .map_err(|e| {
            log::error!("Failed to create snapshot for image '{}': {}", id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    log::info!("Successfully created snapshot '{}'", image.name);
    Ok(Json(image))
}

pub async fn get_image_info(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ImageInfo>, StatusCode> {
    log::info!("Received get info request for image id '{}'", id);
    let settings = state.settings.read().await;
    let manager = ImageManager::new(
        state.db_pool.clone(),
        settings.storage.images_dir.clone(),
        settings.storage.snapshots_dir.clone(),
    );

    let info = manager.get_info(&id).await.map_err(|e| {
        log::error!("Failed to get info for image '{}': {}", id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    log::info!("Successfully retrieved info for image '{}'", id);
    Ok(Json(info))
}

#[derive(Deserialize)]
pub struct ResizeImageRequest {
    pub new_size_gb: u64,
}

pub async fn resize_image(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ResizeImageRequest>,
) -> Result<Json<crate::core::image::Image>, StatusCode> {
    log::info!(
        "Received resize request for image id '{}' to {} GB",
        id,
        request.new_size_gb
    );
    let settings = state.settings.read().await;
    let manager = ImageManager::new(
        state.db_pool.clone(),
        settings.storage.images_dir.clone(),
        settings.storage.snapshots_dir.clone(),
    );

    let image = manager
        .resize(&id, request.new_size_gb)
        .await
        .map_err(|e| {
            log::error!("Failed to resize image '{}': {}", id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    log::info!(
        "Successfully resized image '{}' to {} GB",
        image.name,
        request.new_size_gb
    );
    Ok(Json(image))
}

pub async fn verify_image(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    log::info!("Received verify request for image id '{}'", id);
    let settings = state.settings.read().await;
    let manager = ImageManager::new(
        state.db_pool.clone(),
        settings.storage.images_dir.clone(),
        settings.storage.snapshots_dir.clone(),
    );

    let is_valid = manager.verify(&id).await.map_err(|e| {
        log::error!("Failed to verify image '{}': {}", id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    log::info!("Image '{}' verification result: {}", id, is_valid);
    Ok(Json(serde_json::json!({ "valid": is_valid })))
}

pub async fn delete_snapshot(
    State(state): State<AppState>,
    Path((master_name, snapshot_name)): Path<(String, String)>,
) -> Result<(), StatusCode> {
    log::info!(
        "Received delete snapshot request for master '{}', snapshot '{}'",
        master_name,
        snapshot_name
    );
    let settings = state.settings.read().await;
    let manager = ImageManager::new(
        state.db_pool.clone(),
        settings.storage.images_dir.clone(),
        settings.storage.snapshots_dir.clone(),
    );

    // Get all images to find the master and snapshot
    let images = manager.list().await.map_err(|e| {
        log::error!("Failed to list images: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // First, find the master image by name (could be ID or name)
    let master = images
        .iter()
        .find(|img| img.id == master_name || img.name == master_name)
        .ok_or_else(|| {
            log::error!("Master image '{}' not found", master_name);
            StatusCode::NOT_FOUND
        })?;

    log::info!("Found master image with id: {}", master.id);

    // Find the snapshot by name and parent_id
    let snapshot = images
        .iter()
        .find(|img| img.name == snapshot_name && img.parent_id.as_ref() == Some(&master.id))
        .ok_or_else(|| {
            log::error!(
                "Snapshot '{}' not found for master '{}'",
                snapshot_name,
                master_name
            );
            StatusCode::NOT_FOUND
        })?;

    log::info!("Found snapshot with id: {}", snapshot.id);

    // Delete the snapshot using its ID
    manager.delete(&snapshot.id, false).await.map_err(|e| {
        log::error!("Failed to delete snapshot '{}': {}", snapshot_name, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    log::info!("Successfully deleted snapshot '{}'", snapshot_name);
    Ok(())
}

pub async fn set_default_image(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    log::info!("Received set-default request for image '{}'", id);

    // Clear is_default on all images, then set it on the target
    sqlx::query("UPDATE images SET is_default = 0")
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            log::error!("Failed to clear default image flags: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Find image by id or name and set as default
    let result = sqlx::query("UPDATE images SET is_default = 1 WHERE id = ? OR name = ?")
        .bind(&id)
        .bind(&id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            log::error!("Failed to set default image '{}': {}", id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if result.rows_affected() == 0 {
        log::error!("Image '{}' not found", id);
        return Err(StatusCode::NOT_FOUND);
    }

    // Also persist to app_config for backward compatibility
    let _ = sqlx::query("INSERT OR REPLACE INTO app_config (key, value) VALUES ('default_master', ?)")
        .bind(&id)
        .execute(&state.db_pool)
        .await;

    log::info!("Successfully set '{}' as default image", id);
    Ok(Json(serde_json::json!({
        "message": format!("Image '{}' set as default successfully", id)
    })))
}

pub async fn rollback_snapshot(
    State(state): State<AppState>,
    Path((master_name, snapshot_name)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    log::info!(
        "Received rollback snapshot request for master '{}', snapshot '{}'",
        master_name,
        snapshot_name
    );
    let settings = state.settings.read().await;
    let manager = ImageManager::new(
        state.db_pool.clone(),
        settings.storage.images_dir.clone(),
        settings.storage.snapshots_dir.clone(),
    );

    // Get all images to find the master and snapshot
    let images = manager.list().await.map_err(|e| {
        log::error!("Failed to list images: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // First, find the master image by name (could be ID or name)
    let master = images
        .iter()
        .find(|img| img.id == master_name || img.name == master_name)
        .ok_or_else(|| {
            log::error!("Master image '{}' not found", master_name);
            StatusCode::NOT_FOUND
        })?;

    log::info!("Found master image: {} (id: {})", master.name, master.id);

    // Find the target snapshot to get its creation time
    let target_snapshot = images
        .iter()
        .find(|img| img.name == snapshot_name && img.parent_id.as_ref() == Some(&master.id))
        .ok_or_else(|| {
            log::error!(
                "Snapshot '{}' not found for master '{}'",
                snapshot_name,
                master_name
            );
            StatusCode::NOT_FOUND
        })?;

    log::info!("Target snapshot created at: {}", target_snapshot.created_at);

    // Find all snapshots newer than the target snapshot (will be destroyed by rollback -r)
    let newer_snapshots: Vec<String> = images
        .iter()
        .filter(|img| {
            img.parent_id.as_ref() == Some(&master.id)
                && img.created_at > target_snapshot.created_at
        })
        .map(|img| img.id.clone())
        .collect();

    log::info!(
        "Found {} newer snapshots that will be destroyed by rollback",
        newer_snapshots.len()
    );

    // Construct the full ZFS snapshot path
    let snapshot_full_path = format!("{}@{}", master.name, snapshot_name);
    log::info!("Full ZFS snapshot path: {}", snapshot_full_path);

    // Perform the ZFS rollback (destroys newer snapshots and their clones)
    use crate::cmd::run_command;
    run_command(&["zfs", "rollback", "-r", &snapshot_full_path]).map_err(|e| {
        log::error!(
            "Failed to rollback snapshot '{}': {}",
            snapshot_full_path,
            e
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    log::info!("Successfully rolled back to snapshot '{}'", snapshot_name);

    // Delete newer snapshots from database
    if !newer_snapshots.is_empty() {
        let placeholders = newer_snapshots
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let query = format!("DELETE FROM images WHERE id IN ({})", placeholders);

        let mut query_builder = sqlx::query(&query);
        for id in &newer_snapshots {
            query_builder = query_builder.bind(id);
        }

        query_builder.execute(&state.db_pool).await.map_err(|e| {
            log::error!("Failed to delete newer snapshots from database: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        log::info!(
            "Deleted {} newer snapshots from database",
            newer_snapshots.len()
        );
    }

    Ok(Json(serde_json::json!({
        "message": format!("Successfully rolled back to snapshot '{}' and removed {} newer snapshots", snapshot_name, newer_snapshots.len())
    })))
}
