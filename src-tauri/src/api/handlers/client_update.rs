use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use super::clients::ErrorResponse;
use crate::{
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

/// Compatibility update entrypoint.
///
/// Low-risk persistence-only updates are handled by the typed application
/// service. Storage/game/auth/action updates still delegate to the established
/// legacy orchestrator until their dedicated application services are ready.
pub async fn update_client(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateClientRequest>,
) -> Result<Json<Client>, (StatusCode, Json<ErrorResponse>)> {
    if !can_use_typed_update(&request) {
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

    // Preserve the existing menu refresh behavior for persistence-only saves.
    if existing.enabled {
        if let Some(target_iqn) = existing
            .target_iqn
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
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

            let next = settings.dhcp.next_server_ip.trim();
            let server_ip = if next.is_empty() {
                settings.server.ip_address.trim()
            } else {
                next
            };
            let reservation = crate::infrastructure::dhcp::BootReservation {
                client_name: existing.name.clone(),
                mac: existing.mac.to_string(),
                ip: existing.ip.to_string(),
                target_iqn: target_iqn.to_string(),
                server_ip: server_ip.to_string(),
                chap,
            };
            if let Err(error) =
                crate::infrastructure::dhcp::publish_client_ipxe(&reservation).await
            {
                tracing::warn!(client_id = %id, %error, "failed to regenerate boot menu while updating client");
            }
        }
    }

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
    use super::{can_use_typed_update, UpdateClientRequest};

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
    }

    #[test]
    fn storage_and_action_updates_stay_on_legacy_path() {
        let mut request = empty_request();
        request.master = Some("tank/windows".into());
        assert!(!can_use_typed_update(&request));
        request.master = None;
        request.action = Some("reset".into());
        assert!(!can_use_typed_update(&request));
    }
}
