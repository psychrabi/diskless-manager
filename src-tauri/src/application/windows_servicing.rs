//! A servicing job owns a clone and a unique target; source/client targets are never changed.
use crate::{
    application::image_service::ImageService,
    core::image::{
        DriverRevisionStatus, Image, ImageDriverRevision, ImageKind, OsType, UpdateImageRequest,
    },
    infrastructure::{
        iscsi::{ChapCredentials, IscsiProvisioner, IscsiTargetSpec},
        pxe::{
            windows_iscsi_servicing::{WindowsIscsiConnection, SERVICING_IQN_PREFIX},
            NetworkDriverPackage, RemoteWindowsCapabilitiesRequest, RemoteWindowsServicer,
            RemoteWindowsServicingRequest, StagedDriverPackage, WindowsBootArmConfig,
            WindowsImagePreparationRequest, WindowsImagePreparationResult,
        },
    },
    persistence::repositories::image::ImageRepository,
    validation::validate_zfs_name,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    net::{Ipv4Addr, UdpSocket},
    path::PathBuf,
    sync::Arc,
};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct ManagedWindowsPreparationRequest {
    #[serde(flatten)]
    pub worker: RemoteWindowsCapabilitiesRequest,
    pub image_id: String,
    pub source_snapshot: String,
    pub new_name: String,
    /// Server address reachable from the worker, not the worker's address.
    pub portal: String,
    #[serde(default)]
    pub driver_ids: Vec<String>,
    #[serde(default)]
    pub driver_root: Option<PathBuf>,
    #[serde(default)]
    pub commit: bool,
    #[serde(default)]
    pub report_server_url: Option<String>,
    #[serde(default)]
    pub boot_arm: WindowsBootArmConfig,
}

impl ManagedWindowsPreparationRequest {
    pub fn validate(&self) -> Result<()> {
        validate_zfs_name(&self.new_name)?;
        validate_zfs_name(&self.source_snapshot)?;
        let portal: Ipv4Addr = self
            .portal
            .parse()
            .context("server portal must be an IPv4 address")?;
        if portal.is_unspecified()
            || portal.is_loopback()
            || portal.is_multicast()
            || portal.is_broadcast()
        {
            bail!("server portal must be a reachable server address");
        }
        if self.driver_ids.len() > 64 {
            bail!("select at most 64 driver packages");
        }
        let folder = self
            .driver_root
            .as_ref()
            .is_some_and(|p| !p.as_os_str().is_empty());
        if folder && !self.driver_ids.is_empty() {
            bail!("choose imported driver packages or an existing worker folder, not both");
        }
        if folder && self.commit {
            bail!("managed commits require imported catalog packages so the tested revision records its drivers; the existing worker-folder path is available for dry runs only");
        }
        if self.commit && self.report_server_url.is_none() {
            bail!("a report server URL is required for managed commits");
        }
        if self.commit {
            validate_report_server_url(
                self.report_server_url
                    .as_deref()
                    .context("a report server URL is required for managed commits")?,
            )?;
        }
        Ok(())
    }
}

fn validate_report_server_url(value: &str) -> Result<()> {
    crate::infrastructure::pxe::windows_image_preparation::validate_report_origin(value)
}

#[derive(Serialize)]
pub struct ManagedWindowsPreparationResult {
    pub preparation: WindowsImagePreparationResult,
    pub image: Option<Image>,
    pub snapshot: Option<Image>,
    pub staged_package_count: usize,
    pub uploaded_bytes: u64,
    pub staging_cleanup_succeeded: bool,
    pub revision: Option<ImageDriverRevision>,
}

/// Assignment of a new client to a tested Windows image revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsAssignment {
    pub revision_id: String,
    pub revision_image_id: String,
}

