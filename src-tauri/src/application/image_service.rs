use anyhow::{bail, Context, Result};
use chrono::Utc;
use std::collections::HashSet;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    core::image::{
        BootProbe, CreateImageRequest, DriverRevisionStatus, HealthCheckStatus, Image, ImageFormat,
        ImageHealth, ImageHealthCheck, ImageInfo, ImageKind, ImportImageRequest, OsType,
        UpdateImageRequest,
    },
    infrastructure::image::{ImageBackend, ZfsImageBackend},
    persistence::repositories::image::ImageRepository,
    validation::validate_zfs_name,
};

/// Image orchestration: run synchronous storage work on blocking workers,
/// using the existing runtime for database awaits.
#[derive(Clone)]
pub struct ImageService {
    repository: ImageRepository,
    backend: Arc<dyn ImageBackend>,
}

/// A unit of storage orchestration that may block and still awaits the
/// database internally; it therefore has to run on a blocking-pool thread.
type BlockingJob<T> = Pin<Box<dyn Future<Output = Result<T>> + Send>>;

fn health_check(
    id: &str,
    label: &str,
    status: HealthCheckStatus,
    detail: impl Into<String>,
) -> ImageHealthCheck {
    ImageHealthCheck {
        id: id.to_string(),
        label: label.to_string(),
        status,
        detail: detail.into(),
    }
}

fn is_linux_filesystem(filesystem: &str) -> bool {
    matches!(
        filesystem.to_ascii_lowercase().as_str(),
        "ext2" | "ext3" | "ext4" | "xfs" | "btrfs" | "f2fs"
    )
}

impl ImageService {
    pub fn new(repository: ImageRepository) -> Self {
        Self {
            repository,
            backend: Arc::new(ZfsImageBackend::new()),
        }
    }

    pub fn with_backend(repository: ImageRepository, backend: Arc<dyn ImageBackend>) -> Self {
        Self {
            repository,
            backend,
        }
    }

    /// Run a storage job on the blocking pool while database awaits inside it
    /// keep working.
    ///
    /// `Handle::block_on` is safe on a blocking-pool thread because it is not
    /// an async execution context; the request worker stays free meanwhile.
    /// Only call from async context, and never run blocking-pool work from
    /// inside a job: the pool is finite and nesting can starve it.
    async fn run_blocking<T, F>(&self, job: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(ImageService) -> BlockingJob<T> + Send + 'static,
    {
        let service = self.clone();
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || runtime.block_on(job(service)))
            .await
            .context("image worker failed")?
    }

    pub async fn list(&self) -> Result<Vec<Image>> {
        let mut images = self.repository.list().await?;
        if images.is_empty() {
            return Ok(images);
        }
        let names: Vec<String> = images
            .iter()
            .map(|image| {
                if image.kind == ImageKind::Snapshot {
                    let parent = images
                        .iter()
                        .find(|parent| Some(&parent.id) == image.parent_id.as_ref())
                        .ok_or_else(|| {
                            anyhow::anyhow!("snapshot '{}' has no parent image", image.name)
                        })?;
                    Ok(format!("{}@{}", parent.name, image.name))
                } else {
                    Ok(image.name.clone())
                }
            })
            .collect::<Result<_>>()?;
        let backend = self.backend.clone();
        let present = tokio::task::spawn_blocking(move || backend.check_presence(&names))
            .await
            .context("image worker failed")??;
        for (image, exists) in images.iter_mut().zip(present) {
            if !exists {
                image.status = "missing".to_owned();
            }
        }
        Ok(images)
    }

    pub async fn get(&self, id_or_name: &str) -> Result<Image> {
        self.repository
            .get(id_or_name)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Image not found: {}", id_or_name))
    }

    pub async fn get_optional(&self, id_or_name: &str) -> Result<Option<Image>> {
        self.repository.get(id_or_name).await
    }

    pub async fn create(&self, request: CreateImageRequest) -> Result<Image> {
        self.run_blocking(move |service| {
            Box::pin(async move { service.create_blocking(request).await })
        })
        .await
    }

    async fn create_blocking(&self, request: CreateImageRequest) -> Result<Image> {
        validate_create_request(&request)?;

        let os_type = request.os_type.parse::<OsType>()?;

        let format = request
            .format
            .as_deref()
            .unwrap_or("raw")
            .parse::<ImageFormat>()?;

        let parent = self.backend.image_parent()?;

        let zfs_name = format!("{}/{}", parent, request.name);

        if self.backend.exists(&zfs_name)? {
            bail!("image '{}' already exists", zfs_name);
        }

        self.backend
            .create_volume(&zfs_name, request.size_gb)
            .context("failed to create ZFS image volume")?;

        if let Err(error) = self.backend.set_os_type(&zfs_name, &request.os_type) {
            let _ = self.backend.destroy(&zfs_name);

            return Err(error).context("failed to set image OS type");
        }

        let now = Utc::now();

        let image = Image {
            id: Uuid::new_v4().to_string(),

            name: zfs_name.clone(),

            kind: ImageKind::Master,

            os_type,

            size_gb: request.size_gb,

            path: PathBuf::from(format!("/dev/zvol/{}", zfs_name)),

            format,

            status: "ready".to_string(),

            description: request.description,

            parent_id: None,

            source_snapshot: None,

            checksum: None,

            is_default: false,

            created_at: now,

            updated_at: now,
        };

        if let Err(error) = self.repository.insert(&image).await {
            let _ = self.backend.destroy(&zfs_name);

            return Err(error).context("failed to persist image metadata");
        }

        Ok(image)
    }

    pub async fn rename(&self, id: &str, new_name: &str) -> Result<Image> {
        let id = id.to_owned();
        let new_name = new_name.to_owned();
        self.run_blocking(move |service| {
            Box::pin(async move { service.rename_blocking(&id, &new_name).await })
        })
        .await
    }

    async fn rename_blocking(&self, id: &str, new_name: &str) -> Result<Image> {
        validate_zfs_name(new_name)?;

        let mut image = self.get(id).await?;

        if image.parent_id.is_some() {
            bail!("snapshots cannot be renamed as images");
        }

        let parent = parent_dataset(&image.name)?;

        let new_full_name = format!("{}/{}", parent, new_name);

        if self.backend.exists(&new_full_name)? {
            bail!("image '{}' already exists", new_full_name);
        }

        let old_name = image.name.clone();

        if self.backend.exists(&old_name)? {
            self.backend.rename(&old_name, &new_full_name)?;
        }

        image.name = new_full_name.clone();

        image.path = PathBuf::from(format!("/dev/zvol/{}", new_full_name));

        image.updated_at = Utc::now();

        self.repository.update(&image).await?;

        Ok(image)
    }

    pub async fn update(&self, id: &str, request: UpdateImageRequest) -> Result<Image> {
        let id = id.to_owned();
        self.run_blocking(move |service| {
            Box::pin(async move { service.update_blocking(&id, request).await })
        })
        .await
    }

    async fn update_blocking(&self, id: &str, request: UpdateImageRequest) -> Result<Image> {
        let mut image = self.get(id).await?;

        if let Some(name) = request.name {
            // Already on a blocking-pool thread: call the inner job directly
            // instead of nesting another spawn_blocking.
            image = self.rename_blocking(&image.id, &name).await?;
        }

        if let Some(os_type) = request.os_type {
            let parsed = os_type.parse::<OsType>()?;

            self.backend.set_os_type(&image.name, &os_type)?;

            image.os_type = parsed;
        }

        if let Some(description) = request.description {
            image.description = Some(description);
        }

        if let Some(status) = request.status {
            image.status = status;
        }

        image.updated_at = Utc::now();

        self.repository.update(&image).await?;

        Ok(image)
    }

    pub async fn clone_image(
        &self,
        source_id: &str,
        snapshot_name: &str,
        new_name: &str,
    ) -> Result<Image> {
        let source_id = source_id.to_owned();
        let snapshot_name = snapshot_name.to_owned();
        let new_name = new_name.to_owned();
        self.run_blocking(move |service| {
            Box::pin(async move {
                service
                    .clone_image_blocking(&source_id, &snapshot_name, &new_name)
                    .await
            })
        })
        .await
    }

    /// Create a new master from a snapshot without rolling back the source.
    /// The source image and its newer snapshots are left untouched.
    pub async fn apply_snapshot(
        &self,
        source_id: &str,
        snapshot_name: &str,
        new_name: &str,
    ) -> Result<Image> {
        let source_id = source_id.to_owned();
        let snapshot_name = snapshot_name.to_owned();
        let new_name = new_name.to_owned();
        self.run_blocking(move |service| {
            Box::pin(async move {
                service
                    .create_image_from_snapshot(
                        &source_id,
                        &snapshot_name,
                        &new_name,
                        ImageKind::Master,
                    )
                    .await
            })
        })
        .await
    }

    async fn clone_image_blocking(
        &self,
        source_id: &str,
        snapshot_name: &str,
        new_name: &str,
    ) -> Result<Image> {
        self.create_image_from_snapshot(source_id, snapshot_name, new_name, ImageKind::Clone)
            .await
    }

