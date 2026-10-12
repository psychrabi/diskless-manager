use crate::infrastructure::pxe::{
    normalize_hardware_ids as normalize_report_ids, DriverInjectionStatus,
    NetworkDriverInjectionPlugin, NetworkDriverPackage,
};
use crate::state::AppState;
use axum::{
    extract::{ConnectInfo, Multipart, Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;

async fn plugin(state: &AppState) -> anyhow::Result<NetworkDriverInjectionPlugin> {
    let root = state.settings.read().await.http.root_dir.clone();
    Ok(NetworkDriverInjectionPlugin::new(PathBuf::from(root)))
}

#[derive(Debug, Serialize)]
pub struct NetworkDriverError {
    pub error: String,
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
    mut multipart: Multipart,
) -> Result<Json<NetworkDriverPackage>, (StatusCode, Json<NetworkDriverError>)> {
    let temp_dir = std::env::temp_dir().join("diskless-manager-network-drivers");
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .map_err(|e| internal_message(e.to_string()))?;
    let mut archive = None;

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| bad_request(e.to_string()))?
    {
        if field.name() != Some("file") {
            continue;
        }
        let filename = field.file_name().unwrap_or("driver.zip");
        let safe_name = filename
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();
        let path = temp_dir.join(format!("{}-{}", uuid::Uuid::new_v4(), safe_name));
        let mut file = tokio::fs::File::create(&path)
            .await
            .map_err(|e| internal_message(e.to_string()))?;
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|e| bad_request(e.to_string()))?
        {
            file.write_all(&chunk)
                .await
                .map_err(|e| internal_message(e.to_string()))?;
        }
        file.flush()
            .await
            .map_err(|e| internal_message(e.to_string()))?;
        archive = Some(path);
        break;
    }

    let archive =
        archive.ok_or_else(|| bad_request("multipart field 'file' is required".to_string()))?;
    let plugin = plugin(&state).await.map_err(internal_error)?;
    let archive_for_job = archive.clone();
    let result = tokio::task::spawn_blocking(move || plugin.import_zip(&archive_for_job))
        .await
        .map_err(|e| internal_message(e.to_string()))?
        .map_err(internal_error);
    let _ = tokio::fs::remove_file(&archive).await;
    result.map(Json)
}

pub async fn remove(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<NetworkDriverError>)> {
    let plugin = plugin(&state).await.map_err(internal_error)?;
    tokio::task::spawn_blocking(move || plugin.remove(&id))
        .await
        .map_err(|e| internal_message(e.to_string()))?
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(internal_error)
}

pub async fn rebuild(
    State(state): State<AppState>,
) -> Result<Json<DriverInjectionStatus>, (StatusCode, Json<NetworkDriverError>)> {
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
    pub client_id: String,
    pub windows_version: String,
    pub architecture: String,
    #[serde(default)]
    pub boot_nic_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct WindowsDriverReportResponse {
    pub client_id: String,
    pub windows_version: String,
    pub architecture: String,
    pub boot_nic_ids: Vec<String>,
    pub package_id: Option<String>,
    pub reported_at: String,
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
    let mut archive: Option<PathBuf> = None;
    let temp_dir = std::env::temp_dir().join("diskless-manager-driver-reports");
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .map_err(|e| internal_message(e.to_string()))?;

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| bad_request(e.to_string()))?
    {
        match field.name() {
            Some("report") => {
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| bad_request(e.to_string()))?;
                report_json = Some(
                    String::from_utf8(bytes.to_vec())
                        .map_err(|_| bad_request("report field must be UTF-8 JSON".to_string()))?,
                );
            }
            Some("file") => {
                let path = temp_dir.join(format!("report-{}.zip", uuid::Uuid::new_v4().simple()));
                let mut file = tokio::fs::File::create(&path)
                    .await
                    .map_err(|e| internal_message(e.to_string()))?;
                while let Some(chunk) = field
                    .chunk()
                    .await
                    .map_err(|e| bad_request(e.to_string()))?
                {
                    file.write_all(&chunk)
                        .await
                        .map_err(|e| internal_message(e.to_string()))?;
                }
                file.flush()
                    .await
                    .map_err(|e| internal_message(e.to_string()))?;
                archive = Some(path);
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
    if report.client_id.trim().is_empty()
        || report.windows_version.trim().is_empty()
        || report.architecture.trim().is_empty()
    {
        return Err(bad_request(
            "client_id, windows_version, and architecture are required".to_string(),
        ));
    }

    let client = state
        .application
        .clients
        .get_by_string(report.client_id.trim())
        .await
        .map_err(|e| internal_message(e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(NetworkDriverError {
                    error: "unknown client".to_string(),
                }),
            )
        })?;
    let target_iqn = client.target_iqn.clone().ok_or_else(|| {
        (
            StatusCode::FORBIDDEN,
            Json(NetworkDriverError {
                error: "client has no active iSCSI target".to_string(),
            }),
        )
    })?;
    if !verify_report_session(&target_iqn, &client.name, peer.ip()).await {
        if let Some(path) = archive {
            let _ = tokio::fs::remove_file(&path).await;
        }
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
    let package_id = if let Some(path) = archive.clone() {
        let plugin = plugin(&state).await.map_err(internal_error)?;
        let imported = tokio::task::spawn_blocking(move || plugin.import_zip(&path))
            .await
            .map_err(|e| internal_message(e.to_string()))?
            .map_err(internal_error)?;
        Some(imported.id)
    } else {
        None
    };
    if let Some(path) = archive {
        let _ = tokio::fs::remove_file(&path).await;
    }

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

    Ok(Json(WindowsDriverReportResponse {
        client_id: client.id.to_string(),
        windows_version: report.windows_version.trim().to_string(),
        architecture: report.architecture.trim().to_string(),
        boot_nic_ids,
        package_id,
        reported_at,
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
}
