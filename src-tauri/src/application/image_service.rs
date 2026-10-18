use anyhow::{bail, Context, Result};
use chrono::Utc;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    core::image::{
        CreateImageRequest, Image, ImageFormat, ImageInfo, ImageKind, ImportImageRequest, OsType,
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
        let service = self.clone();

        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || runtime.block_on(service.create_blocking(request)))
            .await
            .context("image worker failed")?
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
        let service = self.clone();
        let id = id.to_owned();
        let new_name = new_name.to_owned();
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            runtime.block_on(service.rename_blocking(&id, &new_name))
        })
        .await
        .context("image worker failed")?
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
        let service = self.clone();
        let id = id.to_owned();
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || runtime.block_on(service.update_blocking(&id, request)))
            .await
            .context("image worker failed")?
    }

    async fn update_blocking(&self, id: &str, request: UpdateImageRequest) -> Result<Image> {
        let mut image = self.get(id).await?;

        if let Some(name) = request.name {
            image = self.rename(&image.id, &name).await?;
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
        let service = self.clone();
        let source_id = source_id.to_owned();
        let snapshot_name = snapshot_name.to_owned();
        let new_name = new_name.to_owned();
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            runtime.block_on(service.clone_image_blocking(&source_id, &snapshot_name, &new_name))
        })
        .await
        .context("image worker failed")?
    }

    async fn clone_image_blocking(
        &self,
        source_id: &str,
        snapshot_name: &str,
        new_name: &str,
    ) -> Result<Image> {
        validate_zfs_name(new_name)?;

        if snapshot_name.trim().is_empty() {
            bail!("snapshot name cannot be empty");
        }

        let source = self.get(source_id).await?;

        /*
         * A clone can be created from either:
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
                bail!("cannot clone a snapshot image directly");
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

        let image = Image {
            id: Uuid::new_v4().to_string(),

            name: destination.clone(),

            kind: ImageKind::Clone,

            os_type: source.os_type,

            size_gb: source.size_gb,

            path: PathBuf::from(format!("/dev/zvol/{}", destination)),

            format: source.format,

            status: "ready".to_string(),

            description: Some(format!("Clone of {}@{}", source.name, snapshot.name)),

            /*
             * Keep the logical source image.
             */
            parent_id: Some(source.id.clone()),

            /*
             * Record the exact snapshot used to create this clone.
             */
            source_snapshot: Some(snapshot.name.clone()),

            checksum: None,

            is_default: false,

            created_at: now,

            updated_at: now,
        };

        if let Err(error) = self.repository.insert(&image).await {
            let _ = self.backend.destroy(&destination);

            return Err(error).context("failed to persist cloned image");
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
    ) -> Result<usize> {
        let service = self.clone();
        let master_id_or_name = master_id_or_name.to_owned();
        let snapshot_name = snapshot_name.to_owned();
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            runtime.block_on(service.rollback_snapshot_blocking(&master_id_or_name, &snapshot_name))
        })
        .await
        .context("image worker failed")?
    }

    async fn rollback_snapshot_blocking(
        &self,
        master_id_or_name: &str,
        snapshot_name: &str,
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

        let newer_ids: Vec<String> = snapshots
            .iter()
            .filter(|snapshot| snapshot.created_at > target.created_at)
            .map(|snapshot| snapshot.id.clone())
            .collect();

        self.backend.rollback_snapshot(&master.name, &target.name)?;

        for id in &newer_ids {
            self.repository.delete(id).await?;
        }

        Ok(newer_ids.len())
    }

    pub async fn import(&self, request: ImportImageRequest) -> Result<Image> {
        let service = self.clone();

        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || runtime.block_on(service.import_blocking(request)))
            .await
            .context("image worker failed")?
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

        /*
         * qemu-img always converts into a
         * temporary raw file before it enters
         * the ZFS ZVOL.
         */
        let temp_path =
            std::env::temp_dir().join(format!("diskless-import-{}.raw", Uuid::new_v4()));

        let conversion_result = if source_info.format == ImageFormat::Raw {
            std::fs::copy(source, &temp_path)
                .map(|_| ())
                .map_err(anyhow::Error::from)
        } else {
            converter.convert_to_raw(source, &temp_path)
        };

        if let Err(error) = conversion_result {
            let _ = std::fs::remove_file(&temp_path);

            return Err(error).context("failed to convert source image to raw");
        }

        let import_result = self
            .backend
            .import_raw(&temp_path, &destination, size_bytes);

        let _ = std::fs::remove_file(&temp_path);

        import_result?;

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
        let service = self.clone();
        let id = id.to_owned();
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            runtime.block_on(service.resize_blocking(&id, new_size_gb))
        })
        .await
        .context("image worker failed")?
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
        let service = self.clone();
        let id = id.to_owned();
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || runtime.block_on(service.verify_blocking(&id)))
            .await
            .context("image worker failed")?
    }

    async fn verify_blocking(&self, id: &str) -> Result<bool> {
        let image = self.get(id).await?;

        self.backend.verify(&image.name)
    }

    pub async fn get_info(&self, id: &str) -> Result<ImageInfo> {
        let service = self.clone();
        let id = id.to_owned();
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || runtime.block_on(service.get_info_blocking(&id)))
            .await
            .context("image worker failed")?
    }

    async fn get_info_blocking(&self, id: &str) -> Result<ImageInfo> {
        let image = self.get(id).await?;

        let info = self.backend.info(&image.name)?;

        Ok(info.into())
    }

    pub async fn create_snapshot(&self, id: &str, snapshot_name: &str) -> Result<Image> {
        let service = self.clone();
        let id = id.to_owned();
        let snapshot_name = snapshot_name.to_owned();
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            runtime.block_on(service.create_snapshot_blocking(&id, &snapshot_name))
        })
        .await
        .context("image worker failed")?
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
        let service = self.clone();
        let id = id.to_owned();
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || runtime.block_on(service.delete_blocking(&id)))
            .await
            .context("image worker failed")?
    }

    async fn delete_blocking(&self, id: &str) -> Result<()> {
        let image = self.get(id).await?;

        if image.is_default && self.backend.exists(&image.name)? {
            bail!("cannot delete the default image");
        }

        match image.kind {
            ImageKind::Snapshot => {
                /*
                 * A snapshot is represented in the database as:
                 *
                 *     name = "v1"
                 *     parent_id = source image ID
                 *
                 * The actual ZFS object is:
                 *
                 *     source.name@image.name
                 */
                let parent_id = image.parent_id.as_deref().ok_or_else(|| {
                    anyhow::anyhow!("snapshot '{}' has no parent image", image.name)
                })?;

                let parent = self.get(parent_id).await?;

                if self
                    .backend
                    .exists(&format!("{}@{}", parent.name, image.name))?
                {
                    self.backend
                        .destroy_snapshot(&parent.name, &image.name)
                        .with_context(|| {
                            format!("failed to delete snapshot '{}@{}'", parent.name, image.name)
                        })?;
                }
            }

            ImageKind::Master | ImageKind::Clone => {
                /*
                 * Both masters and clones are ZFS volumes.
                 *
                 * Therefore both must use:
                 *
                 *     zfs destroy <dataset>
                 *
                 * NOT destroy_snapshot().
                 */

                let children = self.repository.list().await?;

                let has_children = children
                    .iter()
                    .any(|candidate| candidate.parent_id.as_deref() == Some(&image.id));

                if has_children {
                    bail!(
                        "cannot delete image '{}' while dependent snapshots or clones exist",
                        image.name
                    );
                }

                if self.backend.exists(&image.name)? {
                    self.backend
                        .destroy(&image.name)
                        .with_context(|| format!("failed to delete image '{}'", image.name))?;
                }
            }
        }

        self.repository
            .delete(&image.id)
            .await
            .context("failed to remove image metadata")?;

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
        let service = self.clone();

        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            runtime.block_on(service.import_existing_images_blocking())
        })
        .await
        .context("image worker failed")?
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

