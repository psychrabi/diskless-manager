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

    pub async fn publish_verified_draft(
        &self,
        session_id: &str,
        image_id: &str,
        target_iqn: &str,
        attempt: i64,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let now = Utc::now().to_rfc3339();
        let image = sqlx::query("UPDATE images SET status='ready',description='Published after verified normal boot',updated_at=? WHERE id=? AND status='draft'")
            .bind(&now).bind(image_id).execute(&mut *tx).await?.rows_affected();
        if image != 1 {
            anyhow::bail!("verified draft is no longer publishable");
        }
        let session = sqlx::query("UPDATE installation_sessions SET state='published',published_image_id=?,progress=100,phase_deadline=NULL,updated_at=? WHERE id=? AND kind='factory' AND legacy=0 AND closed_at IS NULL AND state='verifying' AND verification_target_iqn=? AND attempt=? AND draft_image_id=?")
            .bind(image_id).bind(&now).bind(session_id).bind(target_iqn).bind(attempt).bind(image_id).execute(&mut *tx).await?.rows_affected();
        if session != 1 {
            anyhow::bail!("verification attempt is no longer current");
        }
        sqlx::query("INSERT INTO installation_events(session_id,state,progress,message,actor,created_at) VALUES (?,'published',100,'Normal boot verified; image published','verification',?)")
            .bind(session_id).bind(now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn delete(&self, id: &str) -> Result<bool> {
        let (transaction, deleted) = self.begin_delete(id, false).await?;
        transaction.commit().await?;
        Ok(deleted)
    }

    pub async fn has_non_snapshot_children(&self, image_id: &str) -> Result<bool> {
        let exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM images WHERE parent_id = ? AND kind != 'snapshot')",
        )
        .bind(image_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists != 0)
    }

    pub async fn assigned_client_targets(&self, image_name: &str) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar(
            "SELECT target_iqn FROM clients WHERE master = ? AND target_iqn IS NOT NULL AND target_iqn != ''",
        )
        .bind(image_name)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Client names whose saved boot assignment names one of `snapshot_names`.
    pub async fn clients_assigned_to_snapshots(
        &self,
        master_name: &str,
        snapshot_names: &[String],
    ) -> Result<Vec<String>> {
        if snapshot_names.is_empty() {
            return Ok(Vec::new());
        }
        let mut query =
            sqlx::QueryBuilder::<sqlx::Sqlite>::new("SELECT name FROM clients WHERE snapshot IN (");
        let mut separated = query.separated(", ");
        for name in snapshot_names {
            separated.push_bind(format!("{}@{}", master_name, name));
        }
        query.push(") ORDER BY name");
        Ok(query.build_query_scalar().fetch_all(&self.pool).await?)
    }

    #[cfg(test)]
    pub(crate) fn test_pool(&self) -> &sqlx::SqlitePool {
        &self.pool
    }

    pub async fn has_pending_installation(&self, image_id: &str) -> Result<bool> {
        let exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM installation_sessions WHERE closed_at IS NULL AND ? IN (draft_image_id, verification_image_id, published_image_id))",
        )
        .bind(image_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists != 0)
    }

    pub async fn begin_delete_snapshots(
        &self,
        ids: &[String],
    ) -> Result<sqlx::Transaction<'static, sqlx::Sqlite>> {
        let mut transaction = self.pool.begin().await?;
        for id in ids {
            sqlx::query("UPDATE windows_revision_boot_tests SET candidate_image_id = NULL WHERE candidate_image_id = ?")
                .bind(id)
                .execute(&mut *transaction)
                .await?;
            sqlx::query("DELETE FROM images WHERE id = ?")
                .bind(id)
                .execute(&mut *transaction)
                .await?;
        }
        Ok(transaction)
    }

    /// Stage metadata removal before destroying storage. Keep the transaction open
    /// until storage succeeds so constraints cannot fail after destructive work.
    /// Dropping it on failure restores the image and its boot-test references.
    pub async fn begin_delete(
        &self,
        id: &str,
        prune_missing_storage_revisions: bool,
    ) -> Result<(sqlx::Transaction<'static, sqlx::Sqlite>, bool)> {
        let mut transaction = self.pool.begin().await?;
        // Preserve historical test results when their candidate image is removed.
        sqlx::query("UPDATE windows_revision_boot_tests SET candidate_image_id = NULL WHERE candidate_image_id = ?")
            .bind(id)
            .execute(&mut *transaction)
            .await?;
        if prune_missing_storage_revisions {
            // The storage is already gone, so no revision can be built from or
            // tested on this image again. Detach candidates and drop revisions
            // whose source was this image; otherwise the FK keeps an unusable
            // record alive forever.
            sqlx::query(
                "DELETE FROM windows_revision_boot_tests WHERE revision_id IN (SELECT id FROM windows_image_driver_revisions WHERE source_image_id = ?)",
            )
            .bind(id)
            .execute(&mut *transaction)
            .await?;
            sqlx::query("DELETE FROM windows_image_driver_revisions WHERE source_image_id = ?")
                .bind(id)
                .execute(&mut *transaction)
                .await?;
            sqlx::query(
                "UPDATE windows_image_driver_revisions SET status = 'failed', revision_image_id = NULL, updated_at = ? WHERE revision_image_id = ? AND status != 'failed'",
            )
            .bind(Utc::now().to_rfc3339())
            .bind(id)
            .execute(&mut *transaction)
            .await?;
            // If the underlying disk is gone, Super sessions that only exist to
            // reference it cannot be preserved; detach or drop them so the
            // metadata can be removed.
            sqlx::query("DELETE FROM super_sessions WHERE prior_image_id = ?")
                .bind(id)
                .execute(&mut *transaction)
                .await?;
        }
        let sessions: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM super_sessions WHERE prior_image_id = ?")
                .bind(id)
                .fetch_one(&mut *transaction)
                .await?;
        let result = sqlx::query("DELETE FROM images WHERE id = ?")
            .bind(id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                if matches!(&error, sqlx::Error::Database(error) if error.is_foreign_key_violation()) {
                    if sessions > 0 {
                        anyhow::anyhow!(
                            "cannot delete image: {sessions} Super session(s) still reference it; save or discard them first"
                        )
                    } else {
                        anyhow::anyhow!("cannot delete image: required by driver revisions or super sessions")
                    }
                } else {
                    error.into()
                }
            })?;
        Ok((transaction, result.rows_affected() > 0))
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
        let mut transaction = self.pool.begin().await?;
        sqlx::query("UPDATE images SET is_default = 0")
            .execute(&mut *transaction)
            .await?;
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
        .execute(&mut *transaction)
        .await?;

        if result.rows_affected() == 0 {
            transaction.rollback().await?;
            return Ok(false);
        }

        transaction.commit().await?;
        Ok(true)
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
        source_snapshot: &str,
        driver_set_hash: &str,
    ) -> Result<Option<ImageDriverRevision>> {
        use anyhow::Context;
        // Failed attempts never block a retry; only the live record matches.
        let row = sqlx::query_as::<_, (String, String, String, Option<String>, String, String, String, String, String)>(
            r#"SELECT id, source_image_id, source_snapshot, revision_image_id, driver_package_ids, driver_set_hash, status, created_at, updated_at
            FROM windows_image_driver_revisions WHERE source_image_id = ? AND source_snapshot = ? AND driver_set_hash = ? AND status != 'failed' LIMIT 1"#,
        )
        .bind(source_image_id)
        .bind(source_snapshot)
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

#[cfg(test)]
mod delete_missing_storage_tests {
    use super::*;
    use crate::core::image::{
        DriverRevisionStatus, Image, ImageDriverRevision, ImageFormat, ImageKind, OsType,
    };

    async fn pool() -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    async fn insert_image(repo: &ImageRepository, id: &str, name: &str) {
        let now = Utc::now();
        repo.insert(&Image {
            id: id.into(),
            name: name.into(),
            kind: ImageKind::Master,
            os_type: OsType::Windows,
            size_gb: 50,
            path: format!("/dev/zvol/{name}").into(),
            format: ImageFormat::Raw,
            status: "ready".into(),
            description: None,
            parent_id: None,
            source_snapshot: None,
            checksum: None,
            is_default: false,
            created_at: now,
            updated_at: now,
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn image_with_missing_storage_drops_its_driver_revisions() {
        let pool = pool().await;
        let repo = ImageRepository::new(pool.clone());
        insert_image(&repo, "img", "pool/image-disk/windows").await;
        let now = Utc::now();
        repo.insert_driver_revision(&ImageDriverRevision {
            id: "rev".into(),
            source_image_id: "img".into(),
            source_snapshot: "installed".into(),
            revision_image_id: None,
            driver_package_ids: vec!["pkg-a".into()],
            driver_set_hash: "hash".into(),
            status: DriverRevisionStatus::AwaitingTest,
            created_at: now,
            updated_at: now,
        })
        .await
        .unwrap();

        // While storage exists the reference must still block the delete.
        assert!(repo.begin_delete("img", false).await.is_err());

        let (transaction, deleted) = repo.begin_delete("img", true).await.unwrap();
        assert!(deleted);
        transaction.commit().await.unwrap();
        assert!(repo.get("img").await.unwrap().is_none());
        assert!(repo.list_driver_revisions("img").await.unwrap().is_empty());
    }
}
