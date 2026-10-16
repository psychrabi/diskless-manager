use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use chrono::Utc;
use log::{error, info};
use serde::Serialize;
use std::process::Command;

use crate::{
    application::image_service::ImageService,
    core::client::{Client, ClientManager, CreateClientRequest, UpdateClientRequest},
    domain::storage::{ClientStorage, ClientStorageSpec, StorageSource, StorageVolume},
    persistence::repositories::image::ImageRepository,
    state::AppState,
    validation::{validate_ip_address, validate_mac_address},
};

/// An explicit saved snapshot must belong to this master and never fall back.
fn select_super_snapshot<'a>(
    snapshots: &'a [crate::core::image::Image],
    master: &str,
    requested: Option<&str>,
) -> Result<Option<&'a crate::core::image::Image>, String> {
    if let Some(requested) = requested {
        return snapshots
            .iter()
            .find(|snapshot| format!("{master}@{}", snapshot.name) == requested)
            .map(Some)
            .ok_or_else(|| {
                "Requested Super snapshot does not belong to the assigned image or no longer exists"
                    .into()
            });
    }
    Ok(snapshots
        .iter()
        .find(|snapshot| snapshot.is_default)
        .or_else(|| snapshots.iter().max_by_key(|snapshot| snapshot.created_at)))
}

fn image_service(state: &AppState) -> ImageService {
    ImageService::with_backend(
        ImageRepository::new(state.db_pool.clone()),
        std::sync::Arc::new(crate::infrastructure::image::ZfsImageBackend::with_config(
            state.config.clone(),
        )),
    )
}

#[derive(Debug)]
pub struct ErrorResponse {
    pub status: u16,
    pub error: String,
}

impl Serialize for ErrorResponse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        crate::api::error::serialize_api_error(
            self.status,
            &self.error,
            serde_json::json!({}),
            serializer,
        )
    }
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> Response {
        (
            StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(self),
        )
            .into_response()
    }
}

// ============================================================================
// Storage helpers
// ============================================================================

/// Build the desired storage specification for a client.
///
/// When a snapshot is configured:
///
/// ```text
/// snapshot
///     │
///     ▼
/// ZFS clone
///     │
///     ▼
/// iSCSI LUN
/// ```
///
/// When no snapshot is configured, the client references the existing
/// master ZVOL instead:
///
/// ```text
/// existing master ZVOL
///     │
///     ▼
/// iSCSI LUN
/// ```
fn build_storage_spec(
    config: &crate::config::RuntimeConfig,
    settings: &crate::core::config::Settings,
    client_id: &str,
    client_name: &str,
    master: &str,
    snapshot: Option<&str>,
    use_game_disk: bool,
) -> Result<ClientStorageSpec, String> {
    let client_name = client_name.trim();

    if client_name.is_empty() {
        return Err("Client name cannot be empty".to_string());
    }

    if master.trim().is_empty() {
        return Err("Master image cannot be empty".to_string());
    }

    let target_iqn = format!(
        "{}:client.{}",
        settings.iscsi.target_prefix,
        client_name.to_lowercase()
    );

    let backstore = format!("block_{}", client_name.to_lowercase());

    match snapshot {
        Some(snapshot) if !snapshot.trim().is_empty() => {
            let dataset = crate::infrastructure::zfs::legacy::get_writeback_or_default_dataset(
                config,
                client_name,
            );

            Ok(ClientStorageSpec {
                client_id: client_id.to_string(),
                client_name: client_name.to_string(),
                source: StorageSource::Snapshot(snapshot.to_string()),
                dataset,
                backstore,
                target_iqn,
                lun: 0,
                use_game_disk,
                // Resolved selection is filled in by callers that manage
                // game disks (create/update); otherwise empty.
                game_disks: Vec::new(),
                // Same for CHAP: callers that enforce resolve credentials.
                chap: None,
            })
        }

        _ => {
            crate::validation::validate_managed_dataset(
                master,
                &crate::config::get_zpool_name(config),
            )
            .map_err(|error| format!("Invalid master dataset: {error}"))?;

            Ok(ClientStorageSpec {
                client_id: client_id.to_string(),
                client_name: client_name.to_string(),
                source: StorageSource::ExistingVolume(master.to_string()),
                dataset: master.to_string(),
                backstore,
                target_iqn,
                lun: 0,
                use_game_disk,
                game_disks: Vec::new(),
                chap: None,
            })
        }
    }
}

/// Replace a generated target IQN with the persisted IQN for an existing client.
///
/// New clients receive an IQN generated from the configured prefix. Existing
/// clients keep their persisted IQN so legacy targets are never renamed by a
/// reset, update, or delete operation.
fn preserve_persisted_target_iqn(spec: &mut ClientStorageSpec, client: &Client) {
    if let Some(target_iqn) = client
        .target_iqn
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        spec.target_iqn = target_iqn.to_string();
    }
}

/// Convert application storage information into the legacy client fields
/// stored in the clients table.
fn apply_storage_to_update_request(request: &mut UpdateClientRequest, storage: &ClientStorage) {
    // `block_store` is the LIO backstore NAME (reconciliation rebuilds LUN
    // specs from it); the device path lives in `block_device`. Storing a
    // device path here breaks repair, so never mix the two.
    request.block_store = Some(storage.backstore().to_string());

    request.block_device = Some(storage.block_device().display().to_string());

    request.target_iqn = Some(storage.target_iqn().to_string());

    // Persist the writeback dataset so inspection and repair can rebuild
    // the spec. Only snapshot clones own a writeback dataset.
    if matches!(storage.source, StorageSource::Snapshot(_)) {
        request.writeback = Some(storage.dataset().to_string());
    }
}

fn storage_configuration_changed(client: &Client, request: &UpdateClientRequest) -> bool {
    request
        .name
        .as_deref()
        .is_some_and(|name| name != client.name)
        || request
            .master
            .as_deref()
            .is_some_and(|master| master != client.master)
        || request.snapshot != client.snapshot
}

fn is_unprovisioned_enrollment(client: &Client) -> bool {
    client.master == "pending"
        && client.snapshot.is_none()
        && client.target_iqn.is_none()
        && client.block_store.is_none()
        && client.block_device.is_none()
        && client.writeback.is_none()
}

/// Whether the game-disk configuration changed.
///
/// Game-only changes take a lightweight path that never touches the boot
/// clone: the flag flipped, or an explicit selection differs from storage.
fn game_configuration_changed(
    existing_use_game_disk: bool,
    stored_game_disks: &[String],
    request: &UpdateClientRequest,
) -> bool {
    request
        .use_game_disk
        .is_some_and(|enabled| enabled != existing_use_game_disk)
        || request
            .game_disks
            .as_ref()
            .is_some_and(|selection| selection.as_slice() != stored_game_disks)
}

/// Resolve an effective game selection against discovered masters.
///
/// Empty + switch on means all currently discovered masters; otherwise
/// the stored/requested list intersected with discovery.
fn resolve_effective_game_selection(
    state: &AppState,
    use_game_disk: bool,
    stored_or_requested: &[String],
) -> anyhow::Result<Vec<String>> {
    let discovered = state
        .application
        .storage
        .discover_game_masters()?
        .into_iter()
        .map(|master| master.dataset)
        .collect::<Vec<_>>();
    Ok(crate::application::storage_service::resolve_game_selection(
        use_game_disk,
        stored_or_requested,
        &discovered,
    ))
}

/// (Re)publish a provisioned client's static boot menu, embedding CHAP
/// credentials when enforced. Best-effort: menus self-heal on the next
/// boot through the enrollment redirect path.
async fn publish_boot_menu(state: &AppState, client_id: &str) {
    if let Err(error) = state.refresh_client_boot_menu(client_id).await {
        tracing::warn!(client_id, %error, "failed to refresh client boot menu");
    }
}

/// Replace a client's stored game master selection wholesale.
async fn persist_game_selection(
    pool: &sqlx::SqlitePool,
    client_id: &str,
    selection: &[String],
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let mut transaction = pool.begin().await.map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: 500,
                error: error.to_string(),
            }),
        )
    })?;
    sqlx::query("DELETE FROM client_game_disks WHERE client_id = ?")
        .bind(client_id)
        .execute(&mut *transaction)
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
    for master in selection {
        sqlx::query("INSERT INTO client_game_disks (client_id, master_dataset) VALUES (?, ?)")
            .bind(client_id)
            .bind(master)
            .execute(&mut *transaction)
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
    }
    transaction.commit().await.map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: 500,
                error: error.to_string(),
            }),
        )
    })?;
    Ok(())
}

/// Reconstruct the application-level storage object from the persisted
/// client record.
///
/// This is required when deleting/resetting a client because the actual
/// application storage object is not persisted separately.
fn storage_from_client(
    config: &crate::config::RuntimeConfig,
    settings: &crate::core::config::Settings,
    client: &Client,
) -> Result<ClientStorage, String> {
    let mut spec = build_storage_spec(
        config,
        settings,
        &client.id,
        &client.name,
        &client.master,
        client.snapshot.as_deref(),
        client.use_game_disk.unwrap_or(false),
    )?;

    preserve_persisted_target_iqn(&mut spec, client);

    Ok(ClientStorage {
        client_id: spec.client_id.clone(),
        source: spec.source.clone(),
        volume: StorageVolume::new(
            spec.dataset.clone(),
            spec.block_device(),
            spec.backstore.clone(),
            spec.target_iqn.clone(),
            spec.lun,
        ),
        use_game_disk: spec.use_game_disk,
    })
}

