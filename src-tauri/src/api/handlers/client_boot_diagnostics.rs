//! Read-only boot/iBFT diagnostics (Phase 6).
//!
//! This endpoint reports what the server already knows and what the current
//! boot script will emit for one client. It never changes the gateway, the boot
//! script, storage or session state: the Phase 6 gateway experiment is opt-in
//! lab work and deliberately has no production toggle.

use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use serde_json::Value;

use crate::{
    api::error::api_error, application::boot_handoff_service, core::image::OsType,
    domain::ClientId, infrastructure::pxe::client_initiator_iqn,
    persistence::repositories::hardware_identity::HardwareIdentityRepository, state::AppState,
    types::Claims,
};

type ApiResult<T> = Result<T, (StatusCode, Json<Value>)>;

#[derive(Debug, Serialize)]
pub struct HandoffView {
    pub attempt_id: String,
    pub phase: &'static str,
    pub age_seconds: i64,
    pub expires_in_seconds: i64,
    pub active: bool,
}

/// Boot inputs for one client, as the server will use them on the next PXE
/// request. `ibft_gateway` is the literal iPXE setting in the script; `dhcp`
/// means the script does not override the gateway.
#[derive(Debug, Serialize)]
pub struct BootDiagnosticsView {
    pub read_only: bool,
    pub client_id: String,
    pub name: String,
    pub enabled: bool,
    pub firmware: String,
    pub assigned_image: String,
    pub os_type: Option<String>,
    pub target_iqn: Option<String>,
    pub initiator_iqn: String,
    pub keep_san: bool,
    pub chap_enabled: bool,
    pub ibft_gateway: &'static str,
    pub boot_nic_ids: Vec<String>,
    pub hardware_architecture: Option<String>,
    pub hardware_firmware: Option<String>,
    /// Which recorded report supplied the NIC/architecture rows.
    pub hardware_source: Option<&'static str>,
    pub hardware_observed_at: Option<String>,
    pub handoff: Option<HandoffView>,
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
    tracing::error!(%source, "boot diagnostics request failed");
    api_error(StatusCode::INTERNAL_SERVER_ERROR, message, Value::Null)
}

#[derive(Debug)]
struct RecordedInventory {
    boot_nic_ids: Vec<String>,
    architecture: Option<String>,
    firmware: Option<String>,
    observed_at: Option<String>,
    source: &'static str,
}

fn normalize_mac(value: &str) -> String {
    value
        .chars()
        .filter(|character| !matches!(character, ':' | '-'))
        .flat_map(char::to_lowercase)
        .collect()
}

fn parse_ids(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

/// Recorded NIC identity for the diagnostics view, in the same precedence the
/// Windows servicing path uses: an explicit identity observation first, then a
/// verified Windows client report, then the MAC-keyed preboot inventory.
///
/// A client that has booted the installer has inventory here even before an
/// identity capture exists, so the panel does not report "none" for a machine
/// the driver path can already describe.
async fn load_inventory(
    pool: &sqlx::SqlitePool,
    client: &crate::domain::Client,
) -> anyhow::Result<Option<RecordedInventory>> {
    let profile = HardwareIdentityRepository::new(pool.clone())
        .profile(client.id.as_str())
        .await?;
    if let Some(profile) = profile {
        return Ok(Some(RecordedInventory {
            boot_nic_ids: parse_ids(&profile.boot_nic_ids),
            architecture: Some(profile.architecture),
            firmware: Some(profile.firmware),
            observed_at: Some(profile.observed_at),
            source: "identity profile",
        }));
    }

    let report: Option<(String, String, String)> = sqlx::query_as(
        "SELECT boot_nic_ids, architecture, reported_at FROM windows_driver_reports WHERE client_id = ?",
    )
    .bind(client.id.as_str())
    .fetch_optional(pool)
    .await?;
    if let Some((ids, architecture, reported_at)) = report {
        let boot_nic_ids = parse_ids(&ids);
        if !boot_nic_ids.is_empty() {
            return Ok(Some(RecordedInventory {
                boot_nic_ids,
                architecture: Some(architecture),
                firmware: None,
                observed_at: Some(reported_at),
                source: "windows client report",
            }));
        }
    }

    let inventory: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT boot_nic_ids, architecture, firmware, observed_at FROM windows_nic_inventory WHERE lower(replace(replace(mac, ':', ''), '-', '')) = ? LIMIT 1",
    )
    .bind(normalize_mac(&client.mac.to_string()))
    .fetch_optional(pool)
    .await?;
    Ok(
        inventory.and_then(|(ids, architecture, firmware, observed_at)| {
            let boot_nic_ids = parse_ids(&ids);
            if boot_nic_ids.is_empty() {
                return None;
            }
            Some(RecordedInventory {
                boot_nic_ids,
                architecture: Some(architecture),
                firmware: Some(firmware),
                observed_at: Some(observed_at),
                source: "WinPE inventory",
            })
        }),
    )
}

