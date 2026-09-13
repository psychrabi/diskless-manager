use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use super::clients::ErrorResponse;
use crate::{
    application::storage_service::StorageService,
    core::client::{Client, UpdateClientRequest},
    domain::{BootMode, ClientStatus, PxeMode, UpdateClient},
    state::AppState,
};

/// A deliberately narrow candidate check. Whether this is truly a game-only
/// update also depends on the currently persisted snapshot and game settings,
/// so the final decision is made after loading the client under the mutation
/// lock.
fn is_game_only_candidate(request: &UpdateClientRequest) -> bool {
    (request.use_game_disk.is_some() || request.game_disks.is_some())
        && request.action.is_none()
        && request.make_super.is_none()
        && request.name.is_none()
        && request.mac.is_none()
        && request.ip.is_none()
        && request.master.is_none()
        && request.keep_writeback.is_none()
        && request.chap_enabled.is_none()
        && request.enabled.is_none()
        && request.block_store.is_none()
        && request.block_device.is_none()
        && request.target_iqn.is_none()
        && request.writeback.is_none()
}

fn is_actual_game_only_update(
    request: &UpdateClientRequest,
    existing: &crate::domain::Client,
) -> bool {
    // Preserve the legacy `storage_configuration_changed` semantics exactly:
    // `request.snapshot != existing.snapshot` is a boot-storage change. In
    // particular, omitting snapshot for a snapshot-backed client is not a
    // game-only update.
    if request.snapshot != existing.snapshot {
        return false;
    }

    request
        .use_game_disk
        .is_some_and(|enabled| enabled != existing.use_game_disk)
        || request
            .game_disks
            .as_ref()
            .is_some_and(|selection| selection.as_slice() != existing.game_disks.as_slice())
}

fn domain_to_legacy(client: crate::domain::Client) -> Client {
    Client {
        id: client.id.to_string(),
        name: client.name,
        mac: client.mac.to_string(),
        ip: client.ip.to_string(),
        master: client.master,
        enabled: client.enabled,
        created_at: client.created_at,
        updated_at: client.updated_at,
        snapshot: client.snapshot,
        block_store: client.block_store,
        target_iqn: client.target_iqn,
        writeback: client.writeback,
        last_modified: client
            .last_modified
            .map(|value| value.format("%Y-%m-%d %H:%M:%S").to_string()),
        block_device: client.block_device,
        status: Some(
            match client.status {
                ClientStatus::Provisioning => "Provisioning",
                ClientStatus::Ready => "Ready",
                ClientStatus::Online => "Online",
                ClientStatus::Offline => "Offline",
                ClientStatus::Error => "Error",
                ClientStatus::Disabled => "Disabled",
            }
            .to_string(),
        ),
        mode: Some(
            match client.mode {
                BootMode::Normal => "normal",
                BootMode::Super => "super",
            }
            .to_string(),
        ),
        pxe_mode: Some(
            match client.pxe_mode {
                PxeMode::Uefi => "uefi",
                PxeMode::Bios => "bios",
            }
            .to_string(),
        ),
        keep_writeback: Some(client.keep_writeback),
        use_game_disk: Some(client.use_game_disk),
        chap_user: client.chap_user,
        chap_secret: client.chap_secret,
        chap_enabled: Some(client.chap_enabled),
    }
}

fn resolve_effective_game_selection(
    use_game_disk: bool,
    stored_or_requested: &[String],
) -> anyhow::Result<Vec<String>> {
    let discovered = StorageService::discover_game_masters()?
        .into_iter()
        .map(|master| master.dataset)
        .collect::<Vec<_>>();

    Ok(StorageService::resolve_game_selection(
        use_game_disk,
        stored_or_requested,
        &discovered,
    ))
}

