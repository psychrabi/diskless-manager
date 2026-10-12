use axum::{
    extract::{ConnectInfo, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use log::info;
use std::{net::SocketAddr, path::PathBuf};
use tracing::{error, warn};

use crate::{
    domain::{CreateClient, MacAddress, PxeMode},
    infrastructure::dhcp::BootReservation,
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

/// The enrollment listener serves a single purpose: let an unprovisioned PXE
/// client register itself so it appears in the clients list for an
/// administrator to assign an image to.
pub fn enroll_router(state: AppState) -> Router {
    Router::new()
        .route("/enroll/{mac}", get(enroll_client))
        .route("/inventory/{mac}", axum::routing::post(submit_inventory))
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
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        script,
    )
        .into_response()
}

/// Build the DHCP/server context needed to regenerate a per-client menu.
fn boot_server_ip(settings: &crate::core::config::Settings) -> String {
    let next = settings.dhcp.next_server_ip.trim();
    if next.is_empty() {
        settings.server.ip_address.trim().to_string()
    } else {
        next.to_string()
    }
}

/// Ensure the per-client iPXE menu for a provisioned client exists on disk,
/// regenerating it from the reserved iSCSI target if missing.
async fn ensure_client_menu(
    client: &crate::domain::Client,
    settings: &crate::core::config::Settings,
) {
    let root = PathBuf::from(&settings.http.root_dir);
    let relative = crate::infrastructure::pxe::client_mac_script_path(client.mac.as_str());

    if !crate::infrastructure::pxe::is_managed_script_path(&root, &relative) {
        warn!(
            client = %client.name,
            "refusing to regenerate unsafe iPXE menu path: {relative}"
        );
        return;
    }

    let path = root.join(&relative);
    if path.exists() {
        return;
    }

    let Some(target_iqn) = client
        .target_iqn
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return;
    };

    let chap = match (
        client.chap_enabled,
        client.chap_user.as_deref(),
        client.chap_secret.as_deref(),
    ) {
        (true, Some(username), Some(password))
            if !username.trim().is_empty() && !password.is_empty() =>
        {
            Some(crate::infrastructure::iscsi::ChapCredentials {
                username: username.to_string(),
                password: password.to_string(),
            })
        }
        _ => None,
    };
    let reservation = BootReservation {
        client_name: client.name.clone(),
        mac: client.mac.to_string(),
        ip: client.ip.to_string(),
        target_iqn: target_iqn.to_string(),
        server_ip: boot_server_ip(settings),
        chap,
    };
    if let Err(error) =
        crate::infrastructure::dhcp::publish_client_ipxe(settings, &reservation).await
    {
        warn!(
            client = %client.name,
            "failed to regenerate missing iPXE menu during enrollment: {error:#}"
        );
    }
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
    let request = CreateClient {
        name: name.clone(),
        mac: mac.to_string(),
        ip: source_ip.to_string(),
        // Placeholder until an administrator assigns a real image during
        // provisioning. The presence of a `target_iqn` marks the client as
        // projectable/bootable.
        master: "pending".to_string(),
        snapshot: None,
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
            } else {
                let provisioning = client
                    .target_iqn
                    .as_deref()
                    .is_some_and(|iqn| !iqn.trim().is_empty());
                if provisioning {
                    let settings = state.settings.read().await;
                    ensure_client_menu(&client, &settings).await;
                    crate::infrastructure::pxe::render_enrollment_redirect()
                } else if consume_inventory_boot_request(&state, &parsed).await {
                    tracing::info!(
                        client = %client.name,
                        mac = %parsed.as_str(),
                        "directing client to WinPE for NIC inventory"
                    );
                    crate::infrastructure::pxe::render_inventory_boot()
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
                    Ok(_) => crate::infrastructure::pxe::render_enrollment_pending(),
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

    ipxe_response(script)
}

/// Inventory submitted from WinPE, keyed by the observed MAC.
///
/// The record stays pending until the client is assigned: the MAC is only the
/// lookup key and matching always uses hardware IDs. Malformed payloads are
/// rejected without touching stored inventory.
async fn submit_inventory(
    State(state): State<AppState>,
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
    let observed_at = chrono::Utc::now().to_rfc3339();
    let stored = sqlx::query(
        r#"INSERT INTO windows_nic_inventory
        (mac, client_id, boot_nic_ids, secondary_ids, architecture, firmware, observed_at)
        VALUES (?, (SELECT id FROM clients WHERE mac = ?), ?, ?, ?, ?, ?)
        ON CONFLICT(mac) DO UPDATE SET
            client_id = excluded.client_id,
            boot_nic_ids = excluded.boot_nic_ids,
            secondary_ids = excluded.secondary_ids,
            architecture = excluded.architecture,
            firmware = excluded.firmware,
            observed_at = excluded.observed_at"#,
    )
    .bind(parsed.as_str())
    .bind(parsed.as_str())
    .bind(serde_json::to_string(&validated.boot_nic_ids).unwrap_or_default())
    .bind(serde_json::to_string(&validated.secondary_ids).unwrap_or_default())
    .bind(&validated.architecture)
    .bind(validated.firmware.to_string())
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
        boot_nics = validated.boot_nic_ids.len(),
        "stored preboot NIC inventory"
    );
    (
        StatusCode::OK,
        Json(serde_json::json!({"ok": true, "mac": parsed.as_str()})),
    )
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
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    async fn test_state() -> AppState {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        AppState {
            config: crate::config::RuntimeConfig::default(),
            client_mutations: std::sync::Arc::new(tokio::sync::Mutex::new(())),
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
        let app = enroll_router(state.clone());
        let peer: ConnectInfo<SocketAddr> =
            ConnectInfo("192.0.2.50:68".parse().expect("test peer must parse"));
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
        assert!(script.contains("clients/${client-mac}.ipxe"));
        let (_, next_boot) = get_enroll(&state, "02:00:00:00:00:0c").await;
        assert!(next_boot.contains("No master image has been assigned yet"));
        let inventory_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM windows_nic_inventory")
            .fetch_one(&state.db_pool)
            .await
            .unwrap();
        assert_eq!(inventory_count, 0);
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

    #[tokio::test]
    async fn inventory_submit_stores_pending_record_by_mac() {
        let state = test_state().await;
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
        // Unassigned: the MAC is only the lookup key until provisioning pins it.
        assert!(row.1.is_none());
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
        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, created_at, updated_at) VALUES ('c1', 'client-c1', '02:00:00:00:00:0b', '192.0.2.11', 'pending', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
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
        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, created_at, updated_at) VALUES ('c1', 'client-c1', '02:00:00:00:00:0b', '192.0.2.11', 'pending', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
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