/// Regenerate DHCP configuration after a client change.
pub(super) async fn refresh_dhcp(
    state: &AppState,
    settings: &crate::core::config::Settings,
    operation: &str,
) {
    if !settings.dhcp.enabled {
        return;
    }

    let dhcp_service = crate::services::DhcpService::new(settings.clone(), state.db_pool.clone());

    if let Err(e) = dhcp_service.reload_clients().await {
        tracing::warn!("Failed to reload DHCP service after {}: {}", operation, e);
    } else {
        info!("DHCP service reloaded successfully after {}", operation);
    }
}

/// Persist the runtime storage/mode fields of a client.
///
/// Used by the Super mode toggle, which switches storage between a shared
/// master volume and a snapshot clone without going through the generic
/// client update path.
async fn persist_client_runtime_state(
    client: &Client,
    pool: &sqlx::SqlitePool,
) -> Result<sqlx::sqlite::SqliteQueryResult, sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE clients
        SET master = ?, snapshot = ?, block_device = ?, block_store = ?,
            target_iqn = ?, mode = ?, status = ?, last_modified = ?, updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(&client.master)
    .bind(&client.snapshot)
    .bind(&client.block_device)
    .bind(&client.block_store)
    .bind(&client.target_iqn)
    .bind(&client.mode)
    .bind(&client.status)
    .bind(&client.last_modified)
    .bind(client.updated_at.to_rfc3339())
    .bind(&client.id)
    .execute(pool)
    .await
}

// ============================================================================
// Client CRUD
// ============================================================================

/// Enforce the Windows boot gate on client creation.
///
/// Resolves `request.master`/`request.snapshot` in place: a Windows master
/// without a tested Boot NIC match refuses creation with an actionable
/// hardware-ID message, while a covered client is assigned the newest `ready`
/// revision. Explicit revisions still require matching inventory and always
/// use their immutable servicing snapshot as the private clone source.
/// Must run before any storage or iPXE side effect.
async fn gate_windows_assignment(
    state: &AppState,
    settings: &crate::core::config::Settings,
    request: &mut CreateClientRequest,
) -> Result<(), (StatusCode, String)> {
    let repository = ImageRepository::new(state.db_pool.clone());
    let Ok(Some(master)) = repository.get(&request.master).await else {
        return Ok(());
    };
    if master.os_type != crate::core::image::OsType::Windows {
        return Ok(());
    }
    let pinned: Option<(String, String, String)> = sqlx::query_as(
        "SELECT id, source_image_id, status FROM windows_image_driver_revisions WHERE revision_image_id = ? LIMIT 1",
    )
    .bind(&master.id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, format!("failed to inspect driver revisions: {error}")))?;
    if pinned
        .as_ref()
        .is_some_and(|(_, _, status)| status != "ready")
    {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "Windows revision has not passed its boot test".into(),
        ));
    }
    let mac = crate::domain::MacAddress::parse(&request.mac).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "client MAC address is invalid".to_string(),
        )
    })?;
    let inventory: Option<(String,)> =
        sqlx::query_as("SELECT boot_nic_ids FROM windows_nic_inventory WHERE mac = ?")
            .bind(mac.as_str())
            .fetch_optional(&state.db_pool)
            .await
            .map_err(|error| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to inspect NIC inventory: {error}"),
                )
            })?;
    let boot_ids: Option<Vec<String>> = inventory
        .map(|(raw,)| serde_json::from_str(&raw))
        .transpose()
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("stored NIC inventory is invalid: {error}"),
            )
        })?;
    let mut revisions = repository
        .list_driver_revisions(
            pinned
                .as_ref()
                .map_or(master.id.as_str(), |(_, source_id, _)| source_id.as_str()),
        )
        .await
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to inspect driver revisions: {error:#}"),
            )
        })?;
    revisions.retain(|revision| {
        revision.status == crate::core::image::DriverRevisionStatus::Ready
            && revision.revision_image_id.is_some()
            && pinned
                .as_ref()
                .map_or(true, |(id, _, _)| &revision.id == id)
    });
    revisions.reverse();
    let root = std::path::PathBuf::from(&settings.http.root_dir);
    let catalog = tokio::task::spawn_blocking(move || {
        crate::infrastructure::pxe::NetworkDriverInjectionPlugin::new(root).list()
    })
    .await
    .map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("driver catalog check failed: {error}"),
        )
    })?
    .map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("driver catalog check failed: {error:#}"),
        )
    })?;
    let passed_rows: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT revision_id, expected_nic_id, reported_revision, reported_nic_id FROM windows_revision_boot_tests WHERE status = 'passed'",
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to inspect passed Windows NIC tests: {error}"),
        )
    })?;
    let mut passed_nic_tests = std::collections::HashMap::<String, Vec<String>>::new();
    for (revision_id, expected_nic, reported_revision, reported_nic) in passed_rows {
        if reported_revision == revision_id && reported_nic.eq_ignore_ascii_case(&expected_nic) {
            passed_nic_tests
                .entry(revision_id)
                .or_default()
                .push(expected_nic);
        }
    }
    let assignment = crate::application::windows_servicing::select_windows_assignment(
        mac.as_str(),
        boot_ids.as_deref(),
        &revisions,
        &passed_nic_tests,
        &catalog,
    )
    .map_err(|error| (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()))?;
    if let Some(assignment) = assignment {
        let Some(image) = repository
            .get(&assignment.revision_image_id)
            .await
            .map_err(|error| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("tested revision image is missing: {error:#}"),
                )
            })?
        else {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "tested revision image is missing".to_string(),
            ));
        };
        tracing::info!(
            client = %request.name,
            revision = %assignment.revision_id,
            image = %image.name,
            "assigning new Windows client to the newest tested revision"
        );
        request.snapshot = Some(revision_clone_snapshot(&repository, &image).await?);
        request.master = image.name;
    }
    Ok(())
}

/// A serviced revision is never exposed as writable client storage.
async fn revision_clone_snapshot(
    repository: &ImageRepository,
    image: &crate::core::image::Image,
) -> Result<String, (StatusCode, String)> {
    let snapshots = repository.list().await.map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to inspect revision snapshot: {error:#}"),
        )
    })?;
    let mut serviced = snapshots.iter().filter(|snapshot| {
        snapshot.kind == crate::core::image::ImageKind::Snapshot
            && snapshot.parent_id.as_deref() == Some(image.id.as_str())
            && snapshot.name.starts_with("serviced-")
            && snapshot.status == "ready"
    });
    let Some(snapshot) = serviced.next() else {
        return Err((
            StatusCode::CONFLICT,
            "Windows revision has no immutable servicing snapshot".into(),
        ));
    };
    if serviced.next().is_some() {
        return Err((
            StatusCode::CONFLICT,
            "Windows revision has ambiguous servicing snapshots".into(),
        ));
    }
    Ok(format!("{}@{}", image.name, snapshot.name))
}

