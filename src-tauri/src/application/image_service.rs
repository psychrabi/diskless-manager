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
        self.repository.list().await
    }

    pub async fn get(&self, id_or_name: &str) -> Result<Image> {
        self.repository
            .get(id_or_name)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Image not found: {}", id_or_name))
    }

    pub async fn create(&self, request: CreateImageRequest) -> Result<Image> {
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

    pub async fn rollback_snapshot(
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
        let image = self.get(id).await?;

        self.backend.verify(&image.name)
    }

    pub async fn get_info(&self, id: &str) -> Result<ImageInfo> {
        let image = self.get(id).await?;

        let info = self.backend.info(&image.name)?;

        Ok(info.into())
    }

    pub async fn create_snapshot(&self, id: &str, snapshot_name: &str) -> Result<Image> {
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
        let image = self.get(id).await?;

        if image.is_default {
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

                self.backend.destroy_snapshot(&parent.name, &image.name)?;
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
                    self.backend.destroy(&image.name)?;
                }
            }
        }

        self.repository.delete(&image.id).await?;

        Ok(())
    }

    pub async fn set_default(&self, id_or_name: &str) -> Result<Image> {
        let image = self.get(id_or_name).await?;

        if image.kind == ImageKind::Snapshot {
            bail!("a snapshot cannot be the default image");
        }

        self.repository.clear_default().await?;

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
