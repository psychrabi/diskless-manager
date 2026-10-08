use axum::{
    extract::{ConnectInfo, Path, State},
    http::header,
    response::{IntoResponse, Response},
    routing::get,
    Router,
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
            Some(
                crate::infrastructure::iscsi::ChapCredentials {
                    username: username.to_string(),
                    password: password.to_string(),
                },
            )
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
    if let Err(error) = crate::infrastructure::dhcp::publish_client_ipxe(&reservation).await {
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

#[cfg(test)]
mod tests {
    use super::*;

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
