use crate::cmd::{run_command, run_command_output_no_sudo};
use crate::config::get_zpool_name;
use crate::validation::validate_zfs_name;
use crate::zfs::{get_snapshots_for_dataset, zfs_clone, zfs_destroy, zfs_exists};
use chrono::{DateTime, Utc};
use log::info;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePool;
use std::path::PathBuf;
use std::process::Command;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OsType {
    Linux,
    Windows,
}

impl std::fmt::Display for OsType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OsType::Linux => write!(f, "linux"),
            OsType::Windows => write!(f, "windows"),
        }
    }
}

impl std::str::FromStr for OsType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "linux" => Ok(OsType::Linux),
            "windows" => Ok(OsType::Windows),
            _ => Err(anyhow::anyhow!("Invalid OS type: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    Raw,
    Qcow2,
    Vmdk,
    Vdi,
    None,
}

impl std::fmt::Display for ImageFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImageFormat::Raw => write!(f, "raw"),
            ImageFormat::Qcow2 => write!(f, "qcow2"),
            ImageFormat::Vmdk => write!(f, "vmdk"),
            ImageFormat::Vdi => write!(f, "vdi"),
            ImageFormat::None => write!(f, "none"),
        }
    }
}

impl std::str::FromStr for ImageFormat {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "raw" | "img" => Ok(ImageFormat::Raw),
            "qcow2" => Ok(ImageFormat::Qcow2),
            "vmdk" => Ok(ImageFormat::Vmdk),
            "vdi" => Ok(ImageFormat::Vdi),
            "none" => Ok(ImageFormat::None),
            _ => Err(anyhow::anyhow!("Invalid image format: {}", s)),
        }
    }
}

