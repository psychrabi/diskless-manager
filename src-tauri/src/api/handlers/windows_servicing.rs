//! Windows image servicing API.
//!
//! These endpoints are admin-only because they mount and mutate offline Windows
//! images on the host. Dry-run preparation still invokes DISM and therefore
//! remains privileged.

use crate::{
    infrastructure::pxe::{
        select_managed_image_driver_ids, windows_servicing_available, NetworkDriverInjectionPlugin,
        RemoteWindowsCapabilitiesRequest, RemoteWindowsCatalogPreparationResult,
        RemoteWindowsServicer, RemoteWindowsServicingCapabilities, RemoteWindowsServicingRequest,
        StagedDriverPackage, WindowsBootArmConfig, WindowsImagePreparationRequest,
        WindowsImagePreparationResult, WindowsImagePreparer,
    },
    state::AppState,
    types::Claims,
};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

const WINDOWS_IMAGE_WORKFLOW_RESOURCE: &str = "windows-image-revisions";

#[derive(Debug, Serialize)]
pub struct WindowsServicingCapabilities {
    pub available: bool,
    pub host_platform: &'static str,
}

#[derive(Debug, Serialize)]
pub struct WindowsServicingError {
    pub error: String,
}

fn default_image_index() -> u32 {
    1
}

#[derive(Debug, Deserialize)]
pub struct CatalogWindowsPreparationRequest {
    pub host: String,
    pub username: String,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub executable_path: Option<String>,
    pub image_path: PathBuf,
    #[serde(default = "default_image_index")]
    pub image_index: u32,
    #[serde(default)]
    pub commit: bool,
    #[serde(default)]
    pub boot_arm: WindowsBootArmConfig,
    pub driver_ids: Vec<String>,
}

fn require_admin(claims: &Claims) -> Result<(), (StatusCode, Json<WindowsServicingError>)> {
    if claims.role != "admin" {
        return Err((
            StatusCode::FORBIDDEN,
            Json(WindowsServicingError {
                error: "Forbidden: admin role required".to_string(),
            }),
        ));
    }
    Ok(())
}

pub async fn capabilities(
    Extension(claims): Extension<Claims>,
) -> Result<Json<WindowsServicingCapabilities>, (StatusCode, Json<WindowsServicingError>)> {
    require_admin(&claims)?;

    let available = tokio::task::spawn_blocking(windows_servicing_available)
        .await
        .map_err(|error| internal_message(error.to_string()))?;

    Ok(Json(WindowsServicingCapabilities {
        available,
        host_platform: std::env::consts::OS,
    }))
}

pub async fn prepare_image(
    Extension(claims): Extension<Claims>,
    Json(request): Json<WindowsImagePreparationRequest>,
) -> Result<Json<WindowsImagePreparationResult>, (StatusCode, Json<WindowsServicingError>)> {
    require_admin(&claims)?;

    let result = tokio::task::spawn_blocking(move || {
        let preparer = WindowsImagePreparer::new()?;
        preparer.prepare(request)
    })
    .await
    .map_err(|error| internal_message(error.to_string()))?
    .map_err(operation_error)?;

    Ok(Json(result))
}

fn operation_error(error: anyhow::Error) -> (StatusCode, Json<WindowsServicingError>) {
    log::error!("Windows servicing operation failed: {error:#}");
    (
        StatusCode::BAD_REQUEST,
        Json(WindowsServicingError {
            error: format!("{error:#}"),
        }),
    )
}

fn internal_message(message: String) -> (StatusCode, Json<WindowsServicingError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(WindowsServicingError { error: message }),
    )
}

pub async fn remote_capabilities(
    Extension(claims): Extension<Claims>,
    Json(request): Json<RemoteWindowsCapabilitiesRequest>,
) -> Result<Json<RemoteWindowsServicingCapabilities>, (StatusCode, Json<WindowsServicingError>)> {
    require_admin(&claims)?;

    RemoteWindowsServicer::new()
        .capabilities(request)
        .await
        .map(Json)
        .map_err(operation_error)
}

pub async fn prepare_image_remote(
    Extension(claims): Extension<Claims>,
    Json(request): Json<RemoteWindowsServicingRequest>,
) -> Result<Json<WindowsImagePreparationResult>, (StatusCode, Json<WindowsServicingError>)> {
    require_admin(&claims)?;

    RemoteWindowsServicer::new()
        .prepare(request)
        .await
        .map(Json)
        .map_err(operation_error)
}