pub async fn create_client(
    State(state): State<AppState>,
    Json(request): Json<CreateClientRequest>,
) -> Result<Json<crate::domain::Client>, (StatusCode, Json<ErrorResponse>)> {
    // Serialize uniqueness checks and same-name storage provisioning without pausing existing clients.
    let _client_guard = state
        .client_mutations
        .lock_resource("client-creation")
        .await;
    if validate_ip_address(&request.ip).is_err() || validate_mac_address(&request.mac).is_err() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                status: StatusCode::BAD_REQUEST.as_u16(),
                error: "Invalid client IP address or MAC address".to_string(),
            }),
        ));
    }

    info!("Creating client: {:?}", request);

    let settings = state.settings.read().await.clone();

    // Windows boot gate: a new Windows client must match a preboot Boot NIC
    // record against a boot-tested revision. This runs before any storage or
    // iPXE side effect, so a refused client creates nothing. Existing
    // clients stay pinned: update paths never re-resolve the image.
    let mut request = request;
    gate_windows_assignment(&state, &settings, &mut request)
        .await
        .map_err(|(status, error)| {
            error!(
                "Windows assignment gate refused client '{}': {}",
                request.name, error
            );
            (
                status,
                Json(ErrorResponse {
                    status: status.as_u16(),
                    error,
                }),
            )
        })?;

    // ------------------------------------------------------------------------
    // Build desired storage state.
    // ------------------------------------------------------------------------

    let mut storage_spec = build_storage_spec(
        &state.config,
        &settings,
        &request.name,
        &request.name,
        &request.master,
        request.snapshot.as_deref(),
        request.use_game_disk.unwrap_or(false),
    )
    .map_err(|error| {
        error!(
            "Invalid storage configuration for client '{}': {}",
            request.name, error
        );

        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                status: StatusCode::BAD_REQUEST.as_u16(),
                error: format!("Invalid storage configuration: {error}"),
            }),
        )
    })?;

    // Resolve the game selection up front: empty + switch on means all
    // currently discovered masters.
    storage_spec.game_disks = resolve_effective_game_selection(
        &state,
        request.use_game_disk.unwrap_or(false),
        request.game_disks.as_deref().unwrap_or(&[]),
    )
    .map_err(|error| {
        error!(
            "Failed to resolve game selection for client '{}': {}",
            request.name, error
        );

        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                error: format!("Failed to resolve game selection: {error:#}"),
            }),
        )
    })?;

    // New clients enforce CHAP unless explicitly opted out. Credentials
    // are generated up front (no client id exists yet), travel into the
    // provisioned target and boot menu together, and are persisted after
    // the insert succeeds.
    let chap_enabled = request.chap_enabled.unwrap_or(true);
    storage_spec.chap = if chap_enabled {
        Some(
            crate::infrastructure::iscsi::ChapCredentials::generate(&request.name).map_err(
                |error| {
                    error!(
                        "Failed to generate CHAP credentials for client '{}': {}",
                        request.name, error
                    );

                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                            error: format!("Failed to generate client CHAP credentials: {error:#}"),
                        }),
                    )
                },
            )?,
        )
    } else {
        None
    };

    // ------------------------------------------------------------------------
    // Provision storage and persist the client through one application
    // transaction shared by every transport adapter.
    // ------------------------------------------------------------------------

    let create = crate::domain::CreateClient {
        name: request.name.clone(),
        mac: request.mac.clone(),
        ip: request.ip.clone(),
        master: request.master.clone(),
        snapshot: request.snapshot.clone(),
        block_store: None,
        block_device: None,
        target_iqn: None,
        pxe_mode: crate::domain::PxeMode::Uefi,
        keep_writeback: request.keep_writeback.unwrap_or(true),
        use_game_disk: request.use_game_disk.unwrap_or(false),
        game_disks: request.game_disks.clone().unwrap_or_default(),
        chap_enabled: request.chap_enabled.unwrap_or(true),
    };

    let client = state
        .application
        .provisioning
        .create_client(create, storage_spec, &settings.dhcp.next_server_ip)
        .await
        .map_err(|error| {
            error!("Failed to create client '{}': {:#}", request.name, error);

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                    error: format!("Failed to create client: {error:#}"),
                }),
            )
        })?;

    info!("Created client: {}", client.name);

    // Refresh client IP cache.
    if let Err(e) = state.refresh_client_ips().await {
        tracing::warn!("Failed to refresh client IPs cache: {}", e);
    }

    Ok(Json(client))
}