    /// Clone a snapshot into a new dataset and persist it as either a dependent
    /// clone or an independent logical master. Neither path promotes or alters
    /// the source snapshot.
    async fn create_image_from_snapshot(
        &self,
        source_id: &str,
        snapshot_name: &str,
        new_name: &str,
        kind: ImageKind,
    ) -> Result<Image> {
        if !matches!(kind, ImageKind::Master | ImageKind::Clone) {
            bail!("snapshot derivatives must be a master or clone");
        }

        validate_zfs_name(new_name)?;

        if snapshot_name.trim().is_empty() {
            bail!("snapshot name cannot be empty");
        }

        let source = self.get(source_id).await?;

        /*
         * A new image can be created from either:
         *
         *     master -> snapshot -> clone
         *
         * or:
         *
         *     clone -> snapshot -> clone
         *
         * A snapshot itself cannot be used as the owning image.
         */
        match source.kind {
            ImageKind::Master | ImageKind::Clone => {}
            ImageKind::Snapshot => {
                bail!("cannot create an image from a snapshot record directly");
            }
        }

        /*
         * Snapshots belonging to this image are represented by child
         * Image records.
         */
        let snapshots = self.snapshots(&source.id).await?;

        let snapshot = snapshots
            .iter()
            .find(|snapshot| snapshot.name == snapshot_name)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "snapshot '{}' not found for '{}'",
                    snapshot_name,
                    source.name
                )
            })?;

        if snapshot.kind != ImageKind::Snapshot {
            bail!("'{}' is not a snapshot of '{}'", snapshot_name, source.name);
        }

        /*
         * Construct the real ZFS snapshot identifier:
         *
         *     diskless/image-disk/stage3e-test@v1
         */
        let snapshot_source = format!("{}@{}", source.name, snapshot.name);

        let parent = parent_dataset(&source.name)?;

        let destination = format!("{}/{}", parent, new_name);

        if self.backend.exists(&destination)? {
            bail!("image '{}' already exists", destination);
        }

        /*
         * ZFS clone MUST use a snapshot as its source.
         */
        self.backend
            .clone_image(&snapshot_source, &destination)
            .context("failed to clone ZFS snapshot")?;

        let now = Utc::now();
        let is_clone = kind == ImageKind::Clone;

        let image = Image {
            id: Uuid::new_v4().to_string(),

            name: destination.clone(),

            kind,

            os_type: source.os_type,

            size_gb: source.size_gb,

            path: PathBuf::from(format!("/dev/zvol/{}", destination)),

            format: source.format,

            status: "ready".to_string(),

            description: Some(if is_clone {
                format!("Clone of {}@{}", source.name, snapshot.name)
            } else {
                format!("Restore point of {}@{}", source.name, snapshot.name)
            }),

            /*
             * A clone keeps its logical source image. An applied restore point
             * becomes a master so it can be assigned to clients directly.
             */
            parent_id: is_clone.then(|| source.id.clone()),

            /*
             * Record the exact snapshot used to create this clone.
             */
            source_snapshot: is_clone.then(|| snapshot.name.clone()),

            checksum: None,

            is_default: false,

            created_at: now,

            updated_at: now,
        };

        if let Err(error) = self.repository.insert(&image).await {
            if let Err(cleanup_error) = self.backend.destroy(&destination) {
                log::error!(
                    "Failed to remove image dataset '{}' after metadata insertion failed: {}",
                    destination,
                    cleanup_error
                );
            }

            return Err(error).context("failed to persist image from snapshot");
        }

        Ok(image)
    }

    /// Publish a completed servicing clone without ZFS promotion, preserving source snapshots.
    pub async fn publish_serviced_clone(&self, id: &str) -> Result<Image> {
        let mut image = self.get(id).await?;
        if image.kind != ImageKind::Clone
            || image.status != "preparing"
            || self.snapshots(id).await?.is_empty()
        {
            bail!("only a completed servicing clone with a snapshot can be published");
        }
        image.kind = ImageKind::Master;
        image.parent_id = None;
        image.source_snapshot = None;
        image.status = "ready".into();
        image.updated_at = Utc::now();
        self.repository.update(&image).await?;
        Ok(image)
    }

    /// Remove the source dependency from a serviced clone before deleting its temporary origin.
    /// Repeated calls are safe after a successful promotion.
    pub async fn promote_clone(&self, name: &str) -> Result<()> {
        let backend = self.backend.clone();
        let name = name.to_owned();
        tokio::task::spawn_blocking(move || {
            if backend.clone_origin(&name)?.is_some() {
                backend.promote_clone(&name)?;
            }
            Ok(())
        })
        .await
        .context("image worker failed")?
    }

    pub async fn rollback_snapshot(
        &self,
        master_id_or_name: &str,
        snapshot_name: &str,
        prune_newer_snapshots: bool,
        expected_newer_snapshots: &[String],
    ) -> Result<usize> {
        let master_id_or_name = master_id_or_name.to_owned();
        let snapshot_name = snapshot_name.to_owned();
        let expected_newer_snapshots = expected_newer_snapshots.to_vec();
        self.run_blocking(move |service| {
            Box::pin(async move {
                service
                    .rollback_snapshot_blocking(
                        &master_id_or_name,
                        &snapshot_name,
                        prune_newer_snapshots,
                        &expected_newer_snapshots,
                    )
                    .await
            })
        })
        .await
    }

    async fn rollback_snapshot_blocking(
        &self,
        master_id_or_name: &str,
        snapshot_name: &str,
        prune_newer_snapshots: bool,
        expected_newer_snapshots: &[String],
    ) -> Result<usize> {
        let master = self.get(master_id_or_name).await?;

        if master.kind == ImageKind::Snapshot {
            bail!("cannot rollback a snapshot");
        }

        let snapshots = self.snapshots(&master.id).await?;

        let target = snapshots
            .iter()
            .find(|snapshot| snapshot.name == snapshot_name)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "snapshot '{}' not found for '{}'",
                    snapshot_name,
                    master.name
                )
            })?;

        let newer_snapshots: Vec<&Image> = snapshots
            .iter()
            .filter(|snapshot| snapshot.created_at > target.created_at)
            .collect();
        let newer_ids: Vec<String> = newer_snapshots
            .iter()
            .map(|snapshot| snapshot.id.clone())
            .collect();
        let newer_names: Vec<String> = newer_snapshots
            .iter()
            .map(|snapshot| snapshot.name.clone())
            .collect();

        if !newer_ids.is_empty() && !prune_newer_snapshots {
            bail!("rollback would delete newer snapshots; explicit prune approval is required");
        }
        if prune_newer_snapshots {
            let mut expected = expected_newer_snapshots.to_vec();
            let mut actual = newer_names.clone();
            expected.sort();
            actual.sort();
            if expected != actual {
                bail!("newer snapshots changed since confirmation; refresh and retry");
            }
        }
        if self
            .repository
            .has_non_snapshot_children(&master.id)
            .await?
        {
            bail!("cannot rollback an image with dependent clones");
        }
        if self.repository.has_pending_installation(&master.id).await? {
            bail!("cannot rollback an image with a pending installer session");
        }
        let assigned_clients = self
            .repository
            .clients_assigned_to_snapshots(&master.name, &newer_names)
            .await?;
        if !assigned_clients.is_empty() {
            bail!(
                "cannot rollback while clients are assigned to newer snapshots: {}",
                assigned_clients.join(", ")
            );
        }
        for target_iqn in self
            .repository
            .assigned_client_targets(&master.name)
            .await?
        {
            if crate::infrastructure::iscsi::target_has_active_sessions(&target_iqn)? {
                bail!("cannot rollback while a client has an active iSCSI session");
            }
        }

        // `zfs rollback -r` also destroys snapshots that have no metadata row.
        // Refuse instead of silently deleting operator- or replication-created
        // snapshots; `import-scan` can register them first.
        let tracked_names: HashSet<&str> = snapshots
            .iter()
            .map(|snapshot| snapshot.name.as_str())
            .collect();
        let storage = self.backend.info(&master.name)?;
        let untracked: Vec<&str> = storage
            .snapshots
            .iter()
            .map(String::as_str)
            .filter(|name| !tracked_names.contains(name))
            .collect();
        if !untracked.is_empty() {
            bail!(
                "cannot rollback while untracked ZFS snapshots exist: {}; import or remove them first",
                untracked.join(", ")
            );
        }

        let transaction = self.repository.begin_delete_snapshots(&newer_ids).await?;
        self.backend.rollback_snapshot(&master.name, &target.name)?;
        transaction.commit().await?;

        Ok(newer_ids.len())
    }

    pub async fn import(&self, request: ImportImageRequest) -> Result<Image> {
        self.run_blocking(move |service| {
            Box::pin(async move { service.import_blocking(request).await })
        })
        .await
    }

    async fn import_blocking(&self, request: ImportImageRequest) -> Result<Image> {
        use crate::infrastructure::image::{ImageConversionBackend, QemuImgBackend};

        validate_zfs_name(&request.name)?;

        let source = std::path::Path::new(&request.source_path);

        if !source.exists() {
            bail!("source image '{}' does not exist", request.source_path);
        }

        let os_type = request.os_type.parse::<OsType>()?;

        let converter = QemuImgBackend::new();

        let source_info = converter.info(source)?;

        let parent = self.backend.image_parent()?;

        let destination = format!("{}/{}", parent, request.name);

        if self.backend.exists(&destination)? {
            bail!("image '{}' already exists", destination);
        }

        let size_bytes = source_info.virtual_size;

        if size_bytes == 0 {
            bail!("source image has zero virtual size");
        }

        import_source(
            self.backend.as_ref(),
            source,
            source_info.format,
            &destination,
            size_bytes,
        )
        .context("failed to import source image")?;

        self.backend.set_os_type(&destination, &request.os_type)?;

        let image = Image {
            id: Uuid::new_v4().to_string(),

            name: destination.clone(),

            kind: ImageKind::Master,

            os_type,

            size_gb: size_bytes.div_ceil(1024 * 1024 * 1024),

            path: PathBuf::from(format!("/dev/zvol/{}", destination)),

            format: ImageFormat::Raw,

            status: "ready".to_string(),

            description: request.description,

            parent_id: None,

            checksum: None,

            source_snapshot: None,

            is_default: false,

            created_at: Utc::now(),

            updated_at: Utc::now(),
        };

        if let Err(error) = self.repository.insert(&image).await {
            let _ = self.backend.destroy(&destination);

            return Err(error).context("failed to persist imported image");
        }

        Ok(image)
    }

    pub async fn resize(&self, id: &str, new_size_gb: u64) -> Result<Image> {
        let id = id.to_owned();
        self.run_blocking(move |service| {
            Box::pin(async move { service.resize_blocking(&id, new_size_gb).await })
        })
        .await
    }

    async fn resize_blocking(&self, id: &str, new_size_gb: u64) -> Result<Image> {
        if new_size_gb == 0 {
            bail!("image size must be greater than zero");
        }

        let mut image = self.get(id).await?;

        if new_size_gb < image.size_gb {
            bail!(
                "cannot shrink image from {} GB to {} GB",
                image.size_gb,
                new_size_gb
            );
        }

        if new_size_gb == image.size_gb {
            return Ok(image);
        }

        self.backend.resize(&image.name, new_size_gb)?;

        image.size_gb = new_size_gb;

        image.updated_at = Utc::now();

        self.repository.update(&image).await?;

        Ok(image)
    }

    pub async fn verify(&self, id: &str) -> Result<bool> {
        let id = id.to_owned();
        self.run_blocking(move |service| {
            Box::pin(async move { service.verify_blocking(&id).await })
        })
        .await
    }

    async fn verify_blocking(&self, id: &str) -> Result<bool> {
        let image = self.get(id).await?;

        if !self.backend.verify(&image.name)? {
            return Ok(false);
        }

        let expected_size = image
            .size_gb
            .checked_mul(1024 * 1024 * 1024)
            .context("image size exceeds supported capacity")?;
        let actual_size = self.backend.info(&image.name)?.virtual_size;
        Ok(actual_size > 0 && actual_size == expected_size)
    }

    pub async fn get_info(&self, id: &str) -> Result<ImageInfo> {
        let id = id.to_owned();
        self.run_blocking(move |service| {
            Box::pin(async move { service.get_info_blocking(&id).await })
        })
        .await
    }

    async fn get_info_blocking(&self, id: &str) -> Result<ImageInfo> {
        let image = self.get(id).await?;

        let info = self.backend.info(&image.name)?;

        Ok(info.into())
    }

    /// Inspect storage and boot metadata without mounting the image.
    pub async fn health(&self, id: &str) -> Result<ImageHealth> {
        let id = id.to_owned();
        self.run_blocking(move |service| {
            Box::pin(async move { service.health_blocking(&id).await })
        })
        .await
    }

    async fn health_blocking(&self, id: &str) -> Result<ImageHealth> {
        let image = self.get(id).await?;
        if image.kind == ImageKind::Snapshot {
            bail!("health inspection requires a master image or clone");
        }

        let mut checks = Vec::new();
        let mut boot = BootProbe::default();

        let storage = match self.backend.verify(&image.name) {
            Ok(true) => match self.backend.info(&image.name) {
                Ok(info) => {
                    checks.push(health_check(
                        "storage",
                        "Storage volume",
                        HealthCheckStatus::Pass,
                        format!("{} is present", image.name),
                    ));
                    Some(info)
                }
                Err(error) => {
                    checks.push(health_check(
                        "storage",
                        "Storage volume",
                        HealthCheckStatus::Fail,
                        format!("{error:#}"),
                    ));
                    None
                }
            },
            Ok(false) => {
                checks.push(health_check(
                    "storage",
                    "Storage volume",
                    HealthCheckStatus::Fail,
                    format!("{} is missing from storage", image.name),
                ));
                None
            }
            Err(error) => {
                checks.push(health_check(
                    "storage",
                    "Storage volume",
                    HealthCheckStatus::Fail,
                    format!("{error:#}"),
                ));
                None
            }
        };

        if let Some(info) = &storage {
            match image
                .size_gb
                .checked_mul(1024 * 1024 * 1024)
                .filter(|expected| info.virtual_size > 0 && info.virtual_size == *expected)
            {
                Some(_) => checks.push(health_check(
                    "size",
                    "Volume size",
                    HealthCheckStatus::Pass,
                    format!("{} GB allocated", image.size_gb),
                )),
                None => checks.push(health_check(
                    "size",
                    "Volume size",
                    HealthCheckStatus::Fail,
                    format!(
                        "metadata says {} GB, storage reports {} bytes",
                        image.size_gb, info.virtual_size
                    ),
                )),
            }

            match self.backend.probe_boot(&image.name) {
                Ok(probe) => boot = probe,
                Err(error) => checks.push(health_check(
                    "boot-probe",
                    "Boot metadata",
                    HealthCheckStatus::Warn,
                    format!("could not read partition metadata: {error:#}"),
                )),
            }
        }

        if storage.is_some() {
            checks.push(if boot.partition_table.is_some() {
                health_check(
                    "partition-table",
                    "Partition table",
                    HealthCheckStatus::Pass,
                    boot.partition_table
                        .clone()
                        .unwrap_or_else(|| "detected".to_string()),
                )
            } else {
                health_check(
                    "partition-table",
                    "Partition table",
                    HealthCheckStatus::Warn,
                    "no partition table detected",
                )
            });

            checks.push(if boot.partitions.is_empty() {
                health_check(
                    "partitions",
                    "Partitions",
                    HealthCheckStatus::Warn,
                    "no partitions detected",
                )
            } else {
                health_check(
                    "partitions",
                    "Partitions",
                    HealthCheckStatus::Pass,
                    format!("{} partition(s) detected", boot.partitions.len()),
                )
            });

            let has_efi = boot.partitions.iter().any(|partition| {
                partition
                    .part_type
                    .as_deref()
                    .is_some_and(|kind| kind.to_ascii_lowercase().contains("efi"))
            });
            let has_boot_flag = boot.partitions.iter().any(|partition| {
                partition.part_flags.as_deref().is_some_and(|flags| {
                    let flags = flags.to_ascii_lowercase();
                    flags.contains("0x80") || flags.contains("boot")
                })
            });

            match image.os_type {
                OsType::Windows => {
                    let has_ntfs = boot
                        .filesystem
                        .as_deref()
                        .is_some_and(|filesystem| filesystem.eq_ignore_ascii_case("ntfs"))
                        || boot.partitions.iter().any(|partition| {
                            partition.filesystem.as_deref().is_some_and(|filesystem| {
                                filesystem.eq_ignore_ascii_case("ntfs")
                                    || filesystem.eq_ignore_ascii_case("bitlocker")
                            })
                        });
                    checks.push(if has_ntfs {
                        health_check(
                            "windows-filesystem",
                            "Windows filesystem",
                            HealthCheckStatus::Pass,
                            "NTFS or BitLocker volume found",
                        )
                    } else {
                        health_check(
                            "windows-filesystem",
                            "Windows filesystem",
                            HealthCheckStatus::Fail,
                            "no NTFS or BitLocker volume found",
                        )
                    });
                    checks.push(if has_efi {
                        health_check(
                            "efi-partition",
                            "EFI System Partition",
                            HealthCheckStatus::Pass,
                            "EFI partition found",
                        )
                    } else {
                        health_check(
                            "efi-partition",
                            "EFI System Partition",
                            HealthCheckStatus::Warn,
                            "no EFI partition found; verify BIOS boot before assigning clients",
                        )
                    });
                }
                OsType::Linux => {
                    let has_linux_filesystem =
                        boot.filesystem.as_deref().is_some_and(is_linux_filesystem)
                            || boot.partitions.iter().any(|partition| {
                                partition
                                    .filesystem
                                    .as_deref()
                                    .is_some_and(is_linux_filesystem)
                            });
                    checks.push(if has_linux_filesystem {
                        health_check(
                            "linux-filesystem",
                            "Linux filesystem",
                            HealthCheckStatus::Pass,
                            "ext, XFS, Btrfs, or F2FS volume found",
                        )
                    } else {
                        health_check(
                            "linux-filesystem",
                            "Linux filesystem",
                            HealthCheckStatus::Warn,
                            "no common Linux filesystem found",
                        )
                    });
                    checks.push(if has_efi {
                        health_check(
                            "efi-partition",
                            "EFI System Partition",
                            HealthCheckStatus::Pass,
                            "EFI partition found",
                        )
                    } else if has_boot_flag {
                        health_check(
                            "efi-partition",
                            "Boot partition",
                            HealthCheckStatus::Pass,
                            "BIOS boot flag found",
                        )
                    } else {
                        health_check(
                            "efi-partition",
                            "Boot partition",
                            HealthCheckStatus::Warn,
                            "no EFI or BIOS boot marker found",
                        )
                    });
                }
            }
        }

        if image.os_type == OsType::Windows {
            match self.repository.list_driver_revisions(&image.id).await {
                Ok(revisions) => {
                    let ready = revisions
                        .iter()
                        .filter(|revision| revision.status == DriverRevisionStatus::Ready)
                        .count();
                    let pending = revisions
                        .iter()
                        .filter(|revision| {
                            matches!(
                                revision.status,
                                DriverRevisionStatus::Preparing
                                    | DriverRevisionStatus::AwaitingTest
                            )
                        })
                        .count();
                    let failed = revisions
                        .iter()
                        .filter(|revision| revision.status == DriverRevisionStatus::Failed)
                        .count();
                    let (status, detail) = if ready > 0 {
                        (
                            HealthCheckStatus::Pass,
                            format!("{ready} tested driver revision(s) available"),
                        )
                    } else if pending > 0 {
                        (
                            HealthCheckStatus::Warn,
                            format!("{pending} driver candidate(s) awaiting a boot test"),
                        )
                    } else if failed > 0 {
                        (
                            HealthCheckStatus::Warn,
                            "no tested driver revision; latest candidate failed, base image drivers will be used"
                                .to_string(),
                        )
                    } else {
                        (
                            HealthCheckStatus::Pass,
                            "no driver revision recorded; base image drivers will be used"
                                .to_string(),
                        )
                    };
                    checks.push(health_check(
                        "driver-readiness",
                        "Driver readiness",
                        status,
                        detail,
                    ));
                }
                Err(error) => checks.push(health_check(
                    "driver-readiness",
                    "Driver readiness",
                    HealthCheckStatus::Warn,
                    format!("driver revision lookup failed: {error:#}"),
                )),
            }
        }

        Ok(ImageHealth::from_checks(image.id, checks, boot))
    }

    pub async fn create_snapshot(&self, id: &str, snapshot_name: &str) -> Result<Image> {
        let id = id.to_owned();
        let snapshot_name = snapshot_name.to_owned();
        self.run_blocking(move |service| {
            Box::pin(async move { service.create_snapshot_blocking(&id, &snapshot_name).await })
        })
        .await
    }

    async fn create_snapshot_blocking(&self, id: &str, snapshot_name: &str) -> Result<Image> {
        validate_zfs_name(snapshot_name)?;

        let source = self.get(id).await?;

        /*
         * Masters and clones are ZFS volumes and may have snapshots.
         *
         * Snapshots themselves cannot have snapshots.
         */
        match source.kind {
            ImageKind::Master | ImageKind::Clone => {}
            ImageKind::Snapshot => {
                bail!("cannot create snapshot from a snapshot");
            }
        }

        /*
         * Prevent duplicate snapshot names for this image.
         */
        let existing_snapshots = self.snapshots(&source.id).await?;

        if existing_snapshots
            .iter()
            .any(|snapshot| snapshot.name == snapshot_name)
        {
            bail!(
                "snapshot '{}' already exists for '{}'",
                snapshot_name,
                source.name
            );
        }

        self.backend.create_snapshot(&source.name, snapshot_name)?;

        let image = Image {
            id: Uuid::new_v4().to_string(),

            name: snapshot_name.to_string(),

            kind: ImageKind::Snapshot,

            os_type: source.os_type,

            size_gb: source.size_gb,

            /*
             * A snapshot doesn't have its own /dev/zvol device.
             * Keep the source path for compatibility with the existing API.
             */
            path: source.path.clone(),

            format: source.format,

            status: "ready".to_string(),

            description: Some(format!("Snapshot of {}", source.name)),

            /*
             * Snapshot belongs to the source image.
             */
            parent_id: Some(source.id.clone()),

            source_snapshot: None,

            checksum: None,

            is_default: false,

            created_at: Utc::now(),

            updated_at: Utc::now(),
        };

        if let Err(error) = self.repository.insert(&image).await {
            let _ = self.backend.destroy_snapshot(&source.name, snapshot_name);

            return Err(error).context("failed to persist snapshot metadata");
        }

        Ok(image)
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        let id = id.to_owned();
        self.run_blocking(move |service| {
            Box::pin(async move { service.delete_blocking(&id).await })
        })
        .await
    }

    async fn delete_blocking(&self, id: &str) -> Result<()> {
        let image = self.get(id).await?;

        if image.is_default && self.backend.exists(&image.name)? {
            bail!("cannot delete the default image");
        }

        let storage_name = match image.kind {
            ImageKind::Snapshot => {
                let parent_id = image.parent_id.as_deref().ok_or_else(|| {
                    anyhow::anyhow!("snapshot '{}' has no parent image", image.name)
                })?;
                let parent = self.get(parent_id).await?;
                format!("{}@{}", parent.name, image.name)
            }
            ImageKind::Master | ImageKind::Clone => {
                let children = self.repository.list().await?;
                if children
                    .iter()
                    .any(|candidate| candidate.parent_id.as_deref() == Some(&image.id))
                {
                    bail!(
                        "cannot delete image '{}' while dependent snapshots or clones exist",
                        image.name
                    );
                }
                image.name.clone()
            }
        };

        // An image whose storage already vanished can never be used again, so
        // its revision references must not keep the record alive, and there is
        // nothing left to destroy (ZFS fails on a snapshot that is already gone).
        let storage_missing = !self.backend.exists(&storage_name)?;
        let (transaction, _) = self
            .repository
            .begin_delete(&image.id, storage_missing)
            .await?;
        if !storage_missing {
            match image.kind {
                ImageKind::Snapshot => {
                    let (parent, snapshot) = storage_name
                        .rsplit_once('@')
                        .context("snapshot storage name has no parent")?;
                    self.backend.destroy_snapshot(parent, snapshot)
                }
                ImageKind::Master | ImageKind::Clone => self.backend.destroy(&storage_name),
            }
            .with_context(|| format!("failed to delete image '{}'", storage_name))?;
        }
        transaction
            .commit()
            .await
            .context("failed to remove image metadata after storage deletion")?;

        Ok(())
    }

    pub async fn set_default(&self, id_or_name: &str) -> Result<Image> {
        let image = self.get(id_or_name).await?;

        if image.kind == ImageKind::Snapshot {
            bail!("a snapshot cannot be the default image");
        }

        self.repository.set_default(&image.id).await?;

        self.get(&image.id).await
    }

    pub async fn snapshots(&self, id: &str) -> Result<Vec<Image>> {
        let images = self.repository.list().await?;

        Ok(images
            .into_iter()
            .filter(|image| {
                image.kind == ImageKind::Snapshot && image.parent_id.as_deref() == Some(id)
            })
            .collect())
    }

    /// Scan the ZFS pool for image ZVOLs (and their snapshots) that live
    /// under the configured `image-disk` parent but are not yet registered in
    /// the database, and register them so they show up in the UI.
    ///
    /// Existing database rows are left untouched; this never deletes or
    /// overwrites anything. A row that cannot be inserted (for example a
    /// duplicate snapshot name) is skipped with a warning rather than aborting
    /// the whole scan.
    pub async fn import_existing_images(&self) -> Result<ImportScanResult> {
        self.run_blocking(|service| {
            Box::pin(async move { service.import_existing_images_blocking().await })
        })
        .await
    }

    async fn import_existing_images_blocking(&self) -> Result<ImportScanResult> {
        use crate::infrastructure::zfs::{ZfsCommand, ZfsDatasetOperations, ZfsSnapshotOperations};

        let parent = self.backend.image_parent()?;

        let zpool = parent
            .split_once('/')
            .map(|(pool, _)| pool.to_string())
            .unwrap_or_else(|| parent.clone());

        let datasets = ZfsDatasetOperations::new(ZfsCommand::new());
        let snapshots = ZfsSnapshotOperations::new(ZfsCommand::new());

        let volumes = datasets.list_datasets(&zpool)?;
        let listed = snapshots.list(&zpool)?;

        let tracked = self.repository.list().await?;

        let mut imported_masters = 0;
        let mut imported_snapshots = 0;

        for volume in &volumes {
            let is_volume = volume.dataset_type == "volume";

            // Only consider ZVOLs under the image parent (e.g. "pool/image-disk/..").
            // Client disks and other datasets live elsewhere and are not images.
            if !is_volume || !volume.name.starts_with(&format!("{}/", parent)) {
                continue;
            }

            let parent_children = listed
                .iter()
                .filter(|snap| snap.dataset == volume.name)
                .collect::<Vec<_>>();

            let size_gb = volume.used.as_deref().and_then(parse_size_gb).unwrap_or(0);

            let os_type = datasets
                .get_property("org.diskless:os", &volume.name)?
                .and_then(|value| value.parse::<OsType>().ok())
                .unwrap_or(OsType::Linux);

            if let Some(master) = tracked
                .iter()
                .find(|img| img.kind == ImageKind::Master && img.name == volume.name)
            {
                // Master already tracked: still register any snapshots that are
                // missing, then move on.
                for snap in &parent_children {
                    let already = tracked.iter().any(|img| {
                        img.kind == ImageKind::Snapshot
                            && img.parent_id.as_deref() == Some(master.id.as_str())
                            && img.name == snap.snapshot
                    });
                    if already {
                        continue;
                    }
                    match self
                        .repository
                        .insert(&snapshot_record(master, snap, &volume.name))
                        .await
                    {
                        Ok(_) => imported_snapshots += 1,
                        Err(error) => {
                            log::warn!(
                                "import_existing: skipping snapshot '{}' of '{}': {}",
                                snap.snapshot,
                                volume.name,
                                error
                            );
                        }
                    }
                }
                continue;
            }

            let image = Image {
                id: Uuid::new_v4().to_string(),
                name: volume.name.clone(),
                kind: ImageKind::Master,
                os_type,
                size_gb,
                path: PathBuf::from(format!("/dev/zvol/{}", volume.name)),
                format: ImageFormat::Raw,
                status: "ready".to_string(),
                description: None,
                parent_id: None,
                source_snapshot: None,
                checksum: None,
                is_default: false,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };

            if let Err(error) = self.repository.insert(&image).await {
                log::warn!("import_existing: skipping '{}': {}", volume.name, error);
                continue;
            }
            imported_masters += 1;

            for snap in &parent_children {
                match self
                    .repository
                    .insert(&snapshot_record(&image, snap, &volume.name))
                    .await
                {
                    Ok(_) => imported_snapshots += 1,
                    Err(error) => {
                        log::warn!(
                            "import_existing: skipping snapshot '{}' of '{}': {}",
                            snap.snapshot,
                            volume.name,
                            error
                        );
                    }
                }
            }
        }

        Ok(ImportScanResult {
            imported_masters,
            imported_snapshots,
        })
    }
}

