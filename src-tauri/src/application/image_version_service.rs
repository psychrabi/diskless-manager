//! Image version publishing (ADR 0009).
//!
//! A version is a named, assignable record that points at one verified image
//! asset. Publishing marks a completed servicing clone ready for assignment
//! and records provenance; it never renames a master, replaces an existing
//! snapshot, or migrates a client as a side effect.

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Deserialize;

use crate::core::image::{Image, OsType};
use crate::domain::image::ImageVersionSummary;
use crate::persistence::repositories::image::ImageRepository;
use crate::persistence::repositories::image_version::{
    ImageVersionRepository, ImageVersionWriteError, NewImageVersion,
};

/// Request to publish a verified servicing clone as an image version.
#[derive(Debug, Clone, Deserialize)]
pub struct PublishVersionRequest {
    /// Image asset id (or name) that the version will point at.
    pub image_id: String,
    /// Version label inside the base name, for example `2026-11-01`.
    pub version_name: String,
    /// Group the version under this base name; defaults to the asset name.
    #[serde(default)]
    pub base_name: Option<String>,
    #[serde(default)]
    pub parent_version_id: Option<String>,
    #[serde(default)]
    pub source_description: Option<String>,
    #[serde(default)]
    pub comment: Option<String>,
}

/// Why a version request changed nothing.
#[derive(Debug, thiserror::Error)]
pub enum ImageVersionError {
    #[error("{0}")]
    NotFound(String),
    /// The request is well-formed but the server refuses to publish it.
    #[error("{0}")]
    Rejected(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

#[derive(Clone)]
pub struct ImageVersionService {
    versions: ImageVersionRepository,
    images: ImageRepository,
}

impl ImageVersionService {
    pub fn new(versions: ImageVersionRepository, images: ImageRepository) -> Self {
        Self { versions, images }
    }

    pub async fn list_latest(&self, base_name: Option<&str>) -> Result<Vec<ImageVersionSummary>> {
        self.list(base_name, false).await
    }

    pub async fn list(
        &self,
        base_name: Option<&str>,
        all: bool,
    ) -> Result<Vec<ImageVersionSummary>> {
        Ok(self
            .versions
            .list(base_name, all)
            .await
            .context("failed to list image versions")?
            .into_iter()
            .map(ImageVersionRepository::row_to_summary)
            .collect())
    }

    pub async fn get(&self, id: &str) -> Result<Option<ImageVersionSummary>> {
        Ok(self
            .versions
            .get(id)
            .await
            .context("failed to load image version")?
            .map(ImageVersionRepository::row_to_summary))
    }

    pub async fn update(
        &self,
        id: &str,
        comment: Option<&str>,
        is_pinned: bool,
        is_locked: bool,
        ordering: Option<i64>,
    ) -> Result<ImageVersionSummary> {
        let existing = self
            .versions
            .get(id)
            .await
            .context("failed to load image version")?
            .ok_or_else(|| anyhow::anyhow!("image version not found"))?;

        if existing.is_locked && (comment.is_some() || is_pinned || is_locked || ordering.is_some())
        {
            anyhow::bail!("locked image version cannot be modified");
        }

        let now = Utc::now().to_rfc3339();
        let updated = self
            .versions
            .update(id, comment, is_pinned, is_locked, ordering, &now)
            .await
            .context("failed to update image version")?
            .ok_or_else(|| anyhow::anyhow!("failed to reload updated image version"))?;

        Ok(ImageVersionRepository::row_to_summary(updated))
    }

    pub async fn set_last_used(&self, id: &str, client_id: Option<&str>) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.versions
            .set_last_used(id, client_id, &now)
            .await
            .context("failed to update image version usage")?;
        Ok(())
    }

    pub async fn delete(&self, id: &str) -> Result<(), ImageVersionError> {
        let existing = self
            .versions
            .get(id)
            .await
            .context("failed to load image version")
            .map_err(ImageVersionError::Internal)?
            .ok_or_else(|| {
                ImageVersionError::NotFound(format!("image version '{id}' was not found"))
            })?;

        if existing.is_locked {
            return Err(ImageVersionError::Rejected(
                "a locked image version cannot be deleted".to_string(),
            ));
        }

        self.versions
            .delete(id)
            .await
            .context("failed to delete image version")
            .map_err(ImageVersionError::Internal)?;
        Ok(())
    }

    pub async fn publish_from_asset(
        &self,
        asset_id: &str,
        name: &str,
        source_description: Option<&str>,
        comment: Option<&str>,
        parent_version_id: Option<&str>,
        published_by: Option<&str>,
    ) -> Result<ImageVersionSummary> {
        let asset = self.load_publishable_asset(asset_id).await?;
        let created = self
            .versions
            .publish(NewImageVersion {
                id: &uuid::Uuid::new_v4().to_string(),
                name,
                image_asset_id: &asset.id,
                parent_version_id,
                source_description,
                comment,
                os_type: asset.os_type.as_str(),
                published_by,
                created_at: &Utc::now().to_rfc3339(),
            })
            .await
            .context("failed to persist image version")?
            .ok_or_else(|| anyhow::anyhow!("failed to reload created image version"))?;

        Ok(ImageVersionRepository::row_to_summary(created))
    }

    /// Publish a completed Windows servicing clone as a new version.
    ///
    /// The clone is marked ready through the existing servicing path, so an
    /// asset that is not a committed clone is refused instead of relabelled.
    /// `published_by` comes from the authenticated caller, never the payload.
    pub async fn publish_version(
        &self,
        settings: &crate::core::config::Settings,
        request: &PublishVersionRequest,
        published_by: Option<&str>,
    ) -> Result<ImageVersionSummary, ImageVersionError> {
        let version = request.version_name.trim();
        if version.is_empty() {
            return Err(ImageVersionError::Rejected(
                "a version name is required".to_string(),
            ));
        }
        if !settings.windows_servicing.is_configured() {
            return Err(ImageVersionError::Rejected(
                "offline Windows image preparation is disabled on this server".to_string(),
            ));
        }

        let asset = self
            .images
            .get(&request.image_id)
            .await
            .context("failed to load image asset")?
            .ok_or_else(|| {
                ImageVersionError::NotFound(format!(
                    "image asset '{}' was not found",
                    request.image_id
                ))
            })?;
        if asset.os_type != OsType::Windows {
            return Err(ImageVersionError::Rejected(
                "only a Windows servicing clone can be published as a version".to_string(),
            ));
        }
        if let Some(parent_id) = request.parent_version_id.as_deref() {
            if self
                .versions
                .get(parent_id)
                .await
                .context("failed to load parent image version")?
                .is_none()
            {
                return Err(ImageVersionError::NotFound(format!(
                    "parent image version '{parent_id}' was not found"
                )));
            }
        }

        let published_asset =
            crate::application::image_service::ImageService::new(self.images.clone())
                .publish_serviced_clone(&asset.id)
                .await
                .map_err(|error| ImageVersionError::Rejected(error.to_string()))?;

        let base_name = request
            .base_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(published_asset.name.as_str())
            .to_string();
        let created = self
            .versions
            .publish(NewImageVersion {
                id: &uuid::Uuid::new_v4().to_string(),
                name: &format!("{base_name}/{version}"),
                image_asset_id: &published_asset.id,
                parent_version_id: request.parent_version_id.as_deref(),
                source_description: request.source_description.as_deref(),
                comment: request.comment.as_deref(),
                os_type: published_asset.os_type.as_str(),
                published_by,
                created_at: &Utc::now().to_rfc3339(),
            })
            .await
            .map_err(|error| match error {
                ImageVersionWriteError::Duplicate(name) => {
                    ImageVersionError::Rejected(format!("image version '{name}' already exists"))
                }
                ImageVersionWriteError::Database(source) => ImageVersionError::Internal(
                    anyhow::Error::new(source).context("failed to persist image version"),
                ),
            })?
            .ok_or_else(|| {
                ImageVersionError::Internal(anyhow::anyhow!(
                    "failed to reload created image version"
                ))
            })?;

        Ok(ImageVersionRepository::row_to_summary(created))
    }

    async fn load_publishable_asset(&self, asset_id: &str) -> Result<Image> {
        let asset: Image = self
            .images
            .get(asset_id)
            .await
            .context("failed to load image asset")?
            .ok_or_else(|| anyhow::anyhow!("image asset not found"))?;
        if !matches!(asset.status.as_str(), "ready" | "draft" | "verifying") {
            anyhow::bail!("asset is not in a publishable state");
        }
        Ok(asset)
    }
}

impl std::fmt::Debug for ImageVersionService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageVersionService")
            .field("versions", &"ImageVersionRepository { .. }")
            .field("images", &"ImageRepository { .. }")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::repositories::image::ImageRepository;

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