pub async fn update_client(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(claims): Extension<crate::types::Claims>,
    Json(mut request): Json<UpdateClientRequest>,
) -> Result<Json<Client>, (StatusCode, Json<ErrorResponse>)> {
    info!("Updating client: {:?}", request);
    let _client_guard = state.client_mutations.lock_client(&id).await;

    // ------------------------------------------------------------------------
    // Validate request.
    // ------------------------------------------------------------------------

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

    let manager = ClientManager::new(state.db_pool.clone());

    let existing_client = manager.get(&id).await.map_err(|_| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                status: StatusCode::NOT_FOUND.as_u16(),
                error: "Client not found".to_string(),
            }),
        )
    })?;

    if request.action.as_deref().map_or(true, |action| {
        !matches!(action, "wake" | "reboot" | "shutdown")
    }) {
        let recovering: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM client_offline_resets WHERE client_id = ? AND operation IS NOT NULL")
            .bind(&id).fetch_one(&state.db_pool).await.map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { status: 500, error: error.to_string() })))?;
        if recovering != 0 {
            return Err((
                StatusCode::CONFLICT,
                Json(ErrorResponse {
                    status: 409,
                    error: "Client storage recovery is pending; retry after recovery completes"
                        .into(),
                }),
            ));
        }
    }

    // ========================================================================
    // Action-based requests
    // ========================================================================

    if let Some(action) = &request.action {
        match action.as_str() {
            // ----------------------------------------------------------------
            // Wake
            // ----------------------------------------------------------------
            "wake" => {
                let mac = &existing_client.mac;

                info!(
                    "Attempting to send WOL packet to client {} ({})",
                    existing_client.name, mac
                );

                let mut wake_cmd = Command::new(crate::platform::wol_binary());
                wake_cmd.arg(mac);
                let result = crate::api::util::run_command(&mut wake_cmd).await;

                match result {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout);

                        let stderr = String::from_utf8_lossy(&output.stderr);

                        if output.status.success() {
                            info!(
                                "Successfully sent WOL packet using wakeonlan to client {} ({}). Output: {}",
                                existing_client.name,
                                mac,
                                stdout
                            );

                            return Ok(Json(existing_client));
                        }

                        error!(
                            "wakeonlan failed for client {} ({}). Status: {:?}, Stderr: {}",
                            existing_client.name, mac, output.status, stderr
                        );
                    }

                    Err(e) => {
                        error!("wakeonlan command not found or failed: {}", e);
                    }
                }

                // Fallback to etherwake.
                let mut ether_cmd = Command::new("etherwake");
                ether_cmd.args(["-b", mac]);
                let ether_result = crate::api::util::run_command(&mut ether_cmd).await;

                match ether_result {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout);

                        let stderr = String::from_utf8_lossy(&output.stderr);

                        if output.status.success() {
                            info!(
                                "Successfully sent WOL packet using etherwake to client {} ({}). Output: {}",
                                existing_client.name,
                                mac,
                                stdout
                            );

                            return Ok(Json(existing_client));
                        }

                        let error_msg = format!(
                            "etherwake failed for client {} ({}): {}",
                            existing_client.name, mac, stderr
                        );

                        error!("{}", error_msg);

                        return Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                                error: error_msg,
                            }),
                        ));
                    }

                    Err(e) => {
                        let error_msg = format!(
                            "Failed to execute etherwake for client {} ({}): {}",
                            existing_client.name, mac, e
                        );

                        error!("{}", error_msg);

                        return Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                                error: error_msg,
                            }),
                        ));
                    }
                }
            }

            // ----------------------------------------------------------------
            // Reboot
            // ----------------------------------------------------------------
            "reboot" => {
                let ip = &existing_client.ip;

                if ip.is_empty() {
                    let error_msg = format!("IP address not found for '{}'", existing_client.name);

                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                            error: error_msg,
                        }),
                    ));
                }

                let master_os =
                    super::control::resolve_master_os(&state.db_pool, &existing_client.master)
                        .await
                        .map_err(|error| {
                            (
                                StatusCode::BAD_REQUEST,
                                Json(ErrorResponse { status: 400, error }),
                            )
                        })?;
                let mut cmd = super::control::build_power_command(
                    "reboot",
                    ip,
                    &master_os,
                    master_os == "windows",
                    "diskless",
                );
                let output = crate::api::util::run_command(&mut cmd)
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
                if !output.status.success() {
                    return Err((
                        StatusCode::BAD_GATEWAY,
                        Json(ErrorResponse {
                            status: 502,
                            error: format!(
                                "Failed to reboot client: {}",
                                String::from_utf8_lossy(&output.stderr)
                            ),
                        }),
                    ));
                }

                info!("Reboot command sent to {} ({})", existing_client.name, ip);

                return Ok(Json(existing_client));
            }

            // ----------------------------------------------------------------
            // Shutdown
            // ----------------------------------------------------------------
            "shutdown" => {
                let ip = &existing_client.ip;

                if ip.is_empty() {
                    let error_msg = format!("IP address not found for '{}'", existing_client.name);

                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                            error: error_msg,
                        }),
                    ));
                }

                let master_os =
                    super::control::resolve_master_os(&state.db_pool, &existing_client.master)
                        .await
                        .map_err(|error| {
                            (
                                StatusCode::BAD_REQUEST,
                                Json(ErrorResponse { status: 400, error }),
                            )
                        })?;
                let mut cmd = super::control::build_power_command(
                    "shutdown",
                    ip,
                    &master_os,
                    master_os == "windows",
                    "diskless",
                );
                let output = crate::api::util::run_command(&mut cmd)
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
                if !output.status.success() {
                    return Err((
                        StatusCode::BAD_GATEWAY,
                        Json(ErrorResponse {
                            status: 502,
                            error: format!(
                                "Failed to shutdown client: {}",
                                String::from_utf8_lossy(&output.stderr)
                            ),
                        }),
                    ));
                }

                info!("Shutdown command sent to {} ({})", existing_client.name, ip);

                return Ok(Json(existing_client));
            }

            // ----------------------------------------------------------------
            // Remote
            // ----------------------------------------------------------------
            "remote" => {
                info!("Remote control for client: {}", existing_client.name);

                return Ok(Json(existing_client));
            }

            // ----------------------------------------------------------------
            // Super mode
            // ----------------------------------------------------------------
            "super" => {
                let Some(make_super) = request.make_super else {
                    return Ok(Json(existing_client));
                };

                if claims.role != "admin" {
                    return Err((
                        StatusCode::FORBIDDEN,
                        Json(ErrorResponse {
                            status: 403,
                            error: "Only administrators can change Super mode".into(),
                        }),
                    ));
                }
                if make_super {
                    return Err((
                        StatusCode::CONFLICT,
                        Json(ErrorResponse {
                            status: StatusCode::CONFLICT.as_u16(),
                            error: "Use the managed Super session API; the legacy toggle can export a shared image as writable storage".into(),
                        }),
                    ));
                }
                if existing_client.is_super_mode() {
                    return Err((
                        StatusCode::CONFLICT,
                        Json(ErrorResponse {
                            status: 409,
                            error: "Use the administrator-only legacy Super exit operation and select a snapshot".into(),
                        }),
                    ));
                }
                let settings = state.settings.read().await;

                let master = existing_client.master.trim().to_string();

                if master.is_empty() || master == "pending" {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            status: StatusCode::BAD_REQUEST.as_u16(),
                            error: format!(
                                "Cannot toggle Super mode for '{}': no master image configured",
                                existing_client.name
                            ),
                        }),
                    ));
                }

                // The assignment decides what "currently super" means. A
                // client that boots a snapshot clone has no direct image
                // writes to return, and rebuilding its storage anyway would
                // discard the clone it is meant to keep.
                if !existing_client.is_super_client() {
                    return Err((
                        StatusCode::CONFLICT,
                        Json(ErrorResponse {
                            status: StatusCode::CONFLICT.as_u16(),
                            error: format!(
                                "Cannot disable Super mode for '{}': it boots a snapshot clone, not an image directly. Nothing was changed.",
                                existing_client.name
                            ),
                        }),
                    ));
                }

                let client_status =
                    crate::utils::network::get_client_status_realtime(existing_client.ip.clone());

                if client_status != "Offline" {
                    return Err((
                        StatusCode::CONFLICT,
                        Json(ErrorResponse {
                            status: StatusCode::CONFLICT.as_u16(),
                            error: format!(
                                "Cannot toggle Super mode for '{}': client must be offline (current status: {})",
                                existing_client.name, client_status
                            ),
                        }),
                    ));
                }

                let mut client = existing_client.clone();

                {
                    // -------------------------------------------------------
                    // Disable Super mode: revert the client to a snapshot
                    // clone of the master.
                    // -------------------------------------------------------
                    info!(
                        "Disabling Super mode for client '{}' using master '{}'",
                        existing_client.name, master
                    );

                    let image = image_service(&state).get(&master).await.map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                                error: format!(
                                    "Failed to look up master image '{}': {}",
                                    master, e
                                ),
                            }),
                        )
                    })?;

                    let snapshots =
                        image_service(&state)
                            .snapshots(&image.id)
                            .await
                            .map_err(|e| {
                                (
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(ErrorResponse {
                                        status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                                        error: format!(
                                            "Failed to list snapshots for master '{}': {}",
                                            master, e
                                        ),
                                    }),
                                )
                            })?;

                    let snapshot_image =
                        select_super_snapshot(&snapshots, &master, request.snapshot.as_deref())
                            .map_err(|error| {
                                (
                                    StatusCode::BAD_REQUEST,
                                    Json(ErrorResponse { status: 400, error }),
                                )
                            })?;

                    let Some(snapshot_image) = snapshot_image else {
                        return Err((
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse {
                                status: StatusCode::BAD_REQUEST.as_u16(),
                                error: format!(
                                    "Cannot disable Super mode for '{}': no snapshots found for master '{}'. Client remains in Super mode.",
                                    existing_client.name, master
                                ),
                            }),
                        ));
                    };

                    let snapshot_source = format!("{}@{}", master, snapshot_image.name);

                    let current_storage =
                        storage_from_client(&state.config, &settings, &existing_client).map_err(
                            |error| {
                                (
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(ErrorResponse {
                                        status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                                        error,
                                    }),
                                )
                            },
                        )?;

                    if let Err(error) = state
                        .application
                        .storage
                        .destroy_client_storage(&current_storage)
                    {
                        tracing::warn!(
                            "Failed to remove existing storage before disabling Super mode for '{}': {}",
                            existing_client.name,
                            error
                        );
                    }

                    let mut spec = build_storage_spec(
                        &state.config,
                        &settings,
                        &existing_client.id,
                        &existing_client.name,
                        &master,
                        Some(&snapshot_source),
                        existing_client.use_game_disk.unwrap_or(false),
                    )
                    .map_err(|error| {
                        (
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse {
                                status: StatusCode::BAD_REQUEST.as_u16(),
                                error,
                            }),
                        )
                    })?;

                    preserve_persisted_target_iqn(&mut spec, &existing_client);

                    // A super-mode rebuild must not silently drop target
                    // authentication.
                    spec.chap = crate::core::reconciliation::ensure_chap_credentials(
                        &state.db_pool,
                        &existing_client.id,
                        &existing_client.name,
                        existing_client.chap_enabled.unwrap_or(false),
                    )
                    .await
                    .map_err(|error| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                                error: format!("Failed to resolve CHAP credentials: {}", error),
                            }),
                        )
                    })?;

                    let storage = state
                        .application
                        .storage
                        .create_client_storage(&spec)
                        .map_err(|error| {
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(ErrorResponse {
                                    status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                                    error: format!(
                                        "Failed to rebuild client storage from snapshot: {}",
                                        error
                                    ),
                                }),
                            )
                        })?;

                    client.mode = None;
                    client.master = master;
                    client.snapshot = Some(snapshot_source.clone());
                    client.block_device = Some(format!("/dev/zvol/{}", storage.dataset()));
                    client.block_store = Some(storage.backstore().to_string());
                    client.target_iqn = Some(storage.target_iqn().to_string());

                    info!(
                        "Super mode disabled for client '{}': storage rebuilt from snapshot '{}'",
                        client.name, snapshot_source
                    );
                }

                client.updated_at = Utc::now();

                client.last_modified =
                    Some(client.updated_at.format("%Y-%m-%d %H:%M:%S").to_string());

                persist_client_runtime_state(&client, &state.db_pool)
                    .await
                    .map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                                error: format!("Failed to persist Super mode state: {}", e),
                            }),
                        )
                    })?;

                info!(
                    "Super mode state saved for client '{}': mode={:?}, device={}",
                    client.name,
                    client.mode,
                    client.block_device.as_deref().unwrap_or_default()
                );

                drop(settings);

                if let Err(e) = state.refresh_client_ips().await {
                    tracing::warn!("Failed to refresh client IPs cache: {}", e);
                }

                let settings = state.settings.read().await;

                refresh_dhcp(&state, &settings, "toggling super mode").await;

                return Ok(Json(client));
            }

            // ----------------------------------------------------------------
            // Reset writeback
            // ----------------------------------------------------------------
            "reset" => {
                info!("Reset writeback for client: {}", existing_client.name);

                let snapshot = match existing_client.snapshot.as_deref() {
                    Some(value) if !value.trim().is_empty() => value,

                    _ => {
                        return Err((
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse {
                                status: StatusCode::BAD_REQUEST.as_u16(),
                                error: "Cannot reset writeback: client has no snapshot configured"
                                    .to_string(),
                            }),
                        ));
                    }
                };

                let settings = state.settings.read().await;

                let mut spec = build_storage_spec(
                    &state.config,
                    &settings,
                    &existing_client.id,
                    &existing_client.name,
                    &existing_client.master,
                    Some(snapshot),
                    existing_client.use_game_disk.unwrap_or(false),
                )
                .map_err(|error| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            status: StatusCode::BAD_REQUEST.as_u16(),
                            error,
                        }),
                    )
                })?;

                preserve_persisted_target_iqn(&mut spec, &existing_client);

                let current = storage_from_client(&state.config, &settings, &existing_client)
                    .map_err(|error| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                                error,
                            }),
                        )
                    })?;

                state
                    .application
                    .storage
                    .reset_client_storage(&current, &spec)
                    .map_err(|error| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                                error: format!("Failed to reset client storage: {error:#}"),
                            }),
                        )
                    })?;

                info!(
                    "Successfully reset writeback for client '{}'",
                    existing_client.name
                );

                return Ok(Json(existing_client));
            }

            // ----------------------------------------------------------------
            // Reset clean
            // ----------------------------------------------------------------
            "reset_clean" => {
                info!("Reset to clean state for client: {}", existing_client.name);

                let settings = state.settings.read().await;

                let source = existing_client
                    .snapshot
                    .as_deref()
                    .filter(|value| !value.trim().is_empty());

                let mut spec = build_storage_spec(
                    &state.config,
                    &settings,
                    &existing_client.id,
                    &existing_client.name,
                    &existing_client.master,
                    source,
                    existing_client.use_game_disk.unwrap_or(false),
                )
                .map_err(|error| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            status: StatusCode::BAD_REQUEST.as_u16(),
                            error,
                        }),
                    )
                })?;

                preserve_persisted_target_iqn(&mut spec, &existing_client);

                let current = storage_from_client(&state.config, &settings, &existing_client)
                    .map_err(|error| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                                error,
                            }),
                        )
                    })?;

                state
                    .application
                    .storage
                    .reset_client_storage(&current, &spec)
                    .map_err(|error| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                                error: format!("Failed to reset client storage: {error:#}"),
                            }),
                        )
                    })?;

                info!(
                    "Successfully reset client '{}' to clean state",
                    existing_client.name
                );

                return Ok(Json(existing_client));
            }

            _ => {
                tracing::warn!("Unknown client action: {}", action);
            }
        }
    }

    // ========================================================================
    // Normal client update
    // ========================================================================

    let settings = state.settings.read().await;

    // A persistence-only PATCH must retain the selected snapshot. A full form
    // explicitly supplies master, allowing a null snapshot to select it directly.
    if request.master.is_none() && request.snapshot.is_none() {
        request.snapshot = existing_client.snapshot.clone();
    }
    if is_unprovisioned_enrollment(&existing_client)
        || request
            .master
            .as_ref()
            .is_some_and(|master| master != &existing_client.master)
    {
        let mut assignment = CreateClientRequest {
            name: request
                .name
                .clone()
                .unwrap_or_else(|| existing_client.name.clone()),
            mac: request
                .mac
                .clone()
                .unwrap_or_else(|| existing_client.mac.clone()),
            ip: request
                .ip
                .clone()
                .unwrap_or_else(|| existing_client.ip.clone()),
            master: request
                .master
                .clone()
                .unwrap_or_else(|| existing_client.master.clone()),
            snapshot: request.snapshot.clone(),
            keep_writeback: request.keep_writeback,
            use_game_disk: request.use_game_disk,
            game_disks: request.game_disks.clone(),
            chap_enabled: request.chap_enabled,
            block_store: None,
            block_device: None,
            target_iqn: None,
        };
        gate_windows_assignment(&state, &settings, &mut assignment)
            .await
            .map_err(|(status, error)| {
                (
                    status,
                    Json(ErrorResponse {
                        status: status.as_u16(),
                        error,
                    }),
                )
            })?;
        request.master = Some(assignment.master);
        request.snapshot = assignment.snapshot;
    } else {
        // A full form may omit the snapshot; a pinned revision still owns a private clone.
        let repository = ImageRepository::new(state.db_pool.clone());
        let revision: Option<(String,)> = sqlx::query_as("SELECT id FROM windows_image_driver_revisions WHERE revision_image_id = (SELECT id FROM images WHERE name = ? OR id = ? LIMIT 1) LIMIT 1")
            .bind(&existing_client.master).bind(&existing_client.master).fetch_optional(&state.db_pool).await.map_err(|error| {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { status: 500, error: error.to_string() }))
            })?;
        if revision.is_some() {
            let image = repository
                .get(&existing_client.master)
                .await
                .map_err(|error| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            status: 500,
                            error: error.to_string(),
                        }),
                    )
                })?
                .ok_or_else(|| {
                    (
                        StatusCode::CONFLICT,
                        Json(ErrorResponse {
                            status: 409,
                            error: "Windows revision image is missing".into(),
                        }),
                    )
                })?;
            request.snapshot = Some(revision_clone_snapshot(&repository, &image).await.map_err(
                |(status, error)| {
                    (
                        status,
                        Json(ErrorResponse {
                            status: status.as_u16(),
                            error,
                        }),
                    )
                },
            )?);
        }
    }
    let stored_game_disks = crate::core::reconciliation::stored_game_selection(&state.db_pool, &id)
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
    let existing_game_flag = existing_client.use_game_disk.unwrap_or(false);
    let boot_changed = storage_configuration_changed(&existing_client, &request);
    let game_changed = game_configuration_changed(existing_game_flag, &stored_game_disks, &request);

    // CHAP: resolve once for every path below. Generation is idempotent
    // (read-first), so resolving on every update is cheap and keeps all
    // provisioning paths converged without per-branch duplication.
    let existing_chap = existing_client.chap_enabled.unwrap_or(false);
    let chap_enabled = request.chap_enabled.unwrap_or(existing_chap);
    let chap_changed = request
        .chap_enabled
        .is_some_and(|value| value != existing_chap);
    let chap_name = request.name.as_deref().unwrap_or(&existing_client.name);
    let chap_creds = if chap_enabled {
        crate::core::reconciliation::ensure_chap_credentials(&state.db_pool, &id, chap_name, true)
            .await
            .map_err(|error| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        status: 500,
                        error: format!("Failed to ensure CHAP credentials: {}", error),
                    }),
                )
            })?
    } else {
        None
    };

    // Boot-menu lifecycle for the enabled switch.
    //
    // Provisioned clients boot through their static per-client iPXE menu
    // and never consult the enrollment endpoint, so flipping `enabled`
    // must add/remove that file: removal denies the next boot (the
    // dispatcher falls through to enrollment, which denies disabled
    // machines), regeneration restores it.
    let will_be_enabled = request.enabled.unwrap_or(existing_client.enabled);
    if existing_client.enabled && !will_be_enabled {
        // Remove BEFORE persisting: if the file cannot go, the record
        // must stay enabled so disk and database cannot disagree about
        // whether this machine boots.
        crate::infrastructure::dhcp::remove_client_ipxe_menu(
            &state.settings.read().await.clone(),
            &existing_client.mac,
        )
        .await
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    status: 500,
                    error: format!(
                        "Failed to remove boot menu for '{}': {}",
                        existing_client.name, error
                    ),
                }),
            )
        })?;
    } else if will_be_enabled {
        // Best-effort: a missing menu self-heals on the next boot via the
        // enrollment redirect path, so regeneration must not fail saves.
        // Runs on enable transitions AND auth changes so the embedded
        // CHAP credentials never go stale.
        if existing_client
            .target_iqn
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
        {
            publish_boot_menu(&state, &existing_client.id).await;
        }
    }

    // Live-apply an auth flip without a storage rebuild: existing sessions
    // persist (authentication happens at login), new logins follow the new
    // state. Skipped when no target exists yet (provisioning enforces).
    if chap_changed {
        if let Some(target_iqn) = existing_client
            .target_iqn
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            state
                .application
                .storage
                .set_target_chap(target_iqn, chap_creds.as_ref())
                .map_err(|error| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            status: 500,
                            error: format!("Failed to apply CHAP change: {}", error),
                        }),
                    )
                })?;
        }
    }

    if !boot_changed && !game_changed {
        request.block_store = existing_client.block_store.clone();
        request.block_device = existing_client.block_device.clone();
        request.target_iqn = existing_client.target_iqn.clone();
        // Persist an explicit selection even on persistence-only updates.
        if request.game_disks.is_some() {
            persist_game_selection(
                &state.db_pool,
                &id,
                request.game_disks.as_deref().unwrap_or(&[]),
            )
            .await?;
        }
        let client = manager.update(&id, request).await.map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    status: 500,
                    error: error.to_string(),
                }),
            )
        })?;
        sqlx::query("DELETE FROM client_offline_resets WHERE client_id = ? AND operation IS NULL AND ? <> ?")
            .bind(&id).bind(client.keep_writeback).bind(existing_client.keep_writeback).execute(&state.db_pool).await
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { status: 500, error: error.to_string() })))?;
        let _ = state.refresh_client_ips().await;
        refresh_dhcp(&state, &settings, "updating client").await;
        return Ok(Json(client));
    }

    if !boot_changed {
        // Game-only change: synchronize clones and LUNs without touching
        // the boot clone.
        let effective_flag = request.use_game_disk.unwrap_or(existing_game_flag);
        let effective_stored = request
            .game_disks
            .clone()
            .unwrap_or_else(|| stored_game_disks.clone());
        let resolved = resolve_effective_game_selection(&state, effective_flag, &effective_stored)
            .map_err(|error| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        status: 500,
                        error: format!("Failed to resolve game selection: {}", error),
                    }),
                )
            })?;
        let target_iqn = existing_client.target_iqn.clone().unwrap_or_else(|| {
            format!(
                "{}:client.{}",
                settings.iscsi.target_prefix,
                existing_client.name.trim().to_lowercase()
            )
        });
        state
            .application
            .storage
            .sync_game_storage(
                &id,
                &existing_client.name,
                &target_iqn,
                &resolved,
                chap_creds.as_ref(),
            )
            .map_err(|error| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        status: 500,
                        error: format!("Failed to synchronize game storage: {}", error),
                    }),
                )
            })?;
        persist_game_selection(&state.db_pool, &id, &effective_stored).await?;
        let client = manager.update(&id, request).await.map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    status: 500,
                    error: error.to_string(),
                }),
            )
        })?;
        let _ = state.refresh_client_ips().await;
        refresh_dhcp(&state, &settings, "updating client").await;
        return Ok(Json(client));
    }

    let new_name = request.name.as_deref().unwrap_or(&existing_client.name);

    let new_master = request.master.as_deref().unwrap_or(&existing_client.master);

    let new_snapshot = request.snapshot.as_deref();

    // ------------------------------------------------------------------------
    // Remove current storage first.
    //
    // This allows us to safely replace:
    //
    //     old snapshot clone
    //
    // with:
    //
    //     new snapshot clone
    //
    // or:
    //
    //     master volume
    // ------------------------------------------------------------------------

    if !is_unprovisioned_enrollment(&existing_client) {
        let current_storage = storage_from_client(&state.config, &settings, &existing_client)
            .map_err(|error| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                        error,
                    }),
                )
            })?;

        // Refuse to rebuild storage under an active session; tearing down its LUN corrupts it.
        let sessions =
            crate::infrastructure::iscsi::target_has_active_sessions(current_storage.target_iqn())
                .map_err(|error| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            status: 500,
                            error: format!("Failed to check iSCSI sessions: {}", error),
                        }),
                    )
                })?;
        crate::core::reconciliation::ensure_storage_repair_safe(
            current_storage.target_iqn(),
            sessions,
        )
        .map_err(|error| {
            (
                StatusCode::CONFLICT,
                Json(ErrorResponse {
                    status: 409,
                    error: error.to_string(),
                }),
            )
        })?;

        state
            .application
            .storage
            .destroy_client_storage(&current_storage)
            .map_err(|error| {
                tracing::error!(
                    "Failed to remove existing storage for client '{}': {:#}",
                    existing_client.name,
                    error
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                        error: format!("Failed to remove existing storage: {error:#}"),
                    }),
                )
            })?;
    }

    // ------------------------------------------------------------------------
    // Create desired new storage.
    // ------------------------------------------------------------------------

    let mut storage_spec = build_storage_spec(
        &state.config,
        &settings,
        &existing_client.id,
        new_name,
        new_master,
        new_snapshot,
        request
            .use_game_disk
            .or(existing_client.use_game_disk)
            .unwrap_or(false),
    )
    .map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                status: StatusCode::BAD_REQUEST.as_u16(),
                error,
            }),
        )
    })?;

    // Resolve the game selection for the rebuild: explicit request wins,
    // otherwise the stored selection (empty + switch on = all discovered).
    let effective_game_flag = request
        .use_game_disk
        .or(existing_client.use_game_disk)
        .unwrap_or(false);
    let effective_game_stored = request
        .game_disks
        .clone()
        .unwrap_or_else(|| stored_game_disks.clone());
    storage_spec.game_disks =
        resolve_effective_game_selection(&state, effective_game_flag, &effective_game_stored)
            .map_err(|error| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        status: 500,
                        error: format!("Failed to resolve game selection: {}", error),
                    }),
                )
            })?;

    // Enforcement travels into the rebuilt target; absence preserves the
    // legacy open portal.
    storage_spec.chap = chap_creds.clone();

    preserve_persisted_target_iqn(&mut storage_spec, &existing_client);

    let storage = state
        .application
        .storage
        .create_client_storage(&storage_spec)
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                    error: format!("Failed to provision new client storage: {}", error),
                }),
            )
        })?;

    apply_storage_to_update_request(&mut request, &storage);

    // ------------------------------------------------------------------------
    // Persist the new client configuration.
    // ------------------------------------------------------------------------

    let client = manager.update(&id, request).await.map_err(|e| {
        // Database update failed. Remove newly created storage.
        if let Err(cleanup_error) = state.application.storage.destroy_client_storage(&storage) {
            error!(
                "Failed to rollback newly created storage after database update failure: {}",
                cleanup_error
            );
        }

        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                error: format!("Failed to update client: {}", e),
            }),
        )
    })?;

    // Renaming rebuilds the initiator ACL, so the menu must use the saved identity.
    if client.enabled && client.target_iqn.as_deref().is_some() {
        publish_boot_menu(&state, &client.id).await;
    }

    // Persist the effective selection and destroy clone datasets that are
    // no longer selected (their LUNs were already pruned by provisioning).
    // Clone cleanup is best-effort: the boot storage above is healthy, and
    // orphans are retried on the next sync.
    persist_game_selection(&state.db_pool, &id, &effective_game_stored).await?;
    if let Err(error) = state
        .application
        .storage
        .destroy_game_clones_except(&id, &storage_spec.game_disks)
    {
        tracing::warn!(
            "Failed to destroy deselected game clones for client '{}': {}",
            client.name,
            error
        );
    }

    info!("Updated client: {}", client.name);

    // Refresh IP cache.
    if let Err(e) = state.refresh_client_ips().await {
        tracing::warn!("Failed to refresh client IPs cache: {}", e);
    }

    // Regenerate DHCP.
    refresh_dhcp(&state, &settings, "updating client").await;

    Ok(Json(client))
}

