use axum::{
    extract::{Extension, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

use crate::{
    api::error::api_error,
    persistence::repositories::installer_media::{InstallerMediaType, StructuralValidationState},
    state::AppState,
    types::Claims,
};

type ApiResult<T> = Result<T, (StatusCode, Json<Value>)>;

const WORKFLOW_RESOURCE: &str = "linux-image-builder";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinuxImagePreflightRequest {
    pub media_id: String,
    pub size_gb: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinuxImageBuildRequest {
    pub media_id: String,
    pub name: String,
    pub size_gb: u64,
    pub username: String,
    pub password: String,
    pub sudo_user: bool,
    pub confirm_create: bool,
}

#[derive(Debug, Serialize)]
pub struct LinuxImagePreflightResponse {
    pub distro: String,
    pub minimum_size_gb: u64,
    pub target_size_gb: u64,
}

fn require_admin(claims: &Claims) -> ApiResult<()> {
    if claims.role == "admin" {
        Ok(())
    } else {
        Err(api_error(
            StatusCode::FORBIDDEN,
            "Administrator access required",
            Value::Null,
        ))
    }
}

async fn linux_iso_path(state: &AppState, media_id: &str) -> ApiResult<PathBuf> {
    let Some(catalog_entry) = state
        .application
        .installer_media
        .get(media_id)
        .await
        .map_err(|error| {
            tracing::error!(media_id, %error, "failed to load Linux installer media");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load installer media",
                Value::Null,
            )
        })?
    else {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Select a valid Linux ISO from Installer Media",
            Value::Null,
        ));
    };
    if catalog_entry.media_type != InstallerMediaType::LinuxIso
        || catalog_entry.validation_state != StructuralValidationState::Valid
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Select a valid Linux ISO from Installer Media",
            Value::Null,
        ));
    }
    let media_service = state.installer_media_service().await;
    let media = media_service
        .verify_integrity(media_id)
        .await
        .map_err(|error| {
            tracing::warn!(media_id, %error, "Linux image builder rejected installer media");
            api_error(
                StatusCode::BAD_REQUEST,
                "Select a valid Linux ISO from Installer Media",
                Value::Null,
            )
        })?;
    Ok(media.managed_path)
}

pub async fn preflight(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<LinuxImagePreflightRequest>,
) -> ApiResult<Json<LinuxImagePreflightResponse>> {
    require_admin(&claims)?;
    if request.size_gb < 20 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Linux boot images must be at least 20 GiB",
            json!({ "minimum_size_gb": 20 }),
        ));
    }
    let iso = linux_iso_path(&state, &request.media_id).await?;
    let result = state
        .application
        .linux_image_builder
        .preflight(&iso, request.size_gb)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Linux image preflight failed");
            api_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                error.to_string(),
                Value::Null,
            )
        })?;
    Ok(Json(LinuxImagePreflightResponse {
        distro: result.distro,
        minimum_size_gb: result.minimum_size_gb,
        target_size_gb: request.size_gb,
    }))
}

pub async fn build(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<LinuxImageBuildRequest>,
) -> ApiResult<Json<crate::core::image::Image>> {
    require_admin(&claims)?;
    if !request.confirm_create {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Confirm creation of the new Linux ZFS image before building",
            Value::Null,
        ));
    }
    if request.size_gb < 20 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Linux boot images must be at least 20 GiB",
            json!({ "minimum_size_gb": 20 }),
        ));
    }
    crate::application::linux_image_builder_service::validate_linux_image_account(
        &request.username,
        &request.password,
    )
    .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string(), Value::Null))?;

    let build_state = state.clone();
    let result = tokio::spawn(async move {
        let _guard = build_state
            .client_mutations
            .lock_resource(WORKFLOW_RESOURCE)
            .await;
        let iso = linux_iso_path(&build_state, &request.media_id).await?;
        build_state
            .application
            .linux_image_builder
            .build(
                &iso,
                request.name,
                request.size_gb,
                crate::application::linux_image_builder_service::LinuxImageAccount {
                    username: request.username,
                    password: request.password,
                    sudo_user: request.sudo_user,
                },
            )
            .await
            .map_err(|error| {
                tracing::error!(%error, "Linux image build failed");
                api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    error.to_string(),
                    Value::Null,
                )
            })
    })
    .await
    .map_err(|error| {
        tracing::error!(%error, "Linux image build task failed");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Linux image build task failed",
            Value::Null,
        )
    })??;
    Ok(Json(result))
}
