use crate::core::image::{CreateImageRequest, ImageManager, UpdateImageRequest};
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
