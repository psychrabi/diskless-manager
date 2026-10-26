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
        StagedDriverPackage, WindowsBootArmConfig, WindowsDriverInstallStrategy,
        WindowsImagePreparationRequest, WindowsImagePreparationResult, WindowsImagePreparer,
    },
    state::AppState,
    types::Claims,
};
use anyhow::Context;
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

#[derive(Debug, Deserialize)]
pub struct PrepareWindowsAssignmentRequest {
    pub image_name: String,
    pub snapshot: Option<String>,
    pub mac: String,
}

#[derive(Debug, Serialize)]
pub struct WindowsAssignmentPreparation {
    pub master: String,
    pub snapshot: Option<String>,
    pub prepared: bool,
    pub reused: bool,
    pub already_covered: bool,
}

fn normalize_mac(value: &str) -> String {
    value
        .chars()
        .filter(|character| !matches!(character, ':' | '-'))
        .flat_map(char::to_lowercase)
        .collect()
}

async fn load_assignment_boot_inventory(
    pool: &sqlx::SqlitePool,
    target_mac: &str,
) -> anyhow::Result<Option<(Vec<String>, String)>> {
    let normalized_mac = normalize_mac(target_mac);
    let report: Option<(String, String)> = sqlx::query_as(
        "SELECT report.boot_nic_ids, report.architecture FROM windows_driver_reports AS report JOIN clients AS client ON client.id = report.client_id WHERE lower(replace(replace(client.mac, ':', ''), '-', '')) = ? LIMIT 1",
    )
    .bind(&normalized_mac)
    .fetch_optional(pool)
    .await
    .context("failed to read verified Windows Boot NIC report")?;
    let record = if let Some(report) = report {
        Some(report)
    } else {
        sqlx::query_as(
            "SELECT boot_nic_ids, architecture FROM windows_nic_inventory WHERE lower(replace(replace(mac, ':', ''), '-', '')) = ? LIMIT 1",
        )
        .bind(&normalized_mac)
        .fetch_optional(pool)
        .await
        .context("failed to read preboot Boot NIC inventory")?
    };
    record
        .map(|(ids, architecture)| {
            let ids: Vec<String> = serde_json::from_str(&ids)
                .context("stored Boot NIC hardware IDs are invalid JSON")?;
            anyhow::ensure!(!ids.is_empty(), "stored Boot NIC inventory is empty");
            Ok((ids, architecture))
        })
        .transpose()
}

async fn load_managed_driver_inventory(
    pool: &sqlx::SqlitePool,
) -> anyhow::Result<Vec<(String, Vec<String>, String)>> {
    let records: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT client.mac, inventory.boot_nic_ids, inventory.architecture FROM windows_nic_inventory AS inventory JOIN clients AS client ON lower(replace(replace(client.mac, ':', ''), '-', '')) = lower(replace(replace(inventory.mac, ':', ''), '-', '')) WHERE NOT EXISTS (SELECT 1 FROM windows_driver_reports AS report WHERE report.client_id = client.id) UNION ALL SELECT client.mac, report.boot_nic_ids, report.architecture FROM windows_driver_reports AS report JOIN clients AS client ON client.id = report.client_id ORDER BY 1",
    )
    .fetch_all(pool)
    .await
    .context("failed to read Windows client NIC inventory")?;
    records
        .into_iter()
        .map(|(mac, ids, architecture)| {
            let ids: Vec<String> = serde_json::from_str(&ids)
                .context("stored Boot NIC hardware IDs are invalid JSON")?;
            anyhow::ensure!(
                !ids.is_empty(),
                "stored Boot NIC inventory is empty for {mac}"
            );
            Ok((mac, ids, architecture))
        })
        .collect()
}