pub async fn prepare_image_remote_catalog(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<CatalogWindowsPreparationRequest>,
) -> Result<Json<RemoteWindowsCatalogPreparationResult>, (StatusCode, Json<WindowsServicingError>)>
{
    require_admin(&claims)?;

    if request.driver_ids.is_empty() {
        return Err(operation_error(anyhow::anyhow!(
            "select at least one imported network driver package"
        )));
    }
    if request.driver_ids.len() > 64 {
        return Err(operation_error(anyhow::anyhow!(
            "no more than 64 driver packages may be staged in one operation"
        )));
    }

    let unique_ids = request
        .driver_ids
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let root = state.settings.read().await.http.root_dir.clone();
    let packages =
        tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<StagedDriverPackage>> {
            let plugin = NetworkDriverInjectionPlugin::new(PathBuf::from(root));
            unique_ids
                .into_iter()
                .map(|id| {
                    let source_dir = plugin.package_directory(&id)?;
                    Ok(StagedDriverPackage { id, source_dir })
                })
                .collect()
        })
        .await
        .map_err(|error| internal_message(error.to_string()))?
        .map_err(operation_error)?;

    let preparation = WindowsImagePreparationRequest {
        image_path: request.image_path,
        driver_root: PathBuf::new(),
        mount_root: None,
        image_index: request.image_index,
        recursive: true,
        commit: request.commit,
        boot_arm: request.boot_arm,
        iscsi: None,
        reporter: None,
    };

    RemoteWindowsServicer::new()
        .prepare_with_driver_packages(
            request.host,
            request.username,
            request.password,
            request.executable_path,
            preparation,
            packages,
        )
        .await
        .map(Json)
        .map_err(operation_error)
}

/// The detached task finishes release even if the HTTP caller closes its connection.
pub async fn prepare_image_managed(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<crate::application::windows_servicing::ManagedWindowsPreparationRequest>,
) -> Result<
    Json<crate::application::windows_servicing::ManagedWindowsPreparationResult>,
    (StatusCode, Json<WindowsServicingError>),
> {
    require_admin(&claims)?;
    request.validate().map_err(operation_error)?;
    let result = tokio::spawn(async move {
        let _guard = state
            .client_mutations
            .lock_resource(WINDOWS_IMAGE_WORKFLOW_RESOURCE)
            .await;
        let settings = state.settings.read().await;
        let root = settings.http.root_dir.clone();
        drop(settings);
        let driver_root = request
            .driver_root
            .as_ref()
            .is_some_and(|path| !path.as_os_str().is_empty());
        let (boot_nic_ids, architectures) = if driver_root {
            (Vec::new(), Vec::new())
        } else {
            let inventory: Vec<(String, String)> = sqlx::query_as(
                "SELECT boot_nic_ids, architecture FROM windows_nic_inventory ORDER BY mac",
            )
            .fetch_all(&state.db_pool)
            .await?;
            let boot_nic_ids = inventory
                .iter()
                .map(|(entry, _)| serde_json::from_str::<Vec<String>>(entry))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            let architectures = inventory
                .into_iter()
                .map(|(_, architecture)| architecture)
                .filter(|architecture| !architecture.trim().is_empty())
                .collect();
            (boot_nic_ids, architectures)
        };
        let explicit_ids = request.driver_ids.clone();
        let packages =
            tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<StagedDriverPackage>> {
                let plugin = NetworkDriverInjectionPlugin::new(PathBuf::from(root));
                let selected_ids = if driver_root {
                    Vec::new()
                } else {
                    let packages = plugin.list()?;
                    select_managed_image_driver_ids(
                        &packages,
                        &boot_nic_ids,
                        &architectures,
                        &explicit_ids,
                    )
                    .map_err(anyhow::Error::msg)?
                };
                selected_ids
                    .into_iter()
                    .map(|id| {
                        let source_dir = plugin.package_directory(&id)?;
                        Ok(StagedDriverPackage { id, source_dir })
                    })
                    .collect()
            })
            .await??;
        crate::application::windows_servicing::prepare_managed(
            state.db_pool.clone(),
            Arc::clone(&state.application.iscsi),
            state.config.clone(),
            request,
            packages,
        )
        .await
    })
    .await
    .map_err(|error| internal_message(error.to_string()))?
    .map_err(operation_error)?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
pub struct RevisionListQuery {
    pub source_image_id: String,
}

#[derive(Debug, Deserialize)]
pub struct StartBootTestRequest {
    pub revision_id: String,
    pub test_client_id: String,
    pub expected_nic_id: String,
}

