use crate::application::image_service::ImageService;
use crate::core::image::Image;
use crate::persistence::repositories::image::ImageRepository;
use crate::state::AppState;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

fn image_service(state: &AppState) -> ImageService {
    ImageService::new(ImageRepository::new(state.db_pool.clone()))
}

fn images_to_snapshots(images: &[Image], parent_id: &str) -> Vec<crate::types::image::Snapshot> {
    images
        .iter()
        .filter(|img| {
            img.kind == crate::core::image::ImageKind::Snapshot
                && img.parent_id.as_deref() == Some(parent_id)
        })
        .map(|snap| crate::types::image::Snapshot {
            name: snap.name.clone(),
            created: snap.created_at.to_rfc3339(),
            used: format!("{}GB", snap.size_gb),
            size: Some(format!("{}GB", snap.size_gb)),
        })
        .collect()
}

pub async fn list_images(State(state): State<AppState>) -> Result<Json<Vec<Image>>, StatusCode> {
    let service = image_service(&state);

    service.list().await.map(Json).map_err(|error| {
        log::error!("Failed to list images: {}", error);

        StatusCode::INTERNAL_SERVER_ERROR
    })
}

#[derive(Serialize)]
pub struct MasterWithSnapshots {
    #[serde(flatten)]
    pub image: Image,

    pub snapshots: Vec<crate::types::image::Snapshot>,
}