fn select_assignment_driver_ids(
    packages: &[crate::infrastructure::pxe::NetworkDriverPackage],
    inventory: &[(String, Vec<String>, String)],
    target_mac: &str,
) -> Result<Vec<String>, String> {
    let Some((_, boot_nic_ids, architecture)) = inventory.iter().find(|(mac, boot_nic_ids, _)| {
        normalize_mac(mac) == normalize_mac(target_mac) && !boot_nic_ids.is_empty()
    }) else {
        return Err(format!(
            "no Boot NIC inventory is recorded for client MAC '{target_mac}'"
        ));
    };
    select_managed_image_driver_ids(
        packages,
        boot_nic_ids,
        std::slice::from_ref(architecture),
        &[],
    )
}

/// True when the selected image is a prepared revision whose recorded driver set
/// already covers every package chosen for the target client.
///
/// Coverage, not hash equality, decides reuse: one prepared image serves every
/// machine whose Boot NIC it covers, and only an uncovered NIC triggers another
/// revision build. Unprepared images have no revision row and are never reused.
async fn existing_revision_covers_driver_set(
    pool: &sqlx::SqlitePool,
    image_id: &str,
    required_package_ids: &[String],
) -> anyhow::Result<bool> {
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT driver_package_ids FROM windows_image_driver_revisions WHERE revision_image_id = ? AND status IN ('awaiting-test', 'ready')",
    )
    .bind(image_id)
    .fetch_all(pool)
    .await
    .context("failed to inspect existing Windows image revisions")?;
    for json_ids in rows {
        let existing: BTreeSet<String> = serde_json::from_str(&json_ids).with_context(|| {
            format!("driver package list recorded for image '{image_id}' is invalid JSON")
        })?;
        if required_package_ids.iter().all(|id| existing.contains(id)) {
            return Ok(true);
        }
    }
    Ok(false)
}

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
        driver_install_strategy: WindowsDriverInstallStrategy::OfflineDism,
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
        let inventory = if driver_root {
            Vec::new()
        } else {
            load_managed_driver_inventory(&state.db_pool).await?
        };
        let explicit_ids = request.driver_ids.clone();
        let packages =
            tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<StagedDriverPackage>> {
                let plugin = NetworkDriverInjectionPlugin::new(PathBuf::from(root));
                let selected_ids = if driver_root {
                    Vec::new()
                } else {
                    let packages = plugin.list()?;
                    let mut selected_ids = BTreeSet::new();
                    for (mac, boot_nic_ids, architecture) in inventory {
                        let selected = select_managed_image_driver_ids(
                            &packages,
                            &boot_nic_ids,
                            &[architecture],
                            &[],
                        )
                        .map_err(|error| anyhow::anyhow!("client inventory {mac}: {error}"))?;
                        selected_ids.extend(selected);
                    }
                    selected_ids.extend(explicit_ids);
                    if selected_ids.is_empty() {
                        return Err(anyhow::anyhow!(
                            "no Windows Boot NIC inventory is available; inventory clients before building a managed image"
                        ));
                    }
                    selected_ids.into_iter().collect()
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

/// Prepare a Windows assignment with every catalog NIC package matching the
/// recorded client inventory. This task is detached so a closed browser does
/// not cancel a worker that is modifying its private image clone.
pub async fn prepare_windows_assignment(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<PrepareWindowsAssignmentRequest>,
) -> Result<Json<WindowsAssignmentPreparation>, (StatusCode, Json<WindowsServicingError>)> {
    require_admin(&claims)?;
    if crate::validation::validate_mac_address(&request.mac).is_err() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(WindowsServicingError {
                error: "Invalid client MAC address".to_string(),
            }),
        ));
    }
    let result = tokio::spawn(async move {
        let _guard = state
            .client_mutations
            .lock_resource(WINDOWS_IMAGE_WORKFLOW_RESOURCE)
            .await;
        let settings = state.settings.read().await.clone();
        let profile = settings.windows_servicing;
        let strategy = if profile.driver_install_strategy == "nexboot_boot_time" {
            WindowsDriverInstallStrategy::NexbootBootTime
        } else {
            WindowsDriverInstallStrategy::OfflineDism
        };
        if !profile.is_configured() {
            anyhow::bail!("Windows servicing worker is not configured in System Settings");
        }

        let images = crate::application::image_service::ImageService::new(
            crate::persistence::repositories::image::ImageRepository::new(
                state.db_pool.clone(),
            ),
        );
        let source = images.get(&request.image_name).await?;
        if source.os_type != crate::core::image::OsType::Windows
            || source.kind == crate::core::image::ImageKind::Snapshot
        {
            return Ok(WindowsAssignmentPreparation {
                master: source.name,
                snapshot: request.snapshot,
                prepared: false,
                reused: false,
                already_covered: false,
            });
        }
        let snapshot = request
            .snapshot
            .as_deref()
            .context("select a ready snapshot before preparing Windows drivers")?;
        let (snapshot_image, snapshot_name) = snapshot
            .split_once('@')
            .context("Windows driver preparation requires an image@snapshot selection")?;
        anyhow::ensure!(
            snapshot_image == source.name,
            "selected snapshot does not belong to Windows image '{}'",
            source.name
        );

        let target_mac = request.mac.clone();
        let Some((boot_nic_ids, architecture)) =
            load_assignment_boot_inventory(&state.db_pool, &target_mac).await?
        else {
            anyhow::bail!("no Boot NIC inventory is recorded for client MAC '{target_mac}'");
        };
        let inventory = vec![(target_mac.clone(), boot_nic_ids, architecture)];
        let root = settings.http.root_dir.clone();
        let (package_ids, packages) = tokio::task::spawn_blocking(move || {
            let plugin = NetworkDriverInjectionPlugin::new(PathBuf::from(root));
            let catalog = plugin.list()?;
            let ids = select_assignment_driver_ids(&catalog, &inventory, &target_mac)
                .map_err(anyhow::Error::msg)?;
            let packages = ids
                .iter()
                .map(|id| {
                    Ok(StagedDriverPackage {
                        id: id.clone(),
                        source_dir: plugin.package_directory(id)?,
                    })
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            Ok::<_, anyhow::Error>((ids, packages))
        })
        .await
        .context("driver catalog selection worker failed")??;

        // One prepared image serves every client whose Boot NIC its driver set
        // covers, so a matching client is assigned that image as-is instead of
        // cloning and servicing another near-identical revision per machine.
        if existing_revision_covers_driver_set(&state.db_pool, &source.id, &package_ids).await? {
            return Ok(WindowsAssignmentPreparation {
                master: source.name,
                snapshot: request.snapshot,
                prepared: false,
                reused: false,
                already_covered: true,
            });
        }
        let planned = crate::application::windows_servicing::plan_driver_revision_with_strategy(
            &source.id,
            snapshot_name,
            &package_ids,
            strategy,
        );

        let repository = crate::persistence::repositories::image::ImageRepository::new(
            state.db_pool.clone(),
        );
        if let Some(existing) = repository
            .find_driver_revision_by_hash(&source.id, snapshot_name, &planned.driver_set_hash)
            .await?
        {
            match existing.status {
                crate::core::image::DriverRevisionStatus::AwaitingTest
                | crate::core::image::DriverRevisionStatus::Ready => {
                    let candidate_id = existing
                        .revision_image_id
                        .context("prepared Windows revision has no image")?;
                    let rows: Vec<(String, String)> = sqlx::query_as(
                        "SELECT image.name, snapshot.name FROM images AS image JOIN images AS snapshot ON snapshot.parent_id = image.id AND snapshot.kind = 'snapshot' AND snapshot.status = 'ready' AND snapshot.name LIKE 'serviced-%' WHERE image.id = ? AND image.status = 'ready' ORDER BY snapshot.created_at DESC",
                    )
                    .bind(candidate_id)
                    .fetch_all(&state.db_pool)
                    .await
                    .context("failed to read prepared Windows revision snapshot")?;
                    anyhow::ensure!(
                        rows.len() == 1,
                        "prepared Windows revision must have one ready servicing snapshot"
                    );
                    let (master, snapshot_name) = rows.into_iter().next().context(
                        "prepared Windows revision snapshot query returned no row",
                    )?;
                    return Ok(WindowsAssignmentPreparation {
                        snapshot: Some(format!("{master}@{snapshot_name}")),
                        master,
                        prepared: true,
                        reused: true,
                        already_covered: false,
                    });
                }
                crate::core::image::DriverRevisionStatus::Preparing => {
                    anyhow::bail!(
                        "an identical Windows driver revision has been preparing since {}; wait for that build to finish, or restart the diskless-manager service to abandon an interrupted one",
                        existing.updated_at.to_rfc3339()
                    );
                }
                crate::core::image::DriverRevisionStatus::Failed => {}
            }
        }

        let helper = profile.helper_executable_path.trim().to_owned();
        let preparation =
            crate::application::windows_servicing::ManagedWindowsPreparationRequest {
                worker: RemoteWindowsCapabilitiesRequest {
                    host: profile.worker_host,
                    username: profile.worker_username,
                    password: None,
                    executable_path: (!helper.is_empty()).then_some(helper),
                },
                image_id: source.id,
                source_snapshot: snapshot_name.to_owned(),
                new_name: format!("auto-serviced-{}", uuid::Uuid::new_v4().simple()),
                portal: profile.iscsi_portal,
                driver_ids: package_ids,
                driver_root: None,
                commit: true,
                report_server_url: Some(profile.report_server_url),
                boot_arm: WindowsBootArmConfig::default(),
                driver_install_strategy: strategy,
            };
        preparation.validate()?;
        let prepared = crate::application::windows_servicing::prepare_managed(
            state.db_pool.clone(),
            Arc::clone(&state.application.iscsi),
            state.config.clone(),
            preparation,
            packages,
        )
        .await?;
        let master = prepared
            .image
            .context("Windows servicing completed without publishing an image")?
            .name;
        let snapshot = prepared
            .snapshot
            .context("Windows servicing completed without publishing a snapshot")?;
        Ok(WindowsAssignmentPreparation {
            master: master.clone(),
            snapshot: Some(format!("{master}@{}", snapshot.name)),
            prepared: true,
            reused: false,
            already_covered: false,
        })
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
    .await
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
/// last. The boot-test result controls revision readiness reporting, not image
/// assignment.
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

#[cfg(test)]
mod automatic_assignment_tests {
    use super::{
        existing_revision_covers_driver_set, load_assignment_boot_inventory,
        load_managed_driver_inventory, select_assignment_driver_ids,
    };
    use crate::infrastructure::pxe::NetworkDriverPackage;
    use sqlx::sqlite::SqlitePoolOptions;

    /// A prepared image is shared: a client whose Boot NIC packages are already
    /// injected boots it unchanged, so a mixed fleet does not need one cloned
    /// and serviced image per machine.
    #[tokio::test]
    async fn prepared_image_is_reused_by_every_covered_client() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO images (id, name, kind, os_type, size_gb, path, format, status, created_at, updated_at) VALUES ('base', 'tank/image/windows', 'master', 'windows', 50, '/dev/zvol/tank/image/windows', 'raw', 'ready', ?, ?)")
            .bind(&now)
            .bind(&now)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO windows_image_driver_revisions (id, source_image_id, source_snapshot, revision_image_id, driver_package_ids, driver_set_hash, status, created_at, updated_at) VALUES ('rev', 'base', 'installed', 'shared', '[\"nic-a\",\"nic-b\"]', 'hash', 'ready', ?, ?)")
            .bind(&now)
            .bind(&now)
            .execute(&pool)
            .await
            .unwrap();

        let covered = existing_revision_covers_driver_set(&pool, "shared", &["nic-a".to_string()])
            .await
            .unwrap();
        assert!(covered, "a superset of the client's packages is enough");
        let covered = existing_revision_covers_driver_set(
            &pool,
            "shared",
            &["nic-a".to_string(), "nic-b".to_string()],
        )
        .await
        .unwrap();
        assert!(covered, "an identical package set is covered");
        let uncovered = existing_revision_covers_driver_set(
            &pool,
            "shared",
            &["nic-a".to_string(), "nic-c".to_string()],
        )
        .await
        .unwrap();
        assert!(!uncovered, "a missing package needs a new revision");
        let unprepared = existing_revision_covers_driver_set(&pool, "base", &["nic-a".to_string()])
            .await
            .unwrap();
        assert!(!unprepared, "the unprotected source image is never reused");
    }

    #[test]
    fn automatic_assignment_ignores_other_clients_with_unmatched_nics() {
        let packages: Vec<NetworkDriverPackage> = serde_json::from_value(serde_json::json!([
            {"id":"pkg-a","name":"A","inf_files":["a.inf"],"architectures":["x64"],"hardware_ids":["PCI\\VEN_1234&DEV_0001"],"catalog_files":["a.cat"],"imported_at":"2026-01-01T00:00:00Z"},
            {"id":"pkg-b","name":"B","inf_files":["b.inf"],"architectures":["x64"],"hardware_ids":["PCI\\VEN_1234&DEV_0002"],"catalog_files":["b.cat"],"imported_at":"2026-01-01T00:00:00Z"}
        ]))
        .unwrap();
        let inventory = vec![
            (
                "02:00:00:00:00:01".into(),
                vec!["PCI\\VEN_9999&DEV_9999".into()],
                "x64".into(),
            ),
            (
                "02:00:00:00:00:02".into(),
                vec!["PCI\\VEN_1234&DEV_0002".into()],
                "x64".into(),
            ),
        ];

        assert_eq!(
            select_assignment_driver_ids(&packages, &inventory, "02-00-00-00-00-02").unwrap(),
            vec!["pkg-b"]
        );
    }

    #[tokio::test]
    async fn assignment_prefers_verified_client_report_to_stale_preboot_inventory() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE clients (id TEXT PRIMARY KEY, mac TEXT NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE windows_driver_reports (client_id TEXT, boot_nic_ids TEXT, architecture TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE windows_nic_inventory (mac TEXT, boot_nic_ids TEXT, architecture TEXT)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO clients (id, mac) VALUES ('client-1', '02:00:00:00:00:01')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO windows_driver_reports (client_id, boot_nic_ids, architecture) VALUES ('client-1', '[\"PCI\\\\VEN_1234&DEV_0002\"]', 'x64')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO windows_nic_inventory (mac, boot_nic_ids, architecture) VALUES ('02:00:00:00:00:01', '[\"PCI\\\\VEN_1234&DEV_0001\"]', 'x64')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO windows_nic_inventory (mac, boot_nic_ids, architecture) VALUES ('02:00:00:00:00:99', '[\"PCI\\\\VEN_9999&DEV_9999\"]', 'x64')")
            .execute(&pool)
            .await
            .unwrap();

        let inventory = load_assignment_boot_inventory(&pool, "02-00-00-00-00-01")
            .await
            .unwrap()
            .unwrap();

        assert_eq!(inventory.0, vec!["PCI\\VEN_1234&DEV_0002"]);
        assert_eq!(inventory.1, "x64");

        let managed_inventory = load_managed_driver_inventory(&pool).await.unwrap();
        assert_eq!(managed_inventory.len(), 1);
        assert_eq!(managed_inventory[0].0, "02:00:00:00:00:01");
        assert_eq!(managed_inventory[0].1, vec!["PCI\\VEN_1234&DEV_0002"]);
    }
}
