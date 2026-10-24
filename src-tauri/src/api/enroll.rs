use crate::api::util::spawn_client_locked;
use axum::{
    extract::{ConnectInfo, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use log::info;
use std::net::SocketAddr;
use tracing::{error, warn};

use crate::{
    domain::{CreateClient, MacAddress, PxeMode},
    persistence::ClientRepository,
    state::AppState,
};

/// Default bind address for the client-enrollment listener.
const DEFAULT_ENROLL_ADDR: &str = "0.0.0.0:4237";

pub fn enroll_address(configured: Option<&str>) -> anyhow::Result<SocketAddr> {
    let value = configured.unwrap_or(DEFAULT_ENROLL_ADDR);
    value
        .parse()
        .map_err(|error| anyhow::anyhow!("invalid enrollment bind address '{value}': {error}"))
}

/// Stable auto-generated client name for a newly enrolled MAC.
fn client_name_from_mac(mac: &MacAddress) -> String {
    let slug: String = mac
        .as_str()
        .chars()
        .filter(char::is_ascii_hexdigit)
        .collect();
    format!("client-{slug}")
}

/// Enroll an unknown PXE client, then collect its hardware inventory before
/// an administrator enables it and assigns an image.
pub fn enroll_router(state: AppState) -> Router {
    Router::new()
        .route("/enroll/{mac}", get(enroll_client))
        .route("/inventory/{mac}", axum::routing::post(submit_inventory))
        .route("/boot/net-config/{mac}", get(client_network_config))
        .with_state(state)
}

pub struct EnrollServer {
    addr: SocketAddr,
    app: Router,
}

pub struct BoundEnrollServer {
    listener: tokio::net::TcpListener,
    app: Router,
}

impl EnrollServer {
    pub fn new(state: AppState, addr: SocketAddr) -> Self {
        Self {
            addr,
            app: enroll_router(state),
        }
    }

    pub async fn bind(self) -> anyhow::Result<BoundEnrollServer> {
        let listener = tokio::net::TcpListener::bind(self.addr)
            .await
            .map_err(|error| {
                anyhow::anyhow!("failed to bind enrollment server to {}: {error}", self.addr)
            })?;
        info!("Enrollment server bound to {}", listener.local_addr()?);
        Ok(BoundEnrollServer {
            listener,
            app: self.app,
        })
    }
}

impl BoundEnrollServer {
    pub async fn serve(self) -> anyhow::Result<()> {
        info!(
            "Serving client enrollment on {}",
            self.listener.local_addr()?
        );
        axum::serve(
            self.listener,
            self.app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .map_err(anyhow::Error::from)
    }
}

fn ipxe_response(script: String) -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        script,
    )
        .into_response()
}

/// Whether unknown MACs may currently self-register.
///
/// The window is lazy-expiring: no background job closes it, the check
/// itself enforces the deadline. `None` means closed.
pub fn enrollment_window_open(open_until: Option<i64>, now_unix: i64) -> bool {
    open_until.is_some_and(|until| now_unix < until)
}

/// Register an unknown MAC as a pending (disabled) client pinning its
/// current lease IP.
///
/// Pool semantics: the machine appears in the clients list but cannot
/// boot until an administrator enables and provisions it. Registration
/// itself is gated on the enrollment window by the caller.
async fn register_pending_client(
    state: &AppState,
    mac: MacAddress,
    source_ip: std::net::IpAddr,
) -> anyhow::Result<String> {
    let _client_guard = state.client_mutations.lock().await;
    let settings = state.settings.read().await.clone();

    let clients = ClientRepository::new(state.db_pool.clone());
    if let Some(existing) = clients.find_by_mac(&mac).await? {
        return Ok(existing.name);
    }

    let name = client_name_from_mac(&mac);
    let images =
        match crate::persistence::repositories::image::ImageRepository::new(state.db_pool.clone())
            .list()
            .await
        {
            Ok(images) => images,
            Err(error) => {
                warn!("failed to find the default image for enrolled client: {error:#}");
                Vec::new()
            }
        };
    let default_assignment = images
        .iter()
        .find(|image| image.is_default && image.kind != crate::core::image::ImageKind::Snapshot)
        .and_then(|default_image| {
            images
                .iter()
                .filter(|image| {
                    image.kind == crate::core::image::ImageKind::Snapshot
                        && image.parent_id.as_deref() == Some(default_image.id.as_str())
                })
                .max_by_key(|snapshot| snapshot.created_at)
                .map(|snapshot| {
                    (
                        default_image.name.clone(),
                        format!("{}@{}", default_image.name, snapshot.name),
                    )
                })
        });
    let (master, snapshot) =
        default_assignment.unwrap_or_else(|| ("pending".into(), String::new()));
    let request = CreateClient {
        installer_media_id: None,
        enabled: None,
        name: name.clone(),
        mac: mac.to_string(),
        ip: source_ip.to_string(),
        // Image selection is metadata only; storage is provisioned after
        // inventory and administrator enablement.
        master,
        snapshot: (!snapshot.is_empty()).then_some(snapshot),
        block_store: None,
        block_device: None,
        target_iqn: None,
        pxe_mode: PxeMode::Uefi,
        keep_writeback: true,
        use_game_disk: false,
        game_disks: Vec::new(),
        // Pool clients get credentials on first provisioning, not here.
        chap_enabled: false,
    };
    let mut client = crate::domain::Client::create(request)
        .map_err(|error| anyhow::anyhow!("invalid enrolled client: {error}"))?;
    // Pool, not provisioned: visible in the UI, barred from booting until
    // an administrator enables the record and assigns an image.
    client.enabled = false;

    clients.insert(&client).await?;
    tracing::info!(
        client = %client.name,
        mac = %client.mac.as_str(),
        ip = %source_ip,
        "registered new client via PXE enrollment"
    );

    // Pin the lease IP as a static reservation. Best-effort: a broken or
    // disabled DHCP stack must not prevent the client from appearing in the
    // UI. Retring on the next boot also self-heals transient failures.
    if settings.dhcp.enabled {
        let dhcp = crate::services::DhcpService::new(settings, state.db_pool.clone());
        if let Err(error) = dhcp.generate_client_configs().await {
            warn!(
                client = %client.name,
                "failed to write DHCP reservation for enrolled client: {error:#}"
            );
        } else if let Err(error) = dhcp.validate_config().await {
            warn!(
                client = %client.name,
                "DHCP configuration validation failed after enrollment: {error:#}"
            );
        } else if let Err(error) = dhcp.reload().await {
            warn!(
                client = %client.name,
                "failed to reload DHCP after enrollment: {error:#}"
            );
        }
    }

    let _ = state.refresh_client_ips().await;

    Ok(name)
}

/// Read-only boot-network profile for a registered, enabled diskless client.
/// Only the client's own reserved source IP can retrieve it. Exposed on the
/// isolated PXE enrollment listener, never on the management API.
async fn client_network_config(
    State(state): State<AppState>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    Path(mac): Path<String>,
) -> Response {
    use axum::{http::StatusCode, Json};
    use serde_json::json;
    let Ok(mac) = MacAddress::parse(&mac) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let repository = ClientRepository::new(state.db_pool.clone());
    let Ok(Some(client)) = repository.find_by_mac(&mac).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !client.enabled || client.ip != remote.ip() || client.target_iqn.is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let settings = state.settings.read().await;
    // Configuration comes from the same saved DHCP settings used for PXE.
    // Client-side logic checks the MAC *and* its existing IPv4 before touching
    // gateway or DNS, so a network transition will not reset the SAN NIC.
    let dns = [&settings.dhcp.dns_server1, &settings.dhcp.dns_server2]
        .into_iter()
        .filter(|value| value.parse::<std::net::Ipv4Addr>().is_ok())
        .collect::<Vec<_>>();
    Json(json!({
        "mac": client.mac.as_str().replace(':', ""),
        "ip": client.ip.to_string(),
        "gateway": settings.dhcp.gateway_ip,
        "dns": dns,
    }))
    .into_response()
}

async fn enroll_client(
    State(state): State<AppState>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    Path(mac): Path<String>,
) -> Response {
    let parsed = match MacAddress::parse(&mac) {
        Ok(parsed) => parsed,
        Err(error) => {
            warn!(mac = %mac, "rejected enrollment request with invalid MAC: {error}");
            return ipxe_response(crate::infrastructure::pxe::render_enrollment_invalid_mac());
        }
    };

    let clients = ClientRepository::new(state.db_pool.clone());
    let enroll_started = std::time::Instant::now();
    let enroll_mac = parsed.to_string();
    let script = match clients.find_by_mac(&parsed).await {
        Ok(Some(client)) => {
            // Binding gate: a disabled record never boots, even when it
            // is fully provisioned. This is what stops decommissioned
            // machines and lets admins quarantine a client instantly.
            if !client.enabled {
                tracing::info!(
                    client = %client.name,
                    mac = %parsed.as_str(),
                    "disabled client attempted to boot; denying"
                );
                crate::infrastructure::pxe::render_enrollment_disabled()
            } else if consume_inventory_boot_request(&state, &parsed).await {
                crate::infrastructure::pxe::render_inventory_boot()
            } else {
                let provisioning = !client.master.trim().is_empty() && client.master != "pending";
                if provisioning {
                    let client_id = client.id.to_string();
                    let lock_client_id = client_id.clone();
                    let requesting_mac = parsed.to_string();
                    let task_state = state.clone();
                    let receive = spawn_client_locked(
                        task_state.client_mutations.clone(),
                        lock_client_id,
                        Some(std::time::Duration::from_secs(2)),
                        async move {
                            let result = crate::application::client_boot::ensure_client_boot_ready(
                                &task_state,
                                &client_id,
                                &requesting_mac,
                            )
                            .await;
                            if let Err(error) = &result {
                                if let Err(record_error) =
                                    crate::application::client_boot::record_boot_failure(
                                        &task_state,
                                        &client_id,
                                        &format!("{error:#}"),
                                    )
                                    .await
                                {
                                    error!(
                                    client_id,
                                    "failed to persist PXE readiness diagnostics: {record_error:#}"
                                );
                                }
                            }
                            result.map_err(|error| format!("{error:#}"))
                        },
                    );
                    match receive.await {
                        Ok(Ok(Ok(script))) => script,
                        Ok(Ok(Err(error))) => {
                            warn!(client = %client.name, "PXE storage readiness deferred: {error}");
                            crate::infrastructure::pxe::render_enrollment_retry()
                        }
                        Ok(Err(error)) => {
                            warn!(client = %client.name, "PXE storage readiness busy: {error}");
                            crate::infrastructure::pxe::render_enrollment_retry()
                        }
                        Err(error) => {
                            warn!(client = %client.name, "PXE storage readiness task stopped: {error}");
                            crate::infrastructure::pxe::render_enrollment_retry()
                        }
                    }
                } else {
                    tracing::info!(
                        client = %client.name,
                        mac = %parsed.as_str(),
                        "enrolled client is awaiting an image assignment"
                    );
                    crate::infrastructure::pxe::render_enrollment_pending()
                }
            }
        }
        Ok(None) => {
            // Unknown machines may only self-register inside an
            // admin-opened window. Outside it they cannot distinguish
            // "closed" from "nonexistent" beyond the reboot loop.
            let window_open = {
                let settings = state.settings.read().await;
                enrollment_window_open(
                    settings.enrollment.open_until,
                    chrono::Utc::now().timestamp(),
                )
            };
            if !window_open {
                tracing::info!(
                    mac = %parsed.as_str(),
                    "rejected enrollment outside the registration window"
                );
                crate::infrastructure::pxe::render_enrollment_closed()
            } else {
                match register_pending_client(&state, parsed, remote.ip()).await {
                    Ok(_) => crate::infrastructure::pxe::render_inventory_boot(),
                    Err(error) => {
                        error!("failed to register enrolled client: {error:#}");
                        crate::infrastructure::pxe::render_enrollment_pending()
                    }
                }
            }
        }
        Err(error) => {
            error!("failed to look up client during enrollment: {error:#}");
            crate::infrastructure::pxe::render_enrollment_pending()
        }
    };

    tracing::info!(
        mac = %enroll_mac,
        elapsed_ms = enroll_started.elapsed().as_millis(),
        "PXE enrollment script served"
    );
    ipxe_response(script)
}

/// Inventory submitted from WinPE for an already registered client.
///
/// The request source must match that client's recorded PXE address. The MAC
/// is only a lookup key, and matching drivers still uses hardware IDs.
async fn submit_inventory(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(mac): Path<String>,
    Json(payload): Json<crate::infrastructure::pxe::NicInventoryPayload>,
) -> (StatusCode, Json<serde_json::Value>) {
    let parsed = match MacAddress::parse(&mac) {
        Ok(parsed) => parsed,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"ok": false, "error": "invalid MAC address"})),
            )
        }
    };
    let validated = match crate::infrastructure::pxe::parse_inventory_payload(&payload) {
        Ok(validated) => validated,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"ok": false, "error": error.to_string()})),
            )
        }
    };
    let client =
        sqlx::query_as::<_, (String, Option<String>)>("SELECT id, ip FROM clients WHERE mac = ?")
            .bind(parsed.as_str())
            .fetch_optional(&state.db_pool)
            .await;
    let client_id = match client {
        Ok(Some((client_id, Some(client_ip)))) if peer_matches_client_ip(&client_ip, peer.ip()) => {
            client_id
        }
        Ok(_) => {
            return (
                StatusCode::FORBIDDEN,
                Json(
                    serde_json::json!({"ok": false, "error": "inventory submission is not authorized for this client address"}),
                ),
            );
        }
        Err(error) => {
            error!("failed to authorize NIC inventory submission: {error:#}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"ok": false, "error": "failed to authorize inventory"})),
            );
        }
    };
    let observed_at = chrono::Utc::now().to_rfc3339();
    let stored = sqlx::query(
        r#"INSERT INTO windows_nic_inventory
        (mac, client_id, boot_nic_ids, secondary_ids, architecture, firmware, subvendor_id, subdevice_id, observed_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(mac) DO UPDATE SET
            client_id = excluded.client_id,
            boot_nic_ids = excluded.boot_nic_ids,
            secondary_ids = excluded.secondary_ids,
            architecture = excluded.architecture,
            firmware = excluded.firmware,
            subvendor_id = excluded.subvendor_id,
            subdevice_id = excluded.subdevice_id,
            observed_at = excluded.observed_at"#,
    )
    .bind(parsed.as_str())
    .bind(&client_id)
    .bind(serde_json::to_string(&validated.boot_nic_ids).unwrap_or_default())
    .bind(serde_json::to_string(&validated.secondary_ids).unwrap_or_default())
    .bind(&validated.architecture)
    .bind(validated.firmware.to_string())
    .bind(validated.subvendor_id.as_deref().unwrap_or_default())
    .bind(validated.subdevice_id.as_deref().unwrap_or_default())
    .bind(&observed_at)
    .execute(&state.db_pool)
    .await;
    if let Err(error) = stored {
        error!("failed to store NIC inventory: {error:#}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"ok": false, "error": "failed to store inventory"})),
        );
    }
    tracing::info!(
        mac = %parsed.as_str(),
        client_id,
        peer_ip = %peer.ip(),
        boot_nics = validated.boot_nic_ids.len(),
        "stored preboot NIC inventory"
    );
    (
        StatusCode::OK,
        Json(serde_json::json!({"ok": true, "mac": parsed.as_str()})),
    )
}