impl ImageFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            ImageFormat::Raw => "img",
            ImageFormat::Qcow2 => "qcow2",
            ImageFormat::Vmdk => "vmdk",
            ImageFormat::Vdi => "vdi",
            ImageFormat::None => "none",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Image {
    pub id: String,
    pub name: String,
    pub os_type: OsType,
    pub size_gb: u64,
    pub path: PathBuf,
    pub format: ImageFormat,
    pub status: String,
    pub description: Option<String>,
    pub parent_id: Option<String>,
    pub checksum: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateImageRequest {
    pub name: String,
    pub os_type: String,
    pub size_gb: u64,
    pub format: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportImageRequest {
    pub name: String,
    pub source_path: String,
    pub os_type: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateImageRequest {
    pub name: Option<String>,
    pub os_type: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    pub virtual_size: u64,
    pub actual_size: u64,
    pub format: String,
    pub backing_file: Option<String>,
    pub snapshots: Vec<String>,
}

pub struct ImageManager {
    pool: SqlitePool,
    images_dir: PathBuf,
    snapshots_dir: PathBuf,
}

impl ImageManager {
    pub fn new(pool: SqlitePool, images_dir: PathBuf, snapshots_dir: PathBuf) -> Self {
        Self {
            pool,
            images_dir,
            snapshots_dir,
        }
    }

    pub async fn list(&self) -> anyhow::Result<Vec<Image>> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                i64,
                String,
                String,
                String,
                Option<String>,
                Option<String>,
                Option<String>,
                String,
                String,
            ),
        >(
            r#"
            SELECT id, name, os_type, size_gb, path, format, status, description, parent_id, checksum, created_at, updated_at 
            FROM images 
            ORDER BY name
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let images = rows
            .into_iter()
            .filter_map(
                |(
                    id,
                    name,
                    os_type,
                    size_gb,
                    path,
                    format,
                    status,
                    description,
                    parent_id,
                    checksum,
                    created_at,
                    updated_at,
                )| {
                    Some(Image {
                        id,
                        name,
                        os_type: os_type.parse().ok()?,
                        size_gb: size_gb as u64,
                        path: PathBuf::from(path),
                        format: format.parse().ok()?,
                        status,
                        description,
                        parent_id,
                        checksum,
                        created_at: DateTime::parse_from_rfc3339(&created_at)
                            .ok()?
                            .with_timezone(&Utc),
                        updated_at: DateTime::parse_from_rfc3339(&updated_at)
                            .ok()?
                            .with_timezone(&Utc),
                    })
                },
            )
            .collect();

        Ok(images)
    }

    pub async fn get(&self, id: &str) -> anyhow::Result<Image> {
        let row = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                i64,
                String,
                String,
                String,
                Option<String>,
                Option<String>,
                Option<String>,
                String,
                String,
            ),
        >(
            r#"
            SELECT id, name, os_type, size_gb, path, format, status, description, parent_id, checksum, created_at, updated_at 
            FROM images 
            WHERE id = ? OR name = ?
            "#,
        )
        .bind(id)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Image not found: {}", id))?;

        let (
            id,
            name,
            os_type,
            size_gb,
            path,
            format,
            status,
            description,
            parent_id,
            checksum,
            created_at,
            updated_at,
        ) = row;

        Ok(Image {
            id,
            name,
            os_type: os_type.parse()?,
            size_gb: size_gb as u64,
            path: PathBuf::from(path),
            format: format.parse()?,
            status,
            description,
            parent_id,
            checksum,
            created_at: DateTime::parse_from_rfc3339(&created_at)?.with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339(&updated_at)?.with_timezone(&Utc),
        })
    }

    pub async fn create(&self, req: CreateImageRequest) -> anyhow::Result<Image> {
        let os_type: OsType = req.os_type.parse()?;

        // Validate ZFS name
        validate_zfs_name(&req.name)?;

        let zpool = get_zpool_name();
        let mut parent_dataset = format!("{}/image-disk", zpool);

        // Find the appropriate parent dataset for images
        if let Ok(get_out) = run_command_output_no_sudo(&[
            "zfs",
            "get",
            "-H",
            "-o",
            "name,value",
            "-r",
            "org.diskless:type",
            &zpool,
        ]) {
            let mut image_datasets = vec![];
            for line in get_out.lines() {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() == 2 {
                    let dataset = parts[0].trim().to_string();
                    let val = parts[1].trim().to_string();

                    if val == "image" {
                        let dataset_parts: Vec<&str> = dataset.split('/').collect();
                        if dataset_parts.len() == 2 {
                            // Direct child of zpool
                            image_datasets.push(dataset);
                        }
                    }
                }
            }

            // If we found image datasets, use the most recently created one
            if !image_datasets.is_empty() {
                // Get creation times for all image datasets
                if let Ok(creation_out) = run_command_output_no_sudo(
                    &["zfs", "get", "-H", "-o", "name,value", "creation"]
                        .iter()
                        .chain(
                            &image_datasets
                                .iter()
                                .map(|s| s.as_str())
                                .collect::<Vec<_>>(),
                        )
                        .cloned()
                        .collect::<Vec<_>>(),
                ) {
                    let mut datasets_with_time: Vec<(String, String)> = vec![];
                    for line in creation_out.lines() {
                        let parts: Vec<&str> = line.split('\t').collect();
                        if parts.len() == 2 {
                            datasets_with_time
                                .push((parts[0].trim().to_string(), parts[1].trim().to_string()));
                        }
                    }

                    // Sort by creation time (newest first)
                    datasets_with_time.sort_by(|a, b| b.1.cmp(&a.1));

                    // Use the most recently created image dataset
                    if let Some((newest_dataset, _)) = datasets_with_time.first() {
                        parent_dataset = newest_dataset.clone();
                    }
                }
            } else {
                // No image datasets found - create the default one
                run_command(&[
                    "zfs",
                    "create",
                    "-o",
                    "org.diskless:type=image",
                    &parent_dataset,
                ])?;
            }
        }

        let full_name = format!("{}/{}", parent_dataset, req.name);

        // Check if ZFS dataset already exists
        if zfs_exists(&full_name) {
            return Err(anyhow::anyhow!(
                "ZFS dataset '{}' already exists.",
                full_name
            ));
        }

        // Create the ZFS volume
        run_command(&[
            "zfs",
            "create",
            "-s",
            "-V",
            &format!("{}G", req.size_gb),
            "-o",
            "volblocksize=128K",
            &full_name,
        ])?;

        // Set the OS type property on the ZFS volume
        run_command(&[
            "zfs",
            "set",
            &format!("org.diskless:os={}", req.os_type),
            &full_name,
        ])?;

        // Create the image record with the ZFS path
        let image = Image {
            id: Uuid::new_v4().to_string(),
            name: full_name.clone(),
            os_type,
            size_gb: req.size_gb,
            path: PathBuf::from(format!("/dev/zvol/{}", full_name)), // Standard path for ZFS volumes
            format: ImageFormat::None,
            status: "ready".to_string(),
            description: req.description,
            parent_id: None,
            checksum: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        sqlx::query(
            r#"
            INSERT INTO images (id, name, os_type, size_gb, path, format, status, description, parent_id, checksum, created_at, updated_at) 
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&image.id)
        .bind(&image.name)
        .bind(image.os_type.to_string())
        .bind(image.size_gb as i64)
        .bind(image.path.to_string_lossy().to_string())
        .bind(image.format.to_string())
        .bind(&image.status)
        .bind(&image.description)
        .bind(&image.parent_id)
        .bind(&image.checksum)
        .bind(image.created_at.to_rfc3339())
        .bind(image.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        info!(
            "Image '{}' created as ZFS volume ({} GB)",
            req.name, req.size_gb
        );
        Ok(image)
    }

    pub async fn update(&self, id: &str, req: UpdateImageRequest) -> anyhow::Result<Image> {
        info!(
            "Attempting to update image with id '{}', request: {:?}",
            id, req
        );
        let mut image = self.get(id).await?;
        info!(
            "Current image data: name='{}', os_type='{}', status='{}', description='{:?}'",
            image.name, image.os_type, image.status, image.description
        );

        // Track if name was updated
        let mut name_was_updated = false;

        // Update fields if provided in the request
        if let Some(new_name) = req.name {
            name_was_updated = true;
            info!(
                "Updating image name from '{}' to '{}'",
                image.name, new_name
            );
            // Validate ZFS name if changing
            validate_zfs_name(&new_name)?;

            // Extract the parent dataset from the current image name
            let current_parts: Vec<&str> = image.name.split('/').collect();
            let parent_dataset = if current_parts.len() > 1 {
                current_parts[..current_parts.len() - 1].join("/")
            } else {
                // If no parent, use the zpool's image-disk as default
                let zpool = get_zpool_name();
                format!("{}/image-disk", zpool)
            };

            let new_full_name = format!("{}/{}", parent_dataset, new_name);
            info!("Calculated new full name: '{}'", new_full_name);

            // Check if the new name already exists as a ZFS dataset
            if zfs_exists(&new_full_name) {
                info!(
                    "ZFS dataset '{}' already exists, aborting name update",
                    new_full_name
                );
                return Err(anyhow::anyhow!(
                    "ZFS dataset '{}' already exists.",
                    new_full_name
                ));
            }

            // Rename the ZFS dataset if it exists
            if zfs_exists(&image.name) {
                info!(
                    "Renaming ZFS dataset from '{}' to '{}'",
                    image.name, new_full_name
                );
                run_command(&["zfs", "rename", &image.name, &new_full_name])?;
                info!("ZFS rename completed successfully");
            } else {
                info!(
                    "ZFS dataset '{}' does not exist, skipping ZFS rename",
                    image.name
                );
            }

            // Update the image name to the new full name
            image.name = new_full_name;
            info!("Updated image name to: {}", image.name);
        }

        if let Some(os_type) = req.os_type {
            info!("Updating OS type from '{}' to '{}'", image.os_type, os_type);
            image.os_type = os_type.parse()?;

            // Update the OS type property on the ZFS volume if it exists
            if zfs_exists(&image.name) {
                info!("Updating OS type property on ZFS volume '{}'", image.name);
                run_command(&[
                    "zfs",
                    "set",
                    &format!("org.diskless:os={}", image.os_type),
                    &image.name,
                ])?;
                info!("OS type property updated successfully");
            }
        }

        if let Some(description) = req.description {
            info!(
                "Updating description from '{:?}' to '{:?}'",
                image.description,
                Some(description.clone())
            );
            image.description = Some(description);
        }

        if let Some(status) = req.status {
            info!("Updating status from '{}' to '{}'", image.status, status);
            image.status = status;
        }

        // Update the timestamp
        image.updated_at = Utc::now();

        // If the name has changed, update the path as well
        if name_was_updated {
            image.path = PathBuf::from(format!("/dev/zvol/{}", image.name));
        }

        // Update the database record
        sqlx::query(
            r#"
            UPDATE images 
            SET name = ?, os_type = ?, status = ?, description = ?, path = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&image.name)
        .bind(image.os_type.to_string())
        .bind(&image.status)
        .bind(&image.description)
        .bind(image.path.to_string_lossy().to_string())
        .bind(image.updated_at.to_rfc3339())
        .bind(&image.id)
        .execute(&self.pool)
        .await?;

        info!("Image '{}' updated successfully", image.name);
        Ok(image)
    }

    pub async fn import(&self, req: ImportImageRequest) -> anyhow::Result<Image> {
        let os_type: OsType = req.os_type.parse()?;
        let source = PathBuf::from(&req.source_path);

        if !source.exists() {
            return Err(anyhow::anyhow!(
                "Source file not found: {}",
                req.source_path
            ));
        }

        let zpool = get_zpool_name();
        let mut parent_dataset = format!("{}/image-disk", zpool);

        // Find the appropriate parent dataset for images
        if let Ok(get_out) = run_command_output_no_sudo(&[
            "zfs",
            "get",
            "-H",
            "-o",
            "name,value",
            "-r",
            "org.diskless:type",
            &zpool,
        ]) {
            let mut image_datasets = vec![];
            for line in get_out.lines() {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() == 2 {
                    let dataset = parts[0].trim().to_string();
                    let val = parts[1].trim().to_string();

                    if val == "image" {
                        let dataset_parts: Vec<&str> = dataset.split('/').collect();
                        if dataset_parts.len() == 2 {
                            // Direct child of zpool
                            image_datasets.push(dataset);
                        }
                    }
                }
            }

            // If we found image datasets, use the most recently created one
            if !image_datasets.is_empty() {
                // Get creation times for all image datasets
                if let Ok(creation_out) = run_command_output_no_sudo(
                    &["zfs", "get", "-H", "-o", "name,value", "creation"]
                        .iter()
                        .chain(
                            &image_datasets
                                .iter()
                                .map(|s| s.as_str())
                                .collect::<Vec<_>>(),
                        )
                        .cloned()
                        .collect::<Vec<_>>(),
                ) {
                    let mut datasets_with_time: Vec<(String, String)> = vec![];
                    for line in creation_out.lines() {
                        let parts: Vec<&str> = line.split('\t').collect();
                        if parts.len() == 2 {
                            datasets_with_time
                                .push((parts[0].trim().to_string(), parts[1].trim().to_string()));
                        }
                    }

                    // Sort by creation time (newest first)
                    datasets_with_time.sort_by(|a, b| b.1.cmp(&a.1));

                    // Use the most recently created image dataset
                    if let Some((newest_dataset, _)) = datasets_with_time.first() {
                        parent_dataset = newest_dataset.clone();
                    }
                }
            } else {
                // No image datasets found - create the default one
                run_command(&[
                    "zfs",
                    "create",
                    "-o",
                    "org.diskless:type=image",
                    &parent_dataset,
                ])?;
            }
        }

        let full_name = format!("{}/{}", parent_dataset, req.name);

        // Check if ZFS dataset already exists
        if zfs_exists(&full_name) {
            return Err(anyhow::anyhow!(
                "ZFS dataset '{}' already exists.",
                full_name
            ));
        }

        // First get the size of the source image
        let output = Command::new("qemu-img")
            .args(["info", "--output=json", &source.to_string_lossy()])
            .output()?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("Failed to get source image info"));
        }

        let info: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        let virtual_size = info["virtual-size"].as_u64().unwrap_or(0);
        let size_gb = (virtual_size / (1024 * 1024 * 1024)).max(1);

        // Create the ZFS volume
        run_command(&[
            "zfs",
            "create",
            "-s",
            "-V",
            &format!("{}G", size_gb),
            "-o",
            "volblocksize=128K",
            &full_name,
        ])?;

        // Use dd to copy the image file to the ZFS volume
        let output = Command::new("dd")
            .args([
                &format!("if={}", source.to_string_lossy()),
                &format!("of=/dev/zvol/{}", full_name),
                "bs=1M",
                "conv=fdatasync",
            ])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Clean up the ZFS volume if import fails
            let _ = zfs_destroy(&full_name);
            return Err(anyhow::anyhow!(
                "Failed to import image to ZFS volume: {}",
                stderr
            ));
        }

        // Set the OS type property on the ZFS volume
        run_command(&[
            "zfs",
            "set",
            &format!("org.diskless:os={}", req.os_type),
            &full_name,
        ])?;

        let image = Image {
            id: Uuid::new_v4().to_string(),
            name: req.name.clone(),
            os_type,
            size_gb,
            path: PathBuf::from(format!("/dev/zvol/{}", full_name)),
            format: ImageFormat::Raw,
            status: "ready".to_string(),
            description: req.description,
            parent_id: None,
            checksum: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        sqlx::query(
            r#"
            INSERT INTO images (id, name, os_type, size_gb, path, format, status, description, parent_id, checksum, created_at, updated_at) 
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&image.id)
        .bind(&image.name)
        .bind(image.os_type.to_string())
        .bind(image.size_gb as i64)
        .bind(image.path.to_string_lossy().to_string())
        .bind(image.format.to_string())
        .bind(&image.status)
        .bind(&image.description)
        .bind(&image.parent_id)
        .bind(&image.checksum)
        .bind(image.created_at.to_rfc3339())
        .bind(image.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        info!("Image '{}' imported from {}", req.name, req.source_path);
        Ok(image)
    }

    pub async fn delete(&self, id: &str, _force: bool) -> anyhow::Result<()> {
        let image = self.get(id).await?;
        info!("Deleting image '{:?}'", image);
        // Check if in use
        // let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM clients WHERE image_id = ?")
        //     .bind(&image.name)
        //     .fetch_one(&self.pool)
        //     .await?;

        // if count.0 > 0 && !force {
        //     return Err(anyhow::anyhow!(
        //         "Image is in use by {} client(s). Use force=true to delete anyway.",
        //         count.0
        //     ));
        // }

        // Delete ZFS dataset
        if zfs_exists(&image.name) {
            // Use the image name as the ZFS dataset name
            zfs_destroy(&image.name)?;
        }

        // Delete from database
        sqlx::query("DELETE FROM images WHERE id = ?")
            .bind(&image.id)
            .execute(&self.pool)
            .await?;

        info!("Image '{}' deleted", image.name);
        Ok(())
    }

    pub async fn clone_image(&self, source_id: &str, new_name: &str) -> anyhow::Result<Image> {
        let source = self.get(source_id).await?;

        // Extract the parent dataset from the source image name
        let source_parts: Vec<&str> = source.name.split('/').collect();
        let parent_dataset = if source_parts.len() > 1 {
            source_parts[..source_parts.len() - 1].join("/")
        } else {
            // If the source doesn't have a parent, use the default images dataset
            let zpool = get_zpool_name();
            format!("{}/image-disk", zpool)
        };

        let new_full_name = format!("{}/{}", parent_dataset, new_name);

        // Check if ZFS dataset already exists
        if zfs_exists(&new_full_name) {
            return Err(anyhow::anyhow!(
                "ZFS dataset '{}' already exists.",
                new_full_name
            ));
        }

        // Perform ZFS clone operation
        zfs_clone(&source.name, &new_full_name)?; // Clone the ZFS dataset

        let image = Image {
            id: Uuid::new_v4().to_string(),
            name: new_name.to_string(),
            os_type: source.os_type,
            size_gb: source.size_gb,
            path: PathBuf::from(format!("/dev/zvol/{}", new_full_name)),
            format: source.format,
            status: "ready".to_string(),
            description: Some(format!("Clone of {}", source.name)),
            parent_id: Some(source.id),
            checksum: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        sqlx::query(
            r#"
            INSERT INTO images (id, name, os_type, size_gb, path, format, status, description, parent_id, checksum, created_at, updated_at) 
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&image.id)
        .bind(&image.name)
        .bind(image.os_type.to_string())
        .bind(image.size_gb as i64)
        .bind(image.path.to_string_lossy().to_string())
        .bind(image.format.to_string())
        .bind(&image.status)
        .bind(&image.description)
        .bind(&image.parent_id)
        .bind(&image.checksum)
        .bind(image.created_at.to_rfc3339())
        .bind(image.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        info!("Image '{}' cloned to '{}'", source.name, new_name);
        Ok(image)
    }

    pub async fn create_snapshot(
        &self,
        source_id: &str,
        snapshot_name: &str,
    ) -> anyhow::Result<Image> {
        let source = self.get(source_id).await?;

        // Create the snapshot name in ZFS format
        let snapshot_full_name = format!("{}@{}", source.name, snapshot_name);

        // Check if ZFS snapshot already exists
        if zfs_exists(&snapshot_full_name) {
            return Err(anyhow::anyhow!(
                "ZFS snapshot '{}' already exists.",
                snapshot_full_name
            ));
        }

        // Create ZFS snapshot
        run_command(&["zfs", "snapshot", &snapshot_full_name])?;

        // Get the source dataset size for the snapshot
        let output = run_command_output_no_sudo(&[
            "zfs",
            "get",
            "-H",
            "-o",
            "value",
            "volsize",
            &source.name,
        ])?;
        let size_str = output.trim();
        let size_gb = size_str.parse::<u64>().unwrap_or(source.size_gb);

        let image = Image {
            id: Uuid::new_v4().to_string(),
            name: snapshot_name.to_string(),
            os_type: source.os_type,
            size_gb,
            path: PathBuf::from(format!("/dev/zvol/{}", snapshot_full_name)),
            format: ImageFormat::Raw, // Snapshots use raw format
            status: "ready".to_string(),
            description: Some(format!("Snapshot of {}", source.name)),
            parent_id: Some(source.id),
            checksum: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        sqlx::query(
            r#"
            INSERT INTO images (id, name, os_type, size_gb, path, format, status, description, parent_id, checksum, created_at, updated_at) 
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&image.id)
        .bind(&image.name)
        .bind(image.os_type.to_string())
        .bind(image.size_gb as i64)
        .bind(image.path.to_string_lossy().to_string())
        .bind(image.format.to_string())
        .bind(&image.status)
        .bind(&image.description)
        .bind(&image.parent_id)
        .bind(&image.checksum)
        .bind(image.created_at.to_rfc3339())
        .bind(image.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        info!(
            "Snapshot '{}' created from '{}'",
            snapshot_full_name, source.name
        );
        Ok(image)
    }

    pub async fn get_info(&self, id: &str) -> anyhow::Result<ImageInfo> {
        let image = self.get(id).await?;

        // Get ZFS properties for the volume
        let output = run_command_output_no_sudo(&[
            "zfs",
            "get",
            "-H",
            "-o",
            "value",
            "volsize,used,compression",
            &image.name,
        ])?;

        let mut lines = output.lines();
        let volsize = lines.next().unwrap_or("0").trim();
        let used = lines.next().unwrap_or("0").trim();

        // Convert volsize from human-readable to bytes if needed
        let virtual_size = Self::parse_size_to_bytes(volsize)?;
        let actual_size = Self::parse_size_to_bytes(used)?;

        // For ZFS volumes, the format is typically raw
        let format = "raw".to_string();

        // Get snapshots for this dataset
        let snapshots = get_snapshots_for_dataset(&image.name)
            .unwrap_or_default()
            .iter()
            .map(|snap| snap.name.clone())
            .collect();

        Ok(ImageInfo {
            virtual_size,
            actual_size,
            format,
            backing_file: None, // ZFS volumes don't have backing files like qcow2
            snapshots,
        })
    }

    pub async fn resize(&self, id: &str, new_size_gb: u64) -> anyhow::Result<Image> {
        let mut image = self.get(id).await?;

        if new_size_gb < image.size_gb {
            return Err(anyhow::anyhow!(
                "Cannot shrink image. Current: {} GB, requested: {} GB",
                image.size_gb,
                new_size_gb
            ));
        }

        // Resize ZFS volume
        run_command(&[
            "zfs",
            "set",
            &format!("volsize={}G", new_size_gb),
            &image.name,
        ])?;

        image.size_gb = new_size_gb;
        image.updated_at = Utc::now();

        sqlx::query("UPDATE images SET size_gb = ?, updated_at = ? WHERE id = ?")
            .bind(image.size_gb as i64)
            .bind(image.updated_at.to_rfc3339())
            .bind(&image.id)
            .execute(&self.pool)
            .await?;

        info!("Image '{}' resized to {} GB", image.name, new_size_gb);
        Ok(image)
    }

    pub async fn verify(&self, id: &str) -> anyhow::Result<bool> {
        let image = self.get(id).await?;

        // Check if ZFS dataset exists
        Ok(zfs_exists(&image.name))
    }

    pub async fn rename(&self, id: &str, new_name: &str) -> anyhow::Result<Image> {
        info!(
            "Attempting to rename image with id '{}' to new name '{}'",
            id, new_name
        );
        let mut image = self.get(id).await?;

        info!("Current image name: '{}'", image.name);

        // Validate the new name
        validate_zfs_name(new_name)?;

        // Extract the parent dataset from the current image name
        // If the image.name doesn't contain '/', we need to find the actual ZFS dataset
        let current_parts: Vec<&str> = image.name.split('/').collect();
        let (parent_dataset, actual_zfs_name) = if current_parts.len() > 1 {
            // Full path provided, extract parent and use as actual ZFS name
            (
                current_parts[..current_parts.len() - 1].join("/"),
                image.name.clone(),
            )
        } else {
            // Simple name provided, need to find the actual ZFS dataset path
            let zpool = get_zpool_name();

            // Look for the actual ZFS dataset that matches this image name
            let mut found_actual_zfs_name = String::new();
            if let Ok(list_output) =
                run_command_output_no_sudo(&["zfs", "list", "-H", "-o", "name"])
            {
                for line in list_output.lines() {
                    let full_dataset_name = line.trim();
                    if full_dataset_name.ends_with(&format!("/{}", image.name)) {
                        // Found the full ZFS path
                        let parts: Vec<&str> = full_dataset_name.rsplitn(2, '/').collect();
                        if parts.len() == 2 {
                            let _actual_name = parts[0]; // actual name
                            let _parent = parts[1]; // parent
                            found_actual_zfs_name = full_dataset_name.to_string();
                            break; // Found it, exit loop
                        }
                    }
                }
            }

            // If we found the actual ZFS name, use it; otherwise, construct default path
            if !found_actual_zfs_name.is_empty() {
                let parts: Vec<&str> = found_actual_zfs_name.rsplitn(2, '/').collect();
                if parts.len() == 2 {
                    (parts[1].to_string(), found_actual_zfs_name) // (parent, actual_zfs_name)
                } else {
                    (
                        format!("{}/image-disk", zpool),
                        format!("{}/image-disk/{}", zpool, image.name),
                    )
                }
            } else {
                (
                    format!("{}/image-disk", zpool),
                    format!("{}/image-disk/{}", zpool, image.name),
                )
            }
        };

        info!("Parent dataset determined to be: '{}'", parent_dataset);

        let new_full_name = format!("{}/{}", parent_dataset, new_name);
        info!("New full name will be: '{}'", new_full_name);

        // Check if the new name already exists as a ZFS dataset
        if zfs_exists(&new_full_name) {
            info!(
                "ZFS dataset '{}' already exists, aborting rename",
                new_full_name
            );
            return Err(anyhow::anyhow!(
                "ZFS dataset '{}' already exists.",
                new_full_name
            ));
        }

        info!(
            "Checking if actual ZFS dataset '{}' exists",
            actual_zfs_name
        );
        // Rename the ZFS dataset if it exists
        if zfs_exists(&actual_zfs_name) {
            info!(
                "ZFS dataset '{}' exists, proceeding with rename to '{}'",
                actual_zfs_name, new_full_name
            );
            run_command(&["zfs", "rename", &actual_zfs_name, &new_full_name])?;
            info!("ZFS rename completed successfully");
        } else {
            info!(
                "ZFS dataset '{}' does not exist, skipping ZFS rename",
                actual_zfs_name
            );
        }

        // Update the image record
        let old_name = image.name.clone();
        let _old_path = image.path.clone();
        image.name = new_full_name;
        // Update the path to match the new name (ZFS volumes follow /dev/zvol/{name} pattern)
        image.path = PathBuf::from(format!("/dev/zvol/{}", image.name));
        image.updated_at = Utc::now();

        // Update the database record
        sqlx::query(
            r#"
            UPDATE images 
            SET name = ?, path = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&image.name)
        .bind(image.path.to_string_lossy().to_string())
        .bind(image.updated_at.to_rfc3339())
        .bind(&image.id)
        .execute(&self.pool)
        .await?;

        info!("Image renamed from '{}' to '{}'", old_name, image.name);
        Ok(image)
    }

    // Helper function to parse ZFS size strings to bytes
    fn parse_size_to_bytes(size_str: &str) -> anyhow::Result<u64> {
        let size_str = size_str.trim();
        if size_str.is_empty() || size_str == "-" {
            return Ok(0);
        }

        // Handle common ZFS size suffixes
        let (num_str, multiplier) = if size_str.ends_with('K') {
            (&size_str[..size_str.len() - 1], 1024u64)
        } else if size_str.ends_with('M') {
            (&size_str[..size_str.len() - 1], 1024u64 * 1024)
        } else if size_str.ends_with('G') {
            (&size_str[..size_str.len() - 1], 1024u64 * 1024 * 1024)
        } else if size_str.ends_with('T') {
            (
                &size_str[..size_str.len() - 1],
                1024u64 * 1024 * 1024 * 1024,
            )
        } else if size_str.ends_with('P') {
            (
                &size_str[..size_str.len() - 1],
                1024u64 * 1024 * 1024 * 1024 * 1024,
            )
        } else {
            (size_str, 1u64) // Assume bytes if no suffix
        };

        let num = num_str.parse::<f64>()?;
        Ok((num * multiplier as f64) as u64)
    }
}
