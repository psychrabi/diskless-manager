use crate::persistence::repositories::{
    InstallerMedia, InstallerMediaRepository, InstallerMediaType, StructuralValidationState,
    SuperSessionRepository,
};
use anyhow::{bail, Context, Result};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use uuid::Uuid;

const MEDIA_DIRECTORY: &str = "installer-media";
const ISO_SECTOR_SIZE: u64 = 2048;
const ISO_DESCRIPTOR_START: u64 = 16;
const MAX_ISO_DESCRIPTORS: u64 = 64;

#[derive(Clone)]
pub struct InstallerMediaService {
    root: PathBuf,
    max_bytes: u64,
    media: InstallerMediaRepository,
    sessions: SuperSessionRepository,
}

impl InstallerMediaService {
    pub fn new(
        root: impl Into<PathBuf>,
        max_bytes: u64,
        media: InstallerMediaRepository,
        sessions: SuperSessionRepository,
    ) -> Self {
        Self {
            root: root.into(),
            max_bytes,
            media,
            sessions,
        }
    }

    pub async fn list(&self) -> Result<Vec<InstallerMedia>> {
        self.media.list().await
    }

    /// The media type is an explicit administrator declaration; ISO family is not inferred from vendor labels.
    pub async fn upload<R: AsyncRead + Unpin>(
        &self,
        display_name: &str,
        media_type: InstallerMediaType,
        mut input: R,
    ) -> Result<InstallerMedia> {
        let display_name = display_name.trim();
        if display_name.is_empty() || display_name.len() > 200 {
            bail!("installer media name must be 1 to 200 bytes");
        }
        let directory = self.media_directory().await?;
        let id = Uuid::new_v4().to_string();
        let temporary_path = directory.join(format!(".{id}.upload"));
        let managed_path = directory.join(format!("{id}.iso"));
        let mut cleanup = UploadCleanup::new(temporary_path.clone(), managed_path.clone());
        // Published files are reconciled against the catalog after cancellation.
        // Disarm their drop cleanup before the awaited publish path can expose one.
        cleanup.preserve_published();
        let result = self
            .write_validate_publish(&mut input, &temporary_path, &managed_path)
            .await;
        let (byte_size, checksum) = match result {
            Ok(result) => result,
            Err(error) => {
                return match cleanup.cleanup().await {
                    Ok(()) => Err(error),
                    Err(cleanup_error) => Err(error).context(format!(
                        "failed to clean staged installer media: {cleanup_error:#}"
                    )),
                };
            }
        };
        let now = Utc::now();
        let media = InstallerMedia {
            id,
            display_name: display_name.to_owned(),
            media_type,
            managed_path,
            byte_size,
            checksum,
            validation_state: StructuralValidationState::Valid,
            is_default: false,
            created_at: now,
            updated_at: now,
        };
        // Cancellation during insert cannot distinguish a failed insert from a commit
        // whose result was lost, so leave the file for catalog-aware reconciliation.
        if let Err(error) = self.media.insert(&media).await {
            return match cleanup.cleanup().await {
                Ok(()) => Err(error).context("failed to catalog uploaded installer media"),
                Err(cleanup_error) => Err(error).context(format!(
                    "catalog insert failed and published ISO cleanup failed: {cleanup_error:#}"
                )),
            };
        }
        cleanup.disarm();
        Ok(media)
    }

