//! Authenticated management and separately scoped preboot installation endpoints.
use crate::{
    application::installation_service::{
        AdaptationInventory, CreateInstallation, InstallationReport, InstallationService,
    },
    state::AppState,
    types::Claims,
};
use axum::{
    body::Body,
    extract::{ConnectInfo, Path, State},
    http::{header, StatusCode},
    response::Response,
    Extension, Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::SocketAddr;
type ApiError = (StatusCode, Json<Value>);
fn error(status: StatusCode, message: &str) -> ApiError {
    (
        status,
        Json(
            json!({"code":if status==StatusCode::FORBIDDEN {"forbidden"} else {"state_conflict"},"message":message,"operation_id":uuid::Uuid::new_v4().to_string(),"details":null}),
        ),
    )
}
fn admin(claims: &Claims) -> Result<(), ApiError> {
    if claims.role == "admin" {
        Ok(())
    } else {
        Err(error(
            StatusCode::FORBIDDEN,
            "Administrator access required",
        ))
    }
}
fn service_error(e: anyhow::Error) -> ApiError {
    // Service errors have intentionally bounded, secret-free messages; nested process stderr is never included.
    error(StatusCode::CONFLICT, &e.to_string())
}
pub async fn list(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    admin(&claims)?;
    let service = InstallationService::new(state);
    let sessions = service.repository.list(&id).await.map_err(service_error)?;
    let mut result = Vec::new();
    for session in sessions {
        let actions = match session.state.as_str() {
            "waiting" | "installing" | "adapting" | "manual" => json!(["cancel"]),
            "saving" => json!(["retry"]),
            "verification_failed" => json!(["retry", "cancel", "close"]),
            "failed" | "cancelled" | "recovery_required" => json!(["close", "retry_cleanup"]),
            "published" if session.closed_at.is_none() => json!(["retry_cleanup"]),
            _ => json!([]),
        };
        let events = service
            .repository
            .events(&session.id)
            .await
            .map_err(service_error)?;
        result.push(json!({"session":session,"events":events,"actions":actions}));
    }
    Ok(Json(json!(result)))
}
pub async fn create(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Json(request): Json<CreateInstallation>,
) -> Result<Json<Value>, ApiError> {
    admin(&claims)?;
    Ok(Json(json!(InstallationService::new(state)
        .create(id, request)
        .await
        .map_err(service_error)?)))
}
pub async fn cancel(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    admin(&claims)?;
    InstallationService::new(state)
        .cancel(&id)
        .await
        .map_err(service_error)?;
    Ok(Json(
        json!({"status":"cancelled","message":"Boot credentials revoked; power off the client before closing recovery"}),
    ))
}
pub async fn close_recovery(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    admin(&claims)?;
    InstallationService::new(state)
        .close_recovery(&id)
        .await
        .map_err(service_error)?;
    Ok(Json(json!({"status":"completed"})))
}
pub async fn retry(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    admin(&claims)?;
    InstallationService::new(state)
        .retry(&id)
        .await
        .map_err(service_error)?;
    Ok(Json(json!({"status":"retry_started"})))
}
pub async fn report(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path((id, token)): Path<(String, String)>,
    Json(request): Json<InstallationReport>,
) -> Result<Json<Value>, ApiError> {
    InstallationService::new(state).report(&id,&token,peer.ip(),request).await.map_err(|_|error(StatusCode::FORBIDDEN,"Installation report denied; verify client address, session state and boot credential"))?;
    Ok(Json(json!({"status":"accepted"})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallationVerificationReport {
    target_iqn: String,
    attempt: i64,
    os_family: String,
    os_version: String,
    architecture: String,
    boot_nic_ids: Vec<String>,
    boot_nic_mac: Option<String>,
}

/// Public because the normal OS reports before management credentials or client identity exist.
/// A mismatched target or peer is intentionally indistinguishable from an unknown target.
pub async fn verify_normal_boot(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(report): Json<InstallationVerificationReport>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    if report.target_iqn.len() > 223
        || report.attempt < 1
        || report.os_version.len() > 128
        || report.architecture.len() > 32
        || report.boot_nic_ids.is_empty()
        || report.boot_nic_ids.len() > 64
        || report
            .boot_nic_ids
            .iter()
            .any(|id| id.is_empty() || id.len() > 512 || id.chars().any(char::is_control))
        || report
            .boot_nic_mac
            .as_deref()
            .is_some_and(|mac| mac.len() > 32 || mac.chars().any(char::is_control))
    {
        return Ok((StatusCode::NO_CONTENT, Json(Value::Null)));
    }
    let repository = crate::persistence::repositories::installation::InstallationRepository::new(
        state.db_pool.clone(),
    );
    let Some(session) = repository
        .verification_target(&report.target_iqn)
        .await
        .map_err(service_error)?
    else {
        return Ok((StatusCode::NO_CONTENT, Json(Value::Null)));
    };
    if session.attempt != report.attempt {
        return Ok((StatusCode::NO_CONTENT, Json(Value::Null)));
    }
    if session.state == "published" && session.published_image_id.is_some() {
        return Ok((StatusCode::OK, Json(json!({"status":"verified"}))));
    }
    let expected_family = if session.recipe == "windows" {
        "windows"
    } else {
        "linux"
    };
    if session.state != "verifying"
        || report.os_family != expected_family
        || !matches!(report.architecture.as_str(), "amd64" | "x86_64")
        || session.source_ip.parse::<std::net::IpAddr>().ok() != Some(peer.ip())
    {
        return Ok((StatusCode::NO_CONTENT, Json(Value::Null)));
    }
    if session
        .phase_deadline
        .as_deref()
        .and_then(|deadline| chrono::DateTime::parse_from_rfc3339(deadline).ok())
        .is_some_and(|deadline| deadline <= chrono::Utc::now())
    {
        let _ = repository
            .transition(
                &session,
                "verification_failed",
                session.progress,
                "Normal boot verification deadline expired",
            )
            .await;
        return Err(error(
            StatusCode::CONFLICT,
            "Verification attempt expired; retry or close the session",
        ));
    }
    let client = state
        .application
        .clients
        .get_by_string(&session.client_id)
        .await
        .map_err(service_error)?;
    let Some(client) = client else {
        return Ok((StatusCode::NO_CONTENT, Json(Value::Null)));
    };
    let target = report.target_iqn.clone();
    let name = client.name.clone();
    let peer_ip = peer.ip();
    let live = tokio::task::spawn_blocking(move || {
        Ok::<_, anyhow::Error>(
            crate::infrastructure::iscsi::active_session_peer_ips(&target, &name)?
                .contains(&peer_ip),
        )
    })
    .await
    .map_err(|error| service_error(error.into()))?
    .map_err(service_error)?;
    if !live {
        return Ok((StatusCode::NO_CONTENT, Json(Value::Null)));
    }
    let Some(draft) = session.draft_image_id.as_deref() else {
        return Ok((StatusCode::NO_CONTENT, Json(Value::Null)));
    };
    state
        .application
        .images
        .publish_verified_draft(&session.id, draft, &report.target_iqn, report.attempt)
        .await
        .map_err(service_error)?;
    InstallationService::new(state.clone()).resume_published_cleanup(session.id.clone());
    Ok((StatusCode::OK, Json(json!({"status":"verified"}))))
}
pub async fn adaptation(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path((id, token)): Path<(String, String)>,
    Json(request): Json<AdaptationInventory>,
) -> Result<Json<Value>, ApiError> {
    InstallationService::new(state)
        .adaptation_manifest(&id, &token, peer.ip(), request)
        .await
        .map(Json)
        .map_err(|_| error(StatusCode::FORBIDDEN, "Adaptation request denied; verify session state, media recipe and assigned client address"))
}

pub async fn adaptation_bundle(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path((id, token)): Path<(String, String)>,
) -> Result<Response<Body>, ApiError> {
    let body = InstallationService::new(state)
        .adaptation_bundle(&id, &token, peer.ip())
        .await
        .map_err(|_| {
            error(
                StatusCode::FORBIDDEN,
                "Adaptation bundle denied; verify session state and assigned client address",
            )
        })?;
    Response::builder()
        .header(header::CONTENT_TYPE, "application/zip")
        .header(header::CACHE_CONTROL, "no-store")
        .header("Referrer-Policy", "no-referrer")
        .body(body)
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not build adaptation bundle response",
            )
        })
}

pub async fn payload(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path((id, token, asset)): Path<(String, String, String)>,
) -> Result<Response<Body>, ApiError> {
    let body=InstallationService::new(state).payload(&id,&token,peer.ip(),&asset).await.map_err(|_|error(StatusCode::FORBIDDEN,"Installation payload denied; verify session state, prerequisites and assigned client address"))?;
    let content_type = match asset.as_str() {
        "user-data" | "meta-data" | "ks.cfg" | "disk-gate.sh" | "install.ps1" | "startnet.cmd" => {
            "text/plain; charset=utf-8"
        }
        _ => "application/octet-stream",
    };
    Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "no-store")
        .header("Referrer-Policy", "no-referrer")
        .body(body)
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not build installation payload",
            )
        })
}
