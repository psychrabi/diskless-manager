use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::future::Future;
use uuid::Uuid;

use crate::{
    api::handlers::clients::ErrorResponse,
    domain::ClientId,
    persistence::repositories::installer_media::{
        BootVerification, InstallerMedia, InstallerMediaType,
    },
    state::AppState,
    types::Claims,
};

fn admin(claims: &Claims) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if claims.role == "admin" {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                status: 403,
                error: "Administrator access required".into(),
            }),
        ))
    }
}

fn safe_error(error: &anyhow::Error) -> (StatusCode, Json<ErrorResponse>) {
    let detail = format!("{error:#}").to_ascii_lowercase();
    let status = if detail.contains("not cataloged") || detail.contains("does not exist") {
        StatusCode::NOT_FOUND
    } else if detail.contains("exceed")
        || detail.contains("must be")
        || detail.contains("invalid")
        || detail.contains("iso")
        || detail.contains("checksum")
        || detail.contains("firmware")
        || detail.contains("revision")
        || detail.contains("media type")
    {
        StatusCode::UNPROCESSABLE_ENTITY
    } else if detail.contains("active")
        || detail.contains("pending")
        || detail.contains("offline")
        || detail.contains("session")
        || detail.contains("default")
        || detail.contains("referenced")
    {
        StatusCode::CONFLICT
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    };
    let message = match status {
        StatusCode::NOT_FOUND => "Requested media or client was not found",
        StatusCode::UNPROCESSABLE_ENTITY => "The media or requested values are invalid",
        StatusCode::CONFLICT => "The operation conflicts with current client or media state",
        _ => "The operation could not be completed",
    };
    (
        status,
        Json(ErrorResponse {
            status: status.as_u16(),
            error: message.into(),
        }),
    )
}

enum UploadCoordinationError {
    Pump,
    Service(anyhow::Error),
}

async fn coordinate_upload<S, P, T>(service: S, pump: P) -> Result<T, UploadCoordinationError>
where
    S: Future<Output = anyhow::Result<T>>,
    P: Future<Output = anyhow::Result<()>>,
{
    tokio::pin!(service);
    tokio::pin!(pump);
    tokio::select! {
        biased;
        result = &mut pump => match result {
            Ok(()) => service.await.map_err(UploadCoordinationError::Service),
            Err(_) => Err(UploadCoordinationError::Pump),
        },
        result = &mut service => result.map_err(UploadCoordinationError::Service),
    }
}

#[derive(Serialize)]
pub struct MediaDto {
    id: String,
    display_name: String,
    media_type: &'static str,
    byte_size: u64,
    checksum: String,
    validation_state: &'static str,
    is_default: bool,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
    bios_verified: bool,
    uefi_verified: bool,
}
impl MediaDto {
    fn new(m: InstallerMedia, bios_verified: bool, uefi_verified: bool) -> Self {
        Self {
        id: m.id, display_name: m.display_name,
        media_type: match m.media_type {
            InstallerMediaType::WindowsIso => "windows_iso",
            InstallerMediaType::WinPe => "winpe",
            InstallerMediaType::LinuxIso => "linux_iso",
            InstallerMediaType::OtherIso => "other_iso",
        },
        byte_size: m.byte_size, checksum: m.checksum,
        validation_state: match m.validation_state { crate::persistence::repositories::installer_media::StructuralValidationState::Valid => "valid", crate::persistence::repositories::installer_media::StructuralValidationState::Pending => "pending", crate::persistence::repositories::installer_media::StructuralValidationState::Invalid => "invalid" },
        is_default: m.is_default, created_at: m.created_at, updated_at: m.updated_at,
        bios_verified, uefi_verified,
    }
    }
}