/// Build the database record for a snapshot child of `master`.
///
/// Matches the semantics used by `create_snapshot`: the stored `name` is the
/// raw snapshot name (the part after `@`), while the actual ZFS object is
/// `master.name@image.name`.
fn snapshot_record(
    master: &Image,
    snap: &crate::infrastructure::zfs::provider::ZfsSnapshotInfo,
    volume_name: &str,
) -> Image {
    Image {
        id: Uuid::new_v4().to_string(),
        name: snap.snapshot.clone(),
        kind: ImageKind::Snapshot,
        os_type: master.os_type,
        size_gb: master.size_gb,
        path: PathBuf::from(format!("/dev/zvol/{}", volume_name)),
        format: master.format,
        status: "ready".to_string(),
        description: Some(format!("Snapshot of {}", master.name)),
        parent_id: Some(master.id.clone()),
        source_snapshot: None,
        checksum: None,
        is_default: false,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

/// Result of a ZFS scan-and-import operation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ImportScanResult {
    pub imported_masters: usize,
    pub imported_snapshots: usize,
}

fn parse_size_gb(value: &str) -> Option<u64> {
    let value = value.trim();
    if value.is_empty() || value == "-" {
        return Some(0);
    }
    let number: f64 = value
        .trim_end_matches(['K', 'M', 'G', 'T', 'P'])
        .trim()
        .parse()
        .ok()?;
    let multiplier: u64 = if value.ends_with('K') {
        1024
    } else if value.ends_with('M') {
        1024 * 1024
    } else if value.ends_with('G') {
        1024 * 1024 * 1024
    } else if value.ends_with('T') {
        1024 * 1024 * 1024 * 1024
    } else if value.ends_with('P') {
        1024 * 1024 * 1024 * 1024 * 1024
    } else {
        1
    };
    Some(
        (number as u64)
            .saturating_mul(multiplier)
            .div_ceil(1024 * 1024 * 1024),
    )
}

fn validate_create_request(request: &CreateImageRequest) -> Result<()> {
    if request.name.trim().is_empty() {
        bail!("image name cannot be empty");
    }

    if request.size_gb == 0 {
        bail!("image size must be greater than zero");
    }

    validate_zfs_name(&request.name)?;

    Ok(())
}

fn parent_dataset(zfs_name: &str) -> Result<String> {
    zfs_name
        .rsplit_once('/')
        .map(|(parent, _)| parent.to_string())
        .ok_or_else(|| anyhow::anyhow!("invalid ZFS image name '{}'", zfs_name))
}

fn import_source(
    backend: &dyn ImageBackend,
    source: &Path,
    format: ImageFormat,
    destination: &str,
    size_bytes: u64,
) -> Result<()> {
    if format == ImageFormat::Raw {
        backend.import_raw(source, destination, size_bytes)
    } else {
        backend.import_converted(source, destination, size_bytes)
    }
}

#[cfg(test)]
mod storage_presence_tests {
    use super::*;
    use crate::infrastructure::image::ImageBackendInfo;
    use std::collections::HashSet;
    use std::path::Path;

    struct FakeStorage {
        present: HashSet<String>,
        virtual_size: u64,
        fail: bool,
        fail_rollback: bool,
        snapshots: Vec<String>,
        boot_probe: BootProbe,
        promoted: Arc<std::sync::atomic::AtomicBool>,
        imports: Arc<std::sync::Mutex<Vec<String>>>,
        log: Arc<std::sync::Mutex<Vec<String>>>,
        allow_destroy: bool,
    }
    impl ImageBackend for FakeStorage {
        fn clone_origin(&self, _: &str) -> Result<Option<String>> {
            Ok((!self.promoted.load(std::sync::atomic::Ordering::SeqCst))
                .then(|| "pool/origin@snapshot".to_owned()))
        }
        fn promote_clone(&self, _: &str) -> Result<()> {
            self.promoted
                .store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
        fn exists(&self, name: &str) -> Result<bool> {
            if self.fail {
                bail!("storage query failed");
            }
            Ok(self.present.contains(name))
        }
        fn destroy(&self, name: &str) -> Result<()> {
            if !self.allow_destroy {
                bail!("unexpected destroy");
            }
            self.log.lock().unwrap().push(format!("destroy:{name}"));
            Ok(())
        }
        fn destroy_snapshot(&self, _: &str, _: &str) -> Result<()> {
            bail!("unexpected snapshot destroy")
        }
        fn create_volume(&self, _: &str, _: u64) -> Result<()> {
            unreachable!()
        }
        fn rename(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn clone_image(&self, source: &str, destination: &str) -> Result<()> {
            self.log
                .lock()
                .unwrap()
                .push(format!("clone:{source}:{destination}"));
            Ok(())
        }
        fn create_snapshot(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn rollback_snapshot(&self, _: &str, _: &str) -> Result<()> {
            if self.fail_rollback {
                bail!("rollback failed");
            }
            Ok(())
        }
        fn resize(&self, _: &str, _: u64) -> Result<()> {
            unreachable!()
        }
        fn import_raw(&self, source: &Path, destination: &str, size_bytes: u64) -> Result<()> {
            self.imports.lock().unwrap().push(format!(
                "raw:{}:{destination}:{size_bytes}",
                source.display()
            ));
            Ok(())
        }
        fn import_converted(
            &self,
            source: &Path,
            destination: &str,
            size_bytes: u64,
        ) -> Result<()> {
            self.imports.lock().unwrap().push(format!(
                "converted:{}:{destination}:{size_bytes}",
                source.display()
            ));
            Ok(())
        }
        fn verify(&self, name: &str) -> Result<bool> {
            Ok(self.present.contains(name))
        }
        fn info(&self, _: &str) -> Result<ImageBackendInfo> {
            Ok(ImageBackendInfo {
                virtual_size: self.virtual_size,
                actual_size: 0,
                format: ImageFormat::Raw,
                backing_file: None,
                snapshots: self.snapshots.clone(),
            })
        }
        fn probe_boot(&self, _: &str) -> Result<BootProbe> {
            Ok(self.boot_probe.clone())
        }
        fn set_os_type(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn image_parent(&self) -> Result<String> {
            unreachable!()
        }
    }
    async fn service(present: &[&str], fail: bool) -> ImageService {
        service_with_virtual_size(present, fail, 50 * 1024 * 1024 * 1024).await
    }

    async fn service_with_virtual_size(
        present: &[&str],
        fail: bool,
        virtual_size: u64,
    ) -> ImageService {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        ImageService::with_backend(
            ImageRepository::new(pool),
            Arc::new(FakeStorage {
                present: present.iter().map(|name| name.to_string()).collect(),
                virtual_size,
                fail,
                fail_rollback: false,
                boot_probe: BootProbe::default(),
                snapshots: Vec::new(),
                promoted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                imports: Arc::new(std::sync::Mutex::new(Vec::new())),
                allow_destroy: false,
                log: Arc::new(std::sync::Mutex::new(Vec::new())),
            }),
        )
    }
    async fn service_with_log(
        present: &[&str],
        log: Arc<std::sync::Mutex<Vec<String>>>,
    ) -> ImageService {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        ImageService::with_backend(
            ImageRepository::new(pool),
            Arc::new(FakeStorage {
                present: present.iter().map(|name| name.to_string()).collect(),
                virtual_size: 50 * 1024 * 1024 * 1024,
                fail: false,
                fail_rollback: false,
                boot_probe: BootProbe::default(),
                snapshots: Vec::new(),
                promoted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                imports: Arc::new(std::sync::Mutex::new(Vec::new())),
                log,
                allow_destroy: true,
            }),
        )
    }

    fn image(id: &str, name: &str, parent: Option<&str>) -> Image {
        Image {
            id: id.into(),
            name: name.into(),
            kind: if parent.is_some() {
                ImageKind::Snapshot
            } else {
                ImageKind::Master
            },
            os_type: OsType::Linux,
            size_gb: 50,
            path: PathBuf::from("/dev/zvol/pool/image"),
            format: ImageFormat::Raw,
            status: "ready".into(),
            description: None,
            parent_id: parent.map(str::to_owned),
            source_snapshot: None,
            checksum: None,
            is_default: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn verify_rejects_a_zvol_whose_capacity_differs_from_image_metadata() {
        let service =
            service_with_virtual_size(&["pool/image"], false, 49 * 1024 * 1024 * 1024).await;
        service
            .repository
            .insert(&image("master", "pool/image", None))
            .await
            .unwrap();

        assert!(!service.verify("master").await.unwrap());

        let matching =
            service_with_virtual_size(&["pool/image"], false, 50 * 1024 * 1024 * 1024).await;
        matching
            .repository
            .insert(&image("master", "pool/image", None))
            .await
            .unwrap();
        assert!(matching.verify("master").await.unwrap());
    }

    #[tokio::test]
    async fn verify_returns_false_when_the_zvol_is_missing() {
        let service = service(&[], false).await;
        service
            .repository
            .insert(&image("master", "pool/image", None))
            .await
            .unwrap();

        assert!(!service.verify("master").await.unwrap());
    }

    #[tokio::test]
    async fn health_fails_when_storage_is_missing() {
        let service = service(&[], false).await;
        service
            .repository
            .insert(&image("master", "pool/image", None))
            .await
            .unwrap();

        let health = service.health("master").await.unwrap();

        assert_eq!(health.status, HealthCheckStatus::Fail);
        assert!(health
            .checks
            .iter()
            .any(|check| check.id == "storage" && check.status == HealthCheckStatus::Fail));
    }

    #[tokio::test]
    async fn health_accepts_a_windows_image_with_ntfs_and_an_efi_partition() {
        let virtual_size = 50 * 1024 * 1024 * 1024;
        let mut service = service_with_virtual_size(&["pool/image"], false, virtual_size).await;
        service.backend = Arc::new(FakeStorage {
            present: HashSet::from(["pool/image".to_owned()]),
            virtual_size,
            fail: false,
            fail_rollback: false,
            boot_probe: BootProbe {
                partition_table: Some("gpt".into()),
                filesystem: None,
                partitions: vec![
                    crate::core::image::BootPartition {
                        name: "zd0p1".into(),
                        size: 100 * 1024 * 1024,
                        filesystem: Some("vfat".into()),
                        part_type: Some("EFI System".into()),
                        part_flags: Some("boot".into()),
                    },
                    crate::core::image::BootPartition {
                        name: "zd0p2".into(),
                        size: virtual_size - 100 * 1024 * 1024,
                        filesystem: Some("ntfs".into()),
                        part_type: Some("Microsoft basic data".into()),
                        part_flags: None,
                    },
                ],
            },
            snapshots: Vec::new(),
            promoted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            imports: Arc::new(std::sync::Mutex::new(Vec::new())),
            allow_destroy: false,
            log: Arc::new(std::sync::Mutex::new(Vec::new())),
        });
        let mut master = image("master", "pool/image", None);
        master.os_type = OsType::Windows;
        service.repository.insert(&master).await.unwrap();

        let health = service.health("master").await.unwrap();

        assert_eq!(health.status, HealthCheckStatus::Pass);
        assert_eq!(health.boot.partitions.len(), 2);
        assert!(health.checks.iter().any(|check| {
            check.id == "windows-filesystem" && check.status == HealthCheckStatus::Pass
        }));
        assert!(health.checks.iter().any(|check| {
            check.id == "efi-partition" && check.status == HealthCheckStatus::Pass
        }));
    }

    #[test]
    fn imports_raw_files_directly_and_converts_other_formats_at_the_destination() {
        let imports = Arc::new(std::sync::Mutex::new(Vec::new()));
        let backend = FakeStorage {
            present: HashSet::new(),
            virtual_size: 0,
            fail: false,
            fail_rollback: false,
            boot_probe: BootProbe::default(),
            snapshots: Vec::new(),
            promoted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            imports: Arc::clone(&imports),
            allow_destroy: false,
            log: Arc::new(std::sync::Mutex::new(Vec::new())),
        };

        import_source(
            &backend,
            Path::new("/images/source.raw"),
            ImageFormat::Raw,
            "pool/image-disk/test",
            1024,
        )
        .unwrap();
        import_source(
            &backend,
            Path::new("/images/source.qcow2"),
            ImageFormat::Qcow2,
            "pool/image-disk/test-2",
            2048,
        )
        .unwrap();

        assert_eq!(
            *imports.lock().unwrap(),
            [
                "raw:/images/source.raw:pool/image-disk/test:1024",
                "converted:/images/source.qcow2:pool/image-disk/test-2:2048"
            ]
        );
    }
    #[tokio::test]
    async fn listing_checks_volumes_and_snapshot_names_without_removing_records() {
        let service = service(&["pool/image", "pool/image@present"], false).await;
        for record in [
            image("master", "pool/image", None),
            image("present", "present", Some("master")),
            image("gone", "gone", Some("master")),
            image("missing", "pool/missing", None),
        ] {
            service.repository.insert(&record).await.unwrap();
        }
        let images = service.list().await.unwrap();
        assert_eq!(
            images.iter().find(|i| i.id == "gone").unwrap().status,
            "missing"
        );
        assert_eq!(
            images.iter().find(|i| i.id == "missing").unwrap().status,
            "missing"
        );
        assert_eq!(
            images.iter().find(|i| i.id == "present").unwrap().status,
            "ready"
        );
        assert_eq!(service.repository.list().await.unwrap().len(), 4);
    }

    #[tokio::test]
    async fn rollback_requires_approval_before_pruning_newer_snapshots() {
        let service = service(&[], false).await;
        let now = Utc::now();
        let mut master = image("master", "pool/image", None);
        master.created_at = now;
        let mut old = image("old", "old", Some("master"));
        old.created_at = now - chrono::Duration::minutes(1);
        let mut newer = image("newer", "newer", Some("master"));
        newer.created_at = now + chrono::Duration::minutes(1);
        for record in [&master, &old, &newer] {
            service.repository.insert(record).await.unwrap();
        }

        let result = service.rollback_snapshot("master", "old", false, &[]).await;

        assert!(result
            .unwrap_err()
            .to_string()
            .contains("explicit prune approval"));
        let images = service.repository.list().await.unwrap();
        assert!(images.iter().any(|record| record.id == "newer"));
    }

    #[tokio::test]
    async fn rollback_rejects_a_stale_prune_approval() {
        let service = service(&[], false).await;
        let now = Utc::now();
        let master = image("master", "pool/image", None);
        let mut old = image("old", "old", Some("master"));
        old.created_at = now - chrono::Duration::minutes(1);
        let mut newer = image("newer", "newer", Some("master"));
        newer.created_at = now + chrono::Duration::minutes(1);
        for record in [&master, &old, &newer] {
            service.repository.insert(record).await.unwrap();
        }

        let error = service
            .rollback_snapshot("master", "old", true, &[])
            .await
            .unwrap_err();

        assert!(error.to_string().contains("changed since confirmation"));
        assert!(service.repository.get("newer").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn rollback_prunes_newer_snapshot_metadata_only_after_storage_succeeds() {
        let service = service(&[], false).await;
        let now = Utc::now();
        let master = image("master", "pool/image", None);
        let mut old = image("old", "old", Some("master"));
        old.created_at = now - chrono::Duration::minutes(1);
        let mut newer = image("newer", "newer", Some("master"));
        newer.created_at = now + chrono::Duration::minutes(1);
        for record in [&master, &old, &newer] {
            service.repository.insert(record).await.unwrap();
        }

        assert_eq!(
            service
                .rollback_snapshot("master", "old", true, &["newer".to_string()])
                .await
                .unwrap(),
            1
        );
        let images = service.repository.list().await.unwrap();
        assert!(!images.iter().any(|record| record.id == "newer"));
        assert!(images.iter().any(|record| record.id == "old"));
    }

    #[tokio::test]
    async fn rollback_failure_keeps_newer_snapshot_metadata() {
        let mut service = service(&[], false).await;
        service.backend = Arc::new(FakeStorage {
            present: HashSet::new(),
            virtual_size: 0,
            fail: false,
            fail_rollback: true,
            boot_probe: BootProbe::default(),
            snapshots: Vec::new(),
            promoted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            imports: Arc::new(std::sync::Mutex::new(Vec::new())),
            allow_destroy: false,
            log: Arc::new(std::sync::Mutex::new(Vec::new())),
        });
        let master = image("master", "pool/image", None);
        let mut old = image("old", "old", Some("master"));
        old.created_at = Utc::now() - chrono::Duration::minutes(1);
        let mut newer = image("newer", "newer", Some("master"));
        newer.created_at = Utc::now() + chrono::Duration::minutes(1);
        for record in [&master, &old, &newer] {
            service.repository.insert(record).await.unwrap();
        }

        assert!(service
            .rollback_snapshot("master", "old", true, &["newer".to_string()])
            .await
            .is_err());
        assert!(service
            .repository
            .list()
            .await
            .unwrap()
            .iter()
            .any(|record| record.id == "newer"));
    }

    #[tokio::test]
    async fn rollback_refuses_images_with_dependent_clones() {
        let service = service(&[], false).await;
        let master = image("master", "pool/image", None);
        let mut snapshot = image("snapshot", "stable", Some("master"));
        snapshot.created_at = Utc::now() - chrono::Duration::minutes(1);
        let mut clone = image("clone", "pool/clone", Some("master"));
        clone.kind = ImageKind::Clone;
        for record in [&master, &snapshot, &clone] {
            service.repository.insert(record).await.unwrap();
        }

        let error = service
            .rollback_snapshot("master", "stable", true, &[])
            .await
            .unwrap_err();

        assert!(error.to_string().contains("dependent clones"));
        assert!(service.repository.get("snapshot").await.unwrap().is_some());
    }
    #[tokio::test]
    async fn rollback_refuses_clients_assigned_to_a_newer_snapshot() {
        let service = service(&[], false).await;
        let now = Utc::now();
        let master = image("master", "pool/image", None);
        let mut old = image("old", "old", Some("master"));
        old.created_at = now - chrono::Duration::minutes(1);
        let mut newer = image("newer", "newer", Some("master"));
        newer.created_at = now + chrono::Duration::minutes(1);
        for record in [&master, &old, &newer] {
            service.repository.insert(record).await.unwrap();
        }
        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, snapshot, created_at, updated_at) VALUES ('client', 'PC-01', '02:00:00:00:00:01', '192.0.2.10', 'pool/image', 'pool/image@newer', '2026-10-12', '2026-10-12')")
            .execute(service.repository.test_pool())
            .await
            .unwrap();

        let error = service
            .rollback_snapshot("master", "old", true, &["newer".to_string()])
            .await
            .unwrap_err();

        assert!(error
            .to_string()
            .contains("clients are assigned to newer snapshots"));
        assert!(service.repository.get("newer").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn rollback_refuses_untracked_zfs_snapshots() {
        let mut service = service(&[], false).await;
        service.backend = Arc::new(FakeStorage {
            present: HashSet::new(),
            virtual_size: 0,
            fail: false,
            fail_rollback: false,
            boot_probe: BootProbe::default(),
            snapshots: vec!["manual-backup".into()],
            promoted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            imports: Arc::new(std::sync::Mutex::new(Vec::new())),
            allow_destroy: false,
            log: Arc::new(std::sync::Mutex::new(Vec::new())),
        });
        let now = Utc::now();
        let master = image("master", "pool/image", None);
        let mut old = image("old", "old", Some("master"));
        old.created_at = now - chrono::Duration::minutes(1);
        for record in [&master, &old] {
            service.repository.insert(record).await.unwrap();
        }

        let error = service
            .rollback_snapshot("master", "old", true, &[])
            .await
            .unwrap_err();

        assert!(error.to_string().contains("untracked ZFS snapshots"));
        assert!(error.to_string().contains("manual-backup"));
    }

    #[tokio::test]
    async fn missing_snapshot_then_missing_default_can_be_removed() {
        let service = service(&[], false).await;
        let mut master = image("master", "pool/image", None);
        master.is_default = true;
        service.repository.insert(&master).await.unwrap();
        service
            .repository
            .insert(&image("snap", "gone", Some("master")))
            .await
            .unwrap();
        service.delete("snap").await.unwrap();
        service.delete("master").await.unwrap();
        assert!(service.repository.list().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn promoting_serviced_clone_severs_its_origin_and_is_retry_safe() {
        let service = service(&["pool/clone"], false).await;
        service.promote_clone("pool/clone").await.unwrap();
        service.promote_clone("pool/clone").await.unwrap();
        assert_eq!(service.backend.clone_origin("pool/clone").unwrap(), None);
    }
    #[tokio::test]
    async fn failed_query_never_allows_metadata_deletion() {
        let service = service(&[], true).await;
        service
            .repository
            .insert(&image("master", "pool/image", None))
            .await
            .unwrap();
        assert!(service.delete("master").await.is_err());
        assert!(service.repository.get("master").await.unwrap().is_some());
        assert!(service.list().await.is_err());
    }

    #[tokio::test]
    async fn awaiting_windows_revision_can_replace_the_default() {
        let service = service(&[], false).await;
        let mut current = image("current", "pool/current", None);
        current.is_default = true;
        service.repository.insert(&current).await.unwrap();
        let candidate = image("candidate", "pool/candidate", None);
        service.repository.insert(&candidate).await.unwrap();
        service
            .repository
            .insert_driver_revision(&crate::core::image::ImageDriverRevision {
                id: "revision".into(),
                source_image_id: "current".into(),
                source_snapshot: "pool/current@snap".into(),
                revision_image_id: Some("candidate".into()),
                driver_package_ids: vec![],
                driver_set_hash: "hash".into(),
                status: crate::core::image::DriverRevisionStatus::AwaitingTest,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .await
            .unwrap();

        assert!(service.set_default("candidate").await.unwrap().is_default);
        assert!(
            !service
                .repository
                .get("current")
                .await
                .unwrap()
                .unwrap()
                .is_default
        );
        assert!(
            service
                .repository
                .get("candidate")
                .await
                .unwrap()
                .unwrap()
                .is_default
        );

        assert!(!service.repository.set_default("missing").await.unwrap());
        assert!(
            service
                .repository
                .get("candidate")
                .await
                .unwrap()
                .unwrap()
                .is_default
        );
    }

    #[tokio::test]
    async fn revision_can_be_default_without_a_matching_test_for_every_recorded_nic() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let service = ImageService::with_backend(
            ImageRepository::new(pool.clone()),
            Arc::new(FakeStorage {
                present: HashSet::new(),
                virtual_size: 0,
                fail: false,
                fail_rollback: false,
                boot_probe: BootProbe::default(),
                snapshots: Vec::new(),
                promoted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                imports: Arc::new(std::sync::Mutex::new(Vec::new())),
                allow_destroy: false,
                log: Arc::new(std::sync::Mutex::new(Vec::new())),
            }),
        );
        let mut current = image("current", "pool/current", None);
        current.is_default = true;
        service.repository.insert(&current).await.unwrap();
        service
            .repository
            .insert(&image("candidate", "pool/candidate", None))
            .await
            .unwrap();
        sqlx::query("INSERT INTO clients(id, name, mac, ip, master, created_at, updated_at) VALUES ('test-client', 'test-client', '02:00:00:00:00:01', '192.0.2.10', 'pool/candidate', '2026-10-12T00:00:00Z', '2026-10-12T00:00:00Z')")
            .execute(&pool)
            .await
            .unwrap();
        service
            .repository
            .insert_driver_revision(&crate::core::image::ImageDriverRevision {
                id: "revision".into(),
                source_image_id: "current".into(),
                source_snapshot: "pool/current@snap".into(),
                revision_image_id: Some("candidate".into()),
                driver_package_ids: vec!["nic-b".into()],
                driver_set_hash: "hash".into(),
                status: crate::core::image::DriverRevisionStatus::Ready,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .await
            .unwrap();
        sqlx::query("INSERT INTO windows_nic_inventory (mac, boot_nic_ids, observed_at) VALUES ('02:00:00:00:00:01', '[\"PCI\\\\VEN_10EC&DEV_8168\"]', '2026-10-12T00:00:00Z')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO windows_revision_boot_tests (id, revision_id, test_client_id, expected_nic_id, candidate_image_id, candidate_snapshot, status, reported_revision, reported_nic_id, result_at, created_at) VALUES ('test', 'revision', 'test-client', 'PCI\\VEN_8086&DEV_15F3', 'candidate', '', 'passed', 'revision', 'PCI\\VEN_8086&DEV_15F3', '2026-10-12T00:00:00Z', '2026-10-12T00:00:00Z')")
            .execute(&pool)
            .await
            .unwrap();

        assert!(service.set_default("candidate").await.unwrap().is_default);
        assert!(
            !service
                .repository
                .get("current")
                .await
                .unwrap()
                .unwrap()
                .is_default
        );
    }

    #[tokio::test]
    async fn deleting_boot_test_candidate_preserves_history() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let mut service = service(&[], false).await;
        service.repository = ImageRepository::new(pool.clone());
        for id in ["source", "candidate"] {
            service
                .repository
                .insert(&image(id, &format!("pool/{id}"), None))
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO windows_image_driver_revisions (id, source_image_id, source_snapshot, driver_set_hash, status, created_at, updated_at) VALUES ('revision', 'source', 'pool/source@snap', 'hash', 'ready', '2026-10-12', '2026-10-12')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO windows_revision_boot_tests (id, revision_id, test_client_id, expected_nic_id, candidate_image_id, candidate_snapshot, status, created_at) VALUES ('test', 'revision', 'client', 'nic', 'candidate', 'pool/candidate@snap', 'passed', '2026-10-12')")
            .execute(&pool).await.unwrap();

        service.backend = Arc::new(FakeStorage {
            present: HashSet::from(["pool/candidate".to_owned()]),
            virtual_size: 50 * 1024 * 1024 * 1024,
            fail: false,
            fail_rollback: false,
            boot_probe: BootProbe::default(),
            snapshots: Vec::new(),
            promoted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            imports: Arc::new(std::sync::Mutex::new(Vec::new())),
            allow_destroy: false,
            log: Arc::new(std::sync::Mutex::new(Vec::new())),
        });
        assert!(service
            .delete("candidate")
            .await
            .unwrap_err()
            .to_string()
            .contains("failed to delete image"));
        assert!(service.get_optional("candidate").await.unwrap().is_some());
        let candidate: Option<String> = sqlx::query_scalar(
            "SELECT candidate_image_id FROM windows_revision_boot_tests WHERE id = 'test'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(candidate.as_deref(), Some("candidate"));
        service.backend = Arc::new(FakeStorage {
            present: HashSet::new(),
            virtual_size: 0,
            fail: false,
            fail_rollback: false,
            boot_probe: BootProbe::default(),
            snapshots: Vec::new(),
            promoted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            imports: Arc::new(std::sync::Mutex::new(Vec::new())),
            allow_destroy: false,
            log: Arc::new(std::sync::Mutex::new(Vec::new())),
        });

        service.delete("candidate").await.unwrap();
        assert!(service.get_optional("candidate").await.unwrap().is_none());
        let history: (Option<String>, String, String) = sqlx::query_as("SELECT candidate_image_id, candidate_snapshot, status FROM windows_revision_boot_tests WHERE id = 'test'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(
            history,
            (None, "pool/candidate@snap".into(), "passed".into())
        );
    }

    #[tokio::test]
    async fn referenced_source_is_rejected_before_destroying_storage() {
        let service = service(&["pool/source"], false).await;
        service
            .repository
            .insert(&image("source", "pool/source", None))
            .await
            .unwrap();
        service
            .repository
            .insert_driver_revision(&crate::core::image::ImageDriverRevision {
                id: "revision".into(),
                source_image_id: "source".into(),
                source_snapshot: "pool/source@snap".into(),
                revision_image_id: None,
                driver_package_ids: vec![],
                driver_set_hash: "hash".into(),
                status: crate::core::image::DriverRevisionStatus::Ready,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .await
            .unwrap();
        let error = service.delete("source").await.unwrap_err();
        assert!(
            format!("{error:#}").contains("required by driver revisions or super sessions"),
            "{error:#}"
        );
        assert!(service.get_optional("source").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn existing_default_and_dependent_records_remain_protected() {
        let service = service(&["pool/image"], false).await;
        let mut master = image("master", "pool/image", None);
        master.is_default = true;
        service.repository.insert(&master).await.unwrap();
        assert!(service
            .delete("master")
            .await
            .unwrap_err()
            .to_string()
            .contains("default"));
        master.is_default = false;
        service.repository.update(&master).await.unwrap();
        service
            .repository
            .insert(&image("snap", "gone", Some("master")))
            .await
            .unwrap();
        assert!(service
            .delete("master")
            .await
            .unwrap_err()
            .to_string()
            .contains("dependent"));
        assert_eq!(service.repository.list().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn applying_a_snapshot_creates_an_assignable_master_and_preserves_the_source() {
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let service = service_with_log(&["pool/image"], log.clone()).await;
        service
            .repository
            .insert(&image("master", "pool/image", None))
            .await
            .unwrap();
        service
            .repository
            .insert(&image("snap", "stable", Some("master")))
            .await
            .unwrap();

        let applied = service
            .apply_snapshot("master", "stable", "image-restored")
            .await
            .unwrap();

        assert_eq!(applied.name, "pool/image-restored");
        assert_eq!(applied.kind, ImageKind::Master);
        assert_eq!(applied.parent_id, None);
        assert_eq!(applied.source_snapshot, None);
        assert_eq!(applied.status, "ready");
        assert_eq!(
            applied.description.as_deref(),
            Some("Restore point of pool/image@stable")
        );

        let source_snapshots = service.snapshots("master").await.unwrap();
        assert_eq!(source_snapshots.len(), 1);
        assert_eq!(source_snapshots[0].id, "snap");
        assert_eq!(
            log.lock().unwrap().as_slice(),
            ["clone:pool/image@stable:pool/image-restored"]
        );
    }

    #[tokio::test]
    async fn cloning_a_snapshot_still_records_a_dependent_clone() {
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let service = service_with_log(&["pool/image"], log.clone()).await;
        service
            .repository
            .insert(&image("master", "pool/image", None))
            .await
            .unwrap();
        service
            .repository
            .insert(&image("snap", "stable", Some("master")))
            .await
            .unwrap();

        let clone = service
            .clone_image("master", "stable", "image-clone")
            .await
            .unwrap();

        assert_eq!(clone.kind, ImageKind::Clone);
        assert_eq!(clone.parent_id.as_deref(), Some("master"));
        assert_eq!(clone.source_snapshot.as_deref(), Some("stable"));
        assert_eq!(
            log.lock().unwrap().as_slice(),
            ["clone:pool/image@stable:pool/image-clone"]
        );
    }

    #[tokio::test]
    async fn applying_a_missing_snapshot_fails_without_creating_an_image() {
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let service = service_with_log(&["pool/image"], log.clone()).await;
        service
            .repository
            .insert(&image("master", "pool/image", None))
            .await
            .unwrap();

        let error = service
            .apply_snapshot("master", "missing", "image-restored")
            .await
            .unwrap_err();

        assert!(error.to_string().contains("snapshot 'missing' not found"));
        assert!(log.lock().unwrap().is_empty());
        assert!(service.snapshots("master").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn applying_from_a_snapshot_record_is_rejected() {
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let service = service_with_log(&["pool/image"], log.clone()).await;
        service
            .repository
            .insert(&image("master", "pool/image", None))
            .await
            .unwrap();
        service
            .repository
            .insert(&image("snap", "stable", Some("master")))
            .await
            .unwrap();

        let error = service
            .apply_snapshot("snap", "stable", "image-restored")
            .await
            .unwrap_err();

        assert!(error
            .to_string()
            .contains("cannot create an image from a snapshot record directly"));
        assert!(log.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn applying_to_an_existing_dataset_is_rejected() {
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let service = service_with_log(&["pool/image", "pool/existing"], log.clone()).await;
        service
            .repository
            .insert(&image("master", "pool/image", None))
            .await
            .unwrap();
        service
            .repository
            .insert(&image("snap", "stable", Some("master")))
            .await
            .unwrap();

        let error = service
            .apply_snapshot("master", "stable", "existing")
            .await
            .unwrap_err();

        assert!(error.to_string().contains("already exists"));
        assert!(log.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn applying_destroys_the_clone_when_metadata_insertion_fails() {
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let service = service_with_log(&["pool/image"], log.clone()).await;
        service
            .repository
            .insert(&image("master", "pool/image", None))
            .await
            .unwrap();
        service
            .repository
            .insert(&image("snap", "stable", Some("master")))
            .await
            .unwrap();
        sqlx::query(
            "CREATE TRIGGER fail_apply_insert BEFORE INSERT ON images \
             BEGIN SELECT RAISE(ABORT, 'forced insert failure'); END",
        )
        .execute(service.repository.test_pool())
        .await
        .unwrap();

        let error = service
            .apply_snapshot("master", "stable", "image-restored")
            .await
            .unwrap_err();

        assert!(format!("{error:#}").contains("failed to persist image from snapshot"));
        assert_eq!(
            log.lock().unwrap().as_slice(),
            [
                "clone:pool/image@stable:pool/image-restored",
                "destroy:pool/image-restored"
            ]
        );
        assert_eq!(service.snapshots("master").await.unwrap().len(), 1);
    }
}
