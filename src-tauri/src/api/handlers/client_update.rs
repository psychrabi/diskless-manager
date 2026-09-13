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
    validation::{validate_ip_address, validate_mac_address},
};

fn can_use_typed_update(request: &UpdateClientRequest) -> bool {
    request.action.is_none()
        && request.make_super.is_none()
        && request.name.is_none()
        && request.master.is_none()
        && request.snapshot.is_none()
        && request.use_game_disk.is_none()
        && request.game_disks.is_none()
        && request.chap_enabled.is_none()
        && request.enabled.is_none()
        && request.block_store.is_none()
        && request.block_device.is_none()
        && request.target_iqn.is_none()
        && request.writeback.is_none()
}

fn is_game_only_update(request: &UpdateClientRequest) -> bool {
    (request.use_game_disk.is_some() || request.game_disks.is_some())
        && request.action.is_none()
        && request.make_super.is_none()
        && request.name.is_none()
        && request.mac.is_none()
        && request.ip.is_none()
        && request.master.is_none()
        && request.snapshot.is_none()
        && request.keep_writeback.is_none()
        && request.chap_enabled.is_none()
        && request.enabled.is_none()
        && request.block_store.is_none()
        && request.block_device.is_none()
        && request.target_iqn.is_none()
        && request.writeback.is_none()
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

async fn refresh_dhcp(state: &AppState, settings: &crate::core::config::Settings) {
    if !settings.dhcp.enabled {
        return;
    }

    let service = crate::services::DhcpService::new(settings.clone(), state.db_pool.clone());
    if let Err(error) = service.generate_client_configs().await {
        tracing::warn!(%error, "failed to regenerate DHCP client configuration after updating client");
        return;
    }
    if let Err(error) = service.validate_config().await {
        tracing::warn!(%error, "DHCP validation failed after updating client; service was not reloaded");
        return;
    }
    if let Err(error) = service.reload().await {
        tracing::warn!(%error, "failed to reload DHCP service after updating client");
    }
}

async fn publish_existing_boot_menu(
    state: &AppState,
    settings: &crate::core::config::Settings,
    client: &crate::domain::Client,
    chap: Option<crate::infrastructure::iscsi::ChapCredentials>,
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

    let next = settings.dhcp.next_server_ip.trim();
    let server_ip = if next.is_empty() {
        settings.server.ip_address.trim()
    } else {
        next
    };
    let reservation = crate::infrastructure::dhcp::BootReservation {
        client_name: client.name.clone(),
        mac: client.mac.to_string(),
        ip: client.ip.to_string(),
        target_iqn: target_iqn.to_string(),
        server_ip: server_ip.to_string(),
        chap,
    };

    if let Err(error) = crate::infrastructure::dhcp::publish_client_ipxe(&reservation).await {
        tracing::warn!(client_id = %client.id, %error, "failed to regenerate boot menu while updating client");
    }
}

async fn update_game_only(
    state: &AppState,
    id: &str,
    request: UpdateClientRequest,
    existing: crate::domain::Client,
    settings: &crate::core::config::Settings,
) -> Result<Json<Client>, (StatusCode, Json<ErrorResponse>)> {
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

    let chap = if existing.chap_enabled {
        crate::core::reconciliation::ensure_chap_credentials(
            &state.db_pool,
            id,
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

    publish_existing_boot_menu(state, settings, &existing, chap.clone()).await;

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
        .sync_game_storage(id, &target_iqn, &resolved, chap.as_ref())
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    status: 500,
                    error: format!("Failed to synchronize game storage: {error}"),
                }),
            )
        })?;

    state
        .application
        .clients
        .set_game_selection(id, &effective_stored)
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
            id,
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
        tracing::warn!(%error, "failed to refresh client IP cache after game update");
    }
    refresh_dhcp(state, settings).await;

    Ok(Json(domain_to_legacy(client)))
}

/// Compatibility update entrypoint.
///
/// Low-risk persistence-only and game-only updates are handled by typed
/// application services. Storage/auth/action updates still delegate to the
/// established legacy orchestrator until their dedicated services are ready.
pub async fn update_client(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateClientRequest>,
) -> Result<Json<Client>, (StatusCode, Json<ErrorResponse>)> {
    let game_only = is_game_only_update(&request);
    if !game_only && !can_use_typed_update(&request) {
        return super::clients::update_client(State(state), Path(id), Json(request)).await;
    }

    let _client_guard = state.client_mutations.lock().await;

    if let Some(ip) = &request.ip {
        if validate_ip_address(ip).is_err() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    status: StatusCode::BAD_REQUEST.as_u16(),
                    error: "Invalid IPv4 address".to_string(),
                }),
            ));
        }
    }
    if let Some(mac) = &request.mac {
        if validate_mac_address(mac).is_err() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    status: StatusCode::BAD_REQUEST.as_u16(),
                    error: "Invalid MAC address".to_string(),
                }),
            ));
        }
    }

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

    if game_only {
        return update_game_only(&state, &id, request, existing, &settings).await;
    }

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
    publish_existing_boot_menu(&state, &settings, &existing, chap).await;

    let update = UpdateClient {
        mac: request.mac,
        ip: request.ip,
        keep_writeback: request.keep_writeback,
        ..UpdateClient::default()
    };

    let client = state
        .application
        .clients
        .update_by_string(&id, update)
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

    sqlx::query(
        "DELETE FROM client_offline_resets WHERE client_id = ? AND operation IS NULL AND ? <> ?",
    )
    .bind(&id)
    .bind(client.keep_writeback)
    .bind(existing.keep_writeback)
    .execute(&state.db_pool)
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
        tracing::warn!(%error, "failed to refresh client IP cache after update");
    }
    refresh_dhcp(&state, &settings).await;

    Ok(Json(domain_to_legacy(client)))
}

#[cfg(test)]
mod tests {
    use super::{can_use_typed_update, is_game_only_update, UpdateClientRequest};

    fn empty_request() -> UpdateClientRequest {
        UpdateClientRequest {
            name: None,
            mac: None,
            ip: None,
            master: None,
            snapshot: None,
            keep_writeback: None,
            use_game_disk: None,
            game_disks: None,
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

    #[test]
    fn simple_identity_and_keep_writeback_updates_use_typed_path() {
        let mut request = empty_request();
        request.ip = Some("192.168.1.42".into());
        request.keep_writeback = Some(false);
        assert!(can_use_typed_update(&request));
        assert!(!is_game_only_update(&request));
    }

    #[test]
    fn game_only_updates_use_game_path() {
        let mut request = empty_request();
        request.use_game_disk = Some(true);
        request.game_disks = Some(vec!["tank/games/a".into()]);
        assert!(is_game_only_update(&request));
        assert!(!can_use_typed_update(&request));
    }

    #[test]
    fn mixed_game_and_identity_updates_stay_on_legacy_path() {
        let mut request = empty_request();
        request.use_game_disk = Some(true);
        request.ip = Some("192.168.1.42".into());
        assert!(!is_game_only_update(&request));
        assert!(!can_use_typed_update(&request));
    }

    #[test]
    fn storage_and_action_updates_stay_on_legacy_path() {
        let mut request = empty_request();
        request.master = Some("tank/windows".into());
        assert!(!can_use_typed_update(&request));
        assert!(!is_game_only_update(&request));
        request.master = None;
        request.action = Some("reset".into());
        assert!(!can_use_typed_update(&request));
        assert!(!is_game_only_update(&request));
    }
}