    /// Reconciles generated staging, orphan ISO, and deletion tombstone files at startup.
    /// Run once before accepting requests so active uploads are never mistaken for orphans.
    pub async fn reconcile_storage(&self) -> Result<()> {
        let directory = self.media_directory().await?;
        let catalog = self.media.list().await?;
        let mut entries = tokio::fs::read_dir(&directory).await?;
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let file_type = entry.file_type().await?;
            if let Some(id) = name
                .strip_prefix('.')
                .and_then(|name| name.strip_suffix(".upload"))
            {
                if is_generated_id(id) && (file_type.is_file() || file_type.is_symlink()) {
                    tokio::fs::remove_file(entry.path())
                        .await
                        .context("failed to remove orphan installer upload stage")?;
                }
                continue;
            }
            if let Some(id) = name
                .strip_prefix('.')
                .and_then(|name| name.strip_suffix(".deleting"))
            {
                if !is_generated_id(id) {
                    continue;
                }
                let tombstone = entry.path();
                let final_path = directory.join(format!("{id}.iso"));
                if catalog
                    .iter()
                    .any(|media| media.id == id && media.managed_path == final_path)
                {
                    match tokio::fs::symlink_metadata(&final_path).await {
                        Ok(_) => tokio::fs::remove_file(&tombstone)
                            .await
                            .context("failed to remove redundant installer deletion tombstone")?,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                            if !file_type.is_file() {
                                bail!(
                                    "cataloged installer deletion tombstone is not a regular file"
                                );
                            }
                            tokio::fs::rename(&tombstone, &final_path).await.context(
                                "failed to restore cataloged installer media from tombstone",
                            )?;
                        }
                        Err(error) => return Err(error.into()),
                    }
                } else if file_type.is_file() || file_type.is_symlink() {
                    tokio::fs::remove_file(&tombstone)
                        .await
                        .context("failed to remove orphan installer deletion tombstone")?;
                }
                continue;
            }
            if let Some(id) = name.strip_suffix(".iso") {
                if !is_generated_id(id) {
                    continue;
                }
                let cataloged = catalog
                    .iter()
                    .any(|media| media.id == id && media.managed_path == entry.path());
                if cataloged {
                    if file_type.is_symlink() || !file_type.is_file() {
                        bail!("cataloged installer media is not a regular file");
                    }
                } else if file_type.is_file() || file_type.is_symlink() {
                    tokio::fs::remove_file(entry.path())
                        .await
                        .context("failed to remove orphan managed installer ISO")?;
                }
            }
        }
        Ok(())
    }

    async fn write_validate_publish<R: AsyncRead + Unpin>(
        &self,
        input: &mut R,
        temporary_path: &Path,
        managed_path: &Path,
    ) -> Result<(u64, String)> {
        let mut options = tokio::fs::OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            options.mode(0o600);
        }
        let mut output = options
            .open(temporary_path)
            .await
            .context("failed to create staged installer media")?;
        let mut hasher = Sha256::new();
        let mut size = 0u64;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = input
                .read(&mut buffer)
                .await
                .context("installer media upload stream failed")?;
            if read == 0 {
                break;
            }
            size = size
                .checked_add(read as u64)
                .context("installer media size overflow")?;
            if size > self.max_bytes {
                bail!("installer media exceeds the configured upload limit");
            }
            output
                .write_all(&buffer[..read])
                .await
                .context("failed to write installer media")?;
            hasher.update(&buffer[..read]);
        }
        validate_iso(&mut output, size).await?;
        output
            .flush()
            .await
            .context("failed to flush installer media")?;
        output
            .sync_all()
            .await
            .context("failed to sync installer media")?;
        drop(output);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(temporary_path, std::fs::Permissions::from_mode(0o644))
                .await
                .context("failed to make published installer media readable")?;
        }
        tokio::fs::rename(temporary_path, managed_path)
            .await
            .context("failed to publish installer media")?;
        Ok((size, format!("{:x}", hasher.finalize())))
    }

    async fn media_directory(&self) -> Result<PathBuf> {
        tokio::fs::create_dir_all(&self.root)
            .await
            .context("failed to create configured boot root")?;
        let root = tokio::fs::canonicalize(&self.root)
            .await
            .context("failed to resolve configured boot root")?;
        let directory = root.join(MEDIA_DIRECTORY);
        match tokio::fs::symlink_metadata(&directory).await {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                bail!("managed installer media directory is not a real directory");
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                tokio::fs::create_dir(&directory)
                    .await
                    .context("failed to create managed installer media directory")?;
            }
            Err(error) => return Err(error.into()),
        }
        let metadata = tokio::fs::symlink_metadata(&directory).await?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            bail!("managed installer media directory is not a real directory");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o755)).await?;
        }
        Ok(directory)
    }

    pub async fn set_default(&self, id: &str) -> Result<bool> {
        let Some(_) = self.media.get(id).await? else {
            return Ok(false);
        };
        self.verify_integrity(id).await?;
        self.media.set_default(id).await
    }

    /// Rechecks the generated path, regular-file type, byte count, and SHA-256.
    /// Call on explicit readiness/default checks; list operations intentionally avoid hashing large ISOs.
    pub async fn verify_integrity(&self, id: &str) -> Result<InstallerMedia> {
        let item = self
            .media
            .get(id)
            .await?
            .with_context(|| format!("installer media '{id}' is not cataloged"))?;
        if item.validation_state != StructuralValidationState::Valid {
            bail!("installer media has not passed structural validation");
        }
        let directory = self.media_directory().await?;
        let expected = directory.join(format!("{id}.iso"));
        if item.managed_path != expected {
            bail!("catalog entry has an unexpected managed path");
        }
        let path_metadata = tokio::fs::symlink_metadata(&expected)
            .await
            .context("managed installer media file is missing")?;
        if path_metadata.file_type().is_symlink() || !path_metadata.is_file() {
            bail!("managed installer media is not a regular file");
        }
        let mut file = tokio::fs::File::open(&expected)
            .await
            .context("failed to open managed installer media")?;
        let file_metadata = file.metadata().await?;
        if !file_metadata.is_file() || file_metadata.len() != item.byte_size {
            bail!("managed installer media size differs from catalog");
        }
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 1024 * 1024];
        let mut size = 0u64;
        loop {
            let read = file.read(&mut buffer).await?;
            if read == 0 {
                break;
            }
            size = size
                .checked_add(read as u64)
                .context("managed installer media size overflow")?;
            hasher.update(&buffer[..read]);
        }
        if size != item.byte_size || format!("{:x}", hasher.finalize()) != item.checksum {
            bail!("managed installer media checksum differs from catalog");
        }
        Ok(item)
    }

    pub async fn delete(&self, id: &str) -> Result<bool> {
        let Some(item) = self.media.get(id).await? else {
            return Ok(false);
        };
        if item.is_default {
            bail!("default installer media cannot be deleted");
        }
        if self.sessions.references_media(id).await? {
            bail!("installer media is referenced by an active or recoverable Super session");
        }
        let session_references = self.sessions.session_ids_referencing_media(id).await?;
        let directory = self.media_directory().await?;
        let managed_path = directory.join(format!("{id}.iso"));
        if managed_path != item.managed_path {
            bail!("catalog entry has an unexpected managed path");
        }
        let metadata = tokio::fs::symlink_metadata(&managed_path).await?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            bail!("managed installer media is not a regular file");
        }
        let tombstone = directory.join(format!(".{id}.deleting"));
        tokio::fs::rename(&managed_path, &tombstone)
            .await
            .context("failed to stage installer media deletion")?;
        let deleted = match self.media.delete_unused(id).await {
            Ok(deleted) => deleted,
            Err(error) => {
                tokio::fs::rename(&tombstone, &managed_path)
                    .await
                    .context("catalog deletion failed and installer media restore failed")?;
                return Err(error).context("failed to delete installer media catalog entry");
            }
        };
        if !deleted {
            tokio::fs::rename(&tombstone, &managed_path)
                .await
                .context("installer media was retained but restore from deletion staging failed")?;
            return Ok(false);
        }
        if let Err(error) = tokio::fs::remove_file(&tombstone).await {
            let catalog_restore = self.media.insert(&item).await;
            let reference_restore = if catalog_restore.is_ok() {
                self.sessions
                    .restore_completed_media_references(&item.id, &session_references)
                    .await
            } else {
                Ok(())
            };
            let file_restore = tokio::fs::rename(&tombstone, &managed_path).await;
            if let Err(rollback_error) = catalog_restore {
                return Err(error).context(format!(
                    "ISO unlink failed; catalog rollback also failed: {rollback_error:#}"
                ));
            }
            if let Err(rollback_error) = reference_restore {
                return Err(error).context(format!(
                    "ISO unlink failed; completed-session reference rollback also failed: {rollback_error:#}"
                ));
            }
            if let Err(rollback_error) = file_restore {
                return Err(error).context(format!(
                    "ISO unlink failed; staged file restore also failed: {rollback_error}"
                ));
            }
            return Err(error).context("failed to remove staged installer media");
        }
        Ok(true)
    }
}