pub async fn list_media(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<MediaDto>>, (StatusCode, Json<ErrorResponse>)> {
    admin(&claims)?;
    let media = state
        .installer_media_service()
        .await
        .list()
        .await
        .map_err(|e| safe_error(&e))?;
    let mut output = Vec::with_capacity(media.len());
    for item in media {
        let bios = state
            .application
            .installer_media
            .has_boot_verification(&item.id, &crate::domain::PxeMode::Bios)
            .await
            .map_err(|e| safe_error(&e))?;
        let uefi = state
            .application
            .installer_media
            .has_boot_verification(&item.id, &crate::domain::PxeMode::Uefi)
            .await
            .map_err(|e| safe_error(&e))?;
        output.push(MediaDto::new(item, bios, uefi));
    }
    Ok(Json(output))
}

pub async fn upload_media(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<MediaDto>), (StatusCode, Json<ErrorResponse>)> {
    admin(&claims)?;
    async fn next_text(
        multipart: &mut Multipart,
        name: &'static str,
    ) -> Result<String, (StatusCode, Json<ErrorResponse>)> {
        let field = multipart
            .next_field()
            .await
            .map_err(|_| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        status: 400,
                        error: "Invalid multipart upload".into(),
                    }),
                )
            })?
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        status: 400,
                        error: format!("{name} is required"),
                    }),
                )
            })?;
        if field.name() != Some(name) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    status: 400,
                    error: format!("Expected {name} before ISO file"),
                }),
            ));
        }
        field.text().await.map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    status: 400,
                    error: format!("Invalid {name}"),
                }),
            )
        })
    }
    let name = next_text(&mut multipart, "display_name").await?;
    let media_type = next_text(&mut multipart, "media_type").await?;
    let kind = match media_type.as_str() {
        "windows_iso" => InstallerMediaType::WindowsIso,
        "winpe" => InstallerMediaType::WinPe,
        "linux_iso" => InstallerMediaType::LinuxIso,
        "other_iso" => InstallerMediaType::OtherIso,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    status: 400,
                    error: "Unsupported media type".into(),
                }),
            ))
        }
    };
    let mut field = multipart
        .next_field()
        .await
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    status: 400,
                    error: "Invalid multipart upload".into(),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    status: 400,
                    error: "ISO file is required".into(),
                }),
            )
        })?;
    if field.name() != Some("file") {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                status: 400,
                error: "ISO file is required".into(),
            }),
        ));
    }
    let (mut writer, reader) = tokio::io::duplex(128 * 1024);
    let pump = async move {
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|_| anyhow::anyhow!("multipart stream failed"))?
        {
            tokio::io::AsyncWriteExt::write_all(&mut writer, &chunk).await?;
        }
        tokio::io::AsyncWriteExt::shutdown(&mut writer).await?;
        anyhow::Ok(())
    };
    let service = state.installer_media_service().await;
    let upload = service.upload(&name, kind, reader);
    match coordinate_upload(upload, pump).await {
        Ok(media) => Ok((
            StatusCode::CREATED,
            Json(MediaDto::new(media, false, false)),
        )),
        Err(UploadCoordinationError::Pump) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                status: 400,
                error: "Invalid multipart upload stream".into(),
            }),
        )),
        Err(UploadCoordinationError::Service(error)) => Err(safe_error(&error)),
    }
}

pub async fn set_media_default(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    admin(&claims)?;
    let _mutation = state.client_mutations.lock().await;
    match state.installer_media_service().await.set_default(&id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                status: 404,
                error: "Installer media not found".into(),
            }),
        )),
        Err(e) => Err(safe_error(&e)),
    }
}

pub async fn delete_media(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    admin(&claims)?;
    let _mutation = state.client_mutations.lock().await;
    if state
        .application
        .installer_media
        .get(&id)
        .await
        .map_err(|e| safe_error(&e))?
        .is_none()
    {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                status: 404,
                error: "Installer media not found".into(),
            }),
        ));
    }
    match state.installer_media_service().await.delete(&id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                status: 409,
                error: "Media is selected or in use".into(),
            }),
        )),
        Err(e) => Err(safe_error(&e)),
    }
}

#[derive(Clone, Deserialize)]
pub struct VerificationRequest {
    checksum: String,
    firmware_mode: String,
    test_client_id: String,
    assigned_image_revision: String,
    observed_luns: Vec<u8>,
    boot_result: String,
}

fn verification_matches(
    req: &VerificationRequest,
    current_checksum: &str,
    current_revision: &str,
    firmware: &crate::domain::PxeMode,
) -> bool {
    let mut observed_luns = req.observed_luns.clone();
    observed_luns.sort_unstable();
    observed_luns.dedup();
    let observed_luns_valid = observed_luns.len() == req.observed_luns.len()
        && observed_luns.iter().all(|lun| *lun <= 1)
        && (req.boot_result == "failed" || observed_luns == [0, 1]);

    req.checksum == current_checksum
        && req.assigned_image_revision == current_revision
        && req.firmware_mode == firmware.as_str()
        && observed_luns_valid
        && matches!(req.boot_result.as_str(), "passed" | "failed")
}