/// Admin-only designation of the disposable test client for one candidate
/// revision. Real hardware tests run only on this client; production
/// assignments never change here.
pub async fn start_boot_test(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<StartBootTestRequest>,
) -> Result<Json<crate::core::image::RevisionBootTest>, (StatusCode, Json<WindowsServicingError>)> {
    require_admin(&claims)?;
    let _guard = state
        .client_mutations
        .lock_client_and_resource(&request.test_client_id, WINDOWS_IMAGE_WORKFLOW_RESOURCE)
        .await;
    let candidates: Vec<(String, String)> = sqlx::query_as(
        "SELECT image.name, snapshot.name FROM windows_image_driver_revisions AS revision JOIN images AS image ON image.id = revision.revision_image_id JOIN images AS snapshot ON snapshot.parent_id = image.id AND snapshot.kind = 'snapshot' AND snapshot.name LIKE 'serviced-%' AND snapshot.status = 'ready' WHERE revision.id = ? AND revision.status = 'awaiting-test' AND image.status = 'ready' LIMIT 2",
    )
    .bind(&request.revision_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|error| internal_message(error.to_string()))?;
    let Some((candidate_master, serviced_snapshot)) = candidates.first() else {
        return Err(operation_error(anyhow::anyhow!(
            "candidate revision has no ready servicing snapshot"
        )));
    };
    if candidates.len() != 1 {
        return Err(operation_error(anyhow::anyhow!(
            "candidate revision has ambiguous servicing snapshots"
        )));
    }
    let candidate_master = candidate_master.clone();
    let candidate_snapshot = format!("{candidate_master}@{serviced_snapshot}");
    let client = state
        .application
        .clients
        .get_by_string(&request.test_client_id)
        .await
        .map_err(operation_error)?
        .ok_or_else(|| operation_error(anyhow::anyhow!("test client not found")))?;
    let expected_nic = request.expected_nic_id.trim().replace('/', "\\");
    if expected_nic.is_empty() || expected_nic.len() > 128 {
        return Err(operation_error(anyhow::anyhow!(
            "a valid expected Boot NIC hardware ID is required"
        )));
    }
    let inventory: Option<(String,)> =
        sqlx::query_as("SELECT boot_nic_ids FROM windows_nic_inventory WHERE mac = ?")
            .bind(client.mac.as_str())
            .fetch_optional(&state.db_pool)
            .await
            .map_err(|error| internal_message(error.to_string()))?;
    let boot_nic_ids: Vec<String> = inventory
        .map(|(ids,)| serde_json::from_str(&ids))
        .transpose()
        .map_err(|error| internal_message(format!("stored NIC inventory is invalid: {error}")))?
        .unwrap_or_default();
    if expected_nic.is_empty()
        || !boot_nic_ids.iter().any(|id| {
            id.trim()
                .replace('/', "\\")
                .eq_ignore_ascii_case(&expected_nic)
        })
    {
        return Err(operation_error(anyhow::anyhow!(
            "expected Boot NIC must match the designated client's inventory"
        )));
    }
    let package_ids_json: String = sqlx::query_scalar(
        "SELECT driver_package_ids FROM windows_image_driver_revisions WHERE id = ?",
    )
    .bind(&request.revision_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|error| internal_message(error.to_string()))?;
    let package_ids: Vec<String> = serde_json::from_str(&package_ids_json)
        .map_err(|error| internal_message(format!("candidate package list is invalid: {error}")))?;
    let root = state.settings.read().await.http.root_dir.clone();
    let catalog = tokio::task::spawn_blocking(move || {
        NetworkDriverInjectionPlugin::new(PathBuf::from(root)).list()
    })
    .await
    .map_err(|error| internal_message(error.to_string()))?
    .map_err(|error| internal_message(format!("{error:#}")))?;
    let package_match = package_ids.iter().any(|package_id| {
        catalog
            .iter()
            .find(|package| package.id == *package_id)
            .is_some_and(|package| {
                crate::application::windows_servicing::package_matches_boot_nic(
                    package,
                    &expected_nic,
                )
            })
    });
    if !package_match {
        return Err(operation_error(anyhow::anyhow!(
            "candidate revision has no driver package matching the expected Boot NIC"
        )));
    }
    let active_test: Option<(String, String, String)> = sqlx::query_as(
        "SELECT test_client_id, expected_nic_id, candidate_snapshot FROM windows_revision_boot_tests WHERE revision_id = ? AND status = 'candidate'",
    )
    .bind(&request.revision_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|error| internal_message(error.to_string()))?;
    if active_test
        .as_ref()
        .is_some_and(|(test_client_id, nic_id, snapshot)| {
            test_client_id != &request.test_client_id
                || !nic_id.eq_ignore_ascii_case(&expected_nic)
                || snapshot != &candidate_snapshot
        })
    {
        return Err(operation_error(anyhow::anyhow!(
            "another boot test is already active for this revision"
        )));
    }
    crate::application::provisioning_service::validate_pending_boot_test_client(
        &client,
        &candidate_master,
        &candidate_snapshot,
    )
    .map_err(operation_error)?;
    let settings = state.settings.read().await.clone();
    let mut storage_spec = crate::application::client_storage_mapping::storage_spec_for_values(
        &state.config,
        &settings,
        client.id.to_string(),
        &client.name,
        &candidate_master,
        Some(&candidate_snapshot),
        client.use_game_disk,
    )
    .map_err(|error| operation_error(anyhow::anyhow!(error)))?;
    let storage = state.application.storage.clone();
    let discovered_games = tokio::task::spawn_blocking(move || storage.discover_game_masters())
        .await
        .map_err(|error| internal_message(error.to_string()))?
        .map_err(operation_error)?;
    storage_spec.game_disks = crate::application::storage_service::resolve_game_selection(
        client.use_game_disk,
        &client.game_disks,
        &discovered_games
            .into_iter()
            .map(|master| master.dataset)
            .collect::<Vec<_>>(),
    );
    storage_spec.chap = crate::core::reconciliation::ensure_chap_credentials(
        &state.db_pool,
        client.id.as_str(),
        &client.name,
        client.chap_enabled,
    )
    .await
    .map_err(operation_error)?;
    state
        .application
        .provisioning
        .assign_pending_boot_test_client(
            client.id.as_str(),
            &candidate_master,
            &candidate_snapshot,
            storage_spec,
            &settings.dhcp.next_server_ip,
        )
        .await
        .map_err(operation_error)?;
    crate::application::windows_servicing::start_boot_test(
        &state.db_pool,
        &request.revision_id,
        &request.test_client_id,
        &request.expected_nic_id,
    )
    .await
    .map(Json)
    .map_err(operation_error)
}

