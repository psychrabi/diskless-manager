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

#[derive(Debug, thiserror::Error)]
#[error("cannot rename an image referenced by a client")]
pub struct ImageInUse;

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

        if self.repository.has_client_references(&image).await? {
            return Err(ImageInUse.into());
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
        for mut snapshot in self.snapshots(&image.id).await? {
            snapshot.path = image.path.clone();
            snapshot.updated_at = image.updated_at;
            self.repository.update(&snapshot).await?;
        }

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
        use crate::infrastructure::image::QemuImgBackend;

        self.import_with_converter(request, &QemuImgBackend::new())
            .await
    }

    async fn import_with_converter(
        &self,
        request: ImportImageRequest,
        converter: &dyn crate::infrastructure::image::ImageConversionBackend,
    ) -> Result<Image> {
        validate_zfs_name(&request.name)?;

        let source = std::path::Path::new(&request.source_path);

        if !source.exists() {
            bail!("source image '{}' does not exist", request.source_path);
        }

        let os_type = request.os_type.parse::<OsType>()?;

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

        // Raw sources can be streamed straight into the ZVOL. Only converted
        // formats need temporary disk space; TempDir cleans up on every exit.
        let temporary = if source_info.format == ImageFormat::Raw {
            None
        } else {
            Some(tempfile::tempdir().context("failed to create conversion directory")?)
        };
        let converted = temporary
            .as_ref()
            .map(|directory| directory.path().join("image.raw"));
        if let Some(path) = &converted {
            converter
                .convert_to_raw(source, path)
                .context("failed to convert source image to raw")?;
        }
        self.backend.import_raw(
            converted.as_deref().unwrap_or(source),
            &destination,
            size_bytes,
        )?;

        if let Err(error) = self.backend.set_os_type(&destination, &request.os_type) {
            self.backend
                .destroy(&destination)
                .with_context(|| format!("failed to clean imported image after: {error}"))?;
            return Err(error).context("failed to set imported image OS type");
        }

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
            self.backend
                .destroy(&destination)
                .with_context(|| format!("failed to clean imported image after: {error}"))?;
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

        if !self.repository.set_default(&image.id).await? {
            bail!("image disappeared before selecting the default");
        }

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
mod audit_tests {
    use super::*;
    use crate::infrastructure::image::ImageBackendInfo;
    use std::path::Path;
    #[derive(Default)]
    struct TestBackend {
        imported: std::sync::Mutex<Option<PathBuf>>,
        volume_present: std::sync::atomic::AtomicBool,
        fail_os_type: bool,
        fail_destroy: bool,
    }

    impl ImageBackend for TestBackend {
        fn exists(&self, _: &str) -> Result<bool> {
            Ok(false)
        }
        fn create_volume(&self, _: &str, _: u64) -> Result<()> {
            unreachable!()
        }
        fn destroy(&self, _: &str) -> Result<()> {
            if self.fail_destroy {
                bail!("injected cleanup failure");
            }
            self.volume_present
                .store(false, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
        fn rename(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn clone_image(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn create_snapshot(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn destroy_snapshot(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn rollback_snapshot(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn resize(&self, _: &str, _: u64) -> Result<()> {
            unreachable!()
        }
        fn import_raw(&self, source: &Path, _: &str, _: u64) -> Result<()> {
            self.volume_present
                .store(true, std::sync::atomic::Ordering::SeqCst);
            *self.imported.lock().unwrap() = Some(source.to_path_buf());
            Ok(())
        }
        fn verify(&self, _: &str) -> Result<bool> {
            unreachable!()
        }
        fn info(&self, _: &str) -> Result<ImageBackendInfo> {
            unreachable!()
        }
        fn set_os_type(&self, _: &str, _: &str) -> Result<()> {
            if self.fail_os_type {
                bail!("injected OS property failure");
            }
            Ok(())
        }
        fn image_parent(&self) -> Result<String> {
            Ok("tank/image".into())
        }
    }

    async fn service() -> (ImageService, ImageRepository, sqlx::SqlitePool) {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let repo = ImageRepository::new(pool.clone());
        (
            ImageService::with_backend(repo.clone(), Arc::new(TestBackend::default())),
            repo,
            pool,
        )
    }
    fn image(id: &str, kind: ImageKind, parent: Option<&str>, seconds: i64) -> Image {
        Image {
            id: id.into(),
            name: if parent.is_none() {
                "tank/image/master".into()
            } else {
                id.into()
            },
            kind,
            parent_id: parent.map(str::to_string),
            os_type: OsType::Windows,
            size_gb: 20,
            path: PathBuf::from("/dev/zvol/tank/image/master"),
            format: ImageFormat::Raw,
            status: "ready".into(),
            description: None,
            source_snapshot: None,
            checksum: None,
            is_default: false,
            created_at: chrono::DateTime::from_timestamp(seconds, 0).unwrap(),
            updated_at: Utc::now(),
        }
    }
    #[tokio::test]
    async fn failed_default_switch_preserves_previous_default() {
        let (service, repo, pool) = service().await;
        let mut previous = image("previous", ImageKind::Master, None, 0);
        previous.is_default = true;
        repo.insert(&previous).await.unwrap();
        let mut next = image("next", ImageKind::Master, None, 0);
        next.name = "tank/image/next".into();
        repo.insert(&next).await.unwrap();
        sqlx::query("CREATE TRIGGER reject_default BEFORE UPDATE ON images WHEN NEW.id = 'next' AND NEW.is_default = 1 BEGIN SELECT RAISE(ABORT, 'injected failure'); END")
            .execute(&pool).await.unwrap();
        assert!(service.set_default("next").await.is_err());
        assert!(repo.get("previous").await.unwrap().unwrap().is_default);
    }

    #[tokio::test]
    async fn stale_metadata_update_preserves_current_default() {
        let (service, repo, _) = service().await;
        let mut previous = image("previous", ImageKind::Master, None, 0);
        previous.is_default = true;
        repo.insert(&previous).await.unwrap();
        let mut next = image("next", ImageKind::Master, None, 0);
        next.name = "tank/image/next".into();
        repo.insert(&next).await.unwrap();
        service.set_default("next").await.unwrap();
        previous.description = Some("Edited after switching default".into());
        repo.update(&previous).await.unwrap();
        assert!(!repo.get("previous").await.unwrap().unwrap().is_default);
        assert!(repo.get("next").await.unwrap().unwrap().is_default);
    }

    #[tokio::test]
    async fn rollback_preserves_clones_and_removes_only_newer_snapshots() {
        let (service, repo, _) = service().await;
        for record in [
            image("master", ImageKind::Master, None, 0),
            image("ready", ImageKind::Snapshot, Some("master"), 1),
            image("later", ImageKind::Snapshot, Some("master"), 2),
            image("clone", ImageKind::Clone, Some("master"), 3),
        ] {
            repo.insert(&record).await.unwrap();
        }
        assert_eq!(
            service.rollback_snapshot("master", "ready").await.unwrap(),
            1
        );
        assert!(repo.get("clone").await.unwrap().is_some());
        assert!(repo.get("ready").await.unwrap().is_some());
        assert!(repo.get("later").await.unwrap().is_none());
        assert!(service.rollback_snapshot("master", "clone").await.is_err());
    }
    #[tokio::test]
    async fn rename_refuses_registered_client_dependencies() {
        let (service, repo, pool) = service().await;
        repo.insert(&image("master", ImageKind::Master, None, 0))
            .await
            .unwrap();
        sqlx::query("INSERT INTO clients (id,name,mac,ip,master,enabled,created_at,updated_at) VALUES ('pc','PC001','00:11:22:33:44:55','192.168.1.101','tank/image/master',1,'now','now')")
            .execute(&pool).await.unwrap();
        assert!(service.rename("master", "renamed").await.is_err());
        assert_eq!(
            repo.get("master").await.unwrap().unwrap().name,
            "tank/image/master"
        );
    }
    #[tokio::test]
    async fn unreferenced_image_rename_updates_snapshot_paths() {
        let (service, repo, _) = service().await;
        repo.insert(&image("master", ImageKind::Master, None, 0))
            .await
            .unwrap();
        repo.insert(&image("ready", ImageKind::Snapshot, Some("master"), 1))
            .await
            .unwrap();
        let renamed = service.rename("master", "renamed").await.unwrap();
        assert_eq!(renamed.name, "tank/image/renamed");
        assert_eq!(repo.get("ready").await.unwrap().unwrap().path, renamed.path);
    }

    struct TestConverter(ImageFormat);
    impl crate::infrastructure::image::ImageConversionBackend for TestConverter {
        fn info(&self, source: &Path) -> Result<crate::infrastructure::image::ImageConversionInfo> {
            Ok(crate::infrastructure::image::ImageConversionInfo {
                format: self.0,
                virtual_size: std::fs::metadata(source)?.len(),
                actual_size: 4096,
                backing_file: None,
            })
        }
        fn convert_to_raw(&self, source: &Path, destination: &Path) -> Result<()> {
            assert_ne!(
                self.0,
                ImageFormat::Raw,
                "raw imports must not be converted"
            );
            std::fs::copy(source, destination)?;
            Ok(())
        }
    }
    #[tokio::test]
    async fn failed_import_metadata_removes_unregistered_volume() {
        let (_, repo, _) = service().await;
        let backend = Arc::new(TestBackend {
            fail_os_type: true,
            ..TestBackend::default()
        });
        let service = ImageService::with_backend(repo.clone(), backend.clone());
        let source = tempfile::NamedTempFile::new().unwrap();
        source.as_file().set_len(4096).unwrap();
        let result = service
            .import_with_converter(
                ImportImageRequest {
                    name: "failed".into(),
                    source_path: source.path().to_string_lossy().into_owned(),
                    os_type: "windows".into(),
                    description: None,
                },
                &TestConverter(ImageFormat::Raw),
            )
            .await;
        assert!(result.is_err());
        assert!(!backend
            .volume_present
            .load(std::sync::atomic::Ordering::SeqCst));
        assert!(repo.list().await.unwrap().is_empty());
        assert!(source.path().exists());
    }

    #[tokio::test]
    async fn failed_import_persistence_reports_cleanup_failure() {
        let (_, repo, pool) = service().await;
        let backend = Arc::new(TestBackend {
            fail_destroy: true,
            ..TestBackend::default()
        });
        let service = ImageService::with_backend(repo.clone(), backend.clone());
        sqlx::query("CREATE TRIGGER reject_import BEFORE INSERT ON images BEGIN SELECT RAISE(ABORT, 'injected persistence failure'); END")
            .execute(&pool).await.unwrap();
        let source = tempfile::NamedTempFile::new().unwrap();
        source.as_file().set_len(4096).unwrap();
        let error = service
            .import_with_converter(
                ImportImageRequest {
                    name: "failed".into(),
                    source_path: source.path().to_string_lossy().into_owned(),
                    os_type: "windows".into(),
                    description: None,
                },
                &TestConverter(ImageFormat::Raw),
            )
            .await
            .unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("injected cleanup failure"), "{message}");
        assert!(
            message.contains("injected persistence failure"),
            "{message}"
        );
        assert!(repo.list().await.unwrap().is_empty());
        assert!(backend
            .volume_present
            .load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn raw_import_uses_original_file_and_converted_import_cleans_temporary_file() {
        let (_, repo, _) = service().await;
        let backend = Arc::new(TestBackend::default());
        let service = ImageService::with_backend(repo, backend.clone());
        let source = tempfile::NamedTempFile::new().unwrap();
        source.as_file().set_len(4096).unwrap();
        for (name, format) in [("raw", ImageFormat::Raw), ("converted", ImageFormat::Qcow2)] {
            let image = service
                .import_with_converter(
                    ImportImageRequest {
                        name: name.into(),
                        source_path: source.path().to_string_lossy().into_owned(),
                        os_type: "windows".into(),
                        description: None,
                    },
                    &TestConverter(format),
                )
                .await
                .unwrap();
            assert_eq!(image.format, ImageFormat::Raw);
            let imported = backend.imported.lock().unwrap().clone().unwrap();
            if format == ImageFormat::Raw {
                assert_eq!(imported, source.path());
                assert!(imported.exists());
            } else {
                assert_ne!(imported, source.path());
                assert!(!imported.exists());
                assert!(!imported.parent().unwrap().exists());
            }
        }
    }
}