fn peer_matches_client_ip(client_ip: &str, peer: std::net::IpAddr) -> bool {
    fn normalize(address: std::net::IpAddr) -> std::net::IpAddr {
        match address {
            std::net::IpAddr::V6(v6) => v6
                .to_ipv4_mapped()
                .map(std::net::IpAddr::V4)
                .unwrap_or(address),
            other => other,
        }
    }
    client_ip
        .parse::<std::net::IpAddr>()
        .is_ok_and(|client_ip| normalize(client_ip) == normalize(peer))
}

/// Consume the one-shot request before sending the client to WinPE. If it never
/// uploads inventory or fails to boot, its next request returns to normal PXE.
async fn consume_inventory_boot_request(state: &AppState, mac: &MacAddress) -> bool {
    sqlx::query("UPDATE clients SET inventory_boot_requested = 0 WHERE mac = ? AND inventory_boot_requested = 1")
        .bind(mac.as_str())
        .execute(&state.db_pool)
        .await
        .ok()
        .is_some_and(|result| result.rows_affected() == 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::util::spawn_locked;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use std::path::PathBuf;
    use std::{collections::HashMap, path::Path, sync::Mutex};
    use tower::ServiceExt;

    use crate::{
        application::storage_service::{GameMaster, StorageService},
        infrastructure::{
            image::{ImageBackend, ImageBackendInfo},
            iscsi::{
                ChapCredentials, IscsiLunState, IscsiProvisioner, IscsiTargetSpec, IscsiTargetState,
            },
        },
    };

    #[derive(Default)]
    struct FakeImage(Mutex<HashMap<String, String>>);

    impl ImageBackend for FakeImage {
        fn validate_unmounted(&self, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn settle_device_changes(&self) -> anyhow::Result<()> {
            Ok(())
        }
        fn clone_origin(&self, name: &str) -> anyhow::Result<Option<String>> {
            Ok(self.0.lock().unwrap().get(name).cloned())
        }
        fn exists(&self, name: &str) -> anyhow::Result<bool> {
            Ok(self.0.lock().unwrap().contains_key(name))
        }
        fn create_volume(&self, _: &str, _: u64) -> anyhow::Result<()> {
            Ok(())
        }
        fn destroy(&self, name: &str) -> anyhow::Result<()> {
            self.0.lock().unwrap().remove(name);
            Ok(())
        }
        fn rename(&self, _: &str, _: &str) -> anyhow::Result<()> {
            anyhow::bail!("unused")
        }
        fn clone_image(&self, source: &str, destination: &str) -> anyhow::Result<()> {
            self.0
                .lock()
                .unwrap()
                .insert(destination.into(), source.into());
            Ok(())
        }
        fn create_snapshot(&self, _: &str, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn destroy_snapshot(&self, _: &str, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn rollback_snapshot(&self, _: &str, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn resize(&self, _: &str, _: u64) -> anyhow::Result<()> {
            Ok(())
        }
        fn import_raw(&self, _: &Path, _: &str, _: u64) -> anyhow::Result<()> {
            Ok(())
        }
        fn verify(&self, name: &str) -> anyhow::Result<bool> {
            self.exists(name)
        }
        fn info(&self, _: &str) -> anyhow::Result<ImageBackendInfo> {
            anyhow::bail!("unused")
        }
        fn set_os_type(&self, _: &str, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn image_parent(&self) -> anyhow::Result<String> {
            Ok("pool".into())
        }
    }

    #[derive(Default)]
    struct FakeIscsi {
        luns: Mutex<Vec<IscsiLunState>>,
        creates: Mutex<usize>,
        acl: Mutex<Option<String>>,
        chap_updates: Mutex<usize>,
    }

    impl IscsiProvisioner for FakeIscsi {
        fn create_target(&self, spec: &IscsiTargetSpec) -> anyhow::Result<()> {
            *self.creates.lock().unwrap() += 1;
            *self.luns.lock().unwrap() = spec
                .luns
                .iter()
                .map(|lun| IscsiLunState {
                    lun: lun.lun,
                    backstore: lun.backstore.clone(),
                    exists: true,
                    backstore_exists: true,
                    block_device_matches: true,
                })
                .collect();
            Ok(())
        }
        fn ensure_initiator_acl(&self, spec: &IscsiTargetSpec) -> anyhow::Result<()> {
            *self.acl.lock().unwrap() = spec.initiator_iqn.clone();
            Ok(())
        }
        fn remove_target(&self, _: &str) -> anyhow::Result<()> {
            anyhow::bail!("unexpected removal")
        }
        fn remove_target_with_backstores(&self, _: &str, _: &[String]) -> anyhow::Result<()> {
            anyhow::bail!("unexpected removal")
        }
        fn target_exists(&self, _: &str) -> anyhow::Result<bool> {
            Ok(!self.luns.lock().unwrap().is_empty())
        }
        fn set_chap_auth(&self, _: &str, _: Option<&ChapCredentials>) -> anyhow::Result<()> {
            *self.chap_updates.lock().unwrap() += 1;
            Ok(())
        }
        fn list_target_luns(&self, _: &str) -> anyhow::Result<Vec<IscsiLunState>> {
            Ok(self.luns.lock().unwrap().clone())
        }
        fn inspect_target(&self, spec: &IscsiTargetSpec) -> anyhow::Result<IscsiTargetState> {
            let attached = self.luns.lock().unwrap().clone();
            let exists = !attached.is_empty();
            let luns = if exists {
                attached
            } else {
                spec.luns
                    .iter()
                    .map(|lun| IscsiLunState {
                        lun: lun.lun,
                        backstore: lun.backstore.clone(),
                        exists: false,
                        backstore_exists: false,
                        block_device_matches: false,
                    })
                    .collect()
            };
            Ok(IscsiTargetState::from_luns(
                spec.target_iqn.clone(),
                exists,
                luns,
                exists,
            ))
        }
        fn reconcile(&self, spec: &IscsiTargetSpec) -> anyhow::Result<()> {
            self.create_target(spec)
        }
    }

    async fn test_state() -> AppState {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        AppState {
            config: crate::config::RuntimeConfig::default(),
            client_mutations: crate::state::ClientMutationLocks::default(),
            settings: std::sync::Arc::new(tokio::sync::RwLock::new(
                crate::core::config::Settings::default(),
            )),
            db_pool: pool.clone(),
            config_path: PathBuf::new(),
            client_ips: std::sync::Arc::new(tokio::sync::RwLock::new(Vec::new())),
            metrics_collector: std::sync::Arc::new(crate::metrics::MetricsCollector::default()),
            ssh_executor: std::sync::Arc::new(crate::ssh_executor::SshExecutor::new()),
            application: std::sync::Arc::new(crate::application::ApplicationServices::new(pool)),
        }
    }

    async fn post_inventory(
        state: &AppState,
        mac: &str,
        body: serde_json::Value,
    ) -> (StatusCode, serde_json::Value) {
        post_inventory_from(state, mac, body, "192.0.2.50").await
    }

    async fn post_inventory_from(
        state: &AppState,
        mac: &str,
        body: serde_json::Value,
        peer_ip: &str,
    ) -> (StatusCode, serde_json::Value) {
        let app = enroll_router(state.clone());
        let peer: ConnectInfo<SocketAddr> = ConnectInfo(
            format!("{peer_ip}:68")
                .parse()
                .expect("test peer must parse"),
        );
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/inventory/{mac}"))
                    .extension(peer)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap_or_default();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(serde_json::json!(null)),
        )
    }

    fn inventory_body() -> serde_json::Value {
        serde_json::json!({
            "boot_nic_ids": ["PCI\\VEN_8086&DEV_15F3"],
            "secondary_ids": ["PCI\\VEN_10EC&DEV_8168"],
            "architecture": "AMD64",
            "firmware": "uefi",
            "subvendor_id": "8086",
            "subdevice_id": "15F3",
        })
    }

    async fn get_enroll(state: &AppState, mac: &str) -> (StatusCode, String) {
        let app = enroll_router(state.clone());
        let peer: ConnectInfo<SocketAddr> =
            ConnectInfo("192.0.2.50:68".parse().expect("test peer must parse"));
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/enroll/{mac}"))
                    .extension(peer)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap_or_default();
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    #[tokio::test]
    async fn assigned_boot_creates_boot_and_selected_game_luns_before_returning_menu() {
        let mut state = test_state().await;
        let image = std::sync::Arc::new(FakeImage::default());
        let iscsi = std::sync::Arc::new(FakeIscsi::default());
        let storage =
            StorageService::new(image.clone(), iscsi.clone()).with_test_game_masters(vec![
                GameMaster {
                    dataset: "pool/games/game".into(),
                    disk_type: "game".into(),
                    size_bytes: Some(1024),
                },
            ]);
        std::sync::Arc::get_mut(&mut state.application)
            .expect("test application has one owner")
            .storage = std::sync::Arc::new(storage);
        let root = tempfile::tempdir().unwrap();
        let menu = root.path().join("clients/02000000000d.ipxe");
        std::fs::create_dir_all(menu.parent().unwrap()).unwrap();
        std::fs::write(&menu, "#!ipxe\necho custom menu\n").unwrap();
        state.settings.write().await.http.root_dir = root.path().display().to_string();
        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, snapshot, writeback, block_device, block_store, target_iqn, keep_writeback, use_game_disk, enabled, created_at, updated_at) VALUES ('boot-game', 'client-boot-game', '02:00:00:00:00:0d', '192.0.2.13', 'pool/master', 'pool/master@ready', 'pool/clients/boot-game', '/dev/zvol/pool/clients/boot-game', 'block_boot_game', 'iqn.test:boot-game', 1, 1, 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
            .execute(&state.db_pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO client_game_disks(client_id, master_dataset) VALUES ('boot-game', 'pool/games/game')")
            .execute(&state.db_pool)
            .await
            .unwrap();

        let ((_, first), (_, second)) = tokio::join!(
            get_enroll(&state, "02:00:00:00:00:0d"),
            get_enroll(&state, "02:00:00:00:00:0d"),
        );

        assert!(first.contains("custom menu") || second.contains("custom menu"));
        for response in [&first, &second] {
            if !response.contains("custom menu") {
                assert!(response.contains("storage is not ready yet"));
            }
        }
        let (_, retry) = get_enroll(&state, "02:00:00:00:00:0d").await;
        assert!(retry.contains("custom menu"));
        assert!(image.exists("pool/clients/boot-game").unwrap());
        assert!(image.exists("pool/games/boot-game-game-games").unwrap());
        assert_eq!(*iscsi.creates.lock().unwrap(), 1);
        assert_eq!(iscsi.luns.lock().unwrap().len(), 2);
        assert_eq!(
            iscsi.acl.lock().unwrap().as_deref(),
            Some("iqn.2026-01.client:client.client-boot-game")
        );
        let reset: (Option<i64>, i64, i64, Option<String>) = sqlx::query_as(
            "SELECT offline_since, completed, failures, last_error FROM client_offline_resets WHERE client_id = 'boot-game'",
        )
        .fetch_one(&state.db_pool)
        .await
        .unwrap();
        assert_eq!(reset, (None, 0, 0, None));
    }

    #[tokio::test]
    async fn boot_readiness_rejects_mac_rebound_before_storage_mutation() {
        let state = test_state().await;
        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, snapshot, writeback, block_device, block_store, target_iqn, keep_writeback, use_game_disk, enabled, created_at, updated_at) VALUES ('boot-rebound', 'client-rebound', '02:00:00:00:00:0d', '192.0.2.13', 'pool/master', 'pool/master@ready', 'pool/clients/rebound', '/dev/zvol/pool/clients/rebound', 'block_rebound', 'iqn.test:rebound', 1, 0, 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
            .execute(&state.db_pool)
            .await
            .unwrap();

        let error = crate::application::client_boot::ensure_client_boot_ready(
            &state,
            "boot-rebound",
            "02:00:00:00:00:fe",
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("reassigned"));
        let reset_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM client_offline_resets")
            .fetch_one(&state.db_pool)
            .await
            .unwrap();
        assert_eq!(reset_rows, 0);
    }

    #[tokio::test]
    async fn busy_boot_returns_retry_without_late_provisioning() {
        let state = test_state().await;
        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, enabled, created_at, updated_at) VALUES ('busy-boot', 'busy-boot', '02:00:00:00:00:ee', '192.0.2.16', 'pool/master', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
            .execute(&state.db_pool).await.unwrap();
        let guard = state.client_mutations.lock().await;
        let (_, script) = tokio::time::timeout(
            std::time::Duration::from_secs(4),
            get_enroll(&state, "02:00:00:00:00:ee"),
        )
        .await
        .unwrap();
        assert!(!script.contains("chain --replace"));
        assert!(script.contains("sleep 10"));
        drop(guard);
        tokio::task::yield_now().await;
        let failures: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM client_boot_failures")
            .fetch_one(&state.db_pool)
            .await
            .unwrap();
        assert_eq!(failures, 0);
    }

    #[tokio::test]
    async fn assigned_client_without_target_can_take_inventory_boot() {
        let state = test_state().await;
        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, inventory_boot_requested, enabled, created_at, updated_at) VALUES ('inventory-assigned', 'inventory-assigned', '02:00:00:00:00:ef', '192.0.2.15', 'pool/master', 1, 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
            .execute(&state.db_pool).await.unwrap();
        let (_, script) = get_enroll(&state, "02:00:00:00:00:ef").await;
        assert!(script.contains("boot-inventory.wim"));
        let requested: i64 = sqlx::query_scalar(
            "SELECT inventory_boot_requested FROM clients WHERE id = 'inventory-assigned'",
        )
        .fetch_one(&state.db_pool)
        .await
        .unwrap();
        assert_eq!(requested, 0);
    }

    #[tokio::test]
    async fn enroll_directs_flagged_client_to_inventory_then_pending() {
        let state = test_state().await;
        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, enabled, created_at, updated_at) VALUES ('c2', 'client-c2', '02:00:00:00:00:0c', '192.0.2.12', 'pending', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
            .execute(&state.db_pool)
            .await
            .unwrap();
        let (_, plain) = get_enroll(&state, "02:00:00:00:00:0c").await;
        assert!(plain.contains("No master image has been assigned yet"));
        sqlx::query("UPDATE clients SET inventory_boot_requested = 1 WHERE id = 'c2'")
            .execute(&state.db_pool)
            .await
            .unwrap();
        let (_, script) = get_enroll(&state, "02:00:00:00:00:0c").await;
        assert!(script.contains("boot-inventory.wim"));
        assert!(!script.contains("clients/${client-mac}.ipxe"));
        let (_, next_boot) = get_enroll(&state, "02:00:00:00:00:0c").await;
        assert!(next_boot.contains("No master image has been assigned yet"));
        let inventory_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM windows_nic_inventory")
            .fetch_one(&state.db_pool)
            .await
            .unwrap();
        assert_eq!(inventory_count, 0);
    }

    #[tokio::test]
    async fn boot_failure_diagnostics_preserve_client_assignments_and_credentials() {
        let state = test_state().await;
        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, snapshot, writeback, block_device, block_store, target_iqn, keep_writeback, use_game_disk, chap_enabled, chap_user, chap_secret, created_at, updated_at) VALUES ('ready', 'client-ready', '02:00:00:00:00:0d', '192.0.2.13', 'pool/master', 'pool/master@ready', 'pool/clients/ready', '/dev/zvol/pool/clients/ready', 'block_ready', 'iqn.test:ready', 1, 1, 1, 'synthetic-user', 'synthetic-password', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
            .execute(&state.db_pool)
            .await
            .unwrap();

        crate::application::client_boot::record_boot_failure(
            &state,
            "ready",
            "synthetic target failure",
        )
        .await
        .unwrap();
        crate::application::client_boot::record_boot_failure(
            &state,
            "ready",
            "synthetic target failure",
        )
        .await
        .unwrap();

        let diagnostic: (i64, String) = sqlx::query_as(
            "SELECT failures, last_error FROM client_boot_failures WHERE client_id = 'ready'",
        )
        .fetch_one(&state.db_pool)
        .await
        .unwrap();
        let assignment: (String, String, String, String, String) = sqlx::query_as(
            "SELECT snapshot, writeback, block_device, target_iqn, chap_secret FROM clients WHERE id = 'ready'",
        )
        .fetch_one(&state.db_pool)
        .await
        .unwrap();
        assert_eq!(diagnostic.0, 2);
        assert_eq!(diagnostic.1, "synthetic target failure");
        let reset_rows: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM client_offline_resets WHERE client_id = 'ready'",
        )
        .fetch_one(&state.db_pool)
        .await
        .unwrap();
        assert_eq!(reset_rows, 0);
        assert_eq!(
            assignment,
            (
                "pool/master@ready".into(),
                "pool/clients/ready".into(),
                "/dev/zvol/pool/clients/ready".into(),
                "iqn.test:ready".into(),
                "synthetic-password".into(),
            )
        );
    }

    #[tokio::test]
    async fn assigned_client_with_unrecoverable_pending_work_gets_retry_not_redirect() {
        let state = test_state().await;
        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, snapshot, writeback, block_device, block_store, target_iqn, created_at, updated_at) VALUES ('retry', 'client-retry', '02:00:00:00:00:0e', '192.0.2.14', 'pool/master', 'pool/master@ready', 'pool/clients/retry', '/dev/zvol/pool/clients/retry', 'block_retry', 'iqn.test:retry', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
            .execute(&state.db_pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO client_offline_resets(client_id, fingerprint, operation) VALUES ('retry', 'old', '{}')")
            .execute(&state.db_pool)
            .await
            .unwrap();

        let (_, script) = get_enroll(&state, "02:00:00:00:00:0e").await;

        assert!(script.contains("storage is not ready yet"));
        assert!(script.contains("sleep 10"));
        assert!(!script.contains("clients/${client-mac}.ipxe"));
        let error: String = sqlx::query_scalar(
            "SELECT last_error FROM client_boot_failures WHERE client_id = 'retry'",
        )
        .fetch_one(&state.db_pool)
        .await
        .unwrap();
        assert!(error.contains("missing field"));
    }

    #[tokio::test]
    async fn disabled_and_unknown_clients_do_not_start_storage_readiness() {
        let state = test_state().await;
        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, enabled, created_at, updated_at) VALUES ('disabled', 'client-disabled', '02:00:00:00:00:0f', '192.0.2.15', 'pool/master', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
            .execute(&state.db_pool)
            .await
            .unwrap();

        let (_, disabled) = get_enroll(&state, "02:00:00:00:00:0f").await;
        let (_, unknown) = get_enroll(&state, "02:00:00:00:00:10").await;

        assert!(disabled.contains("client disabled"));
        assert!(unknown.contains("enrollment closed"));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM client_offline_resets")
            .fetch_one(&state.db_pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn boot_readiness_rejects_shared_target_but_allows_shared_master_volume() {
        let state = test_state().await;
        for (index, (id, target, backstore, snapshot, writeback, device)) in [
            (
                "a",
                "iqn.test:shared",
                "block_shared",
                Some("pool/master@ready"),
                Some("pool/clients/a"),
                "/dev/zvol/pool/clients/a",
            ),
            (
                "b",
                "iqn.test:shared",
                "block_other",
                Some("pool/other@ready"),
                Some("pool/clients/b"),
                "/dev/zvol/pool/clients/b",
            ),
            (
                "master-a",
                "iqn.test:master-a",
                "block_master_a",
                None,
                None,
                "/dev/zvol/pool/shared",
            ),
            (
                "master-b",
                "iqn.test:master-b",
                "block_master_b",
                None,
                None,
                "/dev/zvol/pool/shared",
            ),
        ]
        .into_iter()
        .enumerate()
        {
            sqlx::query("INSERT INTO clients (id, name, mac, ip, master, snapshot, writeback, block_device, block_store, target_iqn, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
                .bind(id)
                .bind(format!("client-{id}"))
                .bind(format!("02:00:00:00:00:{:02x}", index + 20))
                .bind(format!("192.0.2.{}", index + 20))
                .bind(if snapshot.is_some() { "pool/master" } else { "pool/shared" })
                .bind(snapshot)
                .bind(writeback)
                .bind(device)
                .bind(backstore)
                .bind(target)
                .execute(&state.db_pool)
                .await
                .unwrap();
        }
        let clients = ClientRepository::new(state.db_pool.clone());
        let a = clients
            .find_by_id(&crate::domain::ClientId::from_string("a").unwrap())
            .await
            .unwrap()
            .unwrap();
        let master_a = clients
            .find_by_id(&crate::domain::ClientId::from_string("master-a").unwrap())
            .await
            .unwrap()
            .unwrap();

        let shared_target =
            crate::application::client_lifecycle::validate_boot_storage_owner(&state, &a).await;
        assert!(shared_target.is_err());
        crate::application::client_lifecycle::validate_boot_storage_owner(&state, &master_a)
            .await
            .expect("clients may share a configured master volume");
    }

    #[tokio::test]
    async fn cancelled_enrollment_waiter_does_not_release_worker_mutation_lock() {
        let lock = std::sync::Arc::new(tokio::sync::Mutex::new(()));
        let started = std::sync::Arc::new(tokio::sync::Notify::new());
        let release = std::sync::Arc::new(tokio::sync::Notify::new());
        let (done, finished) = tokio::sync::oneshot::channel();
        let worker_started = started.clone();
        let worker_release = release.clone();
        let waiter = spawn_locked(lock.clone(), None, async move {
            worker_started.notify_one();
            worker_release.notified().await;
            let _ = done.send(());
        });
        started.notified().await;
        drop(waiter);
        assert!(lock.try_lock().is_err());

        release.notify_one();
        finished.await.unwrap();
        tokio::task::yield_now().await;
        assert!(lock.try_lock().is_ok());
    }

    #[test]
    fn winpe_payload_paths_match_the_build_instructions() {
        let inventory =
            include_str!("../../../tools/windows-servicer/client-report/winpe-inventory.ps1");
        let startnet =
            include_str!("../../../tools/windows-servicer/client-report/winpe-startnet.cmd");
        let build = include_str!("../../../tools/windows-servicer/client-report/BUILD-WINPE.md");

        assert!(inventory.starts_with("#Requires -RunAsAdministrator\n<#"));
        assert!(inventory.contains("Join-Path $PSScriptRoot \"diskless-server.txt\""));
        assert!(startnet.contains("%SYSTEMROOT%\\System32\\winpe-inventory.ps1"));
        assert!(build.contains("Windows\\System32\\diskless-server.txt"));
        assert!(build.contains("Windows\\System32\\startnet.cmd"));
    }

    #[test]
    fn winpe_inventory_requires_only_the_documented_wmi_components() {
        let inventory =
            include_str!("../../../tools/windows-servicer/client-report/winpe-inventory.ps1");
        for unsupported in [
            "Get-NetIPAddress",
            "Get-NetAdapter",
            "Get-PnpDevice",
            "Get-ComputerInfo",
        ] {
            assert!(
                !inventory.contains(unsupported),
                "WinPE lacks {unsupported}"
            );
        }
        for class in [
            "Win32_NetworkAdapterConfiguration",
            "Win32_NetworkAdapter",
            "Win32_PnPEntity",
        ] {
            assert!(inventory.contains(class), "inventory requires WMI {class}");
        }
    }

    #[tokio::test]
    async fn inventory_submit_stores_pending_record_by_mac() {
        let state = test_state().await;
        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, enabled, created_at, updated_at) VALUES ('inventory-client', 'inventory-client', '02:00:00:00:00:09', '192.0.2.50', 'pending', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
            .execute(&state.db_pool)
            .await
            .unwrap();
        let (status, body) = post_inventory(&state, "02:00:00:00:00:09", inventory_body()).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["ok"], true);
        let row: (String, Option<String>) = sqlx::query_as(
            "SELECT boot_nic_ids, client_id FROM windows_nic_inventory WHERE mac = ?",
        )
        .bind("02:00:00:00:00:09")
        .fetch_one(&state.db_pool)
        .await
        .unwrap();
        assert!(row.0.contains("VEN_8086"));
        assert_eq!(row.1.as_deref(), Some("inventory-client"));
    }

    #[tokio::test]
    async fn inventory_submit_rejects_unregistered_macs_and_unmatched_peer_addresses() {
        let state = test_state().await;
        let (status, _) = post_inventory(&state, "02:00:00:00:00:09", inventory_body()).await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, enabled, created_at, updated_at) VALUES ('inventory-client', 'inventory-client', '02:00:00:00:00:09', '192.0.2.50', 'pending', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
            .execute(&state.db_pool)
            .await
            .unwrap();
        let (status, _) =
            post_inventory_from(&state, "02:00:00:00:00:09", inventory_body(), "192.0.2.51").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM windows_nic_inventory")
            .fetch_one(&state.db_pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn inventory_submit_rejects_invalid_mac_and_ids() {
        let state = test_state().await;
        let (status, _) = post_inventory(&state, "not-a-mac", inventory_body()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = post_inventory(
            &state,
            "02:00:00:00:00:0a",
            serde_json::json!({"boot_nic_ids": [], "secondary_ids": []}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM windows_nic_inventory")
            .fetch_one(&state.db_pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn inventory_boot_request_is_consumed_before_upload() {
        let state = test_state().await;
        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, created_at, updated_at) VALUES ('c1', 'client-c1', '02:00:00:00:00:0b', '192.0.2.50', 'pending', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
            .execute(&state.db_pool)
            .await
            .unwrap();
        sqlx::query("UPDATE clients SET inventory_boot_requested = 1 WHERE id = 'c1'")
            .execute(&state.db_pool)
            .await
            .unwrap();
        assert!(
            consume_inventory_boot_request(
                &state,
                &MacAddress::parse("02:00:00:00:00:0b").unwrap()
            )
            .await
        );
        let (status, _) = post_inventory(&state, "02:00:00:00:00:0b", inventory_body()).await;
        assert_eq!(status, StatusCode::OK);
        let flag: i64 =
            sqlx::query_scalar("SELECT inventory_boot_requested FROM clients WHERE id = 'c1'")
                .fetch_one(&state.db_pool)
                .await
                .unwrap();
        assert_eq!(flag, 0);
        let client_id: Option<String> = sqlx::query_scalar(
            "SELECT client_id FROM windows_nic_inventory WHERE mac = '02:00:00:00:00:0b'",
        )
        .fetch_one(&state.db_pool)
        .await
        .unwrap();
        assert_eq!(client_id.as_deref(), Some("c1"));
    }

    #[tokio::test]
    async fn old_inventory_upload_does_not_clear_a_rearmed_boot_request() {
        let state = test_state().await;
        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, created_at, updated_at) VALUES ('c1', 'client-c1', '02:00:00:00:00:0b', '192.0.2.50', 'pending', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
            .execute(&state.db_pool)
            .await
            .unwrap();
        sqlx::query("UPDATE clients SET inventory_boot_requested = 1 WHERE id = 'c1'")
            .execute(&state.db_pool)
            .await
            .unwrap();
        assert!(
            consume_inventory_boot_request(
                &state,
                &MacAddress::parse("02:00:00:00:00:0b").unwrap()
            )
            .await
        );
        sqlx::query("UPDATE clients SET inventory_boot_requested = 1 WHERE id = 'c1'")
            .execute(&state.db_pool)
            .await
            .unwrap();

        let (status, _) = post_inventory(&state, "02:00:00:00:00:0b", inventory_body()).await;

        assert_eq!(status, StatusCode::OK);
        let flag: i64 =
            sqlx::query_scalar("SELECT inventory_boot_requested FROM clients WHERE id = 'c1'")
                .fetch_one(&state.db_pool)
                .await
                .unwrap();
        assert_eq!(flag, 1);
    }

    #[test]
    fn enroll_address_defaults_to_the_dedicated_listener() {
        assert_eq!(
            enroll_address(None).expect("default address should be valid"),
            "0.0.0.0:4237".parse().unwrap()
        );
    }

    #[test]
    fn enroll_address_accepts_an_explicit_bind_address() {
        assert_eq!(
            enroll_address(Some("0.0.0.0:4321")).expect("configured address should be valid"),
            "0.0.0.0:4321".parse().unwrap()
        );
    }

    #[test]
    fn enroll_address_rejects_invalid_configuration() {
        let error = enroll_address(Some("not-an-address")).expect_err("invalid address must fail");
        assert!(error.to_string().contains("not-an-address"));
    }

    #[test]
    fn auto_names_are_derived_from_the_mac_address() {
        let mac = MacAddress::parse("AA:BB:CC:DD:EE:FF").expect("MAC should parse");
        assert_eq!(client_name_from_mac(&mac), "client-aabbccddeeff");
    }

    #[test]
    fn enrollment_window_is_closed_without_a_deadline() {
        assert!(!enrollment_window_open(None, 1_700_000_000));
    }

    #[test]
    fn enrollment_window_is_open_before_the_deadline() {
        assert!(enrollment_window_open(Some(1_700_000_600), 1_700_000_000));
    }

    #[test]
    fn enrollment_window_expires_at_the_deadline() {
        assert!(!enrollment_window_open(Some(1_700_000_600), 1_700_000_600));
        assert!(!enrollment_window_open(Some(1_700_000_600), 1_700_000_601));
    }
}