pub async fn list_masters(
    State(state): State<AppState>,
) -> Result<Json<Vec<MasterWithSnapshots>>, StatusCode> {
    let service = image_service(&state);

    let images = service.list().await.map_err(|error| {
        log::error!("Failed to list images: {}", error);

        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    log::info!("list_masters: Found {} total images", images.len());

    let mut masters_with_snapshots = Vec::new();

    for image in &images {
        if image.parent_id.is_none() {
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
) -> Result<Json<Image>, StatusCode> {
    let service = image_service(&state);

    service.get(&id).await.map(Json).map_err(|error| {
        log::error!("Failed to get image '{}': {}", id, error);

        StatusCode::NOT_FOUND
    })
}

pub async fn get_snapshots(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<Image>>, StatusCode> {
    let service = image_service(&state);

    service.snapshots(&id).await.map(Json).map_err(|error| {
        log::error!("Failed to get snapshots for '{}': {}", id, error);

        StatusCode::INTERNAL_SERVER_ERROR
    })
}

pub async fn create_image(
    State(state): State<AppState>,
    Json(request): Json<crate::core::image::CreateImageRequest>,
) -> Result<Json<Image>, StatusCode> {
    let service = image_service(&state);

    service.create(request).await.map(Json).map_err(|error| {
        log::error!("Failed to create image: {}", error);

        StatusCode::INTERNAL_SERVER_ERROR
    })
}

pub async fn update_image(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::core::image::UpdateImageRequest>,
) -> Result<Json<Image>, StatusCode> {
    log::info!(
        "Received update request for image id '{}', request: {:?}",
        id,
        request
    );

    let service = image_service(&state);

    let image = service.update(&id, request).await.map_err(|error| {
        log::error!("Failed to update image '{}': {}", id, error);

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
) -> Result<Json<Image>, StatusCode> {
    let service = image_service(&state);

    service
        .rename(&id, &request.new_name)
        .await
        .map(Json)
        .map_err(|error| {
            log::error!("Failed to rename image '{}': {}", id, error);

            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn delete_image(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(), StatusCode> {
    let service = image_service(&state);

    match service.delete(&id).await {
        Ok(()) => Ok(()),

        Err(error) => {
            let message = error.to_string();

            log::error!("Failed to delete image '{}': {}", id, message);

            /*
             * Image lifecycle conflicts are expected application-level
             * conditions, not server failures.
             *
             * Examples:
             *
             * - master has snapshots
             * - master has clones
             * - clone has snapshots
             * - image is marked as default
             */
            if message.contains("while dependent snapshots or clones exist") {
                return Err(StatusCode::CONFLICT);
            }

            if message.contains("cannot delete the default image") {
                return Err(StatusCode::CONFLICT);
            }

            /*
             * Everything else remains an internal error until we
             * introduce a typed application error hierarchy.
             */
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn import_image(
    State(state): State<AppState>,
    Json(request): Json<crate::core::image::ImportImageRequest>,
) -> Result<Json<Image>, StatusCode> {
    let service = image_service(&state);

    service.import(request).await.map(Json).map_err(|error| {
        log::error!("Failed to import image: {}", error);

        StatusCode::INTERNAL_SERVER_ERROR
    })
}

#[derive(Debug, Deserialize)]
pub struct CloneImageRequest {
    pub snapshot_name: String,
    pub new_name: String,
}

pub async fn clone_image(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<CloneImageRequest>,
) -> Result<Json<Image>, StatusCode> {
    let service = image_service(&state);

    service
        .clone_image(&id, &request.snapshot_name, &request.new_name)
        .await
        .map(Json)
        .map_err(|error| {
            log::error!("Failed to clone image '{}': {}", id, error);

            StatusCode::INTERNAL_SERVER_ERROR
        })
}

#[derive(Deserialize)]
pub struct CreateSnapshotRequest {
    pub snapshot_name: String,
}

pub async fn create_snapshot(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<CreateSnapshotRequest>,
) -> Result<Json<Image>, StatusCode> {
    let service = image_service(&state);

    service
        .create_snapshot(&id, &request.snapshot_name)
        .await
        .map(Json)
        .map_err(|error| {
            log::error!(
                "Failed to create snapshot '{}': {}",
                request.snapshot_name,
                error
            );

            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn get_image_info(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<crate::core::image::ImageInfo>, StatusCode> {
    let service = image_service(&state);

    service.get_info(&id).await.map(Json).map_err(|error| {
        log::error!("Failed to get image info '{}': {}", id, error);

        StatusCode::INTERNAL_SERVER_ERROR
    })
}

#[derive(Deserialize)]
pub struct ResizeImageRequest {
    pub new_size_gb: u64,
}

pub async fn resize_image(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ResizeImageRequest>,
) -> Result<Json<Image>, StatusCode> {
    let service = image_service(&state);

    service
        .resize(&id, request.new_size_gb)
        .await
        .map(Json)
        .map_err(|error| {
            log::error!("Failed to resize image '{}': {}", id, error);

            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn verify_image(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let service = image_service(&state);

    let valid = service.verify(&id).await.map_err(|error| {
        log::error!("Failed to verify image '{}': {}", id, error);

        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(serde_json::json!({
        "valid": valid
    })))
}

pub async fn delete_snapshot(
    State(state): State<AppState>,
    Path((master_name, snapshot_name)): Path<(String, String)>,
) -> Result<(), StatusCode> {
    let service = image_service(&state);

    let master = service
        .get(&master_name)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let snapshots = service.snapshots(&master.id).await.map_err(|error| {
        log::error!("Failed to list snapshots for '{}': {}", master.name, error);

        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let snapshot = snapshots
        .into_iter()
        .find(|image| image.name == snapshot_name)
        .ok_or(StatusCode::NOT_FOUND)?;

    service.delete(&snapshot.id).await.map_err(|error| {
        log::error!("Failed to delete snapshot '{}': {}", snapshot_name, error);

        StatusCode::INTERNAL_SERVER_ERROR
    })
}

pub async fn set_default_image(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let service = image_service(&state);

    let image = service.set_default(&id).await.map_err(|error| {
        log::error!("Failed to set default image '{}': {}", id, error);

        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(serde_json::json!({
        "success": true,
        "image": image
    })))
}

pub async fn rollback_snapshot(
    State(state): State<AppState>,
    Path((master_name, snapshot_name)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    /*
     * Rollback is intentionally left as a legacy operation
     * for this stage.
     *
     * Stage 3D should move this into ImageService/ImageBackend
     * so the HTTP handler contains no direct ZFS or SQL logic.
     */

    log::info!(
        "Received rollback request: master='{}', snapshot='{}'",
        master_name,
        snapshot_name
    );

    let service = image_service(&state);

    let images = service.list().await.map_err(|error| {
        log::error!("Failed to list images: {}", error);

        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let master = images
        .iter()
        .find(|image| image.id == master_name || image.name == master_name)
        .ok_or(StatusCode::NOT_FOUND)?;

    let target_snapshot = images
        .iter()
        .find(|image| image.name == snapshot_name && image.parent_id.as_deref() == Some(&master.id))
        .ok_or(StatusCode::NOT_FOUND)?;

    let newer_snapshots: Vec<String> = images
        .iter()
        .filter(|image| {
            image.parent_id.as_deref() == Some(&master.id)
                && image.created_at > target_snapshot.created_at
        })
        .map(|image| image.id.clone())
        .collect();

    let snapshot_full_path = format!("{}@{}", master.name, snapshot_name);

    crate::cmd::run_command(&["zfs", "rollback", "-r", &snapshot_full_path]).map_err(|error| {
        log::error!(
            "Failed to rollback snapshot '{}': {}",
            snapshot_full_path,
            error
        );

        StatusCode::INTERNAL_SERVER_ERROR
    })?;

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

        query_builder
            .execute(&state.db_pool)
            .await
            .map_err(|error| {
                log::error!("Failed to delete newer snapshots from database: {}", error);

                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    }

    Ok(Json(serde_json::json!({
        "message": format!(
            "Successfully rolled back to snapshot '{}' and removed {} newer snapshots",
            snapshot_name,
            newer_snapshots.len()
        )
    })))
}
