//! Image-snapshot retention (Phase 4A).
//!
//! Retention is preview-first. Planning is read-only, `dry_run` is the default,
//! and a destructive run re-checks every candidate immediately before deleting
//! it through the same guarded [`ImageService::delete`] path the API uses.
//!
//! It never proposes a snapshot that a client is assigned to, that a clone or a
//! prewarmed spare was created from, or that an unfinished session owns. Every
//! planned, rejected and attempted deletion is written to
//! `storage_retention_events`.

use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Utc;
use tokio::sync::RwLock;

use crate::{
    application::image_service::ImageService,
    core::config::{RetentionConfig, Settings},
    core::image::{Image, ImageKind},
    persistence::repositories::image::ImageRepository,
};

/// One snapshot the current configuration would remove.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RetentionCandidate {
    pub snapshot_id: String,
    pub image_name: String,
    pub snapshot: String,
    pub full_name: String,
    pub age_hours: i64,
}

/// A snapshot retention considered and refused, with the reason shown to the
/// administrator.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RetentionSkip {
    pub image_name: String,
    pub snapshot: String,
    pub full_name: String,
    pub reason: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RetentionPlan {
    pub enabled: bool,
    pub dry_run: bool,
    pub min_keep_snapshots: u32,
    pub min_age_hours: u32,
    pub candidates: Vec<RetentionCandidate>,
    pub skipped: Vec<RetentionSkip>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RetentionRun {
    /// False when the run only recorded the plan (`dry_run`).
    pub executed: bool,
    pub plan: RetentionPlan,
    pub deleted: usize,
    pub failures: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum RetentionError {
    #[error("{0}")]
    Rejected(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

pub struct RetentionService {
    pool: sqlx::SqlitePool,
    settings: Arc<RwLock<Settings>>,
    images: ImageService,
}

impl RetentionService {
    pub fn new(
        pool: sqlx::SqlitePool,
        settings: Arc<RwLock<Settings>>,
        images: ImageService,
    ) -> Self {
        Self {
            pool,
            settings,
            images,
        }
    }

    /// Read-only: it never destroys or mutates image storage.
    pub async fn plan(&self) -> Result<RetentionPlan> {
        let config = self.settings.read().await.storage.retention.clone();
        let images = ImageRepository::new(self.pool.clone())
            .list()
            .await
            .context("failed to list images")?;
        let now = Utc::now();
        let mut candidates = Vec::new();
        let mut skipped = Vec::new();

        for image in images.iter().filter(|image| {
            matches!(image.kind, ImageKind::Master | ImageKind::Clone) && image.status == "ready"
        }) {
            let mut snapshots = images
                .iter()
                .filter(|candidate| {
                    candidate.kind == ImageKind::Snapshot
                        && candidate.status == "ready"
                        && candidate.parent_id.as_deref() == Some(image.id.as_str())
                })
                .collect::<Vec<_>>();
            snapshots.sort_by_key(|left| std::cmp::Reverse(left.created_at));

            for (index, snapshot) in snapshots.iter().enumerate() {
                if index < config.min_keep_snapshots as usize {
                    continue;
                }
                let full_name = format!("{}@{}", image.name, snapshot.name);
                let age_hours = (now - snapshot.created_at).num_hours();
                if age_hours < i64::from(config.min_age_hours) {
                    skipped.push(skip(
                        image,
                        snapshot,
                        full_name,
                        format!("younger than the {} hour minimum age", config.min_age_hours),
                    ));
                    continue;
                }
                match blocked_reason(&self.pool, &full_name).await? {
                    Some(reason) => skipped.push(skip(image, snapshot, full_name, reason)),
                    None => candidates.push(RetentionCandidate {
                        snapshot_id: snapshot.id.clone(),
                        image_name: image.name.clone(),
                        snapshot: snapshot.name.clone(),
                        full_name,
                        age_hours,
                    }),
                }
            }
        }

        Ok(RetentionPlan {
            enabled: config.enabled,
            dry_run: config.dry_run,
            min_keep_snapshots: config.min_keep_snapshots,
            min_age_hours: config.min_age_hours,
            candidates,
            skipped,
        })
    }

    /// Execute the current plan.
    ///
    /// A disabled configuration and an unconfirmed destructive run are refused
    /// before anything is audited. `dry_run` records the plan and deletes
    /// nothing; a destructive run requires `confirm = true`.
    pub async fn run(&self, actor: &str, confirm: bool) -> Result<RetentionRun, RetentionError> {
        let config: RetentionConfig = self.settings.read().await.storage.retention.clone();
        if !config.enabled {
            return Err(RetentionError::Rejected(
                "image retention is disabled".to_string(),
            ));
        }

        let plan = self.plan().await.map_err(RetentionError::Internal)?;
        if config.dry_run {
            for candidate in &plan.candidates {
                self.record(actor, "planned", candidate, Some("dry run"), None)
                    .await
                    .map_err(RetentionError::Internal)?;
            }
            return Ok(RetentionRun {
                executed: false,
                plan,
                deleted: 0,
                failures: Vec::new(),
            });
        }
        if !confirm {
            return Err(RetentionError::Rejected(
                "a destructive retention run requires confirm=true".to_string(),
            ));
        }

        let mut deleted = 0;
        let mut failures = Vec::new();
        for candidate in &plan.candidates {
            // Re-check under fresh state: a client may have been reassigned, or
            // a new clone may have appeared since the plan was produced.
            match blocked_reason(&self.pool, &candidate.full_name).await {
                Ok(None) => {}
                Ok(Some(reason)) => {
                    self.record(actor, "skipped", candidate, Some(&reason), None)
                        .await
                        .map_err(RetentionError::Internal)?;
                    continue;
                }
                Err(error) => {
                    failures.push(format!("{}: {error:#}", candidate.full_name));
                    continue;
                }
            }

            match self.images.delete(&candidate.snapshot_id).await {
                Ok(()) => {
                    deleted += 1;
                    self.record(
                        actor,
                        "deleted",
                        candidate,
                        Some("eligible for retention"),
                        None,
                    )
                    .await
                    .map_err(RetentionError::Internal)?;
                }
                Err(error) => {
                    let message = format!("{error:#}");
                    self.record(actor, "failed", candidate, None, Some(&message))
                        .await
                        .map_err(RetentionError::Internal)?;
                    failures.push(format!("{}: {message}", candidate.full_name));
                }
            }
        }

        Ok(RetentionRun {
            executed: true,
            plan,
            deleted,
            failures,
        })
    }

    async fn record(
        &self,
        actor: &str,
        action: &str,
        candidate: &RetentionCandidate,
        reason: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO storage_retention_events (id, actor, action, dataset, snapshot, reason, error, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(actor)
        .bind(action)
        .bind(&candidate.image_name)
        .bind(&candidate.snapshot)
        .bind(reason)
        .bind(error)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .context("failed to record retention event")?;
        Ok(())
    }
}

fn skip(image: &Image, snapshot: &Image, full_name: String, reason: String) -> RetentionSkip {
    RetentionSkip {
        image_name: image.name.clone(),
        snapshot: snapshot.name.clone(),
        full_name,
        reason,
    }
}

/// A snapshot is blocked while any live owner still names it.
///
/// `full_name` is the `dataset@snapshot` identity clients and spares persist;
/// image clones store only the bare snapshot name, so both forms are checked.
pub async fn blocked_reason(pool: &sqlx::SqlitePool, full_name: &str) -> Result<Option<String>> {
    let bare = full_name
        .split_once('@')
        .map(|(_, snapshot)| snapshot)
        .unwrap_or(full_name);

    let assigned: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM clients WHERE snapshot = ?")
        .bind(full_name)
        .fetch_one(pool)
        .await
        .context("failed to check client assignments")?;
    if assigned > 0 {
        return Ok(Some(
            "snapshot is a client's assigned boot snapshot".to_string(),
        ));
    }

    let clones: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM images WHERE source_snapshot = ? OR source_snapshot = ?",
    )
    .bind(full_name)
    .bind(bare)
    .fetch_one(pool)
    .await
    .context("failed to check dependent clones")?;
    if clones > 0 {
        return Ok(Some(
            "a managed clone was created from this snapshot".to_string(),
        ));
    }

    let spares: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM client_writeback_spares WHERE source_snapshot = ? OR source_snapshot = ?",
    )
    .bind(full_name)
    .bind(bare)
    .fetch_one(pool)
    .await
    .context("failed to check prewarmed spares")?;
    if spares > 0 {
        return Ok(Some("a prewarmed writeback uses this snapshot".to_string()));
    }

    let sessions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM installation_sessions WHERE closed_at IS NULL AND verification_snapshot = ?",
    )
    .bind(full_name)
    .fetch_one(pool)
    .await
    .context("failed to check image-factory sessions")?;
    if sessions > 0 {
        return Ok(Some(
            "an unfinished image-factory session owns this snapshot".to_string(),
        ));
    }

    let supers: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM super_sessions WHERE prior_snapshot = ? OR prior_snapshot = ?",
    )
    .bind(full_name)
    .bind(bare)
    .fetch_one(pool)
    .await
    .context("failed to check Super session recovery")?;
    if supers > 0 {
        return Ok(Some(
            "a Super session recovery record references this snapshot".to_string(),
        ));
    }

    Ok(None)
}

/// Pure age/keep policy: newest first, `min_keep_snapshots` always retained.
#[cfg(test)]
fn eligible_indexes(
    config: &RetentionConfig,
    snapshots: &[Image],
    now: chrono::DateTime<Utc>,
) -> Vec<usize> {
    let keep = config.min_keep_snapshots as usize;
    snapshots
        .iter()
        .enumerate()
        .filter(|(index, snapshot)| {
            *index >= keep
                && (now - snapshot.created_at).num_hours() >= i64::from(config.min_age_hours)
        })
        .map(|(index, _)| index)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn snapshot(name: &str, age_hours: i64) -> Image {
        let mut image = crate::core::image::Image {
            id: format!("snapshot-{name}"),
            name: name.to_string(),
            kind: ImageKind::Snapshot,
            os_type: crate::core::image::OsType::Windows,
            size_gb: 20,
            path: "/dev/zvol/pool/image-disk/windows11".into(),
            format: crate::core::image::ImageFormat::Raw,
            status: "ready".into(),
            description: None,
            parent_id: Some("master".into()),
            source_snapshot: None,
            checksum: None,
            is_default: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        image.created_at -= chrono::Duration::hours(age_hours);
        image
    }

    #[test]
    fn the_newest_snapshots_are_always_kept() {
        let config = RetentionConfig {
            enabled: true,
            dry_run: true,
            min_keep_snapshots: 2,
            min_age_hours: 1,
        };
        let snapshots = vec![
            snapshot("newest", 1),
            snapshot("middle", 50),
            snapshot("oldest", 500),
        ];
        assert_eq!(
            eligible_indexes(&config, &snapshots, Utc::now()),
            vec![2],
            "only the snapshot past the keep count and the age floor is eligible"
        );
    }

    #[test]
    fn a_young_snapshot_is_not_eligible_after_the_keep_count() {
        let config = RetentionConfig {
            enabled: true,
            dry_run: true,
            min_keep_snapshots: 1,
            min_age_hours: 24,
        };
        let snapshots = vec![snapshot("newest", 1), snapshot("recent", 2)];
        assert!(eligible_indexes(&config, &snapshots, Utc::now()).is_empty());
    }

    #[tokio::test]
    async fn a_client_assignment_blocks_the_snapshot() {
        let pool = pool().await;
        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, snapshot, writeback, block_device, block_store, target_iqn, keep_writeback, use_game_disk, enabled, created_at, updated_at) VALUES ('client', 'PC001', '02:00:00:00:00:01', '192.0.2.10', 'pool/image-disk/windows11', 'pool/image-disk/windows11@installed', '', '', '', 'iqn.test:pc001', 0, 0, 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
            .execute(&pool)
            .await
            .expect("client");

        let reason = blocked_reason(&pool, "pool/image-disk/windows11@installed")
            .await
            .expect("check");
        assert!(
            reason
                .as_deref()
                .is_some_and(|value| value.contains("assigned")),
            "an assigned snapshot must never be proposed"
        );
    }

    #[tokio::test]
    async fn an_unreferenced_snapshot_is_eligible() {
        let pool = pool().await;
        let reason = blocked_reason(&pool, "pool/image-disk/windows11@obsolete")
            .await
            .expect("check");
        assert!(reason.is_none());
    }
}