/// Decide the Windows image for a new client.
///
/// - `inventory_boot_ids` is `None` when no preboot record exists for the
///   client's MAC: creation is refused with an actionable message instead of
///   silently booting an untested image.
/// - `ready_revisions` are ordered newest-first internally; the newest revision whose driver
///   set covers a Boot NIC ID wins, preserving the last known-good revision
///   for rollback by never mutating older records.
/// - A revision with an empty driver set never matches: the Boot NIC driver
///   is required, secondary adapters alone do not qualify.
///
/// Update paths never call this: existing clients stay pinned to the image
/// they were created with.
pub fn select_windows_assignment(
    client_mac: &str,
    inventory_boot_ids: Option<&[String]>,
    ready_revisions: &[ImageDriverRevision],
    passed_boot_test_nics: &std::collections::HashMap<String, Vec<String>>,
    catalog: &[NetworkDriverPackage],
) -> Result<Option<WindowsAssignment>> {
    let Some(boot_ids) = inventory_boot_ids else {
        bail!(
            "no preboot NIC inventory for client MAC '{client_mac}'; request a WinPE inventory boot for this machine before creating it as a Windows client"
        );
    };
    if ready_revisions.is_empty() {
        bail!(
            "no tested Windows image revision is available; build a candidate revision and pass its boot test before creating Windows clients"
        );
    }
    let by_id: std::collections::HashMap<&str, &NetworkDriverPackage> = catalog
        .iter()
        .map(|package| (package.id.as_str(), package))
        .collect();
    let mut ordered: Vec<&ImageDriverRevision> = ready_revisions.iter().collect();
    ordered.sort_by_key(|revision| std::cmp::Reverse(revision.created_at));
    for revision in ordered {
        let tested_nics = passed_boot_test_nics
            .get(&revision.id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let covers = boot_ids.iter().any(|boot_id| {
            tested_nics
                .iter()
                .any(|tested| tested.eq_ignore_ascii_case(boot_id))
                && revision.driver_package_ids.iter().any(|package_id| {
                    by_id
                        .get(package_id.as_str())
                        .is_some_and(|package| package_matches_boot_nic(package, boot_id))
                })
        });
        if !covers {
            continue;
        }
        let Some(image_id) = revision.revision_image_id.clone() else {
            continue;
        };
        return Ok(Some(WindowsAssignment {
            revision_id: revision.id.clone(),
            revision_image_id: image_id,
        }));
    }
    bail!(
        "no ready revision has a passed boot test and matching driver package for Boot NIC hardware ID(s) {}; test a candidate with this exact NIC before assigning it",
        boot_ids.join(", ")
    )
}

pub fn package_matches_boot_nic(package: &NetworkDriverPackage, boot_nic_id: &str) -> bool {
    package
        .hardware_ids
        .iter()
        .chain(package.compatible_ids.iter())
        .any(|candidate| crate::infrastructure::pxe::pnp_id_matches(candidate, boot_nic_id))
}

pub async fn prepare_managed(
    pool: sqlx::SqlitePool,
    iscsi: Arc<dyn IscsiProvisioner>,
    config: crate::config::RuntimeConfig,
    request: ManagedWindowsPreparationRequest,
    packages: Vec<StagedDriverPackage>,
) -> Result<ManagedWindowsPreparationResult> {
    request.validate()?;
    UdpSocket::bind((request.portal.parse::<Ipv4Addr>()?, 0))
        .context("servicing portal is not an address on this server")?;
    let images = ImageService::with_backend(
        ImageRepository::new(pool.clone()),
        std::sync::Arc::new(crate::infrastructure::image::ZfsImageBackend::with_config(
            config,
        )),
    );
    let source = images.get(&request.image_id).await?;
    if source.os_type != OsType::Windows
        || source.kind == ImageKind::Snapshot
        || source.status != "ready"
    {
        bail!("choose a ready Windows image and one of its snapshots");
    }
    // Committed builds publish an immutable candidate revision. A repeat of an
    // already-built driver set reuses the live record instead of cloning and
    // servicing the image again; dry runs record nothing.
    let package_ids = packages
        .iter()
        .map(|package| package.id.clone())
        .collect::<Vec<_>>();
    let mut revision = if request.commit {
        Some(
            ensure_managed_revision(&pool, &source.id, &request.source_snapshot, &package_ids)
                .await?,
        )
    } else {
        None
    };
    if let Some(existing) = revision.as_ref() {
        if matches!(
            existing.status,
            DriverRevisionStatus::AwaitingTest | DriverRevisionStatus::Ready
        ) {
            bail!(
                "driver set already built as revision '{}' ({}); awaiting boot test, not rebuilding",
                existing.id,
                existing.status
            );
        }
    }
    let worker = RemoteWindowsServicer::new();
    let capabilities = worker.capabilities(request.worker.clone()).await?;
    if !capabilities.available || !capabilities.elevated || !capabilities.iscsi_volume_servicing {
        bail!("install the updated helper on an elevated Windows worker with iSCSI volume servicing support");
    }
    let clone = images
        .clone_image(&source.id, &request.source_snapshot, &request.new_name)
        .await?;
    let job = Uuid::new_v4();
    let target = format!("{SERVICING_IQN_PREFIX}{job}");
    let backstore = format!("service_{}", job.simple());
    let operation = async {
        set_status(&images, &clone.id, "preparing").await?;
        let chap = ChapCredentials::generate(&format!("servicing-{job}"))?;
        let mut spec = IscsiTargetSpec::new(&target, &backstore, &clone.path, 0);
        spec.chap = Some(chap.clone());
        if iscsi.target_exists(&target)? {
            bail!("servicing target already exists");
        }
        let size = images.get_info(&clone.id).await?.virtual_size;
        iscsi.create_target_transaction(&spec)?;
        let reporter = if request.commit {
            Some(crate::infrastructure::pxe::windows_image_preparation::WindowsReporterInstallConfig {
                server_url: request
                    .report_server_url
                    .clone()
                    .context("a report server URL is required for managed commits")?,
                export_package: true,
                image_revision: revision
                    .as_ref()
                    .context("committed preparation has no candidate revision")?
                    .id
                    .clone(),
            })
        } else {
            None
        };
        let preparation = WindowsImagePreparationRequest {
            image_path: PathBuf::new(),
            driver_root: request.driver_root.clone().unwrap_or_default(),
            mount_root: None,
            image_index: 1,
            recursive: true,
            commit: request.commit,
            boot_arm: request.boot_arm.clone(),
            iscsi: Some(WindowsIscsiConnection {
                portal: request.portal.clone(),
                target_iqn: target.clone(),
                chap_username: chap.username,
                chap_password: chap.password,
                expected_size_bytes: size,
            }),
            reporter,
        };
        let (preparation, count, bytes, cleaned) = if packages.is_empty() {
            let prepared = worker
                .prepare(RemoteWindowsServicingRequest {
                    host: request.worker.host.clone(),
                    username: request.worker.username.clone(),
                    password: request.worker.password.clone(),
                    executable_path: request.worker.executable_path.clone(),
                    preparation,
                })
                .await?;
            (prepared, 0, 0, true)
        } else {
            let result = worker
                .prepare_with_driver_packages(
                    request.worker.host.clone(),
                    request.worker.username.clone(),
                    request.worker.password.clone(),
                    request.worker.executable_path.clone(),
                    preparation,
                    packages,
                )
                .await?;
            (
                result.preparation,
                result.staged_package_count,
                result.uploaded_bytes,
                result.staging_cleanup_succeeded,
            )
        };
        // Safe provisioner refuses cleanup while a session exists. No snapshot precedes release.
        iscsi.remove_target_with_backstores(&target, std::slice::from_ref(&backstore))?;
        iscsi.remove_target(&target)?;
        let (image, snapshot) = if request.commit {
            let snapshot = images
                .create_snapshot(&clone.id, &format!("serviced-{}", job.simple()))
                .await?;
            let image = images.publish_serviced_clone(&clone.id).await?;
            (Some(image), Some(snapshot))
        } else {
            images.delete(&clone.id).await?;
            (None, None)
        };
        // Record the candidate only after the worker disconnected and all
        // owned temporary iSCSI resources were safely removed above. A
        // failure anywhere above leaves the source image untouched and
        // publishes neither an image nor a revision.
        if let Some(record) = revision.as_mut() {
            if let Some(published) = image.as_ref() {
                *record = mark_revision_awaiting_test(&pool, &record.id, &published.id).await?;
            }
        }
        Ok::<_, anyhow::Error>(ManagedWindowsPreparationResult {
            preparation,
            image,
            snapshot,
            staged_package_count: count,
            uploaded_bytes: bytes,
            staging_cleanup_succeeded: cleaned,
            revision: revision.clone(),
        })
    }
    .await;
    match operation {
        Ok(result) => Ok(result),
        Err(error) => {
            let _ = set_status(&images, &clone.id, "servicing-failed").await;
            // A failed build must not publish: mark the candidate failed so a
            // retry starts a fresh attempt instead of reusing a stale record.
            if let Some(record) = revision {
                let _ = fail_managed_revision(&pool, &record.id).await;
            }
            // An interrupted worker may still be writing. Retain the target and clone for recovery.
            Err(error.context(format!("servicing failed; retained clone '{}' and any remaining target '{}' for inspection", clone.name, target)))
        }
    }
}

async fn set_status(images: &ImageService, id: &str, status: &str) -> Result<()> {
    images
        .update(
            id,
            UpdateImageRequest {
                name: None,
                os_type: None,
                description: None,
                status: Some(status.into()),
            },
        )
        .await?;
    Ok(())
}

/// Build the deterministic candidate revision for a compatible driver set.
///
/// Pure constructor: the driver-set hash makes retries idempotent, so repeated
/// reports for the same source image and package set resolve to the same
/// revision instead of creating duplicates. The source image is never
/// modified; the revision record starts in `preparing`.
pub fn plan_driver_revision(
    source_image_id: &str,
    source_snapshot: &str,
    package_ids: &[String],
) -> ImageDriverRevision {
    let mut ordered: Vec<String> = package_ids.to_vec();
    ordered.sort();
    ordered.dedup();
    let now = chrono::Utc::now();
    ImageDriverRevision {
        id: Uuid::new_v4().to_string(),
        source_image_id: source_image_id.to_string(),
        source_snapshot: source_snapshot.to_string(),
        revision_image_id: None,
        driver_set_hash: ImageDriverRevision::hash_driver_set(&ordered),
        driver_package_ids: ordered,
        status: DriverRevisionStatus::Preparing,
        created_at: now,
        updated_at: now,
    }
}

/// Return the existing revision for this driver set or persist a new
/// `preparing` record. Idempotent by driver-set hash: failed attempts never
/// match, so a retry starts a fresh record, while a concurrent insert that
/// wins the race is returned instead of erroring.
///
/// Note: driver installation runs through DISM signature verification without
/// any unsigned-driver override (no `/ForceUnsigned` is ever passed), so DISM
/// rejects unsigned packages and the revision never publishes on failure.
pub async fn ensure_managed_revision(
    pool: &sqlx::SqlitePool,
    source_image_id: &str,
    source_snapshot: &str,
    package_ids: &[String],
) -> Result<ImageDriverRevision> {
    let repository = ImageRepository::new(pool.clone());
    let planned = plan_driver_revision(source_image_id, source_snapshot, package_ids);
    if let Some(existing) = repository
        .find_driver_revision_by_hash(source_image_id, &planned.driver_set_hash)
        .await
        .with_context(|| {
            format!("failed to check existing driver revisions for image '{source_image_id}'")
        })?
    {
        return Ok(existing);
    }
    match repository.insert_driver_revision(&planned).await {
        Ok(()) => Ok(planned),
        Err(_) => repository
            .find_driver_revision_by_hash(source_image_id, &planned.driver_set_hash)
            .await
            .with_context(|| {
                format!("failed to record driver revision for image '{source_image_id}'")
            })?
            .context("concurrent driver revision insert did not persist"),
    }
}

/// Record a failed build so it publishes nothing and never blocks a retry.
pub async fn fail_managed_revision(
    pool: &sqlx::SqlitePool,
    revision_id: &str,
) -> Result<ImageDriverRevision> {
    let repository = ImageRepository::new(pool.clone());
    repository
        .transition_driver_revision(revision_id, DriverRevisionStatus::Failed, None)
        .await
}

/// Record a successfully serviced candidate as `awaiting-test` with its
/// published image. Only the boot-test gate may later move it to `ready`,
/// keeping existing client assignments on their current revision.
pub async fn mark_revision_awaiting_test(
    pool: &sqlx::SqlitePool,
    revision_id: &str,
    revision_image_id: &str,
) -> Result<ImageDriverRevision> {
    let repository = ImageRepository::new(pool.clone());
    repository
        .transition_driver_revision(
            revision_id,
            DriverRevisionStatus::AwaitingTest,
            Some(revision_image_id),
        )
        .await
}

/// Start a boot test for a candidate revision on its designated test client.
///
/// The revision must be `awaiting-test` and the client registered. Starting
/// never changes production client assignments; it only records which
/// machine may promote the candidate by booting it.
pub async fn start_boot_test(
    pool: &sqlx::SqlitePool,
    revision_id: &str,
    test_client_id: &str,
    expected_nic_id: &str,
) -> Result<crate::core::image::RevisionBootTest> {
    use crate::core::image::{BootTestStatus, RevisionBootTest};
    let row: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT status, revision_image_id FROM windows_image_driver_revisions WHERE id = ?",
    )
    .bind(revision_id)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("failed to inspect revision '{revision_id}'"))?;
    let Some((status, candidate_image_id)) = row else {
        bail!("driver revision '{revision_id}' not found");
    };
    if status != DriverRevisionStatus::AwaitingTest.to_string() {
        bail!("only an awaiting-test revision can start a boot test (revision '{revision_id}' is '{status}')");
    }
    let candidate_image_id =
        candidate_image_id.context("candidate revision has no published image")?;
    let candidate: Option<(String, String)> = sqlx::query_as(
        "SELECT images.name, snapshots.name FROM images JOIN images AS snapshots ON snapshots.parent_id = images.id AND snapshots.kind = 'snapshot' AND snapshots.name LIKE 'serviced-%' AND snapshots.status = 'ready' WHERE images.id = ? AND images.status = 'ready'",
    ).bind(&candidate_image_id).fetch_optional(pool).await.context("failed to inspect candidate image snapshot")?;
    let Some((candidate_image_name, snapshot_name)) = candidate else {
        bail!("candidate revision must have one ready servicing snapshot");
    };
    let duplicate_snapshot: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM images WHERE parent_id = ? AND kind = 'snapshot' AND name LIKE 'serviced-%' AND status = 'ready' LIMIT 1 OFFSET 1",
    ).bind(&candidate_image_id).fetch_optional(pool).await.context("failed to verify candidate snapshot uniqueness")?;
    if duplicate_snapshot.is_some() {
        bail!("candidate revision has ambiguous servicing snapshots");
    }
    let candidate_snapshot = format!("{candidate_image_name}@{snapshot_name}");
    let assigned: Option<(String,)> =
        sqlx::query_as("SELECT id FROM clients WHERE id = ? AND master = ? AND snapshot = ?")
            .bind(test_client_id)
            .bind(&candidate_image_name)
            .bind(&candidate_snapshot)
            .fetch_optional(pool)
            .await
            .context("failed to verify test client image assignment")?;
    if assigned.is_none() {
        bail!("designated test client is not assigned to the candidate revision snapshot");
    }
    let expected = expected_nic_id.trim().replace('/', "\\");
    if expected.is_empty() || expected.len() > 128 {
        bail!("a valid expected Boot NIC hardware ID is required");
    }
    let active: Option<(String, String, String, Option<String>, String, String)> = sqlx::query_as(
        "SELECT id, test_client_id, expected_nic_id, candidate_image_id, candidate_snapshot, created_at FROM windows_revision_boot_tests WHERE revision_id = ? AND status = 'candidate'",
    )
    .bind(revision_id)
    .fetch_optional(pool)
    .await
    .context("failed to check for an active boot test")?;
    if let Some((id, active_client, active_nic, active_image, active_snapshot, created_at)) = active
    {
        if active_client != test_client_id
            || !active_nic.eq_ignore_ascii_case(&expected)
            || active_image.as_deref() != Some(candidate_image_id.as_str())
            || active_snapshot != candidate_snapshot
        {
            bail!("another boot test is already active for revision '{revision_id}'");
        }
        return Ok(RevisionBootTest {
            id,
            revision_id: revision_id.to_string(),
            test_client_id: active_client,
            expected_nic_id: active_nic,
            candidate_image_id: active_image,
            candidate_snapshot: active_snapshot,
            status: BootTestStatus::Candidate,
            reported_revision: None,
            reported_nic_id: None,
            result_at: None,
            created_at: chrono::DateTime::parse_from_rfc3339(&created_at)
                .context("active boot test has an invalid creation timestamp")?
                .with_timezone(&chrono::Utc),
        });
    }
    let now = chrono::Utc::now();
    let test = RevisionBootTest {
        id: Uuid::new_v4().to_string(),
        revision_id: revision_id.to_string(),
        test_client_id: test_client_id.to_string(),
        expected_nic_id: expected,
        candidate_image_id: Some(candidate_image_id),
        candidate_snapshot,
        status: BootTestStatus::Candidate,
        reported_revision: None,
        reported_nic_id: None,
        result_at: None,
        created_at: now,
    };
    sqlx::query(
        r#"INSERT INTO windows_revision_boot_tests
        (id, revision_id, test_client_id, expected_nic_id, candidate_image_id, candidate_snapshot, status, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&test.id)
    .bind(&test.revision_id)
    .bind(&test.test_client_id)
    .bind(&test.expected_nic_id)
    .bind(&test.candidate_image_id)
    .bind(&test.candidate_snapshot)
    .bind(test.status.to_string())
    .bind(now.to_rfc3339())
    .execute(pool)
    .await
    .map_err(|error| anyhow::anyhow!("failed to start boot test: {error}"))?;
    Ok(test)
}

