use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use std::path::PathBuf;

use crate::core::image::{
    DriverRevisionStatus, Image, ImageDriverRevision, ImageFormat, ImageKind, OsType,
};

#[derive(Clone)]
pub struct ImageRepository {
    pool: SqlitePool,
}

type ImageRow = (
    String,         // id
    String,         // name
    String,         // kind
    String,         // os_type
    i64,            // size_gb
    String,         // path
    String,         // format
    String,         // status
    Option<String>, // description
    Option<String>, // parent_id
    Option<String>, // source_snapshot
    Option<String>, // checksum
    i64,            // is_default
    String,         // created_at
    String,         // updated_at
);

impl ImageRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> Result<Vec<Image>> {
        let rows = sqlx::query_as::<_, ImageRow>(
            r#"
            SELECT
                id,
                name,
                kind,
                os_type,
                size_gb,
                path,
                format,
                status,
                description,
                parent_id,
                source_snapshot,
                checksum,
                is_default,
                created_at,
                updated_at
            FROM images
            ORDER BY name
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(Self::map_row).collect()
    }

    pub async fn get(&self, id_or_name: &str) -> Result<Option<Image>> {
        let row = sqlx::query_as::<
            _,
            (
                String,         // id
                String,         // name
                String,         // kind
                String,         // os_type
                i64,            // size_gb
                String,         // path
                String,         // format
                String,         // status
                Option<String>, // description
                Option<String>, // parent_id
                Option<String>, // source_snapshot
                Option<String>, // checksum
                i64,            // is_default
                String,         // created_at
                String,         // updated_at
            ),
        >(
            r#"
            SELECT
                id,
                name,
                kind,
                os_type,
                size_gb,
                path,
                format,
                status,
                description,
                parent_id,
                source_snapshot,
                checksum,
                is_default,
                created_at,
                updated_at
            FROM images
            WHERE id = ? OR name = ?
            LIMIT 1
            "#,
        )
        .bind(id_or_name)
        .bind(id_or_name)
        .fetch_optional(&self.pool)
        .await?;

        row.map(Self::map_row).transpose()
    }

