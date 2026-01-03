use chrono::{DateTime, Utc};
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
}

impl std::fmt::Display for ImageFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImageFormat::Raw => write!(f, "raw"),
            ImageFormat::Qcow2 => write!(f, "qcow2"),
            ImageFormat::Vmdk => write!(f, "vmdk"),
            ImageFormat::Vdi => write!(f, "vdi"),
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
        let format: ImageFormat = req.format.unwrap_or_else(|| "raw".to_string()).parse()?;

        // Ensure images directory exists
        std::fs::create_dir_all(&self.images_dir)?;

        let path = self
            .images_dir
            .join(format!("{}.{}", req.name, format.extension()));

        // Create the image using qemu-img
        let output = Command::new("qemu-img")
            .args([
                "create",
                "-f",
                &format.to_string(),
                &path.to_string_lossy(),
                &format!("{}G", req.size_gb),
            ])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to create image: {}", stderr));
        }

        let image = Image {
            id: Uuid::new_v4().to_string(),
            name: req.name.clone(),
            os_type,
            size_gb: req.size_gb,
            path,
            format,
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

        tracing::info!("Image '{}' created ({} GB)", req.name, req.size_gb);
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

        std::fs::create_dir_all(&self.images_dir)?;

        let dest = self.images_dir.join(format!("{}.img", req.name));

        // Convert to raw format for iSCSI compatibility
        let output = Command::new("qemu-img")
            .args([
                "convert",
                "-p",
                "-O",
                "raw",
                &source.to_string_lossy(),
                &dest.to_string_lossy(),
            ])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to import image: {}", stderr));
        }

        // Get size
        let metadata = std::fs::metadata(&dest)?;
        let size_gb = metadata.len() / (1024 * 1024 * 1024);

        let image = Image {
            id: Uuid::new_v4().to_string(),
            name: req.name.clone(),
            os_type,
            size_gb: size_gb.max(1),
            path: dest,
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

        tracing::info!("Image '{}' imported from {}", req.name, req.source_path);
        Ok(image)
    }

    pub async fn delete(&self, id: &str, _force: bool) -> anyhow::Result<()> {
        let image = self.get(id).await?;

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

        // Delete file
        if image.path.exists() {
            std::fs::remove_file(&image.path)?;
        }

        // Delete from database
        sqlx::query("DELETE FROM images WHERE id = ?")
            .bind(&image.id)
            .execute(&self.pool)
            .await?;

        tracing::info!("Image '{}' deleted", image.name);
        Ok(())
    }

    pub async fn clone_image(&self, source_id: &str, new_name: &str) -> anyhow::Result<Image> {
        let source = self.get(source_id).await?;

        let dest_path = self
            .images_dir
            .join(format!("{}.{}", new_name, source.format.extension()));

        // Copy with sparse support
        let output = Command::new("cp")
            .args([
                "--sparse=auto",
                "--reflink=auto",
                &source.path.to_string_lossy(),
                &dest_path.to_string_lossy(),
            ])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to clone image: {}", stderr));
        }

        let image = Image {
            id: Uuid::new_v4().to_string(),
            name: new_name.to_string(),
            os_type: source.os_type,
            size_gb: source.size_gb,
            path: dest_path,
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

        tracing::info!("Image '{}' cloned to '{}'", source.name, new_name);
        Ok(image)
    }

    pub async fn create_snapshot(
        &self,
        source_id: &str,
        snapshot_name: &str,
    ) -> anyhow::Result<Image> {
        let source = self.get(source_id).await?;

        std::fs::create_dir_all(&self.snapshots_dir)?;

        let snapshot_path = self.snapshots_dir.join(format!("{}.qcow2", snapshot_name));

        // Create COW snapshot using qcow2
        let output = Command::new("qemu-img")
            .args([
                "create",
                "-f",
                "qcow2",
                "-b",
                &source.path.to_string_lossy(),
                "-F",
                &source.format.to_string(),
                &snapshot_path.to_string_lossy(),
            ])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to create snapshot: {}", stderr));
        }

        let image = Image {
            id: Uuid::new_v4().to_string(),
            name: snapshot_name.to_string(),
            os_type: source.os_type,
            size_gb: source.size_gb,
            path: snapshot_path,
            format: ImageFormat::Qcow2,
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

        tracing::info!(
            "Snapshot '{}' created from '{}'",
            snapshot_name,
            source.name
        );
        Ok(image)
    }

    pub async fn get_info(&self, id: &str) -> anyhow::Result<ImageInfo> {
        let image = self.get(id).await?;

        let output = Command::new("qemu-img")
            .args(["info", "--output=json", &image.path.to_string_lossy()])
            .output()?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("Failed to get image info"));
        }

        let info: serde_json::Value = serde_json::from_slice(&output.stdout)?;

        Ok(ImageInfo {
            virtual_size: info["virtual-size"].as_u64().unwrap_or(0),
            actual_size: info["actual-size"].as_u64().unwrap_or(0),
            format: info["format"].as_str().unwrap_or("unknown").to_string(),
            backing_file: info["backing-filename"].as_str().map(String::from),
            snapshots: info["snapshots"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|s| s["name"].as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
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

        let output = Command::new("qemu-img")
            .args([
                "resize",
                &image.path.to_string_lossy(),
                &format!("{}G", new_size_gb),
            ])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to resize image: {}", stderr));
        }

        image.size_gb = new_size_gb;
        image.updated_at = Utc::now();

        sqlx::query("UPDATE images SET size_gb = ?, updated_at = ? WHERE id = ?")
            .bind(image.size_gb as i64)
            .bind(image.updated_at.to_rfc3339())
            .bind(&image.id)
            .execute(&self.pool)
            .await?;

        tracing::info!("Image '{}' resized to {} GB", image.name, new_size_gb);
        Ok(image)
    }

    pub async fn verify(&self, id: &str) -> anyhow::Result<bool> {
        let image = self.get(id).await?;

        if !image.path.exists() {
            return Ok(false);
        }

        let output = Command::new("qemu-img")
            .args(["check", &image.path.to_string_lossy()])
            .output()?;

        Ok(output.status.success())
    }
}