/// Outcome of evaluating one boot report against its candidate test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootTestOutcome {
    Passed,
    Failed,
    NoCandidate,
}

type CandidateBootTest = (
    String,
    String,
    String,
    String,
    Option<String>,
    String,
    String,
    String,
);

/// Record a boot report against the client's open candidate test.
///
/// The caller must have already verified the reporter's active iSCSI session;
/// this function matches identity only. A pass moves the test to `passed`
/// and the revision to `ready` in one transaction, making that driver version
/// eligible for new client assignments. A mismatch marks the test `failed`
/// without changing the revision, keeping the prior revision current.
pub async fn record_boot_test_result(
    pool: &sqlx::SqlitePool,
    client_id: &str,
    reported_revision: &str,
    reported_nic_id: &str,
) -> Result<BootTestOutcome> {
    let mut transaction = pool
        .begin()
        .await
        .context("failed to record boot test result")?;
    let test: Option<CandidateBootTest> = sqlx::query_as(
        r#"SELECT tests.id, tests.revision_id, tests.test_client_id, tests.expected_nic_id,
                  tests.candidate_image_id, tests.candidate_snapshot, clients.master, COALESCE(clients.snapshot, '')
        FROM windows_revision_boot_tests AS tests
        JOIN clients ON clients.id = tests.test_client_id
        WHERE tests.test_client_id = ? AND tests.status = 'candidate'
        ORDER BY tests.created_at DESC LIMIT 1"#,
    )
    .bind(client_id)
    .fetch_optional(&mut *transaction)
    .await
    .context("failed to look up the candidate boot test")?;
    let Some((
        test_id,
        revision_id,
        _,
        expected_nic,
        candidate_image_id,
        candidate_snapshot,
        assigned_master,
        assigned_snapshot,
    )) = test
    else {
        return Ok(BootTestOutcome::NoCandidate);
    };
    let image_name: Option<(String,)> = if let Some(candidate_image_id) = &candidate_image_id {
        sqlx::query_as(
            "SELECT images.name FROM windows_image_driver_revisions AS revisions JOIN images ON images.id = revisions.revision_image_id WHERE revisions.id = ? AND revisions.revision_image_id = ?",
        )
        .bind(&revision_id)
        .bind(candidate_image_id)
        .fetch_optional(&mut *transaction)
        .await
        .context("failed to verify candidate image provenance")?
    } else {
        None
    };
    let provenance_matches = image_name.is_some_and(|(name,)| {
        !candidate_snapshot.is_empty()
            && assigned_master == name
            && assigned_snapshot == candidate_snapshot
    });
    let matches = provenance_matches
        && revision_id == reported_revision.trim()
        && expected_nic.eq_ignore_ascii_case(reported_nic_id.trim());
    let now = chrono::Utc::now().to_rfc3339();
    if matches {
        let test_update = sqlx::query(
            "UPDATE windows_revision_boot_tests SET status = 'passed', reported_revision = ?, reported_nic_id = ?, result_at = ? WHERE id = ?",
        )
        .bind(reported_revision.trim())
        .bind(reported_nic_id.trim())
        .bind(&now)
        .bind(&test_id)
        .execute(&mut *transaction)
        .await
        .context("failed to record a passing boot test")?;
        if test_update.rows_affected() != 1 {
            bail!("candidate boot test changed before it could be recorded");
        }
        let revision_update = sqlx::query(
            "UPDATE windows_image_driver_revisions SET status = 'ready', updated_at = ? WHERE id = ? AND status = 'awaiting-test'",
        )
        .bind(&now)
        .bind(&revision_id)
        .execute(&mut *transaction)
        .await
        .context("failed to promote the tested revision")?;
        if revision_update.rows_affected() != 1 {
            bail!("candidate revision changed before it could be promoted");
        }
        transaction
            .commit()
            .await
            .context("failed to record a passing boot test")?;
        tracing::info!(
            revision = %revision_id,
            client = %client_id,
            "candidate revision passed its boot test and is ready for new clients"
        );
        Ok(BootTestOutcome::Passed)
    } else {
        let test_update = sqlx::query(
            "UPDATE windows_revision_boot_tests SET status = 'failed', reported_revision = ?, reported_nic_id = ?, result_at = ? WHERE id = ?",
        )
        .bind(reported_revision.trim())
        .bind(reported_nic_id.trim())
        .bind(&now)
        .bind(&test_id)
        .execute(&mut *transaction)
        .await
        .context("failed to record a failed boot test")?;
        if test_update.rows_affected() != 1 {
            bail!("candidate boot test changed before it could be recorded");
        }
        transaction
            .commit()
            .await
            .context("failed to record a failed boot test")?;
        Ok(BootTestOutcome::Failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_managed_request_rejects_ambiguous_drivers_and_invalid_names() {
        let mut request: ManagedWindowsPreparationRequest = serde_json::from_value(serde_json::json!({
            "host":"worker", "username":"admin", "image_id":"image", "source_snapshot":"installed",
            "new_name":"windows-serviced", "portal":"192.168.123.1", "driver_ids":["netkvm"]
        })).unwrap();
        assert!(request.validate().is_ok());
        request.driver_root = Some("C:\\drivers".into());
        assert!(request.validate().is_err());
        request.driver_ids.clear();
        assert!(request.validate().is_ok());
        request.commit = true;
        assert!(request.validate().is_err());
        request.new_name = "../source".into();
        assert!(request.validate().is_err());
        request.new_name = "new-image".into();
        request.portal = "0.0.0.0".into();
        assert!(request.validate().is_err());
    }

    #[test]
    fn managed_commits_require_a_valid_report_origin_but_dry_runs_do_not() {
        let mut request: ManagedWindowsPreparationRequest = serde_json::from_value(serde_json::json!({
            "host":"worker", "username":"admin", "image_id":"image", "source_snapshot":"installed",
            "new_name":"windows-serviced", "portal":"192.168.123.1", "driver_ids":["netkvm"]
        })).unwrap();
        request.commit = true;
        assert!(request
            .validate()
            .unwrap_err()
            .to_string()
            .contains("report server URL"));
        request.report_server_url = Some("https://manager.example".into());
        assert!(request.validate().is_ok());
        request.report_server_url = Some("https://user:secret@manager.example".into());
        assert!(request.validate().is_err());
        request.report_server_url = None;
        request.commit = false;
        assert!(request.validate().is_ok());
    }
    #[tokio::test]
    async fn windows_publication_requires_a_completed_clone_and_preserves_source() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let repository = ImageRepository::new(pool.clone());
        let now = chrono::Utc::now();
        let source = Image {
            id: "source".into(),
            name: "tank/image/windows".into(),
            kind: ImageKind::Master,
            os_type: OsType::Windows,
            size_gb: 50,
            path: "/dev/zvol/tank/image/windows".into(),
            format: crate::core::image::ImageFormat::Raw,
            status: "ready".into(),
            description: None,
            parent_id: None,
            source_snapshot: None,
            checksum: None,
            is_default: false,
            created_at: now,
            updated_at: now,
        };
        repository.insert(&source).await.unwrap();
        let mut clone = source.clone();
        clone.id = "clone".into();
        clone.name = "tank/image/prepared".into();
        clone.kind = ImageKind::Clone;
        clone.parent_id = Some(source.id.clone());
        clone.source_snapshot = Some("installed".into());
        clone.status = "preparing".into();
        repository.insert(&clone).await.unwrap();
        let images = ImageService::new(repository.clone());
        assert!(images.publish_serviced_clone(&clone.id).await.is_err());
        let mut snapshot = clone.clone();
        snapshot.id = "snapshot".into();
        snapshot.name = "serviced-test".into();
        snapshot.kind = ImageKind::Snapshot;
        snapshot.parent_id = Some(clone.id.clone());
        repository.insert(&snapshot).await.unwrap();
        let published = images.publish_serviced_clone(&clone.id).await.unwrap();
        assert_eq!(published.kind, ImageKind::Master);
        assert_eq!(published.status, "ready");
        assert!(published.parent_id.is_none());
        assert_eq!(images.get("source").await.unwrap().name, source.name);
        assert!(images.publish_serviced_clone("source").await.is_err());
    }
    #[test]
    fn revision_plan_is_deterministic_and_dedupes_packages() {
        let a = plan_driver_revision("img", "snap", &["b".into(), "a".into(), "a".into()]);
        let b = plan_driver_revision("img", "snap", &["a".into(), "b".into()]);
        assert_eq!(a.driver_package_ids, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(a.driver_set_hash, b.driver_set_hash);
        assert_eq!(a.status, DriverRevisionStatus::Preparing);
        // Terminal/invalid transitions are rejected at the model level.
        assert!(a.can_transition(DriverRevisionStatus::AwaitingTest));
        assert!(!a.can_transition(DriverRevisionStatus::Ready));
    }
    #[tokio::test]
    async fn managed_revision_is_idempotent_and_gates_awaiting_test() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let repo = ImageRepository::new(pool.clone());
        let now = chrono::Utc::now();
        repo.insert(&Image {
            id: "img".into(),
            name: "tank/image/windows".into(),
            kind: ImageKind::Master,
            os_type: OsType::Windows,
            size_gb: 50,
            path: "/dev/zvol/tank/image/windows".into(),
            format: crate::core::image::ImageFormat::Raw,
            status: "ready".into(),
            description: None,
            parent_id: None,
            source_snapshot: None,
            checksum: None,
            is_default: false,
            created_at: now,
            updated_at: now,
        })
        .await
        .unwrap();
        let ids = vec!["pkg-a".to_string()];
        let first = ensure_managed_revision(&pool, "img", "snap", &ids)
            .await
            .unwrap();
        let second = ensure_managed_revision(&pool, "img", "snap", &ids)
            .await
            .unwrap();
        assert_eq!(first.id, second.id);
        let awaiting = mark_revision_awaiting_test(&pool, &first.id, "rev-image")
            .await
            .unwrap();
        assert_eq!(awaiting.status, DriverRevisionStatus::AwaitingTest);
        assert_eq!(awaiting.revision_image_id.as_deref(), Some("rev-image"));
        // Duplicate revision IDs are rejected by the primary key.
        let repo = ImageRepository::new(pool.clone());
        assert!(repo.insert_driver_revision(&first).await.is_err());
        // awaiting-test -> preparing is an invalid transition.
        assert!(repo
            .transition_driver_revision(&first.id, DriverRevisionStatus::Preparing, None)
            .await
            .is_err());
    }
    #[tokio::test]
    async fn failed_revision_never_publishes_and_never_blocks_retry() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let repo = ImageRepository::new(pool.clone());
        let now = chrono::Utc::now();
        repo.insert(&Image {
            id: "img".into(),
            name: "tank/image/windows".into(),
            kind: ImageKind::Master,
            os_type: OsType::Windows,
            size_gb: 50,
            path: "/dev/zvol/tank/image/windows".into(),
            format: crate::core::image::ImageFormat::Raw,
            status: "ready".into(),
            description: None,
            parent_id: None,
            source_snapshot: None,
            checksum: None,
            is_default: false,
            created_at: now,
            updated_at: now,
        })
        .await
        .unwrap();
        let ids = vec!["pkg-a".to_string()];
        let attempt = ensure_managed_revision(&pool, "img", "snap", &ids)
            .await
            .unwrap();
        let failed = fail_managed_revision(&pool, &attempt.id).await.unwrap();
        assert_eq!(failed.status, DriverRevisionStatus::Failed);
        // A failed attempt is invisible to the idempotency lookup...
        assert!(repo
            .find_driver_revision_by_hash("img", &attempt.driver_set_hash)
            .await
            .unwrap()
            .is_none());
        // ...so a retry starts a fresh record instead of reusing stale state.
        let retry = ensure_managed_revision(&pool, "img", "snap", &ids)
            .await
            .unwrap();
        assert_ne!(retry.id, attempt.id);
        assert_eq!(retry.status, DriverRevisionStatus::Preparing);
        // And a failed record can never become a candidate again.
        assert!(repo
            .transition_driver_revision(
                &attempt.id,
                DriverRevisionStatus::AwaitingTest,
                Some("rev-image")
            )
            .await
            .is_err());
    }

    async fn boot_test_pool() -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let now = chrono::Utc::now();
        sqlx::query("INSERT INTO images (id, name, kind, os_type, size_gb, path, format, status, description, created_at, updated_at) VALUES ('img', 'tank/image/windows', 'master', 'windows', 50, '/dev/zvol/tank/image/windows', 'raw', 'ready', NULL, ?, ?)")
            .bind(now.to_rfc3339())
            .bind(now.to_rfc3339())
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, target_iqn, created_at, updated_at) VALUES ('test-1', 'client-test-1', '02:00:00:00:00:71', '192.0.2.71', 'pending', 'iqn.2026-01.test:target.t1', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    async fn awaiting_revision(pool: &sqlx::SqlitePool) -> ImageDriverRevision {
        let revision = ensure_managed_revision(pool, "img", "snap", &["pkg-a".to_string()])
            .await
            .unwrap();
        mark_revision_awaiting_test(pool, &revision.id, "rev-image")
            .await
            .unwrap()
    }

    async fn assign_test_client(pool: &sqlx::SqlitePool, image_name: &str, snapshot: &str) {
        sqlx::query("UPDATE clients SET master = ?, snapshot = ? WHERE id = 'test-1'")
            .bind(image_name)
            .bind(snapshot)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn add_revision_servicing_snapshot(pool: &sqlx::SqlitePool) {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO images (id, name, kind, os_type, size_gb, path, format, status, description, created_at, updated_at) VALUES ('rev-image', 'tank/image/revision', 'master', 'windows', 50, '/dev/zvol/tank/image/revision', 'raw', 'ready', NULL, ?, ?)")
            .bind(&now).bind(&now).execute(pool).await.unwrap();
        sqlx::query("INSERT INTO images (id, name, kind, os_type, size_gb, path, format, status, description, parent_id, created_at, updated_at) VALUES ('rev-snapshot', 'serviced-test', 'snapshot', 'windows', 50, '/dev/zvol/tank/image/revision', 'raw', 'ready', NULL, 'rev-image', ?, ?)")
            .bind(&now).bind(&now).execute(pool).await.unwrap();
        assign_test_client(
            pool,
            "tank/image/revision",
            "tank/image/revision@serviced-test",
        )
        .await;
    }

    async fn revision_status(pool: &sqlx::SqlitePool, id: &str) -> String {
        sqlx::query_scalar::<_, String>(
            "SELECT status FROM windows_image_driver_revisions WHERE id = ?",
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn boot_test_pass_promotes_revision_for_new_clients() {
        let pool = boot_test_pool().await;
        let revision = awaiting_revision(&pool).await;
        add_revision_servicing_snapshot(&pool).await;
        let test = start_boot_test(&pool, &revision.id, "test-1", "PCI\\VEN_8086&DEV_15F3")
            .await
            .unwrap();
        assert_eq!(test.status, crate::core::image::BootTestStatus::Candidate);
        let outcome =
            record_boot_test_result(&pool, "test-1", &revision.id, "PCI\\VEN_8086&DEV_15F3")
                .await
                .unwrap();
        assert_eq!(outcome, BootTestOutcome::Passed);
        assert_eq!(revision_status(&pool, &revision.id).await, "ready");
    }

    #[tokio::test]
    async fn boot_test_start_retries_same_client_and_rejects_a_second_client() {
        let pool = boot_test_pool().await;
        let revision = awaiting_revision(&pool).await;
        add_revision_servicing_snapshot(&pool).await;
        let first = start_boot_test(&pool, &revision.id, "test-1", "PCI\\VEN_8086&DEV_15F3")
            .await
            .unwrap();
        let retry = start_boot_test(&pool, &revision.id, "test-1", "pci/ven_8086&dev_15f3")
            .await
            .unwrap();
        assert_eq!(first.id, retry.id);

        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, snapshot, target_iqn, created_at, updated_at) VALUES ('test-2', 'client-test-2', '02:00:00:00:00:72', '192.0.2.72', 'tank/image/revision', 'tank/image/revision@serviced-test', 'iqn.2026-01.test:target.t2', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            start_boot_test(&pool, &revision.id, "test-2", "PCI\\VEN_8086&DEV_15F3")
                .await
                .is_err(),
            "a second client cannot replace the active test"
        );
    }

    #[tokio::test]
    async fn boot_test_mismatch_fails_without_changing_revision() {
        for (reported_revision, reported_nic) in [
            ("other-revision", "PCI\\VEN_8086&DEV_15F3"),
            ("REV-PLACEHOLDER", "PCI\\VEN_10EC&DEV_8168"),
        ] {
            let pool = boot_test_pool().await;
            let revision = awaiting_revision(&pool).await;
            add_revision_servicing_snapshot(&pool).await;
            let reported = if reported_revision == "REV-PLACEHOLDER" {
                revision.id.clone()
            } else {
                reported_revision.to_string()
            };
            start_boot_test(&pool, &revision.id, "test-1", "PCI\\VEN_8086&DEV_15F3")
                .await
                .unwrap();
            let outcome = record_boot_test_result(&pool, "test-1", &reported, reported_nic)
                .await
                .unwrap();
            assert_eq!(
                outcome,
                BootTestOutcome::Failed,
                "report {reported}/{reported_nic}"
            );
            assert_eq!(revision_status(&pool, &revision.id).await, "awaiting-test");
        }
    }

    #[tokio::test]
    async fn changed_candidate_assignment_fails_claim_and_can_be_retried() {
        let pool = boot_test_pool().await;
        let revision = awaiting_revision(&pool).await;
        add_revision_servicing_snapshot(&pool).await;
        start_boot_test(&pool, &revision.id, "test-1", "PCI\\VEN_8086&DEV_15F3")
            .await
            .unwrap();
        sqlx::query(
            "UPDATE clients SET master = 'tank/other', snapshot = NULL WHERE id = 'test-1'",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            record_boot_test_result(&pool, "test-1", &revision.id, "PCI\\VEN_8086&DEV_15F3")
                .await
                .unwrap(),
            BootTestOutcome::Failed
        );
        assert_eq!(revision_status(&pool, &revision.id).await, "awaiting-test");

        assign_test_client(
            &pool,
            "tank/image/revision",
            "tank/image/revision@serviced-test",
        )
        .await;
        start_boot_test(&pool, &revision.id, "test-1", "PCI\\VEN_8086&DEV_15F3")
            .await
            .unwrap();
        assert_eq!(
            record_boot_test_result(&pool, "test-1", &revision.id, "PCI\\VEN_8086&DEV_15F3")
                .await
                .unwrap(),
            BootTestOutcome::Passed
        );
        assert_eq!(revision_status(&pool, &revision.id).await, "ready");
    }

    #[tokio::test]
    async fn failed_promotion_rolls_back_test_pass_update() {
        let pool = boot_test_pool().await;
        let revision = awaiting_revision(&pool).await;
        add_revision_servicing_snapshot(&pool).await;
        let test = start_boot_test(&pool, &revision.id, "test-1", "PCI\\VEN_8086&DEV_15F3")
            .await
            .unwrap();
        sqlx::query("UPDATE windows_image_driver_revisions SET status = 'ready' WHERE id = ?")
            .bind(&revision.id)
            .execute(&pool)
            .await
            .unwrap();

        assert!(
            record_boot_test_result(&pool, "test-1", &revision.id, "PCI\\VEN_8086&DEV_15F3")
                .await
                .is_err()
        );
        let status: String =
            sqlx::query_scalar("SELECT status FROM windows_revision_boot_tests WHERE id = ?")
                .bind(test.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            status, "candidate",
            "the test update must roll back when promotion loses its state race"
        );
    }

    #[tokio::test]
    async fn provenance_migration_fails_unverifiable_open_rows_and_keeps_history() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql("CREATE TABLE clients (id TEXT PRIMARY KEY, master TEXT, snapshot TEXT); CREATE TABLE images (id TEXT PRIMARY KEY, name TEXT, kind TEXT, status TEXT, parent_id TEXT); CREATE TABLE windows_image_driver_revisions (id TEXT PRIMARY KEY, revision_image_id TEXT); CREATE TABLE windows_revision_boot_tests (id TEXT PRIMARY KEY, revision_id TEXT, test_client_id TEXT, expected_nic_id TEXT, status TEXT, reported_revision TEXT, reported_nic_id TEXT, result_at TEXT, created_at TEXT);")
            .execute(&pool).await.unwrap();
        sqlx::raw_sql("INSERT INTO clients VALUES ('client', 'old-image', NULL); INSERT INTO images VALUES ('img', 'old-image', 'master', 'ready', NULL); INSERT INTO windows_image_driver_revisions VALUES ('rev', 'img'); INSERT INTO windows_revision_boot_tests VALUES ('open', 'rev', 'client', 'NIC', 'candidate', NULL, NULL, NULL, '2026-01-01'); INSERT INTO windows_revision_boot_tests VALUES ('done', 'rev', 'client', 'NIC', 'passed', 'rev', 'NIC', '2026-01-02', '2026-01-01');")
            .execute(&pool).await.unwrap();
        sqlx::raw_sql(include_str!(
            "../../migrations/0011_windows_boot_test_provenance_retries.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();

        let statuses: Vec<(String, String)> =
            sqlx::query_as("SELECT id, status FROM windows_revision_boot_tests ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            statuses,
            vec![
                ("done".into(), "passed".into()),
                ("open".into(), "failed".into())
            ]
        );
        sqlx::query("INSERT INTO windows_revision_boot_tests (id, revision_id, test_client_id, expected_nic_id, candidate_image_id, candidate_snapshot, status, created_at) VALUES ('retry', 'rev', 'client', 'NIC', 'img', 'old-image@serviced-new', 'candidate', '2026-01-03')")
            .execute(&pool).await.unwrap();
    }

    #[tokio::test]
    async fn boot_test_without_candidate_changes_nothing() {
        let pool = boot_test_pool().await;
        let revision = awaiting_revision(&pool).await;
        let outcome = record_boot_test_result(
            &pool,
            "ghost-client",
            &revision.id,
            "PCI\\VEN_8086&DEV_15F3",
        )
        .await
        .unwrap();
        assert_eq!(outcome, BootTestOutcome::NoCandidate);
        assert_eq!(revision_status(&pool, &revision.id).await, "awaiting-test");
    }

    #[tokio::test]
    async fn boot_test_start_rejects_unknown_or_unready_subjects() {
        let pool = boot_test_pool().await;
        assert!(
            start_boot_test(&pool, "missing", "test-1", "PCI\\VEN_1&DEV_2")
                .await
                .is_err()
        );
        let preparing = ensure_managed_revision(&pool, "img", "snap", &["pkg-b".to_string()])
            .await
            .unwrap();
        assert!(
            start_boot_test(&pool, &preparing.id, "test-1", "PCI\\VEN_1&DEV_2")
                .await
                .is_err()
        );
        let revision = awaiting_revision(&pool).await;
        assert!(
            start_boot_test(&pool, &revision.id, "test-1", "PCI\\VEN_1&DEV_2")
                .await
                .is_err(),
            "must reject a client that is not booting the candidate"
        );
        add_revision_servicing_snapshot(&pool).await;
        assert!(
            start_boot_test(&pool, &revision.id, "ghost", "PCI\\VEN_1&DEV_2")
                .await
                .is_err()
        );
        assert!(start_boot_test(&pool, &revision.id, "test-1", "")
            .await
            .is_err());
    }
    fn gate_package(id: &str, hardware_ids: &[&str]) -> NetworkDriverPackage {
        NetworkDriverPackage {
            id: id.to_string(),
            name: id.to_string(),
            service_name: None,
            driver_name: None,
            pnp_device_id: None,
            guid: None,
            mac_address: None,
            inf_files: vec!["nic.inf".to_string()],
            provider: None,
            version: None,
            architectures: vec!["x64".to_string()],
            hardware_ids: hardware_ids.iter().map(|value| value.to_string()).collect(),
            compatible_ids: Vec::new(),
            service_names: Vec::new(),
            catalog_files: vec!["nic.cat".to_string()],
            imported_at: chrono::Utc::now(),
        }
    }

    fn gate_revision(id: &str, packages: &[&str], image: Option<&str>) -> ImageDriverRevision {
        // Deterministic timestamps: fixture order must not decide newness.
        let now = if id.contains("new") {
            chrono::DateTime::parse_from_rfc3339("2026-02-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc)
        } else {
            chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc)
        };
        let package_ids = packages
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>();
        ImageDriverRevision {
            id: id.to_string(),
            source_image_id: "img".to_string(),
            source_snapshot: "snap".to_string(),
            revision_image_id: image.map(str::to_string),
            driver_set_hash: ImageDriverRevision::hash_driver_set(&package_ids),
            driver_package_ids: package_ids,
            status: DriverRevisionStatus::Ready,
            created_at: now,
            updated_at: now,
        }
    }

    fn passed_nic_tests(
        entries: &[(&str, &str)],
    ) -> std::collections::HashMap<String, Vec<String>> {
        let mut passed = std::collections::HashMap::new();
        for (revision_id, nic_id) in entries {
            passed
                .entry((*revision_id).to_string())
                .or_insert_with(Vec::new)
                .push((*nic_id).to_string());
        }
        passed
    }

    #[test]
    fn assignment_requires_inventory_and_names_the_mac() {
        let catalog = vec![gate_package("pkg", &["PCI\\VEN_8086&DEV_15F3"])];
        let revisions = vec![gate_revision("rev-1", &["pkg"], Some("img-1"))];
        let error = select_windows_assignment(
            "02:00:00:00:00:09",
            None,
            &revisions,
            &passed_nic_tests(&[]),
            &catalog,
        )
        .unwrap_err();
        assert!(error.to_string().contains("02:00:00:00:00:09"), "{error:#}");
        assert!(error.to_string().contains("inventory"), "{error:#}");
    }

    #[test]
    fn assignment_prefers_newest_covering_revision() {
        let catalog = vec![
            gate_package("old-pkg", &["PCI\\VEN_8086&DEV_15F3"]),
            gate_package("new-pkg", &["PCI\\VEN_8086&DEV_15F3"]),
        ];
        let revisions = vec![
            gate_revision("rev-new", &["new-pkg"], Some("img-new")),
            gate_revision("rev-old", &["old-pkg"], Some("img-old")),
        ];
        let boot = vec!["PCI\\VEN_8086&DEV_15F3".to_string()];
        let passed = passed_nic_tests(&[
            ("rev-new", "PCI\\VEN_8086&DEV_15F3"),
            ("rev-old", "PCI\\VEN_8086&DEV_15F3"),
        ]);
        let assignment =
            select_windows_assignment("mac", Some(&boot), &revisions, &passed, &catalog).unwrap();
        assert_eq!(
            assignment,
            Some(WindowsAssignment {
                revision_id: "rev-new".to_string(),
                revision_image_id: "img-new".to_string(),
            })
        );
    }

    #[test]
    fn assignment_refuses_untested_boot_nic_with_hardware_id() {
        let catalog = vec![gate_package("pkg", &["PCI\\VEN_ABCD&DEV_EF01"])];
        let revisions = vec![gate_revision("rev-1", &["pkg"], Some("img-1"))];
        let boot = vec!["PCI\\VEN_8086&DEV_15F3".to_string()];
        let passed = passed_nic_tests(&[("rev-1", "PCI\\VEN_8086&DEV_15F3")]);
        let error = select_windows_assignment("mac", Some(&boot), &revisions, &passed, &catalog)
            .unwrap_err();
        assert!(error.to_string().contains("VEN_8086"), "{error:#}");
        // A revision without a published image or without packages never matches.
        let empty = vec![
            gate_revision("rev-noimg", &["pkg"], None),
            gate_revision("rev-empty", &[], Some("img-empty")),
        ];
        assert!(select_windows_assignment("mac", Some(&boot), &empty, &passed, &catalog).is_err());
        // No ready revision at all is a distinct actionable refusal.
        let error =
            select_windows_assignment("mac", Some(&boot), &[], &passed, &catalog).unwrap_err();
        assert!(error.to_string().contains("no tested"), "{error:#}");
    }

    #[test]
    fn assignment_refuses_package_covered_nic_without_its_own_passed_test() {
        let nic_a = "PCI\\VEN_8086&DEV_15F3";
        let nic_b = "PCI\\VEN_10EC&DEV_8168";
        let catalog = vec![
            gate_package("pkg-a", &[nic_a]),
            gate_package("pkg-b", &[nic_b]),
        ];
        let revisions = vec![gate_revision("rev-1", &["pkg-a", "pkg-b"], Some("img-1"))];
        let boot = vec![nic_b.to_string()];
        let passed = passed_nic_tests(&[("rev-1", nic_a)]);

        let error = select_windows_assignment("mac", Some(&boot), &revisions, &passed, &catalog)
            .unwrap_err();

        assert!(error.to_string().contains(nic_b), "{error:#}");
    }
}
