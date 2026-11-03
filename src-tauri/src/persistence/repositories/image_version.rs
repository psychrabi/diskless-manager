use anyhow::Result;

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ImageVersionRow {
    pub id: String,
    pub name: String,
    pub base_name: String,
    pub version: String,
    pub image_asset_id: String,
    pub parent_version_id: Option<String>,
    pub os_type: String,
    pub source_description: Option<String>,
    pub comment: Option<String>,
    pub is_pinned: bool,
    pub is_locked: bool,
    pub ordering: i64,
    pub published_by: Option<String>,
    pub published_at: String,
    pub last_used_at: Option<String>,
    pub last_used_by_client_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// One version to insert. Grouped in a struct because the version name and its
/// provenance are otherwise indistinguishable positional arguments.
pub struct NewImageVersion<'a> {
    pub id: &'a str,
    /// `base_name/version`; the split is derived here so callers cannot insert
    /// a row whose name disagrees with its identity columns.
    pub name: &'a str,
    pub image_asset_id: &'a str,
    pub parent_version_id: Option<&'a str>,
    pub source_description: Option<&'a str>,
    pub comment: Option<&'a str>,
    pub os_type: &'a str,
    pub published_by: Option<&'a str>,
    pub created_at: &'a str,
}

/// Why a version row could not be written.
///
/// `Duplicate` is reported by the `(base_name, version)` unique index, so an
/// HTTP caller can answer `409` instead of a generic storage failure.
#[derive(Debug, thiserror::Error)]
pub enum ImageVersionWriteError {
    #[error("image version '{0}' already exists")]
    Duplicate(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

#[derive(Clone)]
pub struct ImageVersionRepository {
    pool: sqlx::SqlitePool,
}

impl ImageVersionRepository {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list(
        &self,
        base_name: Option<&str>,
        all: bool,
    ) -> Result<Vec<ImageVersionRow>, sqlx::Error> {
        let rows = sqlx::query_as::<_, ImageVersionRow>(
            r#"
            SELECT id, name, base_name, version, image_asset_id, parent_version_id,
                   os_type, source_description, comment, is_pinned, is_locked,
                   ordering, published_by, published_at, last_used_at,
                   last_used_by_client_id, created_at, updated_at
            FROM image_versions
            WHERE ? IS NULL OR base_name = ?
            ORDER BY base_name, ordering DESC, created_at DESC
            "#,
        )
        .bind(base_name)
        .bind(base_name)
        .fetch_all(&self.pool)
        .await?;

        if all {
            return Ok(rows);
        }
        // Rows are newest-first inside each base name, so the first one seen is
        // the version an assignment should default to.
        let mut seen = std::collections::HashSet::new();
        Ok(rows
            .into_iter()
            .filter(|row| seen.insert(row.base_name.clone()))
            .collect())
    }

    pub async fn get(&self, id: &str) -> Result<Option<ImageVersionRow>, sqlx::Error> {
        let row = sqlx::query_as::<_, ImageVersionRow>(
            r#"
            SELECT id, name, base_name, version, image_asset_id, parent_version_id,
                   os_type, source_description, comment, is_pinned, is_locked,
                   ordering, published_by, published_at, last_used_at,
                   last_used_by_client_id, created_at, updated_at
            FROM image_versions
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }
    pub async fn publish(
        &self,
        version: NewImageVersion<'_>,
    ) -> Result<Option<ImageVersionRow>, ImageVersionWriteError> {
        let (base_name, label) = version
            .name
            .split_once('/')
            .unwrap_or((version.image_asset_id, version.name));

        sqlx::query(
            r#"
            -- image_id is the pre-0033 NOT NULL column; it stays in sync with
            -- image_asset_id because migration 0033 cannot be rewritten after
            -- being applied, and SQLite CHECK/NOT NULL apply to it.
            INSERT INTO image_versions (
                id, name, base_name, version, image_id, image_asset_id, parent_version_id,
                os_type, source_description, comment, is_pinned, is_locked,
                ordering, published_by, published_at, last_used_at,
                last_used_by_client_id, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 0, 0, ?, ?, NULL, NULL, ?, ?)
            "#,
        )
        .bind(version.id)
        .bind(version.name)
        .bind(base_name)
        .bind(label)
        .bind(version.image_asset_id)
        .bind(version.image_asset_id)
        .bind(version.parent_version_id)
        .bind(version.os_type)
        .bind(version.source_description)
        .bind(version.comment)
        .bind(version.published_by)
        .bind(version.created_at)
        .bind(version.created_at)
        .bind(version.created_at)
        .execute(&self.pool)
        .await
        .map_err(|error| match &error {
            sqlx::Error::Database(database) if database.is_unique_violation() => {
                ImageVersionWriteError::Duplicate(version.name.to_owned())
            }
            _ => ImageVersionWriteError::Database(error),
        })?;

        self.get(version.id)
            .await
            .map_err(ImageVersionWriteError::from)
    }

    pub async fn update(
        &self,
        id: &str,
        comment: Option<&str>,
        is_pinned: bool,
        is_locked: bool,
        ordering: Option<i64>,
        updated_at: &str,
    ) -> Result<Option<ImageVersionRow>, sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE image_versions SET
                comment = COALESCE(?, comment),
                is_pinned = ?,
                is_locked = ?,
                ordering = COALESCE(?, ordering),
                updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(comment)
        .bind(is_pinned)
        .bind(is_locked)
        .bind(ordering)
        .bind(updated_at)
        .bind(id)
        .execute(&self.pool)
        .await?;

        self.get(id).await
    }

    pub async fn set_last_used(
        &self,
        id: &str,
        client_id: Option<&str>,
        updated_at: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE image_versions SET
                last_used_at = ?,
                last_used_by_client_id = ?
            WHERE id = ?
            "#,
        )
        .bind(updated_at)
        .bind(client_id)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete(&self, id: &str) -> Result<(), sqlx::Error> {
        let existing = self.get(id).await?;
        let exists = existing.is_some();
        if !exists {
            return Ok(());
        }
        sqlx::query("DELETE FROM image_versions WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub fn row_to_summary(row: ImageVersionRow) -> crate::domain::image::ImageVersionSummary {
        crate::domain::image::ImageVersionSummary::from(row)
    }
}

#[cfg(test)]
mod tests {
    #![allow(dead_code, non_snake_case)]
    use super::*;

    #[test]
    fn image_version_row_fields_match_summary_shape() {
        let row = ImageVersionRow {
            id: "id".into(),
            name: "name".into(),
            base_name: "base".into(),
            version: "1".into(),
            image_asset_id: "asset".into(),
            parent_version_id: None,
            os_type: "windows".into(),
            source_description: None,
            comment: None,
            is_pinned: false,
            is_locked: false,
            ordering: 0,
            published_by: None,
            published_at: "now".into(),
            last_used_at: None,
            last_used_by_client_id: None,
            created_at: "now".into(),
            updated_at: "now".into(),
        };

        let summary = ImageVersionRepository::row_to_summary(row);

        assert_eq!(summary.id, "id");
        assert_eq!(summary.name, "name");
        assert_eq!(summary.version, "1");
    }

    #[test]
    fn row_to_summary_maps_every_field() {
        let row = ImageVersionRow {
            id: "id".into(),
            name: "published-name".into(),
            base_name: "base".into(),
            version: "2".into(),
            image_asset_id: "asset".into(),
            parent_version_id: Some("parent".into()),
            os_type: "linux".into(),
            source_description: Some("desc".into()),
            comment: Some("comment".into()),
            is_pinned: true,
            is_locked: true,
            ordering: 7,
            published_by: Some("by".into()),
            published_at: "pub".into(),
            last_used_at: Some("used".into()),
            last_used_by_client_id: Some("client".into()),
            created_at: "created".into(),
            updated_at: "updated".into(),
        };

        let summary = ImageVersionRepository::row_to_summary(row);
        assert_eq!(summary.id, "id");
        assert_eq!(summary.name, "published-name");
        assert_eq!(summary.base_name, "base");
        assert_eq!(summary.version, "2");
        assert_eq!(summary.image_asset_id, "asset");
        assert_eq!(summary.parent_version_id.as_deref(), Some("parent"));
        assert_eq!(summary.os_type, "linux");
        assert_eq!(summary.source_description.as_deref(), Some("desc"));
        assert_eq!(summary.comment.as_deref(), Some("comment"));
        assert!(summary.is_pinned);
        assert!(summary.is_locked);
        assert_eq!(summary.ordering, 7);
        assert_eq!(summary.published_by.as_deref(), Some("by"));
        assert_eq!(summary.published_at, "pub");
        assert_eq!(summary.last_used_at.as_deref(), Some("used"));
        assert_eq!(summary.last_used_by_client_id.as_deref(), Some("client"));
        assert_eq!(summary.created_at, "created");
        assert_eq!(summary.updated_at, "updated");
    }

    async fn pool() -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("database");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("schema");
        pool
    }

    fn version<'a>(id: &'a str, name: &'a str) -> NewImageVersion<'a> {
        NewImageVersion {
            id,
            name,
            image_asset_id: "asset",
            parent_version_id: None,
            source_description: None,
            comment: None,
            os_type: "windows",
            published_by: Some("admin"),
            created_at: "2026-11-03T00:00:00Z",
        }
    }

    /// The unique index must surface as a duplicate, not as a storage failure,
    /// and the refused publish must leave the first row alone.
    #[tokio::test]
    async fn a_repeated_base_name_and_version_reports_a_duplicate() {
        let pool = pool().await;
        let repository = ImageVersionRepository::new(pool.clone());

        let first = repository
            .publish(version("one", "windows11/2026-11-03"))
            .await
            .expect("first publish")
            .expect("created row");
        assert_eq!(first.base_name, "windows11");
        assert_eq!(first.version, "2026-11-03");

        let error = repository
            .publish(version("two", "windows11/2026-11-03"))
            .await
            .expect_err("duplicate refused");
        assert!(
            matches!(error, ImageVersionWriteError::Duplicate(ref name) if name == "windows11/2026-11-03"),
            "expected a duplicate error, got a different failure"
        );

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM image_versions")
            .fetch_one(&pool)
            .await
            .expect("count");
        assert_eq!(count, 1, "a refused publish must leave one row");
    }

    #[tokio::test]
    async fn different_labels_share_one_base_name() {
        let pool = pool().await;
        let repository = ImageVersionRepository::new(pool.clone());
        repository
            .publish(version("one", "windows11/a"))
            .await
            .expect("first publish");
        repository
            .publish(version("two", "windows11/b"))
            .await
            .expect("second publish");

        let rows = repository
            .list(Some("windows11"), true)
            .await
            .expect("list");
        assert_eq!(rows.len(), 2);
        assert_eq!(
            repository
                .list(Some("windows11"), false)
                .await
                .expect("latest")
                .len(),
            1,
            "the default listing shows one row per base name"
        );
    }
}
