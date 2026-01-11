use crate::core::image::{CreateImageRequest, ImageManager, ImportImageRequest, ImageInfo};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

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

pub async fn list_masters(
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

    // Filter images where parent_id matches the given id
    let snapshots: Vec<crate::types::image::Snapshot> = images
        .into_iter()
        .filter(|img| img.parent_id.as_ref() == Some(&id))
        .map(|snap| crate::types::image::Snapshot {
            name: snap.name,
            created: snap.created_at.to_rfc3339(),
            used: format!("{}GB", snap.size_gb),
        })
        .collect();

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
    log::info!("Successfully resized image '{}' to {} GB", image.name, request.new_size_gb);
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
