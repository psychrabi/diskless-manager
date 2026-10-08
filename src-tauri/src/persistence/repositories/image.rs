use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use std::path::PathBuf;

use crate::core::image::{Image, ImageFormat, ImageKind, OsType};

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

    /// Returns whether any client references this image or one of its snapshots.
    pub async fn has_client_references(&self, image: &Image) -> Result<bool> {
        let exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM clients WHERE master = ? OR master = ? OR snapshot = ? OR substr(snapshot, 1, length(?) + 1) = ? || '@' OR writeback = ? OR block_device = ?)",
        )
        .bind(&image.id).bind(&image.name).bind(&image.id)
        .bind(&image.name).bind(&image.name).bind(&image.name)
        .bind(image.path.to_string_lossy().as_ref())
        .fetch_one(&self.pool).await?;
        Ok(exists != 0)
    }

    /// Returns every image ordered by name, with validated metadata.
    ///
    /// Database and metadata decoding errors are returned without omitting invalid rows.
    pub async fn list(&self) -> Result<Vec<Image>> {
        // Transfer one document across SQLite's worker boundary instead of one message
        // per row. Decode into the same owned records; no inventory cache is retained.
        let document: String = sqlx::query_scalar(
            r#"
            SELECT json_group_array(json_array(
                id, name, kind, os_type, size_gb, path, format, status, description,
                parent_id, source_snapshot, checksum, is_default, created_at, updated_at
            ))
            FROM (SELECT * FROM images ORDER BY name)
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let rows: Vec<ImageRow> =
            serde_json::from_str(&document).context("failed to decode image inventory rows")?;
        drop(document);
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

    /// Selects one default image atomically, preserving the previous default on failure.
    ///
    /// Returns false if `id` does not exist. Database errors leave the selection unchanged.
    pub async fn set_default(&self, id: &str) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE images SET is_default = (id = ?), updated_at = ?
             WHERE EXISTS (SELECT 1 FROM images WHERE id = ?)",
        )
        .bind(id)
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

#[cfg(test)]
mod listing_tests {
    use super::*;

    async fn repository() -> ImageRepository {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        ImageRepository::new(pool)
    }

    async fn insert(repository: &ImageRepository, id: &str) {
        sqlx::query("INSERT INTO images (id,name,kind,os_type,size_gb,path,format,status,description,is_default,created_at,updated_at) VALUES (?,?,'MASTER','Windows',-1,'/dev/zvol/тест','IMG','ready','quoted \"text\"',2,'2026-01-01T03:30:00+03:30','2026-01-02T00:00:00Z')")
            .bind(id).bind(id).execute(&repository.pool).await.unwrap();
    }

    #[tokio::test]
    async fn dependency_checks_cover_every_reference_without_like_wildcards() {
        let repository = repository().await;
        insert(&repository, "image").await;
        let mut image = repository.get("image").await.unwrap().unwrap();
        image.name = "diskless/reference-image".into();
        assert!(!repository.has_client_references(&image).await.unwrap());
        sqlx::query("INSERT INTO clients (id,name,mac,ip,master,enabled,created_at,updated_at) VALUES ('pc','PC','mac','ip','unrelated',1,'now','now')")
            .execute(&repository.pool).await.unwrap();
        for (column, value) in [
            ("master", image.id.clone()),
            ("master", image.name.clone()),
            ("snapshot", image.id.clone()),
            ("snapshot", format!("{}@ready", image.name)),
            ("writeback", image.name.clone()),
            ("block_device", image.path.to_string_lossy().into_owned()),
        ] {
            sqlx::query(&format!("UPDATE clients SET {column} = ?"))
                .bind(value)
                .execute(&repository.pool)
                .await
                .unwrap();
            assert!(
                repository.has_client_references(&image).await.unwrap(),
                "missed {column}"
            );
            sqlx::query("UPDATE clients SET master='unrelated',snapshot=NULL,writeback=NULL,block_device=NULL")
                .execute(&repository.pool).await.unwrap();
            assert!(!repository.has_client_references(&image).await.unwrap());
        }
        sqlx::query("UPDATE clients SET snapshot = ?")
            .bind(format!("{}2@ready", image.name))
            .execute(&repository.pool)
            .await
            .unwrap();
        assert!(!repository.has_client_references(&image).await.unwrap());
        let mut literal = image;
        literal.name = "diskless/win_11%".into();
        sqlx::query("UPDATE clients SET snapshot='diskless/winX11other@ready'")
            .execute(&repository.pool)
            .await
            .unwrap();
        assert!(!repository.has_client_references(&literal).await.unwrap());
        sqlx::query("UPDATE clients SET snapshot='diskless/win_11%@ready'")
            .execute(&repository.pool)
            .await
            .unwrap();
        assert!(repository.has_client_references(&literal).await.unwrap());
    }

    #[tokio::test]
    async fn listing_matches_single_record_decoding_and_ordering() {
        let repository = repository().await;
        assert!(repository.list().await.unwrap().is_empty());
        insert(&repository, "z-image").await;
        insert(&repository, "a-image").await;
        let expected = vec![
            repository.get("a-image").await.unwrap().unwrap(),
            repository.get("z-image").await.unwrap().unwrap(),
        ];
        assert_eq!(
            serde_json::to_vec(&repository.list().await.unwrap()).unwrap(),
            serde_json::to_vec(&expected).unwrap()
        );
    }

    #[tokio::test]
    async fn listing_matches_sqlx_storage_class_rules() {
        let repository = repository().await;
        insert(&repository, "image").await;
        // SQLite affinity converts integral numeric text and numeric TEXT fields.
        sqlx::query("UPDATE images SET size_gb = '20', status = 123")
            .execute(&repository.pool)
            .await
            .unwrap();
        let expected = repository.get("image").await.unwrap().unwrap();
        assert_eq!(
            serde_json::to_vec(&repository.list().await.unwrap()[0]).unwrap(),
            serde_json::to_vec(&expected).unwrap()
        );
        for value in ["12.5", "not a number"] {
            sqlx::query("UPDATE images SET size_gb = ?")
                .bind(value)
                .execute(&repository.pool)
                .await
                .unwrap();
            assert!(repository.get("image").await.is_err());
            assert!(repository.list().await.is_err());
        }
        sqlx::query("UPDATE images SET size_gb = 20, description = ?")
            .bind(vec![65_u8, 66])
            .execute(&repository.pool)
            .await
            .unwrap();
        assert!(repository.get("image").await.is_err());
        assert!(repository.list().await.is_err());
    }

    #[tokio::test]
    async fn listing_rejects_invalid_metadata_instead_of_silently_dropping_rows() {
        for (column, value) in [
            ("kind", "invalid"),
            ("os_type", "invalid"),
            ("format", "invalid"),
            ("created_at", "invalid"),
            ("updated_at", "invalid"),
        ] {
            let repository = repository().await;
            insert(&repository, "image").await;
            sqlx::query(&format!("UPDATE images SET {column} = ?"))
                .bind(value)
                .execute(&repository.pool)
                .await
                .unwrap();
            assert!(
                repository.list().await.is_err(),
                "accepted invalid {column}"
            );
        }
    }
}
