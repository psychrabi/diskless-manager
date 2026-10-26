use crate::domain::Client;
use crate::infrastructure::pxe::{
    normalize_hardware_ids as normalize_report_ids, DriverInjectionStatus,
    NetworkDriverInjectionPlugin, NetworkDriverPackage,
};
use crate::state::AppState;
use crate::types::Claims;
use axum::{
    extract::{ConnectInfo, Multipart, Path, State},
    http::StatusCode,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;

const MAX_DRIVER_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_DRIVER_REPORT_BYTES: usize = 64 * 1024;

async fn plugin(state: &AppState) -> anyhow::Result<NetworkDriverInjectionPlugin> {
    let root = state.settings.read().await.http.root_dir.clone();
    Ok(NetworkDriverInjectionPlugin::new(PathBuf::from(root)))
}

#[derive(Debug, Serialize)]
pub struct NetworkDriverError {
    pub error: String,
}

fn require_admin(claims: &Claims) -> Result<(), (StatusCode, Json<NetworkDriverError>)> {
    if claims.role != "admin" {
        return Err((
            StatusCode::FORBIDDEN,
            Json(NetworkDriverError {
                error: "Forbidden: admin role required".to_string(),
            }),
        ));
    }
    Ok(())
}

pub async fn status(
    State(state): State<AppState>,
) -> Result<Json<DriverInjectionStatus>, (StatusCode, Json<NetworkDriverError>)> {
    let plugin = plugin(&state).await.map_err(internal_error)?;
    tokio::task::spawn_blocking(move || plugin.status())
        .await
        .map_err(|e| internal_message(e.to_string()))?
        .map(Json)
        .map_err(internal_error)
}

pub async fn list(
    State(state): State<AppState>,
) -> Result<Json<Vec<NetworkDriverPackage>>, (StatusCode, Json<NetworkDriverError>)> {
    let plugin = plugin(&state).await.map_err(internal_error)?;
    tokio::task::spawn_blocking(move || plugin.list())
        .await
        .map_err(|e| internal_message(e.to_string()))?
        .map(Json)
        .map_err(internal_error)
}

pub async fn import(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    mut multipart: Multipart,
) -> Result<Json<NetworkDriverPackage>, (StatusCode, Json<NetworkDriverError>)> {
    require_admin(&claims)?;
    let mut archive = None;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| bad_request(e.to_string()))?
    {
        if field.name() != Some("file") {
            continue;
        }
        if archive.is_some() {
            return Err(bad_request("only one file field is allowed".to_string()));
        }
        archive = Some(receive_archive(&mut field).await?);
    }

    let archive =
        archive.ok_or_else(|| bad_request("multipart field 'file' is required".to_string()))?;
    let plugin = plugin(&state).await.map_err(internal_error)?;
    // The worker owns cleanup even if the waiting HTTP request is cancelled.
    tokio::task::spawn_blocking(move || plugin.import_zip(archive.path()))
        .await
        .map_err(|e| internal_message(e.to_string()))?
        .map(Json)
        .map_err(internal_error)
}

async fn receive_archive(
    field: &mut axum::extract::multipart::Field<'_>,
) -> Result<tempfile::NamedTempFile, (StatusCode, Json<NetworkDriverError>)> {
    let archive = tokio::task::spawn_blocking(|| {
        tempfile::Builder::new()
            .prefix("diskless-driver-upload-")
            .suffix(".zip")
            .tempfile()
    })
    .await
    .map_err(|e| internal_message(e.to_string()))?
    .map_err(|e| internal_message(e.to_string()))?;
    let handle = archive
        .reopen()
        .map_err(|e| internal_message(e.to_string()))?;
    let mut file = tokio::fs::File::from_std(handle);
    let mut total = 0u64;
    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|e| bad_request(e.to_string()))?
    {
        total = total
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| payload_too_large("driver archive exceeds 512 MiB limit"))?;
        if total > MAX_DRIVER_ARCHIVE_BYTES {
            return Err(payload_too_large("driver archive exceeds 512 MiB limit"));
        }
        file.write_all(&chunk)
            .await
            .map_err(|e| internal_message(e.to_string()))?;
    }
    file.flush()
        .await
        .map_err(|e| internal_message(e.to_string()))?;
    Ok(archive)
}

