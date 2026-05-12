#![allow(dead_code)]

use crate::core::image::{CreateImageRequest, Image, ImageInfo, ImageManager, ImportImageRequest};
use crate::state::AppState;
use tauri::State;

pub async fn list_images(state: State<'_, AppState>) -> Result<Vec<Image>, String> {
    let settings = state.settings.read().await;
    let manager = ImageManager::new(
        state.db_pool.clone(),
        settings.storage.images_dir.clone(),
        settings.storage.snapshots_dir.clone(),
    );
    manager.list().await.map_err(|e| e.to_string())
}

pub async fn get_image(state: State<'_, AppState>, id: String) -> Result<Image, String> {
    let settings = state.settings.read().await;
    let manager = ImageManager::new(
        state.db_pool.clone(),
        settings.storage.images_dir.clone(),
        settings.storage.snapshots_dir.clone(),
    );
    manager.get(&id).await.map_err(|e| e.to_string())
}

pub async fn create_image_command(
    state: State<'_, AppState>,
    request: CreateImageRequest,
) -> Result<Image, String> {
    let settings = state.settings.read().await;
    let manager = ImageManager::new(
        state.db_pool.clone(),
        settings.storage.images_dir.clone(),
        settings.storage.snapshots_dir.clone(),
    );
    manager.create(request).await.map_err(|e| e.to_string())
}

pub async fn import_image(
    state: State<'_, AppState>,
    request: ImportImageRequest,
) -> Result<Image, String> {
    let settings = state.settings.read().await;
    let manager = ImageManager::new(
        state.db_pool.clone(),
        settings.storage.images_dir.clone(),
        settings.storage.snapshots_dir.clone(),
    );
    manager.import(request).await.map_err(|e| e.to_string())
}

pub async fn delete_image_command(
    state: State<'_, AppState>,
    id: String,
    force: bool,
) -> Result<(), String> {
    let settings = state.settings.read().await;
    let manager = ImageManager::new(
        state.db_pool.clone(),
        settings.storage.images_dir.clone(),
        settings.storage.snapshots_dir.clone(),
    );
    manager.delete(&id, force).await.map_err(|e| e.to_string())
}

pub async fn clone_image(
    state: State<'_, AppState>,
    source_id: String,
    new_name: String,
) -> Result<Image, String> {
    let settings = state.settings.read().await;
    let manager = ImageManager::new(
        state.db_pool.clone(),
        settings.storage.images_dir.clone(),
        settings.storage.snapshots_dir.clone(),
    );
    manager
        .clone_image(&source_id, &new_name)
        .await
        .map_err(|e| e.to_string())
}

pub async fn create_snapshot_command(
    state: State<'_, AppState>,
    source_id: String,
    snapshot_name: String,
) -> Result<Image, String> {
    let settings = state.settings.read().await;
    let manager = ImageManager::new(
        state.db_pool.clone(),
        settings.storage.images_dir.clone(),
        settings.storage.snapshots_dir.clone(),
    );
    manager
        .create_snapshot(&source_id, &snapshot_name)
        .await
        .map_err(|e| e.to_string())
}

pub async fn get_image_info(state: State<'_, AppState>, id: String) -> Result<ImageInfo, String> {
    let settings = state.settings.read().await;
    let manager = ImageManager::new(
        state.db_pool.clone(),
        settings.storage.images_dir.clone(),
        settings.storage.snapshots_dir.clone(),
    );
    manager.get_info(&id).await.map_err(|e| e.to_string())
}

pub async fn resize_image(
    state: State<'_, AppState>,
    id: String,
    new_size_gb: u64,
) -> Result<Image, String> {
    let settings = state.settings.read().await;
    let manager = ImageManager::new(
        state.db_pool.clone(),
        settings.storage.images_dir.clone(),
        settings.storage.snapshots_dir.clone(),
    );
    manager
        .resize(&id, new_size_gb)
        .await
        .map_err(|e| e.to_string())
}

pub async fn verify_image(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let settings = state.settings.read().await;
    let manager = ImageManager::new(
        state.db_pool.clone(),
        settings.storage.images_dir.clone(),
        settings.storage.snapshots_dir.clone(),
    );
    manager.verify(&id).await.map_err(|e| e.to_string())
}
