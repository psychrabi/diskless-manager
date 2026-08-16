use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use std::path::PathBuf;

use crate::core::image::{Image, ImageFormat, ImageKind, OsType};

#[derive(Clone)]
pub struct ImageRepository {
    pool: SqlitePool,
}

impl ImageRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> Result<Vec<Image>> {
        let rows = sqlx::query_as::<
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

    fn map_row(
        row: (
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
    ) -> Result<Image> {
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

        let created_at = DateTime::parse_from_rfc3339(&created_at)
            .with_context(|| format!("invalid created_at '{}' for image '{}'", created_at, name))?
            .with_timezone(&Utc);

        let updated_at = DateTime::parse_from_rfc3339(&updated_at)
            .with_context(|| format!("invalid updated_at '{}' for image '{}'", updated_at, name))?
            .with_timezone(&Utc);

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
}