    fn service(pool: &sqlx::SqlitePool) -> ImageVersionService {
        ImageVersionService::new(
            ImageVersionRepository::new(pool.clone()),
            ImageRepository::new(pool.clone()),
        )
    }

    async fn insert(pool: &sqlx::SqlitePool, id: &str, locked: bool) {
        sqlx::query(
            "INSERT INTO image_versions (id, name, base_name, version, image_id, image_asset_id, os_type, is_locked, published_at, created_at, updated_at) \
             VALUES (?, ?, 'windows11', ?, 'asset', 'asset', 'windows', ?, '2026-11-03T00:00:00Z', '2026-11-03T00:00:00Z', '2026-11-03T00:00:00Z')",
        )
        .bind(id)
        .bind(format!("windows11/{id}"))
        .bind(id)
        .bind(i64::from(locked))
        .execute(pool)
        .await
        .expect("inserted version");
    }

    #[tokio::test]
    async fn deleting_an_unknown_version_is_not_found() {
        let pool = pool().await;
        let error = service(&pool)
            .delete("missing")
            .await
            .expect_err("missing version");
        assert!(matches!(error, ImageVersionError::NotFound(_)));
    }

    #[tokio::test]
    async fn a_locked_version_refuses_deletion() {
        let pool = pool().await;
        insert(&pool, "locked", true).await;

        let error = service(&pool)
            .delete("locked")
            .await
            .expect_err("locked version");
        assert!(matches!(error, ImageVersionError::Rejected(_)));

        let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM image_versions")
            .fetch_one(&pool)
            .await
            .expect("count");
        assert_eq!(remaining, 1, "a refused delete must keep the row");
    }

    #[tokio::test]
    async fn an_unlocked_version_is_deleted_without_touching_the_asset() {
        let pool = pool().await;
        insert(&pool, "open", false).await;

        service(&pool).delete("open").await.expect("delete");

        let versions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM image_versions")
            .fetch_one(&pool)
            .await
            .expect("count");
        let assets: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM images")
            .fetch_one(&pool)
            .await
            .expect("assets");
        assert_eq!(versions, 0);
        assert_eq!(assets, 0, "deleting a version never deletes its asset");
    }
}