#[derive(Debug, serde::Deserialize)]
pub struct LegacySuperExitRequest {
    /// Fully qualified selected assignment, such as `pool/windows@ready`.
    pub snapshot: String,
}

/// Explicitly move an unmanaged legacy Super client to a selected image snapshot.
pub async fn exit_legacy_super(
    State(state): State<AppState>,
    Extension(claims): Extension<crate::types::Claims>,
    Path(id): Path<String>,
    Json(request): Json<LegacySuperExitRequest>,
) -> Result<Json<crate::domain::Client>, (StatusCode, Json<ErrorResponse>)> {
    if claims.role != "admin" {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                status: 403,
                error: "Only administrators can exit legacy Super mode".into(),
            }),
        ));
    }
    let current = ClientManager::new(state.db_pool.clone())
        .get(&id)
        .await
        .map_err(|_| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    status: 404,
                    error: "Client not found".into(),
                }),
            )
        })?;
    if crate::utils::network::get_client_status_realtime(current.ip.clone()) != "Offline" {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                status: 409,
                error: "Legacy Super exit requires the client to be offline".into(),
            }),
        ));
    }
    state
        .super_client_service()
        .await
        .exit_legacy(&id, &request.snapshot)
        .await
        .map(Json)
        .map_err(|error| {
            let detail = format!("{error:#}");
            let conflict = detail.contains("offline")
                || detail.contains("active iSCSI")
                || detail.contains("mounted")
                || detail.contains("pending")
                || detail.contains("legacy Super")
                || detail.contains("snapshot");
            let status = if conflict {
                StatusCode::CONFLICT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (
                status,
                Json(ErrorResponse {
                    status: status.as_u16(),
                    error: if conflict {
                        "Legacy Super exit conflicts with the client's current state".into()
                    } else {
                        "Legacy Super exit could not be completed".into()
                    },
                }),
            )
        })
}

