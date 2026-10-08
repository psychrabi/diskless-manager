use super::client_compat::domain_to_legacy;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use super::clients::ErrorResponse;
use crate::{
    core::client::{Client, UpdateClientRequest},
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
        boot_image: client.boot_image,
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

pub async fn update_client(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateClientRequest>,
) -> Result<Json<Client>, (StatusCode, Json<ErrorResponse>)> {
    if !is_enabled_only_update(&request) {
        return super::client_game_update::update_client(State(state), Path(id), Json(request))
            .await;
    }

    let _client_guard = state.client_mutations.lock().await;

    let existing = state
        .application
        .clients
        .get_by_string(&id)
        .await
        .map_err(|error| {
            tracing::error!(client_id = %id, %error, "failed to load client for enablement update");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                    error: "Failed to load client".to_string(),
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

    let enabled = request
        .enabled
        .expect("enabled-only request has enabled value");
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
    super::clients::refresh_dhcp(&state, &settings, "enabled-state update").await;

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
            boot_image: None,
        }
    }

    #[test]
    fn enabled_only_request_uses_typed_path() {
        assert!(is_enabled_only_update(&request()));
    }

    #[test]
    fn enabled_plus_other_changes_delegate() {
        let mut ip_request = request();
        ip_request.ip = Some("192.168.1.42".into());
        assert!(!is_enabled_only_update(&ip_request));

        let mut chap_request = request();
        chap_request.chap_enabled = Some(true);
        assert!(!is_enabled_only_update(&chap_request));

        let mut game_request = request();
        game_request.game_disks = Some(vec!["tank/game/a".into()]);
        assert!(!is_enabled_only_update(&game_request));
    }
}
