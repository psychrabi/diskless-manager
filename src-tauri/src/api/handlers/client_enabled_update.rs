use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use super::clients::ErrorResponse;
use crate::{
    core::client::{Client, UpdateClientRequest},
    domain::{BootMode, ClientStatus, PxeMode},
    state::AppState,
};

fn is_enabled_only_update(request: &UpdateClientRequest) -> bool {
    request.enabled.is_some()
        && request.action.is_none()
        && request.make_super.is_none()
        && request.name.is_none()
        && request.mac.is_none()
        && request.ip.is_none()
        && request.master.is_none()
        && request.snapshot.is_none()
        && request.keep_writeback.is_none()
        && request.use_game_disk.is_none()
        && request.game_disks.is_none()
        && request.chap_enabled.is_none()
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

async fn publish_boot_menu(
    settings: &crate::core::config::Settings,
    client: &crate::domain::Client,
    chap: Option<&crate::infrastructure::iscsi::ChapCredentials>,
) {
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
        tracing::warn!(
            client_id = %client.id,
            %error,
            "failed to regenerate boot menu while enabling client"
        );
    }
}

async fn refresh_dhcp(state: &AppState, settings: &crate::core::config::Settings) {
    if !settings.dhcp.enabled {
        return;
    }

    let service = crate::services::DhcpService::new(settings.clone(), state.db_pool.clone());
    if let Err(error) = service.generate_client_configs().await {
        tracing::warn!(%error, "failed to regenerate DHCP client configuration after enabled-state update");
        return;
    }
    if let Err(error) = service.validate_config().await {
        tracing::warn!(%error, "DHCP validation failed after enabled-state update; service was not reloaded");
        return;
    }
    if let Err(error) = service.reload().await {
        tracing::warn!(%error, "failed to reload DHCP service after enabled-state update");
    }
}

pub async fn update_client(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateClientRequest>,
) -> Result<Json<Client>, (StatusCode, Json<ErrorResponse>)> {
    if !is_enabled_only_update(&request) {
        return super::client_game_update::update_client(State(state), Path(id), Json(request)).await;
    }

    let _client_guard = state.client_mutations.lock().await;

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

    let enabled = request.enabled.expect("enabled-only request has enabled value");
    let settings = state.settings.read().await.clone();

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

    if existing.enabled && !enabled {
        crate::infrastructure::dhcp::remove_client_ipxe_menu(existing.mac.as_str())
            .await
            .map_err(|error| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        status: 500,
                        error: format!(
                            "Failed to remove boot menu for '{}': {error}",
                            existing.name
                        ),
                    }),
                )
            })?;
    } else if enabled {
        publish_boot_menu(&settings, &existing, chap.as_ref()).await;
    }

    let client = state
        .application
        .clients
        .set_enabled_flag(&id, enabled)
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
        tracing::warn!(%error, "failed to refresh client IP cache after enabled-state update");
    }
    refresh_dhcp(&state, &settings).await;

    Ok(Json(domain_to_legacy(client)))
}

#[cfg(test)]
mod tests {
    use super::is_enabled_only_update;
    use crate::core::client::UpdateClientRequest;

    fn request() -> UpdateClientRequest {
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
            enabled: Some(false),
            block_store: None,
            block_device: None,
            target_iqn: None,
            writeback: None,
            action: None,
            make_super: None,
        }
    }

    #[test]
    fn enabled_only_request_uses_typed_path() {
        assert!(is_enabled_only_update(&request()));
    }

    #[test]
    fn enabled_plus_other_changes_delegate() {
        let mut request = request();
        request.ip = Some("192.168.1.42".into());
        assert!(!is_enabled_only_update(&request));

        let mut request = request();
        request.chap_enabled = Some(true);
        assert!(!is_enabled_only_update(&request));

        let mut request = request();
        request.game_disks = Some(vec!["tank/game/a".into()]);
        assert!(!is_enabled_only_update(&request));
    }
}