// ============================================================================
// Delete client
// ============================================================================

/// Rotate a client's iSCSI CHAP secret without a storage rebuild.
///
/// The username is stable; only the secret changes. Enforcement turns on
/// as a side effect (rotating for an open target would be pointless), the
/// new credentials apply live to the target, and the boot menu is
/// republished so the next boot carries them. Existing sessions persist;
/// only new logins need the new secret.
pub async fn rotate_client_chap(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let _client_guard = state.client_mutations.lock_client(&id).await;
    let err = |error: String| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { status: 500, error }),
        )
    };

    let manager = ClientManager::new(state.db_pool.clone());
    let existing = manager.get(&id).await.map_err(|_| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                status: 404,
                error: format!("Client not found: {}", id),
            }),
        )
    })?;

    let username = existing
        .chap_user
        .clone()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| {
            crate::infrastructure::iscsi::ChapCredentials::username_for_client(&existing.name)
        });
    let mut creds = crate::infrastructure::iscsi::ChapCredentials::generate(&existing.name)
        .map_err(|error| err(format!("Failed to generate CHAP secret: {}", error)))?;
    creds.username = username.clone();

    sqlx::query("UPDATE clients SET chap_user = ?, chap_secret = ?, chap_enabled = 1 WHERE id = ?")
        .bind(&creds.username)
        .bind(&creds.password)
        .bind(&id)
        .execute(&state.db_pool)
        .await
        .map_err(|error| err(format!("Failed to persist CHAP secret: {}", error)))?;

    if let Some(target_iqn) = existing
        .target_iqn
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        state
            .application
            .storage
            .set_target_chap(target_iqn, Some(&creds))
            .map_err(|error| err(format!("Failed to apply CHAP secret: {}", error)))?;

        publish_boot_menu(&state, &existing.id).await;
    }

    tracing::info!(
        client_id = %id,
        user = %creds.username,
        "rotated iSCSI CHAP secret"
    );
    Ok(Json(serde_json::json!({
        "message": format!("CHAP secret rotated for '{}'", existing.name),
        "username": creds.username,
        "password": creds.password,
        "chap_enabled": true,
    })))
}

/// Ask an unprovisioned client to boot WinPE for NIC inventory on its next
/// enrollment. The flag is consumed when the PXE handler dispatches the
/// inventory boot; the next boot returns to normal dispatch.
pub async fn request_inventory_boot(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let manager = ClientManager::new(state.db_pool.clone());
    let existing = manager.get(&id).await.map_err(|_| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                status: 404,
                error: format!("Client not found: {}", id),
            }),
        )
    })?;
    if existing
        .target_iqn
        .as_deref()
        .is_some_and(|iqn| !iqn.trim().is_empty())
    {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                status: 409,
                error: "client is already provisioned; inventory boot is for unprovisioned clients"
                    .to_string(),
            }),
        ));
    }
    sqlx::query("UPDATE clients SET inventory_boot_requested = 1 WHERE id = ?")
        .bind(&id)
        .execute(&state.db_pool)
        .await
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    status: 500,
                    error: format!("Failed to request inventory boot: {error}"),
                }),
            )
        })?;
    tracing::info!(client_id = %id, "requested WinPE NIC inventory boot");
    Ok(Json(serde_json::json!({
        "message": format!("Inventory boot requested for '{}'", existing.name),
    })))
}