/// GET /api/clients/{id}/boot-diagnostics
///
/// Read-only. Nothing here mutates settings, storage or an active session.
pub async fn get_boot_diagnostics(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> ApiResult<Json<BootDiagnosticsView>> {
    require_admin(&claims)?;
    let client_id = ClientId::from_string(id.clone())
        .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string(), Value::Null))?;
    let client = state
        .application
        .clients
        .get(&client_id)
        .await
        .map_err(|error| internal("Failed to load the client", error))?
        .ok_or_else(|| {
            api_error(
                StatusCode::NOT_FOUND,
                format!("Client not found: {id}"),
                Value::Null,
            )
        })?;

    // An image lookup failure means the assignment cannot be classified, not
    // that diagnostics failed: report what is known instead of a 500.
    let image = state.application.images.get(&client.master).await.ok();
    let os_type = image
        .as_ref()
        .map(|image| image.os_type.as_str().to_string());
    let windows = image
        .as_ref()
        .is_some_and(|image| image.os_type == OsType::Windows);

    let inventory = load_inventory(&state.db_pool, &client)
        .await
        .map_err(|error| internal("Failed to load the hardware inventory", error))?;

    let now = chrono::Utc::now().timestamp();
    let handoff = boot_handoff_service::load(&state.db_pool, client.id.as_str())
        .await
        .map_err(|error| internal("Failed to load the boot handoff", error))?
        .map(|handoff| {
            let active = handoff.is_live_at(now);
            HandoffView {
                attempt_id: handoff.attempt_id,
                phase: handoff.phase.as_str(),
                age_seconds: now.saturating_sub(handoff.started_at),
                expires_in_seconds: handoff.expires_at.saturating_sub(now).max(0),
                active,
            }
        });

    Ok(Json(BootDiagnosticsView {
        read_only: true,
        client_id: client.id.to_string(),
        name: client.name.clone(),
        enabled: client.enabled,
        firmware: client.pxe_mode.as_str().to_string(),
        assigned_image: client.master.clone(),
        os_type,
        target_iqn: client.target_iqn.clone(),
        initiator_iqn: client_initiator_iqn(&client.name),
        keep_san: true,
        chap_enabled: client.chap_enabled,
        // Mirrors the Windows SAN-boot script; Linux kernels receive DHCP.
        ibft_gateway: if windows { "0.0.0.0" } else { "dhcp" },
        boot_nic_ids: inventory
            .as_ref()
            .map(|inventory| inventory.boot_nic_ids.clone())
            .unwrap_or_default(),
        hardware_architecture: inventory
            .as_ref()
            .and_then(|inventory| inventory.architecture.clone()),
        hardware_firmware: inventory
            .as_ref()
            .and_then(|inventory| inventory.firmware.clone()),
        hardware_source: inventory.as_ref().map(|inventory| inventory.source),
        hardware_observed_at: inventory
            .as_ref()
            .and_then(|inventory| inventory.observed_at.clone()),
        handoff,
    }))
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
        Router,
    };
    use tower::ServiceExt;

    async fn fetch(app: &Router, token: &str) -> serde_json::Value {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/clients/client/boot-diagnostics")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        serde_json::from_slice(&to_bytes(response.into_body(), 16384).await.unwrap()).unwrap()
    }

    #[tokio::test]
    async fn boot_diagnostics_are_admin_only_and_never_mutate_boot_state() {
        let (state, app, admin, user) = crate::api::security_tests::setup().await;
        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, snapshot, writeback, block_device, block_store, target_iqn, keep_writeback, use_game_disk, enabled, created_at, updated_at) VALUES ('client', 'PC001', '02:00:00:00:00:01', '192.0.2.10', 'pool/master', '', '', '', '', 'iqn.test:pc001', 0, 0, 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
            .execute(&state.db_pool)
            .await
            .unwrap();

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/clients/client/boot-diagnostics")
                    .header("authorization", format!("Bearer {user}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let body = fetch(&app, &admin).await;
        assert_eq!(body["read_only"], true);
        assert_eq!(body["client_id"], "client");
        assert_eq!(body["firmware"], "uefi");
        assert_eq!(body["initiator_iqn"], "iqn.2026-01.client:client.pc001");
        assert_eq!(body["keep_san"], true);
        // The assigned image is not in the catalog here, so the gateway stays
        // DHCP-driven instead of guessing a Windows SAN path.
        assert_eq!(body["ibft_gateway"], "dhcp");
        assert!(body["handoff"].is_null());
        assert_eq!(body["boot_nic_ids"].as_array().map(Vec::len), Some(0));
        assert!(body["hardware_source"].is_null());

        // WinPE inventory is keyed by MAC, so a client that booted the
        // installer is described before any identity capture exists.
        sqlx::query("INSERT INTO windows_nic_inventory (mac, boot_nic_ids, architecture, firmware, observed_at) VALUES ('02:00:00:00:00:01', ?, 'AMD64', 'uefi', '2026-10-17T04:16:49Z')")
            .bind(serde_json::json!(["PCI\\VEN_10EC&DEV_8125"]).to_string())
            .execute(&state.db_pool)
            .await
            .unwrap();
        let body = fetch(&app, &admin).await;
        assert_eq!(body["hardware_source"], "WinPE inventory");
        assert_eq!(body["boot_nic_ids"][0], "PCI\\VEN_10EC&DEV_8125");
        assert_eq!(body["hardware_architecture"], "AMD64");
        assert_eq!(body["hardware_firmware"], "uefi");

        // An explicit identity observation outranks the preboot inventory.
        sqlx::query("INSERT INTO hardware_profiles (client_id, profile_key, boot_nic_ids, architecture, firmware, observed_at) VALUES ('client', 'k', ?, 'x86_64', 'bios', '2026-10-18T00:00:00Z')")
            .bind(serde_json::json!(["PCI\\VEN_1234&DEV_0001"]).to_string())
            .execute(&state.db_pool)
            .await
            .unwrap();
        let body = fetch(&app, &admin).await;
        assert_eq!(body["hardware_source"], "identity profile");
        assert_eq!(body["boot_nic_ids"][0], "PCI\\VEN_1234&DEV_0001");
        assert_eq!(body["hardware_firmware"], "bios");
    }
}