    pub async fn insert(&self, image: &Image) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO images (
                id,
                name,
                kind,
                os_type,
                size_gb,
                path,
                format,
                status,
                description,
                parent_id,
                source_snapshot,
                checksum,
                is_default,
                created_at,
                updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&image.id)
        .bind(&image.name)
        // ImageKind -> SQLite TEXT
        .bind(image.kind.to_string())
        .bind(image.os_type.to_string())
        .bind(image.size_gb as i64)
        .bind(image.path.to_string_lossy().to_string())
        .bind(image.format.to_string())
        .bind(&image.status)
        .bind(&image.description)
        .bind(&image.parent_id)
        .bind(&image.source_snapshot)
        .bind(&image.checksum)
        .bind(if image.is_default { 1_i64 } else { 0_i64 })
        .bind(image.created_at.to_rfc3339())
        .bind(image.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn update(&self, image: &Image) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE images
            SET
                name = ?,
                kind = ?,
                os_type = ?,
                size_gb = ?,
                path = ?,
                format = ?,
                status = ?,
                description = ?,
                parent_id = ?,
                source_snapshot = ?,
                checksum = ?,
                is_default = ?,
                updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&image.name)
        // ImageKind -> SQLite TEXT
        .bind(image.kind.to_string())
        .bind(image.os_type.to_string())
        .bind(image.size_gb as i64)
        .bind(image.path.to_string_lossy().to_string())
        .bind(image.format.to_string())
        .bind(&image.status)
        .bind(&image.description)
        .bind(&image.parent_id)
        .bind(&image.source_snapshot)
        .bind(&image.checksum)
        .bind(if image.is_default { 1_i64 } else { 0_i64 })
        .bind(image.updated_at.to_rfc3339())
        .bind(&image.id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete(&self, id: &str) -> Result<bool> {
        let result = sqlx::query(
            r#"
            DELETE FROM images
            WHERE id = ?
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn clear_default(&self) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE images
            SET is_default = 0
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn set_default(&self, id: &str) -> Result<bool> {
        let result = sqlx::query(
            r#"
            UPDATE images
            SET
                is_default = 1,
                updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(Utc::now().to_rfc3339())
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    fn map_row(row: ImageRow) -> Result<Image> {
        let (
            id,
            name,
            kind,
            os_type,
            size_gb,
            path,
            format,
            status,
            description,
            parent_id,
            source_snapshot,
            checksum,
            is_default,
            created_at,
            updated_at,
        ) = row;

        let kind = kind
            .parse::<ImageKind>()
            .with_context(|| format!("invalid image kind '{}' for image '{}'", kind, name))?;

        let os_type = os_type
            .parse::<OsType>()
            .with_context(|| format!("invalid OS type '{}' for image '{}'", os_type, name))?;

        let format = format
            .parse::<ImageFormat>()
            .with_context(|| format!("invalid image format '{}' for image '{}'", format, name))?;

        let created_at = parse_image_datetime(&created_at)
            .with_context(|| format!("invalid created_at '{}' for image '{}'", created_at, name))?;

        let updated_at = parse_image_datetime(&updated_at)
            .with_context(|| format!("invalid updated_at '{}' for image '{}'", updated_at, name))?;

        Ok(Image {
            id,
            name,
            kind,
            os_type,
            size_gb: size_gb.max(0) as u64,
            path: PathBuf::from(path),
            format,
            status,
            description,
            parent_id,
            source_snapshot,
            checksum,
            is_default: is_default != 0,
            created_at,
            updated_at,
        })
    }

    /// Create a driver revision record; duplicate IDs are rejected by the PK.
    pub async fn insert_driver_revision(&self, revision: &ImageDriverRevision) -> Result<()> {
        use anyhow::Context;
        sqlx::query(
            r#"INSERT INTO windows_image_driver_revisions
            (id, source_image_id, source_snapshot, revision_image_id, driver_package_ids, driver_set_hash, status, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&revision.id)
        .bind(&revision.source_image_id)
        .bind(&revision.source_snapshot)
        .bind(&revision.revision_image_id)
        .bind(serde_json::to_string(&revision.driver_package_ids).context("failed to encode driver package list")?)
        .bind(&revision.driver_set_hash)
        .bind(revision.status.to_string())
        .bind(revision.created_at.to_rfc3339())
        .bind(revision.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .with_context(|| format!("failed to insert driver revision '{}'", revision.id))?;
        Ok(())
    }

    pub async fn list_driver_revisions(
        &self,
        source_image_id: &str,
    ) -> Result<Vec<ImageDriverRevision>> {
        use anyhow::Context;
        let rows = sqlx::query_as::<_, (String, String, String, Option<String>, String, String, String, String, String)>(
            r#"SELECT id, source_image_id, source_snapshot, revision_image_id, driver_package_ids, driver_set_hash, status, created_at, updated_at
            FROM windows_image_driver_revisions WHERE source_image_id = ? ORDER BY created_at"#,
        )
        .bind(source_image_id)
        .fetch_all(&self.pool)
        .await
        .with_context(|| format!("failed to list driver revisions for image '{source_image_id}'"))?;
        rows.into_iter().map(Self::map_revision_row).collect()
    }

    pub async fn find_driver_revision_by_hash(
        &self,
        source_image_id: &str,
        driver_set_hash: &str,
    ) -> Result<Option<ImageDriverRevision>> {
        use anyhow::Context;
        let row = sqlx::query_as::<_, (String, String, String, Option<String>, String, String, String, String, String)>(
            r#"SELECT id, source_image_id, source_snapshot, revision_image_id, driver_package_ids, driver_set_hash, status, created_at, updated_at
            FROM windows_image_driver_revisions WHERE source_image_id = ? AND driver_set_hash = ? LIMIT 1"#,
        )
        .bind(source_image_id)
        .bind(driver_set_hash)
        .fetch_optional(&self.pool)
        .await
        .with_context(|| format!("failed to look up driver revision for image '{source_image_id}'"))?;
        row.map(Self::map_revision_row).transpose()
    }

    /// Transition a revision's lifecycle status; invalid transitions are rejected.
    pub async fn transition_driver_revision(
        &self,
        id: &str,
        next: DriverRevisionStatus,
        revision_image_id: Option<&str>,
    ) -> Result<ImageDriverRevision> {
        let row = sqlx::query_as::<_, (String, String, String, Option<String>, String, String, String, String, String)>(
            r#"SELECT id, source_image_id, source_snapshot, revision_image_id, driver_package_ids, driver_set_hash, status, created_at, updated_at
            FROM windows_image_driver_revisions WHERE id = ?"#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("driver revision '{}' not found", id))?;
        let mut revision = Self::map_revision_row(row)?;
        if !revision.can_transition(next) {
            anyhow::bail!(
                "invalid driver revision transition from '{}' to '{}'",
                revision.status,
                next
            );
        }
        revision.status = next;
        if let Some(image_id) = revision_image_id {
            revision.revision_image_id = Some(image_id.to_string());
        }
        revision.updated_at = Utc::now();
        sqlx::query(
            r#"UPDATE windows_image_driver_revisions SET status = ?, revision_image_id = ?, updated_at = ? WHERE id = ?"#,
        )
        .bind(revision.status.to_string())
        .bind(&revision.revision_image_id)
        .bind(revision.updated_at.to_rfc3339())
        .bind(&revision.id)
        .execute(&self.pool)
        .await?;
        Ok(revision)
    }

    fn map_revision_row(
        row: (
            String,
            String,
            String,
            Option<String>,
            String,
            String,
            String,
            String,
            String,
        ),
    ) -> Result<ImageDriverRevision> {
        let (
            id,
            source_image_id,
            source_snapshot,
            revision_image_id,
            package_ids,
            hash,
            status,
            created_at,
            updated_at,
        ) = row;
        Ok(ImageDriverRevision {
            id,
            source_image_id,
            source_snapshot,
            revision_image_id,
            driver_package_ids: serde_json::from_str(&package_ids)
                .context("invalid driver package list")?,
            driver_set_hash: hash,
            status: status.parse::<DriverRevisionStatus>()?,
            created_at: parse_image_datetime(&created_at)?,
            updated_at: parse_image_datetime(&updated_at)?,
        })
    }
}

fn parse_image_datetime(value: &str) -> Result<DateTime<Utc>> {
    if let Ok(value) = DateTime::parse_from_rfc3339(value) {
        return Ok(value.with_timezone(&Utc));
    }
    if let Ok(value) = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S") {
        return Ok(value.and_utc());
    }
    anyhow::bail!("invalid stored datetime: {value}")
}