pub async fn remove(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<NetworkDriverError>)> {
    require_admin(&claims)?;
    let plugin = plugin(&state).await.map_err(internal_error)?;
    tokio::task::spawn_blocking(move || plugin.remove(&id))
        .await
        .map_err(|e| internal_message(e.to_string()))?
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(internal_error)
}

pub async fn rebuild(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<DriverInjectionStatus>, (StatusCode, Json<NetworkDriverError>)> {
    require_admin(&claims)?;
    let plugin = plugin(&state).await.map_err(internal_error)?;
    tokio::task::spawn_blocking(move || plugin.rebuild())
        .await
        .map_err(|e| internal_message(e.to_string()))?
        .map(Json)
        .map_err(internal_error)
}

fn bad_request(message: String) -> (StatusCode, Json<NetworkDriverError>) {
    (
        StatusCode::BAD_REQUEST,
        Json(NetworkDriverError { error: message }),
    )
}

/// Report submitted by a working managed Windows client: its Boot NIC
/// hardware IDs plus an optional `pnputil`-exported driver archive.
#[derive(Debug, Deserialize)]
pub struct WindowsDriverReportRequest {
    /// Legacy reporter identity. Shared images should identify the client by
    /// their per-client iSCSI target instead.
    #[serde(default)]
    pub client_id: Option<String>,
    /// Target IQN uniquely assigned to the reporting client.
    #[serde(default)]
    pub target_iqn: Option<String>,
    pub windows_version: String,
    pub architecture: String,
    #[serde(default)]
    pub boot_nic_ids: Vec<String>,
    /// Exact candidate image revision this boot claims, when the reporter
    /// runs on a designated test client. Absent for plain driver reports.
    #[serde(default)]
    pub image_revision: Option<String>,
    /// Boot NIC hardware ID observed for this boot, when reporting a test.
    #[serde(default)]
    pub boot_nic_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WindowsDriverReportResponse {
    pub client_id: String,
    pub windows_version: String,
    pub architecture: String,
    pub boot_nic_ids: Vec<String>,
    pub package_id: Option<String>,
    pub reported_at: String,
    /// Boot-test evaluation for reports that claim a candidate revision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boot_test: Option<String>,
}

/// A report is bound to the registered client and its *active* iSCSI session:
/// the TCP peer of this request must appear in the named-ACL session list of
/// the client's own target. A caller-supplied client ID or MAC alone is never
/// sufficient, and no shared credential is embedded in the common image.
pub async fn submit_report(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    mut multipart: Multipart,
) -> Result<Json<WindowsDriverReportResponse>, (StatusCode, Json<NetworkDriverError>)> {
    let mut report_json: Option<String> = None;
    let mut archive = None;

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| bad_request(e.to_string()))?
    {
        match field.name() {
            Some("report") => {
                if report_json.is_some() {
                    return Err(bad_request("only one report field is allowed".to_string()));
                }
                let bytes = receive_report_json(&mut field).await?;
                report_json = Some(
                    String::from_utf8(bytes.to_vec())
                        .map_err(|_| bad_request("report field must be UTF-8 JSON".to_string()))?,
                );
            }
            Some("file") => {
                if archive.is_some() {
                    return Err(bad_request("only one file field is allowed".to_string()));
                }
                archive = Some(receive_archive(&mut field).await?);
            }
            _ => {}
        }
    }

    let report: WindowsDriverReportRequest = serde_json::from_str(
        &report_json
            .ok_or_else(|| bad_request("multipart field 'report' is required".to_string()))?,
    )
    .map_err(|e| bad_request(format!("invalid report JSON: {e}")))?;
    let boot_nic_ids = normalize_report_ids(&report.boot_nic_ids).map_err(bad_request)?;
    if boot_nic_ids.is_empty() {
        return Err(bad_request(
            "at least one Boot NIC hardware ID is required".to_string(),
        ));
    }
    if report.windows_version.trim().is_empty() || report.architecture.trim().is_empty() {
        return Err(bad_request(
            "windows_version and architecture are required".to_string(),
        ));
    }

    let client = resolve_report_client(
        &state,
        report.client_id.as_deref(),
        report.target_iqn.as_deref(),
    )
    .await?;
    let target_iqn = client.target_iqn.clone().ok_or_else(|| {
        (
            StatusCode::FORBIDDEN,
            Json(NetworkDriverError {
                error: "client has no active iSCSI target".to_string(),
            }),
        )
    })?;
    if !verify_report_session(&target_iqn, &client.name, peer.ip()).await {
        return Err((
            StatusCode::FORBIDDEN,
            Json(NetworkDriverError {
                error: "no active iSCSI session for this client from the reporting address"
                    .to_string(),
            }),
        ));
    }

    // Harvested packages enter through the same safe ZIP extraction and INF
    // inspection path as administrator imports, before the report is stored,
    // so a rejected archive leaves both the report and the catalog unchanged.
    let package_id = if let Some(archive) = archive {
        let plugin = plugin(&state).await.map_err(internal_error)?;
        let imported = tokio::task::spawn_blocking(move || plugin.import_zip(archive.path()))
            .await
            .map_err(|e| internal_message(e.to_string()))?
            .map_err(internal_error)?;
        Some(imported.id)
    } else {
        None
    };

    let reported_at = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        r#"INSERT INTO windows_driver_reports
        (client_id, windows_version, architecture, boot_nic_ids, package_ids, peer_address, reported_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(client_id) DO UPDATE SET
            windows_version = excluded.windows_version,
            architecture = excluded.architecture,
            boot_nic_ids = excluded.boot_nic_ids,
            package_ids = excluded.package_ids,
            peer_address = excluded.peer_address,
            reported_at = excluded.reported_at"#,
    )
    .bind(client.id.to_string())
    .bind(report.windows_version.trim())
    .bind(report.architecture.trim())
    .bind(serde_json::to_string(&boot_nic_ids).unwrap_or_default())
    .bind(serde_json::to_string(&package_id.clone().into_iter().collect::<Vec<_>>()).unwrap_or_default())
    .bind(peer.ip().to_string())
    .bind(&reported_at)
    .execute(&state.db_pool)
    .await
    .map_err(|e| internal_message(e.to_string()))?;

    // A test claim is evaluated only after the session binding above
    // passed: the test passes solely when the report names the exact
    // candidate revision and Boot NIC. Mismatches fail the test without
    // changing the revision, keeping the prior revision current.
    let boot_test = evaluate_test_claim(
        &state.db_pool,
        client.id.as_ref(),
        report.image_revision.as_deref(),
        report.boot_nic_id.as_deref(),
    )
    .await;

    Ok(Json(WindowsDriverReportResponse {
        client_id: client.id.to_string(),
        windows_version: report.windows_version.trim().to_string(),
        architecture: report.architecture.trim().to_string(),
        boot_nic_ids,
        package_id,
        reported_at,
        boot_test,
    }))
}

/// True when the reporting address holds an active session on the client's
/// own named initiator ACL. Unreadable session state fails closed. IPv4-mapped
/// IPv6 peers are normalized before comparison.
fn peer_authorized(peers: &[IpAddr], peer: IpAddr) -> bool {
    fn normalize(address: IpAddr) -> IpAddr {
        match address {
            IpAddr::V6(v6) => v6.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(address),
            other => other,
        }
    }
    let peer = normalize(peer);
    peers.iter().map(|p| normalize(*p)).any(|p| p == peer)
}

async fn verify_report_session(target_iqn: &str, client_name: &str, peer: IpAddr) -> bool {
    let target = target_iqn.to_string();
    let name = client_name.to_string();
    let peers = tokio::task::spawn_blocking(move || {
        crate::infrastructure::iscsi::active_session_peer_ips(&target, &name)
    })
    .await
    .ok()
    .and_then(|result| result.ok())
    .unwrap_or_default();
    peer_authorized(&peers, peer)
}

async fn resolve_report_client(
    state: &AppState,
    client_id: Option<&str>,
    target_iqn: Option<&str>,
) -> Result<Client, (StatusCode, Json<NetworkDriverError>)> {
    let client_id = client_id.map(str::trim).filter(|value| !value.is_empty());
    let target_iqn = target_iqn.map(str::trim).filter(|value| !value.is_empty());
    if client_id.is_none() && target_iqn.is_none() {
        return Err(bad_request(
            "client_id or target_iqn is required".to_string(),
        ));
    }

    let by_id = if let Some(client_id) = client_id {
        Some(
            state
                .application
                .clients
                .get_by_string(client_id)
                .await
                .map_err(|e| internal_message(e.to_string()))?
                .ok_or_else(|| {
                    (
                        StatusCode::NOT_FOUND,
                        Json(NetworkDriverError {
                            error: "unknown client".to_string(),
                        }),
                    )
                })?,
        )
    } else {
        None
    };

    let by_target = if let Some(target_iqn) = target_iqn {
        let client_ids: Vec<String> =
            sqlx::query_scalar("SELECT id FROM clients WHERE target_iqn = ?")
                .bind(target_iqn)
                .fetch_all(&state.db_pool)
                .await
                .map_err(|e| internal_message(e.to_string()))?;
        match client_ids.as_slice() {
            [] => {
                return Err((
                    StatusCode::NOT_FOUND,
                    Json(NetworkDriverError {
                        error: "unknown target_iqn".to_string(),
                    }),
                ));
            }
            [client_id] => Some(
                state
                    .application
                    .clients
                    .get_by_string(client_id)
                    .await
                    .map_err(|e| internal_message(e.to_string()))?
                    .ok_or_else(|| {
                        internal_message("target IQN references a missing client".to_string())
                    })?,
            ),
            _ => {
                return Err((
                    StatusCode::CONFLICT,
                    Json(NetworkDriverError {
                        error: "target_iqn does not identify exactly one client".to_string(),
                    }),
                ));
            }
        }
    } else {
        None
    };

    match (by_id, by_target) {
        (Some(client), Some(target_client)) if client.id != target_client.id => Err(bad_request(
            "client_id and target_iqn identify different clients".to_string(),
        )),
        (Some(client), _) => Ok(client),
        (None, Some(client)) => Ok(client),
        (None, None) => Err(bad_request(
            "client_id or target_iqn is required".to_string(),
        )),
    }
}

/// Evaluate a test claim carried by an already session-authorized report.
/// Returns the outcome string for the response, or `None` for plain driver
/// reports and reports with no open candidate.
async fn evaluate_test_claim(
    pool: &sqlx::SqlitePool,
    client_id: &str,
    image_revision: Option<&str>,
    boot_nic_id: Option<&str>,
) -> Option<String> {
    let (revision, nic) = match (image_revision, boot_nic_id) {
        (Some(revision), Some(nic)) if !revision.trim().is_empty() && !nic.trim().is_empty() => {
            (revision, nic)
        }
        _ => return None,
    };
    match crate::application::windows_servicing::record_boot_test_result(
        pool, client_id, revision, nic,
    )
    .await
    {
        Ok(crate::application::windows_servicing::BootTestOutcome::Passed) => {
            Some("passed".to_string())
        }
        Ok(crate::application::windows_servicing::BootTestOutcome::Failed) => {
            Some("failed".to_string())
        }
        Ok(crate::application::windows_servicing::BootTestOutcome::NoCandidate) => None,
        Err(error) => {
            log::error!("Boot test evaluation failed: {error:#}");
            None
        }
    }
}

fn payload_too_large(message: &str) -> (StatusCode, Json<NetworkDriverError>) {
    (
        StatusCode::PAYLOAD_TOO_LARGE,
        Json(NetworkDriverError {
            error: message.to_string(),
        }),
    )
}

async fn receive_report_json(
    field: &mut axum::extract::multipart::Field<'_>,
) -> Result<Vec<u8>, (StatusCode, Json<NetworkDriverError>)> {
    let mut bytes = Vec::new();
    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|e| bad_request(e.to_string()))?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_DRIVER_REPORT_BYTES {
            return Err(payload_too_large("driver report exceeds 64 KiB limit"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn internal_message(message: String) -> (StatusCode, Json<NetworkDriverError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(NetworkDriverError { error: message }),
    )
}
fn internal_error(error: anyhow::Error) -> (StatusCode, Json<NetworkDriverError>) {
    log::error!("Network driver operation failed: {error:#}");
    internal_message(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn report_identity_state() -> AppState {
        let (state, _, _, _) = crate::api::security_tests::setup().await;
        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, target_iqn, created_at, updated_at) VALUES ('report-a', 'report-a', '02:00:00:00:00:81', '192.0.2.81', 'pending', 'iqn.test:report-a', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'), ('report-b', 'report-b', '02:00:00:00:00:82', '192.0.2.82', 'pending', 'iqn.test:report-b', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
            .execute(&state.db_pool)
            .await
            .unwrap();
        state
    }

    #[tokio::test]
    async fn report_can_resolve_client_from_target_iqn_without_client_id() {
        let state = report_identity_state().await;
        let report: WindowsDriverReportRequest = serde_json::from_value(serde_json::json!({
            "target_iqn": "iqn.test:report-a",
            "windows_version": "11",
            "architecture": "x64"
        }))
        .unwrap();

        let client = resolve_report_client(
            &state,
            report.client_id.as_deref(),
            report.target_iqn.as_deref(),
        )
        .await
        .unwrap();

        assert_eq!(client.id.to_string(), "report-a");
    }

    #[tokio::test]
    async fn conflicting_report_identities_are_rejected_without_mutation() {
        let state = report_identity_state().await;
        let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM windows_driver_reports")
            .fetch_one(&state.db_pool)
            .await
            .unwrap();

        let error = resolve_report_client(&state, Some("report-a"), Some("iqn.test:report-b"))
            .await
            .unwrap_err();

        assert_eq!(error.0, StatusCode::BAD_REQUEST);
        let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM windows_driver_reports")
            .fetch_one(&state.db_pool)
            .await
            .unwrap();
        assert_eq!(after, before);
    }

    #[tokio::test]
    async fn unknown_target_iqn_is_rejected_without_mutation() {
        let state = report_identity_state().await;
        let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM windows_driver_reports")
            .fetch_one(&state.db_pool)
            .await
            .unwrap();

        let error = resolve_report_client(&state, None, Some("iqn.test:unknown"))
            .await
            .unwrap_err();

        assert_eq!(error.0, StatusCode::NOT_FOUND);
        let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM windows_driver_reports")
            .fetch_one(&state.db_pool)
            .await
            .unwrap();
        assert_eq!(after, before);
    }

    async fn upload_with_files(
        payload: &str,
        ending: &str,
        duplicate: bool,
        admin_import: bool,
    ) -> StatusCode {
        use axum::{body::Body, http::Request};
        use tower::ServiceExt as _;
        let (state, app, admin, _) = crate::api::security_tests::setup().await;
        let media = tempfile::tempdir().unwrap();
        state.settings.write().await.http.root_dir = media.path().to_string_lossy().into_owned();
        let boundary = "upload-regression";
        let file = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"driver.zip\"\r\n\r\n{payload}\r\n");
        let mut body = file.clone();
        if duplicate {
            body.push_str(&file);
            body.push_str(&format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"report\"\r\n\r\n{}\r\n",
                serde_json::json!({
                    "client_id": "unknown", "windows_version": "11", "architecture": "x64",
                    "boot_nic_ids": ["PCI\\VEN_8086&DEV_15F3"]
                })
            ));
        }
        body.push_str(ending);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(if admin_import {
                        "/api/pxe/network-drivers"
                    } else {
                        "/api/pxe/network-drivers/reports"
                    })
                    .header("authorization", format!("Bearer {admin}"))
                    .extension(ConnectInfo(
                        "192.0.2.99:49152".parse::<SocketAddr>().unwrap(),
                    ))
                    .header(
                        "content-type",
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        response.status()
    }

    fn assert_upload_removed(marker: &str) {
        // Locate only this test's unique bytes; concurrent uploads are unrelated.
        let directory = std::env::temp_dir().join("diskless-manager-driver-reports");
        if let Ok(entries) = std::fs::read_dir(directory) {
            for entry in entries {
                let path = entry.unwrap().path();
                if std::fs::read(&path).is_ok_and(|bytes| bytes.starts_with(marker.as_bytes())) {
                    let _ = std::fs::remove_file(&path);
                    panic!("rejected upload remained on disk");
                }
            }
        }
        if let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) {
            for entry in entries {
                let path = entry.unwrap().path();
                if path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("diskless-driver-upload-")
                    && std::fs::read(&path).is_ok_and(|bytes| bytes.starts_with(marker.as_bytes()))
                {
                    let _ = std::fs::remove_file(&path);
                    panic!("rejected upload remained on disk");
                }
            }
        }
    }

    #[tokio::test]
    async fn rejected_and_interrupted_report_uploads_are_removed() {
        for ending in ["--upload-regression--\r\n", ""] {
            let marker = uuid::Uuid::new_v4().to_string();
            assert_eq!(
                upload_with_files(&marker, ending, false, false).await,
                StatusCode::BAD_REQUEST
            );
            assert_upload_removed(&marker);
        }
    }

    #[tokio::test]
    async fn duplicate_report_archives_are_rejected_without_leaking_files() {
        let marker = uuid::Uuid::new_v4().to_string();
        assert_eq!(
            upload_with_files(&marker, "--upload-regression--\r\n", true, false).await,
            StatusCode::BAD_REQUEST
        );
        assert_upload_removed(&marker);
    }

    #[tokio::test]
    async fn malformed_and_unauthorized_reports_remove_their_archive() {
        for (report, expected) in [
            ("{", StatusCode::BAD_REQUEST),
            (
                r#"{"client_id":"unknown","windows_version":"11","architecture":"x64","boot_nic_ids":["PCI\\VEN_8086&DEV_15F3"]}"#,
                StatusCode::NOT_FOUND,
            ),
        ] {
            let marker = uuid::Uuid::new_v4().to_string();
            let ending = format!("--upload-regression\r\nContent-Disposition: form-data; name=\"report\"\r\n\r\n{report}\r\n--upload-regression--\r\n");
            assert_eq!(
                upload_with_files(&marker, &ending, false, false).await,
                expected
            );
            assert_upload_removed(&marker);
        }
    }

    #[tokio::test]
    async fn admin_import_rejects_duplicate_archives_without_leaking() {
        let marker = uuid::Uuid::new_v4().to_string();
        assert_eq!(
            upload_with_files(&marker, "--upload-regression--\r\n", true, true).await,
            StatusCode::BAD_REQUEST
        );
        assert_upload_removed(&marker);
    }

    #[test]
    fn session_peer_match_accepts_exact_and_mapped_addresses() {
        let peers = vec!["192.0.2.10".parse().unwrap()];
        assert!(peer_authorized(&peers, "192.0.2.10".parse().unwrap()));
        assert!(peer_authorized(
            &peers,
            "::ffff:192.0.2.10".parse().unwrap()
        ));
    }

    #[test]
    fn session_peer_mismatch_and_empty_list_are_rejected() {
        let peers = vec!["192.0.2.10".parse().unwrap()];
        assert!(!peer_authorized(&peers, "192.0.2.11".parse().unwrap()));
        assert!(!peer_authorized(&[], "192.0.2.10".parse().unwrap()));
    }

    #[test]
    fn malformed_hardware_ids_are_rejected() {
        assert!(normalize_report_ids(&["PCI\\VEN_8086&DEV_15F3".to_string()]).is_ok());
        assert!(normalize_report_ids(&["".to_string()]).is_err());
        assert!(normalize_report_ids(&["has space; DROP TABLE".to_string()]).is_err());
        assert!(
            normalize_report_ids(&["ok".to_string(), "ok".to_string()])
                .unwrap()
                .len()
                == 1
        );
    }

    #[test]
    fn upload_limits_have_explicit_boundaries() {
        assert_eq!(MAX_DRIVER_ARCHIVE_BYTES, 512 * 1024 * 1024);
        assert_eq!(MAX_DRIVER_REPORT_BYTES, 64 * 1024);
        assert_eq!(
            payload_too_large("oversized").0,
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }

    async fn claim_pool() -> (sqlx::SqlitePool, String) {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO images (id, name, kind, os_type, size_gb, path, format, status, created_at, updated_at) VALUES ('img', 'tank/image/windows', 'master', 'windows', 50, '/dev/zvol/tank/image/windows', 'raw', 'ready', ?, ?)")
            .bind(&now)
            .bind(&now)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, target_iqn, created_at, updated_at) VALUES ('test-1', 'client-test-1', '02:00:00:00:00:71', '192.0.2.71', 'pending', 'iqn.test:t1', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
            .execute(&pool)
            .await
            .unwrap();
        let revision = crate::application::windows_servicing::ensure_managed_revision(
            &pool,
            "img",
            "snap",
            &["pkg-a".to_string()],
        )
        .await
        .unwrap();
        crate::application::windows_servicing::mark_revision_awaiting_test(
            &pool,
            &revision.id,
            "rev-image",
        )
        .await
        .unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO images (id, name, kind, os_type, size_gb, path, format, status, created_at, updated_at) VALUES ('rev-image', 'tank/image/revision', 'master', 'windows', 50, '/dev/zvol/tank/image/revision', 'raw', 'ready', ?, ?)")
            .bind(&now)
            .bind(&now)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO images (id, name, kind, os_type, size_gb, path, format, status, parent_id, created_at, updated_at) VALUES ('rev-snapshot', 'serviced-claim', 'snapshot', 'windows', 50, '/dev/zvol/tank/image/revision', 'raw', 'ready', 'rev-image', ?, ?)")
            .bind(&now)
            .bind(&now)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE clients SET master = 'tank/image/revision', snapshot = 'tank/image/revision@serviced-claim' WHERE id = 'test-1'")
            .execute(&pool)
            .await
            .unwrap();
        crate::application::windows_servicing::start_boot_test(
            &pool,
            &revision.id,
            "test-1",
            "PCI\\VEN_8086&DEV_15F3",
        )
        .await
        .unwrap();
        (pool, revision.id)
    }

    #[tokio::test]
    async fn test_claim_passes_exact_match_and_ignores_plain_reports() {
        let (pool, revision) = claim_pool().await;
        assert_eq!(
            evaluate_test_claim(
                &pool,
                "test-1",
                Some(&revision),
                Some("PCI\\VEN_8086&DEV_15F3")
            )
            .await,
            Some("passed".to_string())
        );
        assert_eq!(evaluate_test_claim(&pool, "test-1", None, None).await, None);
        assert_eq!(
            evaluate_test_claim(&pool, "test-1", Some(""), Some("PCI\\VEN_8086&DEV_15F3")).await,
            None
        );
    }

    #[tokio::test]
    async fn test_claim_mismatch_fails_without_promoting() {
        let (pool, revision) = claim_pool().await;
        assert_eq!(
            evaluate_test_claim(
                &pool,
                "test-1",
                Some("other"),
                Some("PCI\\VEN_8086&DEV_15F3")
            )
            .await,
            Some("failed".to_string())
        );
        let status: String =
            sqlx::query_scalar("SELECT status FROM windows_image_driver_revisions WHERE id = ?")
                .bind(&revision)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "awaiting-test");
    }
}
