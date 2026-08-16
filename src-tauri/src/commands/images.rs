#![expect(
    dead_code,
    reason = "Old Tauri commands replaced by Axum handlers in api/handlers/images.rs"
)]

use crate::application::image_service::ImageService;
use crate::core::image::{CreateImageRequest, Image, ImageInfo, ImportImageRequest};
use crate::persistence::repositories::image::ImageRepository;
use crate::state::AppState;

use tauri::State;

fn image_service(state: &AppState) -> ImageService {
    ImageService::new(ImageRepository::new(state.db_pool.clone()))
}

pub async fn list_images(state: State<'_, AppState>) -> Result<Vec<Image>, String> {
    image_service(&state)
        .list()
        .await
        .map_err(|error| error.to_string())
}

pub async fn get_image(state: State<'_, AppState>, id: String) -> Result<Image, String> {
    image_service(&state)
        .get(&id)
        .await
        .map_err(|error| error.to_string())
}

pub async fn create_image_command(
    state: State<'_, AppState>,
    request: CreateImageRequest,
) -> Result<Image, String> {
    image_service(&state)
        .create(request)
        .await
        .map_err(|error| error.to_string())
}

pub async fn import_image(
    state: State<'_, AppState>,
    request: ImportImageRequest,
) -> Result<Image, String> {
    image_service(&state)
        .import(request)
        .await
        .map_err(|error| error.to_string())
}

pub async fn delete_image_command(
    state: State<'_, AppState>,
    id: String,
    _force: bool,
) -> Result<(), String> {
    /*
     * `force` is retained in the command signature for
     * compatibility with the old frontend/Tauri API.
     *
     * The new ImageService intentionally does not support
     * force deletion because deleting a master with dependent
     * snapshots is a lifecycle violation.
     */
    image_service(&state)
        .delete(&id)
        .await
        .map_err(|error| error.to_string())
}

pub async fn clone_image(
    state: State<'_, AppState>,
    source_id: String,
    snapshot_name: String,
    new_name: String,
) -> Result<Image, String> {
    image_service(&state)
        .clone_image(&source_id, &snapshot_name, &new_name)
        .await
        .map_err(|error| error.to_string())
}

pub async fn create_snapshot_command(
    state: State<'_, AppState>,
    source_id: String,
    snapshot_name: String,
) -> Result<Image, String> {
    image_service(&state)
        .create_snapshot(&source_id, &snapshot_name)
        .await
        .map_err(|error| error.to_string())
}

pub async fn get_image_info(state: State<'_, AppState>, id: String) -> Result<ImageInfo, String> {
    image_service(&state)
        .get_info(&id)
        .await
        .map_err(|error| error.to_string())
}

pub async fn resize_image(
    state: State<'_, AppState>,
    id: String,
    new_size_gb: u64,
) -> Result<Image, String> {
    image_service(&state)
        .resize(&id, new_size_gb)
        .await
        .map_err(|error| error.to_string())
}

pub async fn verify_image(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    image_service(&state)
        .verify(&id)
        .await
        .map_err(|error| error.to_string())
}
