//! A servicing job owns a clone and a unique target; source/client targets are never changed.
use crate::{
    application::image_service::ImageService,
    core::{
        config::IscsiBackend,
        image::{Image, ImageKind, OsType, UpdateImageRequest},
    },
    infrastructure::{
        iscsi::{ChapCredentials, IscsiProvisioner, IscsiTargetSpec, SafeIscsiProvisioner},
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
}

pub async fn prepare_managed(
    pool: sqlx::SqlitePool,
    iscsi_backend: IscsiBackend,
    config: crate::config::RuntimeConfig,
    request: ManagedWindowsPreparationRequest,
    packages: Vec<StagedDriverPackage>,
) -> Result<ManagedWindowsPreparationResult> {
    request.validate()?;
    UdpSocket::bind((request.portal.parse::<Ipv4Addr>()?, 0))
        .context("servicing portal is not an address on this server")?;
    let images = ImageService::with_backend(
        ImageRepository::new(pool),
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
    let iscsi = SafeIscsiProvisioner::with_backend(iscsi_backend);
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
        Ok::<_, anyhow::Error>(ManagedWindowsPreparationResult {
            preparation,
            image,
            snapshot,
            staged_package_count: count,
            uploaded_bytes: bytes,
            staging_cleanup_succeeded: cleaned,
        })
    }
    .await;
    match operation {
        Ok(result) => Ok(result),
        Err(error) => {
            let _ = set_status(&images, &clone.id, "servicing-failed").await;
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
}
