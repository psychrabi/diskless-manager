use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::core::image::{CreateImageRequest, ImageManager};
use crate::state::AppState;

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
    State(_state): State<AppState>,
    Path(_id): Path<String>,
    Json(_request): Json<serde_json::Value>,
) -> Result<Json<crate::core::image::Image>, StatusCode> {
    // Placeholder implementation - update functionality may need to be added to ImageManager
    Err(StatusCode::NOT_IMPLEMENTED)
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