#[cfg(test)]
mod error_contract_tests {
    use super::ErrorResponse;
    use axum::{body::to_bytes, http::StatusCode, response::IntoResponse};

    fn admin_claims() -> crate::types::Claims {
        crate::types::Claims {
            sub: "admin".into(),
            username: "admin".into(),
            role: "admin".into(),
            exp: i64::MAX,
            iat: 0,
            session_version: 0,
        }
    }

    #[test]
    fn super_save_selects_exact_snapshot_instead_of_default() {
        let snapshot = |name: &str, is_default| crate::core::image::Image {
            id: name.into(),
            name: name.into(),
            kind: crate::core::image::ImageKind::Snapshot,
            os_type: crate::core::image::OsType::Windows,
            size_gb: 1,
            path: "/tmp/unused".into(),
            format: crate::core::image::ImageFormat::Raw,
            status: "ready".into(),
            description: None,
            parent_id: Some("master-id".into()),
            source_snapshot: None,
            checksum: None,
            is_default,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let snapshots = vec![snapshot("old", true), snapshot("saved", false)];
        let selected =
            super::select_super_snapshot(&snapshots, "tank/windows", Some("tank/windows@saved"))
                .unwrap()
                .unwrap();
        assert_eq!(selected.name, "saved");
        assert!(
            super::select_super_snapshot(&snapshots, "tank/windows", Some("tank/other@saved"))
                .is_err()
        );
        assert!(super::select_super_snapshot(
            &snapshots,
            "tank/windows",
            Some("tank/windows@missing")
        )
        .is_err());
        assert_eq!(
            super::select_super_snapshot(&snapshots, "tank/windows", None)
                .unwrap()
                .unwrap()
                .name,
            "old"
        );
    }

    #[test]
    fn persistence_change_does_not_request_storage_recreation() {
        let client = super::Client::new(serde_json::from_value(serde_json::json!({
            "name": "PC001", "mac": "00:11:22:33:44:55", "ip": "192.168.1.101", "master": "pool/master", "snapshot": "pool/master@ready", "keep_writeback": false
        })).unwrap()).unwrap();
        let mut request: super::UpdateClientRequest = serde_json::from_value(serde_json::json!({"name": "PC001", "master": "pool/master", "snapshot": "pool/master@ready", "keep_writeback": true})).unwrap();
        assert!(!super::storage_configuration_changed(&client, &request));
        request.snapshot = Some("pool/master@new".into());
        assert!(super::storage_configuration_changed(&client, &request));
    }

    #[tokio::test]
    async fn client_errors_use_the_shared_structured_contract() {
        let response = ErrorResponse {
            status: StatusCode::BAD_REQUEST.as_u16(),
            error: "invalid client".to_string(),
        }
        .into_response();
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(payload["code"], "invalid_request");
        assert_eq!(payload["message"], "invalid client");
        assert!(payload["operation_id"].as_str().is_some());
        assert_eq!(payload["details"], serde_json::json!({}));
        assert!(payload.get("error").is_none());
    }
    async fn gate_state(
        catalog_json: Option<&str>,
    ) -> (super::AppState, crate::core::config::Settings) {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        for (id, name, os) in [
            ("img-win", "tank/image/windows", "windows"),
            ("img-linux", "tank/image/linux", "linux"),
            ("img-rev", "tank/image/windows-rev", "windows"),
        ] {
            sqlx::query("INSERT INTO images (id, name, kind, os_type, size_gb, path, format, status, created_at, updated_at) VALUES (?, ?, 'master', ?, 50, '/dev/zvol/x', 'raw', 'ready', ?, ?)")
                .bind(id)
                .bind(name)
                .bind(os)
                .bind(&now)
                .bind(&now)
                .execute(&pool)
                .await
                .unwrap();
        }
        let mut settings = crate::core::config::Settings::default();
        let root = tempfile::tempdir().unwrap();
        if let Some(catalog) = catalog_json {
            let dir = root.path().join("pxe/network-drivers");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("index.json"), catalog).unwrap();
        }
        // Keep the tempdir alive via the settings path is impossible; leak it
        // for the test lifetime instead.
        let root = root.keep();
        settings.http.root_dir = root.to_string_lossy().into_owned();
        let state = super::AppState {
            config: crate::config::RuntimeConfig::default(),
            client_mutations: crate::state::ClientMutationLocks::default(),
            settings: std::sync::Arc::new(tokio::sync::RwLock::new(settings.clone())),
            db_pool: pool.clone(),
            config_path: std::path::PathBuf::new(),
            client_ips: std::sync::Arc::new(tokio::sync::RwLock::new(Vec::new())),
            metrics_collector: std::sync::Arc::new(crate::metrics::MetricsCollector::default()),
            ssh_executor: std::sync::Arc::new(crate::ssh_executor::SshExecutor::new()),
            application: std::sync::Arc::new(crate::application::ApplicationServices::new(pool)),
        };
        (state, settings)
    }

    fn gate_request(master: &str) -> super::CreateClientRequest {
        super::CreateClientRequest {
            name: "PC001".to_string(),
            mac: "02:00:00:00:00:09".to_string(),
            ip: "192.0.2.9".to_string(),
            master: master.to_string(),
            snapshot: Some("snap".to_string()),
            keep_writeback: None,
            use_game_disk: None,
            game_disks: None,
            chap_enabled: None,
            block_store: None,
            block_device: None,
            target_iqn: None,
        }
    }

    fn catalog_with(package_id: &str, hardware_id: &str) -> String {
        serde_json::json!({
            "version": 2,
            "drivers": [{
                "id": package_id,
                "name": package_id,
                "service_name": null,
                "driver_name": null,
                "pnp_device_id": null,
                "guid": null,
                "mac_address": null,
                "inf_files": ["nic.inf"],
                "provider": null,
                "version": null,
                "architectures": ["x64"],
                "hardware_ids": [hardware_id],
                "compatible_ids": [],
                "service_names": [],
                "catalog_files": ["nic.cat"],
                "imported_at": "2026-01-01T00:00:00Z",
            }],
        })
        .to_string()
    }