async fn publish_boot_menu(
    state: &AppState,
    settings: &crate::core::config::Settings,
    client: &crate::domain::Client,
    chap: Option<&crate::infrastructure::iscsi::ChapCredentials>,
) {
    if !client.enabled {
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

    let next_server = settings.dhcp.next_server_ip.trim();
    let server_ip = if next_server.is_empty() {
        settings.server.ip_address.trim()
    } else {
        next_server
    };

    let reservation = crate::infrastructure::dhcp::BootReservation {
        client_name: client.name.clone(),
        mac: client.mac.to_string(),
        ip: client.ip.to_string(),
        target_iqn: target_iqn.to_string(),
        server_ip: server_ip.to_string(),
        chap: chap.cloned(),
    };

    if let Err(error) = crate::infrastructure::dhcp::publish_client_ipxe(&reservation).await {
        tracing::warn!(client_id = %client.id, %error, "failed to regenerate boot menu during game-disk update");
    }
}

async fn refresh_dhcp(state: &AppState, settings: &crate::core::config::Settings) {
    if !settings.dhcp.enabled {
        return;
    }

    let service = crate::services::DhcpService::new(settings.clone(), state.db_pool.clone());
    if let Err(error) = service.generate_client_configs().await {
        tracing::warn!(%error, "failed to regenerate DHCP client configuration after game-disk update");
        return;
    }
    if let Err(error) = service.validate_config().await {
        tracing::warn!(%error, "DHCP validation failed after game-disk update; service was not reloaded");
        return;
    }
    if let Err(error) = service.reload().await {
        tracing::warn!(%error, "failed to reload DHCP service after game-disk update");
    }
}

/// Game-only compatibility wrapper for PUT /api/clients/{id}.
///
/// Requests outside this narrow category delegate to the already-routed
/// `client_update` adapter, which in turn handles simple typed updates or
/// delegates complex storage/action requests to the legacy orchestrator.
pub async fn update_client(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateClientRequest>,
) -> Result<Json<Client>, (StatusCode, Json<ErrorResponse>)> {
    if !is_game_only_candidate(&request) {
        return super::client_update::update_client(State(state), Path(id), Json(request)).await;
    }

    let client_guard = state.client_mutations.lock().await;

    let existing = state
        .application
        .clients
        .get_by_string(&id)
        .await
        .map_err(|_| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    status: StatusCode::NOT_FOUND.as_u16(),
                    error: "Client not found".to_string(),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    status: StatusCode::NOT_FOUND.as_u16(),
                    error: "Client not found".to_string(),
                }),
            )
        })?;

    if !is_actual_game_only_update(&request, &existing) {
        drop(client_guard);
        return super::client_update::update_client(State(state), Path(id), Json(request)).await;
    }

    let recovering: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM client_offline_resets WHERE client_id = ? AND operation IS NOT NULL",
    )
    .bind(&id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: 500,
                error: error.to_string(),
            }),
        )
    })?;
    if recovering != 0 {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                status: 409,
                error: "Client storage recovery is pending; retry after recovery completes".into(),
            }),
        ));
    }

    let settings = state.settings.read().await.clone();
    let effective_flag = request.use_game_disk.unwrap_or(existing.use_game_disk);
    let effective_stored = request
        .game_disks
        .clone()
        .unwrap_or_else(|| existing.game_disks.clone());
    let resolved = resolve_effective_game_selection(effective_flag, &effective_stored).map_err(
        |error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    status: 500,
                    error: format!("Failed to resolve game selection: {error}"),
                }),
            )
        },
    )?;

    // Match the legacy game-only path: when CHAP is enabled, credentials are
    // ensured before game LUN synchronization. A missing persisted IQN is
    // allowed because the target IQN is synthesized below.
    let chap = if existing.chap_enabled {
        crate::core::reconciliation::ensure_chap_credentials(
            &state.db_pool,
            &id,
            &existing.name,
            true,
        )
        .await
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    status: 500,
                    error: format!("Failed to ensure CHAP credentials: {error}"),
                }),
            )
        })?
    } else {
        None
    };

    publish_boot_menu(&state, &settings, &existing, chap.as_ref()).await;

    let target_iqn = existing.target_iqn.clone().unwrap_or_else(|| {
        format!(
            "{}:client.{}",
            settings.iscsi.target_prefix,
            existing.name.trim().to_lowercase()
        )
    });

    state
        .application
        .storage
        .sync_game_storage(&id, &target_iqn, &resolved, chap.as_ref())
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    status: 500,
                    error: format!("Failed to synchronize game storage: {error}"),
                }),
            )
        })?;

    // Preserve the requested/stored selection independently from the resolved
    // discovery intersection used for the live export.
    state
        .application
        .clients
        .set_game_selection(&id, &effective_stored)
        .await
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    status: 500,
                    error: error.to_string(),
                }),
            )
        })?;

    let client = state
        .application
        .clients
        .update_by_string(
            &id,
            UpdateClient {
                use_game_disk: request.use_game_disk,
                ..UpdateClient::default()
            },
        )
        .await
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    status: 500,
                    error: error.to_string(),
                }),
            )
        })?;

    if let Err(error) = state.refresh_client_ips().await {
        tracing::warn!(%error, "failed to refresh client IP cache after game-disk update");
    }
    refresh_dhcp(&state, &settings).await;

    Ok(Json(domain_to_legacy(client)))
}

#[cfg(test)]
mod tests {
    use super::{is_actual_game_only_update, is_game_only_candidate};
    use crate::{
        core::client::UpdateClientRequest,
        domain::{Client, CreateClient, PxeMode},
    };

    fn request() -> UpdateClientRequest {
        UpdateClientRequest {
            name: None,
            mac: None,
            ip: None,
            master: None,
            snapshot: None,
            keep_writeback: None,
            use_game_disk: Some(true),
            game_disks: Some(vec!["tank/game/a".into()]),
            chap_enabled: None,
            enabled: None,
            block_store: None,
            block_device: None,
            target_iqn: None,
            writeback: None,
            action: None,
            make_super: None,
        }
    }

    fn client(snapshot: Option<&str>) -> Client {
        let mut client = Client::create(CreateClient {
            name: "PC001".into(),
            mac: "00:11:22:33:44:55".into(),
            ip: "192.168.1.10".into(),
            master: "tank/master".into(),
            snapshot: snapshot.map(str::to_string),
            block_store: None,
            block_device: None,
            target_iqn: None,
            pxe_mode: PxeMode::Uefi,
            keep_writeback: true,
            use_game_disk: false,
            game_disks: Vec::new(),
            chap_enabled: false,
        })
        .unwrap();
        client.game_disks = Vec::new();
        client
    }

    #[test]
    fn plain_game_request_is_a_candidate() {
        assert!(is_game_only_candidate(&request()));
    }

    #[test]
    fn omitted_snapshot_on_snapshot_backed_client_stays_legacy() {
        let existing = client(Some("tank/master@ready"));
        assert!(!is_actual_game_only_update(&request(), &existing));
    }

    #[test]
    fn matching_snapshot_and_changed_game_settings_are_game_only() {
        let existing = client(Some("tank/master@ready"));
        let mut request = request();
        request.snapshot = existing.snapshot.clone();
        assert!(is_actual_game_only_update(&request, &existing));
    }

    #[test]
    fn unchanged_game_values_delegate_instead_of_forcing_game_sync() {
        let mut existing = client(None);
        existing.use_game_disk = true;
        existing.game_disks = vec!["tank/game/a".into()];
        assert!(!is_actual_game_only_update(&request(), &existing));
    }
}