#[cfg(test)]
mod storage_presence_tests {
    use super::*;
    use crate::infrastructure::image::ImageBackendInfo;
    use std::collections::HashSet;
    use std::path::Path;

    struct FakeStorage {
        present: HashSet<String>,
        fail: bool,
        promoted: Arc<std::sync::atomic::AtomicBool>,
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
        fn destroy(&self, _: &str) -> Result<()> {
            bail!("unexpected destroy")
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
        fn clone_image(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn create_snapshot(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn rollback_snapshot(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn resize(&self, _: &str, _: u64) -> Result<()> {
            unreachable!()
        }
        fn import_raw(&self, _: &Path, _: &str, _: u64) -> Result<()> {
            unreachable!()
        }
        fn verify(&self, _: &str) -> Result<bool> {
            unreachable!()
        }
        fn info(&self, _: &str) -> Result<ImageBackendInfo> {
            unreachable!()
        }
        fn set_os_type(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn image_parent(&self) -> Result<String> {
            unreachable!()
        }
    }
    async fn service(present: &[&str], fail: bool) -> ImageService {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        ImageService::with_backend(
            ImageRepository::new(pool),
            Arc::new(FakeStorage {
                present: present.iter().map(|name| name.to_string()).collect(),
                fail,
                promoted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
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
                fail: false,
                promoted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
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
}