fn verification_session_matches(
    session: &crate::persistence::SuperSession,
    client_id: &str,
    media_id: &str,
    image_id: &str,
    assigned_snapshot: Option<&str>,
) -> bool {
    session.client_id == client_id
        && session.state == crate::persistence::SuperSessionState::Active
        && session.verification_only
        && session.installer_media_id.as_deref() == Some(media_id)
        && session.prior_image_id == image_id
        && session.prior_snapshot.as_deref() == assigned_snapshot
}

pub async fn record_verification(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Json(req): Json<VerificationRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    admin(&claims)?;
    let _mutation = state.client_mutations.lock().await;
    let media = state
        .installer_media_service()
        .await
        .verify_integrity(&id)
        .await
        .map_err(|e| safe_error(&e))?;
    let client_id = ClientId::from_string(req.test_client_id.clone()).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                status: 400,
                error: "Invalid test client".into(),
            }),
        )
    })?;
    let client = state
        .application
        .client_repository
        .find_by_id(&client_id)
        .await
        .map_err(|_| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    status: 404,
                    error: "Test client not found".into(),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    status: 404,
                    error: "Test client not found".into(),
                }),
            )
        })?;
    let session = state
        .application
        .super_sessions
        .get_for_client(client.id.as_ref())
        .await
        .map_err(|_| safe_error(&anyhow::anyhow!("failed to read Super-session")))?
        .ok_or_else(|| {
            (
                StatusCode::CONFLICT,
                Json(ErrorResponse {
                    status: 409,
                    error: "The test client has no active verification session".into(),
                }),
            )
        })?;
    let image = state
        .application
        .images
        .get(&client.master)
        .await
        .map_err(|e| safe_error(&e))?;
    if !verification_session_matches(
        &session,
        client.id.as_ref(),
        &id,
        &image.id,
        client.snapshot.as_deref(),
    ) || !verification_matches(
        &req,
        &media.checksum,
        client.snapshot.as_deref().unwrap_or(&client.master),
        &client.pxe_mode,
    ) {
        return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(ErrorResponse { status: 422, error: "Verification does not match current media, client assignment, firmware, or observed LUNs".into() })));
    }
    state
        .super_client_service()
        .await
        .verify_boot_verification_target(&client, &session, &media)
        .await
        .map_err(|error| safe_error(&error))?;
    state
        .application
        .installer_media
        .record_boot_verification(&BootVerification {
            id: Uuid::new_v4().to_string(),
            installer_media_id: id,
            checksum: media.checksum,
            firmware_mode: client.pxe_mode,
            test_client_id: client.id.to_string(),
            assigned_image_revision: req.assigned_image_revision,
            observed_luns: req.observed_luns,
            boot_result: req.boot_result,
            verified_at: Utc::now(),
        })
        .await
        .map_err(|e| safe_error(&e))?;
    Ok(StatusCode::CREATED)
}

pub async fn super_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Value>, (StatusCode, Json<ErrorResponse>)> {
    admin(&claims)?;
    let row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT mode, master FROM clients WHERE id = ?")
            .bind(&id)
            .fetch_optional(&state.db_pool)
            .await
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        status: 500,
                        error: "Unable to read client status".into(),
                    }),
                )
            })?;
    let Some((mode, _)) = row else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                status: 404,
                error: "Client not found".into(),
            }),
        ));
    };
    let session = state
        .application
        .super_sessions
        .get_for_client(&id)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    status: 500,
                    error: "Unable to read session status".into(),
                }),
            )
        })?;
    let value = if let Some(s) = session {
        json!({"id":s.id,"client_id":s.client_id,"prior_image_id":s.prior_image_id,"prior_snapshot":s.prior_snapshot,"installer_media_id":s.installer_media_id,"verification_only":s.verification_only,"state":format!("{:?}",s.state).to_lowercase(),"created_at":s.created_at,"updated_at":s.updated_at})
    } else {
        Value::Null
    };
    Ok(Json(
        json!({"client_id":id,"mode":mode,"legacy":mode == "super" && value.is_null(),"session":value}),
    ))
}

