//! Image version publishing endpoints (ADR 0009).
//!
//! A version is a named, assignable record that points at one verified image
//! asset. Publishing never renames or promotes source datasets; the asset
//! keeps its own identity and only the version record is created.

use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;

use crate::api::error::api_error;
use crate::application::image_version_service::{
    ImageVersionError, ImageVersionService, PublishVersionRequest,
};
use crate::domain::image::ImageVersionSummary;
use crate::infrastructure::pxe::windows_personalization::normalize_computer_name;
use crate::persistence::repositories::{
    image::ImageRepository, image_version::ImageVersionRepository,
};
use crate::state::AppState;
use crate::types::Claims;

type ApiResult<T> = Result<T, (StatusCode, Json<Value>)>;

fn service(state: &AppState) -> ImageVersionService {
    ImageVersionService::new(
        ImageVersionRepository::new(state.db_pool.clone()),
        ImageRepository::new(state.db_pool.clone()),
    )
}

fn require_admin(claims: &Claims) -> ApiResult<()> {
    if claims.role != "admin" {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "Administrator access required",
            Value::Null,
        ));
    }
    Ok(())
}

fn internal(message: &str, source: impl std::fmt::Display) -> (StatusCode, Json<Value>) {
    tracing::error!(%source, "image version request failed");
    api_error(StatusCode::INTERNAL_SERVER_ERROR, message, Value::Null)
}

#[derive(Debug, Default, Deserialize)]
pub struct ListVersionsQuery {
    pub base_name: Option<String>,
    /// Include every version instead of only the newest per base name.
    #[serde(default)]
    pub all: bool,
}

#[derive(Debug, Deserialize)]
pub struct ValidateComputerNameQuery {
    #[serde(default)]
    pub computer_name: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct ValidateComputerNameResponse {
    pub valid: bool,
    pub error: Option<String>,
}

/// Validate a NetBIOS computer name exactly as offline servicing will write it.
///
/// An absent name means "inherit the image identity" and is valid; the same
/// rule lives in the Windows personalization plan so both surfaces agree.
fn validate_name(raw: Option<&str>) -> ValidateComputerNameResponse {
    match raw.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => match normalize_computer_name(name) {
            Ok(_) => ValidateComputerNameResponse {
                valid: true,
                error: None,
            },
            Err(error) => ValidateComputerNameResponse {
                valid: false,
                error: Some(error.to_string()),
            },
        },
        None => ValidateComputerNameResponse {
            valid: true,
            error: None,
        },
    }
}

pub async fn list_versions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<ListVersionsQuery>,
) -> ApiResult<Json<Vec<ImageVersionSummary>>> {
    require_admin(&claims)?;
    let versions = service(&state)
        .list(query.base_name.as_deref(), query.all)
        .await
        .map_err(|error| internal("Failed to list image versions", error))?;
    Ok(Json(versions))
}

pub async fn get_version(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> ApiResult<Json<ImageVersionSummary>> {
    require_admin(&claims)?;
    let version = service(&state)
        .get(&id)
        .await
        .map_err(|error| internal("Failed to load image version", error))?
        .ok_or_else(|| {
            api_error(
                StatusCode::NOT_FOUND,
                format!("Image version '{id}' was not found"),
                Value::Null,
            )
        })?;
    Ok(Json(version))
}

pub async fn delete_version(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    require_admin(&claims)?;
    service(&state)
        .delete(&id)
        .await
        .map_err(|error| match error {
            ImageVersionError::NotFound(message) => {
                api_error(StatusCode::NOT_FOUND, message, Value::Null)
            }
            ImageVersionError::Rejected(message) => {
                api_error(StatusCode::CONFLICT, message, Value::Null)
            }
            ImageVersionError::Internal(source) => {
                internal("Failed to delete image version", source)
            }
        })?;
    Ok(StatusCode::NO_CONTENT)
}

/// Publish a verified servicing clone as a new image version.
///
/// Mounted at `POST /api/image-versions` and `POST /api/image-versions/post-commit`
/// so the servicing workflow can publish the clone it just committed.
pub async fn publish_version(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<PublishVersionRequest>,
) -> ApiResult<Json<ImageVersionSummary>> {
    require_admin(&claims)?;
    let settings = state.settings.read().await.clone();
    let published = service(&state)
        .publish_version(&settings, &request, Some(claims.sub.as_str()))
        .await
        .map_err(|error| match error {
            ImageVersionError::NotFound(message) => {
                api_error(StatusCode::NOT_FOUND, message, Value::Null)
            }
            ImageVersionError::Rejected(message) => {
                api_error(StatusCode::CONFLICT, message, Value::Null)
            }
            ImageVersionError::Internal(source) => {
                internal("Failed to publish image version", source)
            }
        })?;
    Ok(Json(published))
}

pub async fn validate_computer_name(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<ValidateComputerNameQuery>,
) -> ApiResult<Json<ValidateComputerNameResponse>> {
    require_admin(&claims)?;
    Ok(Json(validate_name(query.computer_name.as_deref())))
}

pub fn image_version_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/image-versions",
            get(list_versions).post(publish_version),
        )
        .route("/api/image-versions/post-commit", post(publish_version))
        .route(
            "/api/image-versions/validate-computer-name",
            get(validate_computer_name),
        )
        .route(
            "/api/image-versions/{id}",
            get(get_version).delete(delete_version),
        )
}

#[cfg(test)]
mod tests {
    use super::validate_name;

    #[test]
    fn an_absent_name_inherits_the_image_identity() {
        assert!(validate_name(None).valid);
        assert!(validate_name(Some("   ")).valid);
    }

    #[test]
    fn names_are_checked_against_the_netbios_rules_the_worker_writes() {
        assert!(validate_name(Some("PC-LAB-07")).valid);
        let invalid = validate_name(Some("pc_lab"));
        assert!(!invalid.valid);
        assert!(invalid
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("letters, digits and hyphens"));
        assert!(!validate_name(Some("12345")).valid);
        assert!(!validate_name(Some("CON")).valid);
        assert!(!validate_name(Some(&"a".repeat(16))).valid);
    }
}