#[derive(Debug, Deserialize)]
pub struct BootTestListQuery {
    pub revision_id: String,
}

/// Admin-only listing of boot tests for one candidate revision, newest last.
pub async fn list_boot_tests(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<BootTestListQuery>,
) -> Result<
    Json<Vec<crate::core::image::RevisionBootTest>>,
    (StatusCode, Json<WindowsServicingError>),
> {
    require_admin(&claims)?;
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            Option<String>,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
        ),
    >(
        r#"SELECT id, revision_id, test_client_id, expected_nic_id, candidate_image_id, candidate_snapshot, status,
        reported_revision, reported_nic_id, result_at, created_at
        FROM windows_revision_boot_tests WHERE revision_id = ? ORDER BY created_at"#,
    )
    .bind(&query.revision_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(WindowsServicingError {
                error: format!("failed to list boot tests: {error}"),
            }),
        )
    })?;
    let mut tests = Vec::with_capacity(rows.len());
    for (
        id,
        revision_id,
        test_client_id,
        expected_nic_id,
        candidate_image_id,
        candidate_snapshot,
        status,
        reported_revision,
        reported_nic_id,
        result_at,
        created_at,
    ) in rows
    {
        let parse_time = |value: &str| {
            chrono::DateTime::parse_from_rfc3339(value)
                .map(|value| value.with_timezone(&chrono::Utc))
                .map_err(|error| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(WindowsServicingError {
                            error: format!("invalid boot test timestamp: {error}"),
                        }),
                    )
                })
        };
        tests.push(crate::core::image::RevisionBootTest {
            id,
            revision_id,
            test_client_id,
            expected_nic_id,
            candidate_image_id,
            candidate_snapshot,
            status: status.parse().map_err(|error| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(WindowsServicingError {
                        error: format!("invalid boot test status: {error}"),
                    }),
                )
            })?,
            reported_revision,
            reported_nic_id,
            result_at: result_at.as_deref().map(parse_time).transpose()?,
            created_at: parse_time(&created_at)?,
        });
    }
    Ok(Json(tests))
}

/// Admin-only listing of driver revision records for one source image, newest
/// last. Existing client assignments stay pinned; only the boot-test gate
/// moves a candidate to `ready` for new clients.
pub async fn list_revisions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<RevisionListQuery>,
) -> Result<
    Json<Vec<crate::core::image::ImageDriverRevision>>,
    (StatusCode, Json<WindowsServicingError>),
> {
    require_admin(&claims)?;
    crate::persistence::repositories::image::ImageRepository::new(state.db_pool.clone())
        .list_driver_revisions(&query.source_image_id)
        .await
        .map(Json)
        .map_err(operation_error)
}