#[derive(Deserialize)]
pub struct EnterRequest {
    #[serde(default)]
    verification_only: bool,
    #[serde(default)]
    confirmed: bool,
}
pub async fn enter_super(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Json(req): Json<EnterRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ErrorResponse>)> {
    admin(&claims)?;
    state.super_client_service().await.enter(&id, req.verification_only, req.confirmed).await.map(|s| Json(json!({"id":s.id,"client_id":s.client_id,"state":format!("{:?}",s.state).to_lowercase(),"verification_only":s.verification_only}))).map_err(|e| safe_error(&e))
}
pub async fn save_super(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    admin(&claims)?;
    state
        .super_client_service()
        .await
        .save(&id)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| safe_error(&e))
}
pub async fn discard_super(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    admin(&claims)?;
    state
        .super_client_service()
        .await
        .discard(&id)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| safe_error(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verification() -> VerificationRequest {
        VerificationRequest {
            checksum: "current-sha".into(),
            firmware_mode: "uefi".into(),
            test_client_id: "client".into(),
            assigned_image_revision: "pool/image@revision-7".into(),
            observed_luns: vec![0, 1],
            boot_result: "passed".into(),
        }
    }

    #[tokio::test]
    async fn early_service_failure_cancels_a_blocked_multipart_pump() {
        let result: Result<(), UploadCoordinationError> = coordinate_upload(
            async { anyhow::bail!("upload limit reached") },
            std::future::pending::<anyhow::Result<()>>(),
        )
        .await;
        assert!(matches!(result, Err(UploadCoordinationError::Service(_))));
    }

    #[tokio::test]
    async fn multipart_pump_failure_cancels_the_unpublished_upload() {
        struct Dropped(std::sync::Arc<std::sync::atomic::AtomicBool>);
        impl Drop for Dropped {
            fn drop(&mut self) {
                self.0.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }

        let dropped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let upload_was_dropped = dropped.clone();
        let upload = async move {
            let _drop_guard = Dropped(upload_was_dropped);
            std::future::pending::<anyhow::Result<()>>().await
        };
        let result = coordinate_upload(upload, async {
            tokio::task::yield_now().await;
            anyhow::bail!("request body failed")
        })
        .await;
        assert!(matches!(result, Err(UploadCoordinationError::Pump)));
        assert!(dropped.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn verification_claims_must_match_current_assignment_and_firmware() {
        let valid = verification();
        assert!(verification_matches(
            &valid,
            "current-sha",
            "pool/image@revision-7",
            &crate::domain::PxeMode::Uefi,
        ));
        let mut bad = verification();
        bad.assigned_image_revision = "pool/image@stale".into();
        assert!(!verification_matches(
            &bad,
            "current-sha",
            "pool/image@revision-7",
            &crate::domain::PxeMode::Uefi,
        ));
        let mut bad = verification();
        bad.checksum = "old-sha".into();
        assert!(!verification_matches(
            &bad,
            "current-sha",
            "pool/image@revision-7",
            &crate::domain::PxeMode::Uefi,
        ));
        let mut bad = verification();
        bad.firmware_mode = "bios".into();
        assert!(!verification_matches(
            &bad,
            "current-sha",
            "pool/image@revision-7",
            &crate::domain::PxeMode::Uefi,
        ));
        let mut failed = valid.clone();
        failed.boot_result = "failed".into();
        failed.observed_luns = vec![0];
        assert!(verification_matches(
            &failed,
            "current-sha",
            "pool/image@revision-7",
            &crate::domain::PxeMode::Uefi,
        ));
        let mut fabricated_pass = failed.clone();
        fabricated_pass.boot_result = "passed".into();
        assert!(!verification_matches(
            &fabricated_pass,
            "current-sha",
            "pool/image@revision-7",
            &crate::domain::PxeMode::Uefi,
        ));
        failed.observed_luns = vec![0, 0];
        assert!(!verification_matches(
            &failed,
            "current-sha",
            "pool/image@revision-7",
            &crate::domain::PxeMode::Uefi,
        ));
    }

    #[test]
    fn verification_requires_a_matching_active_disposable_session() {
        let now = Utc::now();
        let session = crate::persistence::SuperSession {
            id: "session-1".into(),
            client_id: "client-1".into(),
            prior_image_id: "image-1".into(),
            prior_snapshot: Some("pool/image@revision-7".into()),
            working_origin_snapshot: Some("pool/image@revision-7".into()),
            working_clone: "pool/super-session-1".into(),
            installer_media_id: Some("media-1".into()),
            verification_only: true,
            state: crate::persistence::SuperSessionState::Active,
            resume_state: None,
            created_at: now,
            updated_at: now,
        };
        assert!(verification_session_matches(
            &session,
            "client-1",
            "media-1",
            "image-1",
            Some("pool/image@revision-7")
        ));
        assert!(!verification_session_matches(
            &session,
            "client-1",
            "media-2",
            "image-1",
            Some("pool/image@revision-7")
        ));
        let mut normal_session = session.clone();
        normal_session.verification_only = false;
        assert!(!verification_session_matches(
            &normal_session,
            "client-1",
            "media-1",
            "image-1",
            Some("pool/image@revision-7")
        ));
    }
}
