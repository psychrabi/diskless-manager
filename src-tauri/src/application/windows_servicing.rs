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
            RemoteWindowsCapabilitiesRequest, RemoteWindowsServicer, RemoteWindowsServicingRequest,
            StagedDriverPackage, WindowsBootArmConfig, WindowsImagePreparationRequest,
            WindowsImagePreparationResult,
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
        if self.driver_ids.is_empty() == !folder {
            bail!("choose imported driver packages or an existing worker folder");
        }
        Ok(())
    }
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
        request.new_name = "../source".into();
        assert!(request.validate().is_err());
        request.new_name = "new-image".into();
        request.portal = "0.0.0.0".into();
        assert!(request.validate().is_err());
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
}
