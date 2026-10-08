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

pub async fn list_images(State(state): State<AppState>) -> Result<Json<Vec<Image>>, StatusCode> {
    let service = image_service(&state);

    service.list().await.map(Json).map_err(|error| {
        log::error!("Failed to list images: {}", error);

        StatusCode::INTERNAL_SERVER_ERROR
    })
}

/// A root image and its child snapshot summaries, preserving repository order.
#[derive(Serialize)]
pub struct MasterWithSnapshots {
    #[serde(flatten)]
    pub image: Image,

    pub snapshots: Vec<crate::types::image::Snapshot>,
}

/// Lists root images and snapshot summaries using the state's database.
///
/// Returns a name-ordered JSON response, or an internal-server error on database
/// or metadata decoding failure.
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
    let mut snapshots_by_parent: std::collections::HashMap<
        String,
        Vec<crate::types::image::Snapshot>,
    > = std::collections::HashMap::new();

    // Consume records to retain repository ordering without cloning master metadata.
    for image in images {
        if image.parent_id.is_none() {
            masters_with_snapshots.push(MasterWithSnapshots {
                image,
                snapshots: Vec::new(),
            });
        } else if image.kind == crate::core::image::ImageKind::Snapshot {
            if let Some(parent_id) = image.parent_id {
                let size = format!("{}GB", image.size_gb);
                snapshots_by_parent.entry(parent_id).or_default().push(
                    crate::types::image::Snapshot {
                        name: image.name,
                        created: image.created_at.to_rfc3339(),
                        used: size.clone(),
                        size: Some(size),
                    },
                );
            }
        }
    }
    for master in &mut masters_with_snapshots {
        master.snapshots = snapshots_by_parent
            .remove(&master.image.id)
            .unwrap_or_default();
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

    let _client_guard = state.client_mutations.lock().await;
    let service = image_service(&state);

    let image = service.update(&id, request).await.map_err(|error| {
        log::error!("Failed to update image '{}': {}", id, error);
        image_error_status(&error)
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
    let _client_guard = state.client_mutations.lock().await;
    let service = image_service(&state);

    service
        .rename(&id, &request.new_name)
        .await
        .map(Json)
        .map_err(|error| {
            log::error!("Failed to rename image '{}': {}", id, error);
            image_error_status(&error)
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

/// Scan the ZFS pool for existing image ZVOLs and snapshots and register any
/// that are not already tracked in the database.
pub async fn import_existing_images(
    State(state): State<AppState>,
) -> Result<Json<crate::application::image_service::ImportScanResult>, StatusCode> {
    let service = image_service(&state);

    service
        .import_existing_images()
        .await
        .map(Json)
        .map_err(|error| {
            log::error!("Failed to scan for existing images: {}", error);

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
    axum::Extension(claims): axum::Extension<crate::types::Claims>,
    Path((master_name, snapshot_name)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if claims.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }
    let service = image_service(&state);
    // Resolve these separately to preserve the API's not-found response.
    let master = service
        .get(&master_name)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    if !service
        .snapshots(&master.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .iter()
        .any(|snapshot| snapshot.name == snapshot_name)
    {
        return Err(StatusCode::NOT_FOUND);
    }
    let removed = service
        .rollback_snapshot(&master.id, &snapshot_name)
        .await
        .map_err(|error| {
            log::error!(
                "Failed to rollback '{}@{}': {}",
                master.name,
                snapshot_name,
                error
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(serde_json::json!({
        "message": format!(
            "Successfully rolled back to snapshot '{}' and removed {} newer snapshots",
            snapshot_name,
            removed
        )
    })))
}

fn image_error_status(error: &anyhow::Error) -> StatusCode {
    if error
        .downcast_ref::<crate::application::image_service::ImageInUse>()
        .is_some()
    {
        StatusCode::CONFLICT
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

#[cfg(test)]
mod listing_tests {
    use super::*;
    use serde_json::json;

    fn image(id: &str, kind: &str, parent: Option<&str>) -> Image {
        serde_json::from_value(json!({
            "id": id, "name": id, "kind": kind, "os_type": "windows",
            "size_gb": 20, "path": "/dev/zvol/diskless/image", "format": "raw",
            "status": "ready", "description": "preserved metadata",
            "parent_id": parent, "source_snapshot": null, "checksum": "checksum",
            "is_default": id == "master-a",
            "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-02T00:00:00Z"
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn listing_preserves_metadata_order_and_excludes_clones_and_orphans() {
        let (state, _, _, _) = crate::api::security_tests::setup().await;
        let repository = ImageRepository::new(state.db_pool.clone());
        let master_a = image("master-a", "master", None);
        let master_b = image("master-b", "master", None);
        for record in [
            master_b.clone(),
            image("z-snapshot", "snapshot", Some("master-a")),
            image("clone", "clone", Some("master-a")),
            master_a.clone(),
            image("a-snapshot", "snapshot", Some("master-a")),
            image("orphan", "snapshot", Some("missing")),
            image("b-snapshot", "snapshot", Some("master-b")),
        ] {
            repository.insert(&record).await.unwrap();
        }
        let result = list_masters(State(state)).await.unwrap();
        let snapshot = |name| {
            json!({"name": name, "created": "2026-01-01T00:00:00+00:00",
            "used": "20GB", "size": "20GB"})
        };
        let expected = vec![
            MasterWithSnapshots {
                image: master_a,
                snapshots: serde_json::from_value(json!([
                    snapshot("a-snapshot"),
                    snapshot("z-snapshot")
                ]))
                .unwrap(),
            },
            MasterWithSnapshots {
                image: master_b,
                snapshots: serde_json::from_value(json!([snapshot("b-snapshot")])).unwrap(),
            },
        ];
        assert_eq!(
            serde_json::to_vec(&result.0).unwrap(),
            serde_json::to_vec(&expected).unwrap()
        );
    }

    #[tokio::test]
    async fn listing_empty_inventory_returns_empty_response() {
        let (state, _, _, _) = crate::api::security_tests::setup().await;
        assert!(list_masters(State(state)).await.unwrap().0.is_empty());
    }
}