    async fn ready_revision(pool: &sqlx::SqlitePool, packages: &[&str]) -> String {
        let owned: Vec<String> = packages.iter().map(|value| value.to_string()).collect();
        let revision = crate::application::windows_servicing::ensure_managed_revision(
            pool, "img-win", "snap", &owned,
        )
        .await
        .unwrap();
        crate::persistence::repositories::image::ImageRepository::new(pool.clone())
            .transition_driver_revision(
                &revision.id,
                crate::core::image::DriverRevisionStatus::AwaitingTest,
                Some("img-rev"),
            )
            .await
            .unwrap();
        crate::persistence::repositories::image::ImageRepository::new(pool.clone())
            .transition_driver_revision(
                &revision.id,
                crate::core::image::DriverRevisionStatus::Ready,
                None,
            )
            .await
            .unwrap();
        sqlx::query("INSERT INTO images (id, name, kind, os_type, size_gb, path, format, status, parent_id, created_at, updated_at) VALUES ('img-rev-snapshot', 'serviced-test', 'snapshot', 'windows', 50, '', 'raw', 'ready', 'img-rev', ?, ?)")
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT OR IGNORE INTO clients (id, name, mac, ip, master, enabled, created_at, updated_at) VALUES ('boot-test-client', 'Boot Test Client', '02:00:00:00:00:99', '192.0.2.99', 'img-win', 1, 'now', 'now')")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO windows_revision_boot_tests (id, revision_id, test_client_id, expected_nic_id, candidate_snapshot, status, reported_revision, reported_nic_id, result_at, created_at) VALUES (?, ?, 'boot-test-client', 'PCI\\VEN_8086&DEV_15F3', 'img-rev@serviced-test', 'passed', ?, 'PCI\\VEN_8086&DEV_15F3', 'now', 'now')")
            .bind(format!("passed-{}", revision.id))
            .bind(&revision.id)
            .bind(&revision.id)
            .execute(pool)
            .await
            .unwrap();
        revision.id
    }

    #[tokio::test]
    async fn gate_passes_non_windows_and_unknown_masters_untouched() {
        let (state, settings) = gate_state(None).await;
        for master in ["tank/image/linux", "tank/image/missing"] {
            let mut request = gate_request(master);
            super::gate_windows_assignment(&state, &settings, &mut request)
                .await
                .unwrap();
            assert_eq!(request.master, master);
            assert_eq!(request.snapshot.as_deref(), Some("snap"));
        }
    }

    #[tokio::test]
    async fn gate_refuses_windows_client_without_inventory_and_creates_nothing() {
        let (state, settings) = gate_state(None).await;
        let mut request = gate_request("tank/image/windows");
        let (status, message) = super::gate_windows_assignment(&state, &settings, &mut request)
            .await
            .unwrap_err();
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(message.contains("02:00:00:00:00:09"), "{message}");
        // Refusal happens before any storage or client side effect.
        let clients: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM clients")
            .fetch_one(&state.db_pool)
            .await
            .unwrap();
        assert_eq!(clients, 0);
    }

    #[tokio::test]
    async fn gate_assigns_newest_ready_revision_for_covered_boot_nic() {
        let (state, settings) =
            gate_state(Some(&catalog_with("pkg-1", "PCI\\VEN_8086&DEV_15F3"))).await;
        sqlx::query("INSERT INTO windows_nic_inventory (mac, boot_nic_ids, observed_at) VALUES ('02:00:00:00:00:09', '[\"PCI\\\\VEN_8086&DEV_15F3\"]', '2026-01-01T00:00:00Z')")
            .execute(&state.db_pool)
            .await
            .unwrap();
        ready_revision(&state.db_pool, &["pkg-1"]).await;
        let mut request = gate_request("tank/image/windows");
        super::gate_windows_assignment(&state, &settings, &mut request)
            .await
            .unwrap();
        assert_eq!(request.master, "tank/image/windows-rev");
        assert_eq!(
            request.snapshot.as_deref(),
            Some("tank/image/windows-rev@serviced-test")
        );
        let first = super::build_storage_spec(
            &state.config,
            &settings,
            "id-1",
            "PC001",
            &request.master,
            request.snapshot.as_deref(),
            false,
        )
        .unwrap();
        let second = super::build_storage_spec(
            &state.config,
            &settings,
            "id-2",
            "PC002",
            &request.master,
            request.snapshot.as_deref(),
            false,
        )
        .unwrap();
        assert_eq!(
            first.source,
            crate::domain::storage::StorageSource::Snapshot(
                "tank/image/windows-rev@serviced-test".into()
            )
        );
        assert_eq!(second.source, first.source);
        assert_ne!(first.dataset, second.dataset);
        assert_ne!(first.dataset, request.master);
        assert_ne!(second.dataset, request.master);
    }

    #[tokio::test]
    async fn explicit_revision_requires_matching_inventory_and_private_storage() {
        let (state, settings) =
            gate_state(Some(&catalog_with("pkg-1", "PCI\\VEN_8086&DEV_15F3"))).await;
        ready_revision(&state.db_pool, &["pkg-1"]).await;
        let mut request = gate_request("tank/image/windows-rev");
        request.snapshot = None;
        assert!(
            super::gate_windows_assignment(&state, &settings, &mut request)
                .await
                .is_err()
        );
        sqlx::query(
            "INSERT INTO windows_nic_inventory (mac, boot_nic_ids, observed_at) VALUES (?, ?, ?)",
        )
        .bind(&request.mac)
        .bind(serde_json::json!(["PCI\\VEN_8086&DEV_15F3"]).to_string())
        .bind("2026-01-01T00:00:00Z")
        .execute(&state.db_pool)
        .await
        .unwrap();
        super::gate_windows_assignment(&state, &settings, &mut request)
            .await
            .unwrap();
        assert_eq!(
            request.snapshot.as_deref(),
            Some("tank/image/windows-rev@serviced-test")
        );
    }

    #[tokio::test]
    async fn pending_enrollment_update_requires_windows_inventory_before_storage() {
        let (state, _) = gate_state(None).await;
        let mut pending = gate_request("pending");
        pending.snapshot = None;
        let existing = super::ClientManager::new(state.db_pool.clone())
            .create(pending)
            .await
            .unwrap();
        // This pool is deliberately outside the configured pool, so the broken
        // path refuses validation before it can call privileged infrastructure.
        let request =
            serde_json::from_value(serde_json::json!({"master": "tank/image/windows"})).unwrap();
        let (status, _) = super::update_client(
            axum::extract::State(state),
            axum::extract::Path(existing.id),
            axum::Extension(admin_claims()),
            axum::Json(request),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn generic_toggle_cannot_enable_super_for_a_revision_image() {
        let (state, _) = gate_state(None).await;
        ready_revision(&state.db_pool, &["pkg-1"]).await;
        let mut request = gate_request("tank/image/windows-rev");
        request.snapshot = Some("tank/image/windows-rev@serviced-test".into());
        let existing = super::ClientManager::new(state.db_pool.clone())
            .create(request)
            .await
            .unwrap();
        let request =
            serde_json::from_value(serde_json::json!({"action": "super", "make_super": true}))
                .unwrap();
        let (status, error) = super::update_client(
            axum::extract::State(state),
            axum::extract::Path(existing.id),
            axum::Extension(admin_claims()),
            axum::Json(request),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(error.0.error.contains("managed Super session"));
    }

    #[tokio::test]
    async fn generic_client_update_cannot_enable_legacy_shared_master_super_mode() {
        let (state, _) = gate_state(None).await;
        let mut request = gate_request("tank/image/linux");
        request.snapshot = Some("tank/image/linux@ready".into());
        let existing = super::ClientManager::new(state.db_pool.clone())
            .create(request)
            .await
            .unwrap();
        let request =
            serde_json::from_value(serde_json::json!({"action": "super", "make_super": true}))
                .unwrap();

        let (status, error) = super::update_client(
            axum::extract::State(state.clone()),
            axum::extract::Path(existing.id.clone()),
            axum::Extension(admin_claims()),
            axum::Json(request),
        )
        .await
        .expect_err("generic update must refuse the unsafe legacy Super toggle");

        assert_eq!(status, StatusCode::CONFLICT);
        assert!(error.0.error.contains("managed Super session"));
        let unchanged = super::ClientManager::new(state.db_pool.clone())
            .get(&existing.id)
            .await
            .unwrap();
        assert!(!unchanged.is_super_mode());
    }

    #[tokio::test]
    async fn disabling_super_is_refused_for_a_client_booting_a_snapshot_clone() {
        let (state, _) = gate_state(None).await;
        let mut request = gate_request("tank/image/windows");
        request.snapshot = Some("tank/image/windows@clean".into());
        let existing = super::ClientManager::new(state.db_pool.clone())
            .create(request)
            .await
            .unwrap();
        assert!(!existing.is_super_client());

        let request =
            serde_json::from_value(serde_json::json!({"action": "super", "make_super": false}))
                .unwrap();
        let (status, error) = super::update_client(
            axum::extract::State(state),
            axum::extract::Path(existing.id),
            axum::Extension(admin_claims()),
            axum::Json(request),
        )
        .await
        .err()
        .unwrap();

        assert_eq!(status, StatusCode::CONFLICT);
        assert!(
            error.0.error.contains("snapshot clone"),
            "{:?}",
            error.0.error
        );
    }

    #[tokio::test]
    async fn a_direct_image_assignment_is_the_super_client() {
        let (state, _) = gate_state(None).await;
        let mut request = gate_request("tank/image/windows");
        request.snapshot = None;
        let existing = super::ClientManager::new(state.db_pool.clone())
            .create(request)
            .await
            .unwrap();
        assert!(existing.is_super_client());
        assert!(existing.is_direct_image_assignment());
        assert!(!existing.is_super_mode());
    }

    #[tokio::test]
    async fn explicit_revision_refuses_missing_or_ambiguous_immutable_snapshot() {
        for ambiguous in [false, true] {
            let (state, settings) =
                gate_state(Some(&catalog_with("pkg-1", "PCI\\VEN_8086&DEV_15F3"))).await;
            ready_revision(&state.db_pool, &["pkg-1"]).await;
            sqlx::query("INSERT INTO windows_nic_inventory (mac, boot_nic_ids, observed_at) VALUES (?, ?, ?)")
                .bind("02:00:00:00:00:09")
                .bind(serde_json::json!(["PCI\\VEN_8086&DEV_15F3"]).to_string())
                .bind("2026-01-01T00:00:00Z")
                .execute(&state.db_pool).await.unwrap();
            if ambiguous {
                sqlx::query("INSERT INTO images (id, name, kind, os_type, size_gb, path, format, status, parent_id, created_at, updated_at) VALUES ('second', 'serviced-other', 'snapshot', 'windows', 50, '', 'raw', 'ready', 'img-rev', ?, ?)")
                    .bind(chrono::Utc::now().to_rfc3339()).bind(chrono::Utc::now().to_rfc3339()).execute(&state.db_pool).await.unwrap();
            } else {
                sqlx::query("DELETE FROM images WHERE id = 'img-rev-snapshot'")
                    .execute(&state.db_pool)
                    .await
                    .unwrap();
            }
            let mut request = gate_request("tank/image/windows-rev");
            let (status, _) = super::gate_windows_assignment(&state, &settings, &mut request)
                .await
                .unwrap_err();
            assert_eq!(status, StatusCode::CONFLICT);
        }
    }

    #[tokio::test]
    async fn gate_names_missing_hardware_id_for_uncovered_boot_nic() {
        let (state, settings) =
            gate_state(Some(&catalog_with("pkg-1", "PCI\\VEN_ABCD&DEV_EF01"))).await;
        sqlx::query("INSERT INTO windows_nic_inventory (mac, boot_nic_ids, observed_at) VALUES ('02:00:00:00:00:09', '[\"PCI\\\\VEN_8086&DEV_15F3\"]', '2026-01-01T00:00:00Z')")
            .execute(&state.db_pool)
            .await
            .unwrap();
        ready_revision(&state.db_pool, &["pkg-1"]).await;
        let mut request = gate_request("tank/image/windows");
        let (status, message) = super::gate_windows_assignment(&state, &settings, &mut request)
            .await
            .unwrap_err();
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(message.contains("VEN_8086"), "{message}");
    }
}