fn is_generated_id(value: &str) -> bool {
    Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value)
}

struct UploadCleanup {
    staged_path: PathBuf,
    published_path: PathBuf,
    staged_armed: bool,
    published_armed: bool,
}

impl UploadCleanup {
    fn new(staged_path: PathBuf, published_path: PathBuf) -> Self {
        Self {
            staged_path,
            published_path,
            staged_armed: true,
            published_armed: true,
        }
    }

    fn preserve_published(&mut self) {
        self.published_armed = false;
    }

    fn disarm(&mut self) {
        self.staged_armed = false;
        self.published_armed = false;
    }

    async fn cleanup(&mut self) -> Result<()> {
        let mut first_error = None;
        for path in [&self.staged_path, &self.published_path] {
            if let Err(error) = tokio::fs::remove_file(path).await {
                if error.kind() != std::io::ErrorKind::NotFound && first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        if let Some(error) = first_error {
            return Err(error).context("failed to remove temporary installer media files");
        }
        self.disarm();
        Ok(())
    }
}

impl Drop for UploadCleanup {
    fn drop(&mut self) {
        if self.staged_armed {
            let _ = std::fs::remove_file(&self.staged_path);
        }
        if self.published_armed {
            let _ = std::fs::remove_file(&self.published_path);
        }
    }
}

/// Checks the bounded ISO9660 descriptor area and requires an El Torito boot record.
async fn validate_iso(file: &mut tokio::fs::File, size: u64) -> Result<()> {
    if size < (ISO_DESCRIPTOR_START + 2) * ISO_SECTOR_SIZE || size % ISO_SECTOR_SIZE != 0 {
        bail!("installer media is empty, truncated, or not sector-aligned");
    }
    file.seek(std::io::SeekFrom::Start(
        ISO_DESCRIPTOR_START * ISO_SECTOR_SIZE,
    ))
    .await?;
    let mut descriptor = [0u8; ISO_SECTOR_SIZE as usize];
    let mut volume_sectors = None;
    let mut catalog_lba = None;
    let mut terminator = false;
    let mut last_descriptor_lba = None;
    for descriptor_count in 0..MAX_ISO_DESCRIPTORS {
        let descriptor_lba = ISO_DESCRIPTOR_START + descriptor_count;
        last_descriptor_lba = Some(descriptor_lba);
        file.read_exact(&mut descriptor)
            .await
            .context("truncated ISO volume descriptor")?;
        if &descriptor[1..6] != b"CD001" || descriptor[6] != 1 {
            bail!("unsupported ISO volume descriptor");
        }
        match descriptor[0] {
            1 => {
                let sectors = u32::from_le_bytes(descriptor[80..84].try_into()?) as u64;
                let sectors_be = u32::from_be_bytes(descriptor[84..88].try_into()?) as u64;
                let block_size = u16::from_le_bytes(descriptor[128..130].try_into()?);
                if block_size != ISO_SECTOR_SIZE as u16
                    || sectors != sectors_be
                    || sectors
                        .checked_mul(ISO_SECTOR_SIZE)
                        .map_or(true, |end| end > size)
                {
                    bail!("invalid ISO primary volume descriptor");
                }
                if descriptor[40..72]
                    .iter()
                    .all(|byte| *byte == 0 || *byte == b' ')
                {
                    bail!("ISO primary volume descriptor has no volume identifier");
                }
                volume_sectors = Some(sectors);
            }
            0 if &descriptor[7..30] == b"EL TORITO SPECIFICATION" => {
                catalog_lba = Some(u32::from_le_bytes(descriptor[71..75].try_into()?) as u64);
            }
            255 => {
                terminator = true;
                break;
            }
            _ => {}
        }
    }
    if !terminator {
        bail!("ISO volume descriptor chain has no terminator");
    }
    let sectors = volume_sectors.context("ISO is missing a primary volume descriptor")?;
    if last_descriptor_lba.context("ISO has no volume descriptors")? >= sectors {
        bail!("ISO volume descriptor lies outside the declared volume");
    }
    let catalog_lba = catalog_lba.context("ISO is missing an El Torito boot record")?;
    let catalog_offset = catalog_lba
        .checked_mul(ISO_SECTOR_SIZE)
        .context("El Torito catalog address overflow")?;
    let catalog_end = catalog_offset
        .checked_add(64)
        .context("El Torito catalog length overflow")?;
    if catalog_end > size || catalog_end > sectors * ISO_SECTOR_SIZE {
        bail!("El Torito boot catalog is outside the ISO volume");
    }
    file.seek(std::io::SeekFrom::Start(catalog_offset)).await?;
    let mut catalog = [0u8; 64];
    file.read_exact(&mut catalog)
        .await
        .context("truncated El Torito boot catalog")?;
    let validation_sum = catalog[..32].chunks_exact(2).fold(0u16, |sum, word| {
        sum.wrapping_add(u16::from_le_bytes([word[0], word[1]]))
    });
    if catalog[0] != 1 || catalog[2] != 0 || validation_sum != 0 || catalog[30..32] != [0x55, 0xaa]
    {
        bail!("invalid El Torito validation entry");
    }
    let boot_entry = &catalog[32..64];
    let load_sector_count = u16::from_le_bytes(boot_entry[6..8].try_into()?) as u64;
    let load_rba = u32::from_le_bytes(boot_entry[8..12].try_into()?) as u64;
    let load_blocks = load_sector_count
        .checked_add(3)
        .context("El Torito load sector count overflow")?
        / 4;
    let load_end = load_rba
        .checked_add(load_blocks)
        .context("El Torito boot image extent overflow")?;
    if boot_entry[0] != 0x88
        || boot_entry[1] > 4
        || load_sector_count == 0
        || load_rba == 0
        || load_end > sectors
    {
        bail!("El Torito catalog has no valid initial boot entry");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::InstallerMediaService;
    use crate::persistence::repositories::{InstallerMediaType, StructuralValidationState};
    use crate::persistence::{InstallerMediaRepository, SuperSessionRepository};
    use sha2::Digest;
    use sqlx::sqlite::SqlitePoolOptions;
    use tokio::io::AsyncWriteExt;

    async fn service(
        root: &std::path::Path,
        max_bytes: u64,
    ) -> (InstallerMediaService, sqlx::SqlitePool) {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        (
            InstallerMediaService::new(
                root.to_owned(),
                max_bytes,
                InstallerMediaRepository::new(pool.clone()),
                SuperSessionRepository::new(pool.clone()),
            ),
            pool,
        )
    }

    fn bootable_iso(label: &[u8]) -> Vec<u8> {
        let mut image = vec![0; 20 * 2048];
        let pvd = &mut image[16 * 2048..17 * 2048];
        pvd[0] = 1;
        pvd[1..6].copy_from_slice(b"CD001");
        pvd[6] = 1;
        pvd[40..40 + label.len()].copy_from_slice(label);
        pvd[80..84].copy_from_slice(&20u32.to_le_bytes());
        pvd[84..88].copy_from_slice(&20u32.to_be_bytes());
        pvd[128..130].copy_from_slice(&2048u16.to_le_bytes());
        let boot = &mut image[17 * 2048..18 * 2048];
        boot[0] = 0;
        boot[1..6].copy_from_slice(b"CD001");
        boot[6] = 1;
        boot[7..30].copy_from_slice(b"EL TORITO SPECIFICATION");
        boot[71..75].copy_from_slice(&19u32.to_le_bytes());
        let term = &mut image[18 * 2048..19 * 2048];
        term[0] = 255;
        term[1..6].copy_from_slice(b"CD001");
        term[6] = 1;
        let catalog = &mut image[19 * 2048..19 * 2048 + 64];
        catalog[0] = 1;
        catalog[1] = 0;
        catalog[30] = 0x55;
        catalog[31] = 0xaa;
        let validation_sum: u16 = catalog[..32]
            .chunks_exact(2)
            .map(|word| u16::from_le_bytes([word[0], word[1]]))
            .sum();
        catalog[28..30].copy_from_slice(&0u16.wrapping_sub(validation_sum).to_le_bytes());
        catalog[32] = 0x88;
        catalog[38..40].copy_from_slice(&4u16.to_le_bytes());
        catalog[40..44].copy_from_slice(&18u32.to_le_bytes());
        image
    }

    #[tokio::test]
    async fn uploads_bounded_iso_and_persists_size_checksum_and_public_file() {
        let root = tempfile::tempdir().unwrap();
        let bytes = bootable_iso(b"LINUX LIVE");
        let (service, _) = service(root.path(), bytes.len() as u64).await;
        let expected = format!("{:x}", sha2::Sha256::digest(&bytes));
        let media = service
            .upload(
                "Linux installer",
                InstallerMediaType::LinuxIso,
                bytes.as_slice(),
            )
            .await
            .unwrap();
        assert_eq!(media.byte_size as usize, bytes.len());
        assert_eq!(media.checksum, expected);
        assert_eq!(media.validation_state, StructuralValidationState::Valid);
        assert_eq!(tokio::fs::read(&media.managed_path).await.unwrap(), bytes);
        assert_eq!(service.list().await.unwrap(), vec![media]);
        let reopened = InstallerMediaService::new(
            root.path(),
            bytes.len() as u64,
            service.media.clone(),
            service.sessions.clone(),
        );
        assert_eq!(
            reopened.list().await.unwrap(),
            service.list().await.unwrap()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(root.path().join("installer-media"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o755
            );
            assert_eq!(
                std::fs::metadata(service.list().await.unwrap()[0].managed_path.clone())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o644
            );
        }
    }

    #[tokio::test]
    async fn rejects_empty_truncated_malformed_and_over_limit_without_publishing() {
        let root = tempfile::tempdir().unwrap();
        let (service, _) = service(root.path(), 64 * 1024).await;
        for invalid in [Vec::new(), vec![0; 40_000], {
            let mut bad = bootable_iso(b"BAD");
            bad[16 * 2048 + 1] = b'X';
            bad
        }] {
            assert!(service
                .upload("bad", InstallerMediaType::WinPe, invalid.as_slice())
                .await
                .is_err());
        }
        let too_large = vec![1; 64 * 1024 + 1];
        assert!(service
            .upload("large", InstallerMediaType::WinPe, too_large.as_slice())
            .await
            .is_err());
        assert!(service.list().await.unwrap().is_empty());
        assert_eq!(
            std::fs::read_dir(root.path().join("installer-media"))
                .unwrap()
                .count(),
            0
        );
    }

    #[tokio::test]
    async fn rejects_bad_el_torito_catalog_and_missing_boot_entry() {
        let root = tempfile::tempdir().unwrap();
        let (service, _) = service(root.path(), 64 * 1024).await;
        let malformed = [
            {
                let mut image = bootable_iso(b"BAD CHECKSUM");
                image[19 * 2048 + 28] ^= 1;
                image
            },
            {
                let mut image = bootable_iso(b"BAD SIGNATURE");
                image[19 * 2048 + 30] = 0;
                image
            },
            {
                let mut image = bootable_iso(b"NO BOOT ENTRY");
                image[19 * 2048 + 32] = 0;
                image
            },
            {
                let mut image = bootable_iso(b"CATALOG OOB");
                image[17 * 2048 + 71..17 * 2048 + 75].copy_from_slice(&u32::MAX.to_le_bytes());
                image
            },
            {
                let mut image = bootable_iso(b"ZERO LOAD COUNT");
                image[19 * 2048 + 38..19 * 2048 + 40].copy_from_slice(&0u16.to_le_bytes());
                image
            },
            {
                let mut image = bootable_iso(b"LOAD EXTENT OOB");
                image[19 * 2048 + 38..19 * 2048 + 40].copy_from_slice(&5u16.to_le_bytes());
                image[19 * 2048 + 40..19 * 2048 + 44].copy_from_slice(&19u32.to_le_bytes());
                image
            },
        ];
        for image in malformed {
            assert!(service
                .upload(
                    "invalid catalog",
                    InstallerMediaType::LinuxIso,
                    image.as_slice()
                )
                .await
                .is_err());
        }
        assert!(service.list().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn rejects_descriptor_terminator_outside_declared_volume_with_trailing_file_sector() {
        let root = tempfile::tempdir().unwrap();
        let (service, _) = service(root.path(), 64 * 1024).await;
        let mut image = bootable_iso(b"TRAILING SECTOR");
        image.resize(22 * 2048, 0);
        let pvd = &mut image[16 * 2048..17 * 2048];
        pvd[80..84].copy_from_slice(&18u32.to_le_bytes());
        pvd[84..88].copy_from_slice(&18u32.to_be_bytes());

        let catalog = image[19 * 2048..19 * 2048 + 64].to_vec();
        image[15 * 2048..15 * 2048 + 64].copy_from_slice(&catalog);
        image[15 * 2048 + 40..15 * 2048 + 44].copy_from_slice(&16u32.to_le_bytes());
        image[17 * 2048 + 71..17 * 2048 + 75].copy_from_slice(&15u32.to_le_bytes());

        assert!(service
            .upload("trailing", InstallerMediaType::LinuxIso, image.as_slice())
            .await
            .is_err());
        assert!(service.list().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn integrity_verification_rejects_changed_and_missing_managed_files() {
        let root = tempfile::tempdir().unwrap();
        let (service, _) = service(root.path(), 64 * 1024).await;
        let changed = service
            .upload(
                "changed",
                InstallerMediaType::LinuxIso,
                bootable_iso(b"CHANGED").as_slice(),
            )
            .await
            .unwrap();
        let mut bytes = tokio::fs::read(&changed.managed_path).await.unwrap();
        bytes[100] ^= 1;
        tokio::fs::write(&changed.managed_path, bytes)
            .await
            .unwrap();
        assert!(service.verify_integrity(&changed.id).await.is_err());
        assert!(service.set_default(&changed.id).await.is_err());

        let missing = service
            .upload(
                "missing",
                InstallerMediaType::WinPe,
                bootable_iso(b"MISSING").as_slice(),
            )
            .await
            .unwrap();
        tokio::fs::remove_file(&missing.managed_path).await.unwrap();
        assert!(service.verify_integrity(&missing.id).await.is_err());
        assert!(service.set_default(&missing.id).await.is_err());
    }

    #[tokio::test]
    async fn interrupted_stream_removes_partial_file() {
        let root = tempfile::tempdir().unwrap();
        let (service, _) = service(root.path(), 64 * 1024).await;
        let (mut writer, reader) = tokio::io::duplex(8);
        let upload = tokio::spawn(async move {
            service
                .upload("interrupted", InstallerMediaType::LinuxIso, reader)
                .await
        });
        let partial = bootable_iso(b"PARTIAL");
        writer.write_all(&partial[..30_000]).await.unwrap();
        drop(writer);
        assert!(upload.await.unwrap().is_err());
        assert!(std::fs::read_dir(root.path().join("installer-media"))
            .unwrap()
            .next()
            .is_none());
    }

    #[tokio::test]
    async fn cancelling_upload_removes_its_partial_stage_file() {
        let root = tempfile::tempdir().unwrap();
        let (service, _) = service(root.path(), 64 * 1024).await;
        let (mut writer, reader) = tokio::io::duplex(8);
        let upload = tokio::spawn(async move {
            service
                .upload("cancelled", InstallerMediaType::LinuxIso, reader)
                .await
        });
        let partial = bootable_iso(b"CANCELLED");
        writer.write_all(&partial[..30_000]).await.unwrap();
        let directory = root.path().join("installer-media");
        let staged_exists = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if tokio::fs::try_exists(&directory).await.unwrap_or(false) {
                    let mut entries = tokio::fs::read_dir(&directory).await.unwrap();
                    if entries.next_entry().await.unwrap().is_some() {
                        return true;
                    }
                }
                if upload.is_finished() {
                    return false;
                }
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
        assert!(staged_exists);
        upload.abort();
        let _ = upload.await;
        assert_eq!(std::fs::read_dir(directory).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn cancelling_catalog_insert_removes_published_uncommitted_iso() {
        let root = tempfile::tempdir().unwrap();
        let (service, pool) = service(root.path(), 64 * 1024).await;
        let recovery_service = service.clone();
        let transaction = pool.begin().await.unwrap();
        let upload = tokio::spawn(async move {
            service
                .upload(
                    "cancelled after publish",
                    InstallerMediaType::LinuxIso,
                    bootable_iso(b"PUBLISHED").as_slice(),
                )
                .await
        });
        let directory = root.path().join("installer-media");
        let published = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if tokio::fs::try_exists(&directory).await.unwrap_or(false) {
                    let mut entries = tokio::fs::read_dir(&directory).await.unwrap();
                    while let Some(entry) = entries.next_entry().await.unwrap() {
                        if entry.file_name().to_string_lossy().ends_with(".iso") {
                            return true;
                        }
                    }
                }
                if upload.is_finished() {
                    return false;
                }
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
        assert!(published);
        upload.abort();
        let _ = upload.await;
        let mut entries = tokio::fs::read_dir(&directory).await.unwrap();
        let published_path = entries.next_entry().await.unwrap().unwrap().path();
        assert!(published_path.extension().is_some_and(|ext| ext == "iso"));
        assert!(tokio::fs::try_exists(&published_path).await.unwrap());
        drop(transaction);
        recovery_service.reconcile_storage().await.unwrap();
        assert_eq!(std::fs::read_dir(directory).unwrap().count(), 0);
        let media = InstallerMediaRepository::new(pool);
        assert!(media.list().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn startup_reconciliation_recovers_tombstones_and_removes_orphan_generated_files() {
        let root = tempfile::tempdir().unwrap();
        let (service, _) = service(root.path(), 64 * 1024).await;
        let media = service
            .upload(
                "kept",
                InstallerMediaType::LinuxIso,
                bootable_iso(b"KEPT").as_slice(),
            )
            .await
            .unwrap();
        let directory = root.path().join("installer-media");
        let staged = directory.join(".00000000-0000-0000-0000-000000000001.upload");
        let orphan = directory.join("00000000-0000-0000-0000-000000000002.iso");
        let orphan_tombstone = directory.join(".00000000-0000-0000-0000-000000000003.deleting");
        tokio::fs::write(&staged, b"partial").await.unwrap();
        tokio::fs::write(&orphan, b"orphan").await.unwrap();
        tokio::fs::write(&orphan_tombstone, b"orphan")
            .await
            .unwrap();
        let media_tombstone = directory.join(format!(".{}.deleting", media.id));
        tokio::fs::rename(&media.managed_path, &media_tombstone)
            .await
            .unwrap();

        service.reconcile_storage().await.unwrap();

        assert!(tokio::fs::try_exists(&media.managed_path).await.unwrap());
        assert!(!tokio::fs::try_exists(&staged).await.unwrap());
        assert!(!tokio::fs::try_exists(&orphan).await.unwrap());
        assert!(!tokio::fs::try_exists(&orphan_tombstone).await.unwrap());
        assert!(!tokio::fs::try_exists(&media_tombstone).await.unwrap());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn refuses_a_symlinked_managed_directory() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), root.path().join("installer-media")).unwrap();
        let (service, _) = service(root.path(), 64 * 1024).await;
        assert!(service
            .upload(
                "Linux",
                InstallerMediaType::LinuxIso,
                bootable_iso(b"LINUX").as_slice()
            )
            .await
            .is_err());
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn defaults_are_switchable_and_active_session_media_cannot_be_deleted() {
        let root = tempfile::tempdir().unwrap();
        let (service, pool) = service(root.path(), 64 * 1024).await;
        let first = service
            .upload(
                "Linux one",
                InstallerMediaType::LinuxIso,
                bootable_iso(b"LINUX ONE").as_slice(),
            )
            .await
            .unwrap();
        let second = service
            .upload(
                "Linux two",
                InstallerMediaType::LinuxIso,
                bootable_iso(b"LINUX TWO").as_slice(),
            )
            .await
            .unwrap();
        assert!(service.set_default(&first.id).await.unwrap());
        assert!(service.set_default(&second.id).await.unwrap());
        assert!(service.delete(&first.id).await.unwrap());
        let third = service
            .upload(
                "Linux three",
                InstallerMediaType::LinuxIso,
                bootable_iso(b"LINUX THREE").as_slice(),
            )
            .await
            .unwrap();
        assert!(service.set_default(&third.id).await.unwrap());

        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, created_at, updated_at) VALUES ('client-1', 'Client', '00:11:22:33:44:55', '192.0.2.1', 'image-1', 'now', 'now')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO images (id, name, os_type, size_gb, path, created_at, updated_at) VALUES ('image-1', 'Image', 'linux', 20, '/tmp/image', 'now', 'now')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO super_sessions (id, client_id, prior_image_id, working_clone, installer_media_id, state, created_at, updated_at) VALUES ('session-1', 'client-1', 'image-1', 'clone-1', ?, 'active', 'now', 'now')").bind(&second.id).execute(&pool).await.unwrap();
        assert!(service.delete(&second.id).await.is_err());
        assert!(tokio::fs::try_exists(&second.managed_path).await.unwrap());
        assert!(service
            .list()
            .await
            .unwrap()
            .iter()
            .any(|media| media.id == second.id));
        sqlx::query("UPDATE super_sessions SET state = 'completed' WHERE id = 'session-1'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(service.delete(&second.id).await.unwrap());
        let session_media: Option<String> = sqlx::query_scalar(
            "SELECT installer_media_id FROM super_sessions WHERE id = 'session-1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(session_media, None);
    }
}
