//! Dormant writeback cleanup (Phase 4A capacity upkeep).
//!
//! A client's writable ZFS dataset outlives the client row when a deletion is
//! interrupted or when storage was created outside the managed flow. Left
//! alone those datasets keep consuming pool space forever.
//!
//! Cleanup is preview-first and fail-closed. A dataset is only ever proposed
//! when this server can prove nobody owns it any more:
//!
//! - it is a direct child of a dataset tagged `org.diskless:type=writeback`;
//! - its name is exactly `<CLIENT>-disk`, never a spare, game or working clone;
//! - no client row carries that name (case-insensitively);
//! - no spare row references the dataset;
//! - the iSCSI target the name maps to does not exist;
//! - the dataset is not mounted, and the mount check actually succeeded;
//! - it is older than `storage.dormant_cleanup.min_age_hours`.
//!
//! Every planned, skipped, deleted and failed dataset is recorded in
//! `storage_dormant_cleanup_events`, and a destructive run needs the feature
//! enabled, `dry_run` off and an explicit confirmation.

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Utc;
use tokio::sync::RwLock;

use crate::{
    core::config::{DormantCleanupConfig, Settings},
    infrastructure::{image::ImageBackend, iscsi::IscsiProvisioner, zfs::ZfsCommand},
};

/// One dataset as reported by `zfs list`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetEntry {
    pub name: String,
    /// `org.diskless:type`; `-` or empty when unset.
    pub kind: String,
    /// Creation time as epoch seconds; `None` when ZFS did not report one.
    pub created_at: Option<i64>,
}

/// Source of the ZFS dataset listing.
///
/// A trait so the plan/run logic can be exercised without ZFS.
pub trait DatasetInventory: Send + Sync {
    fn list(&self, pool: &str) -> Result<Vec<DatasetEntry>>;
}

/// Real inventory: one recursive `zfs list` of the pool.
pub struct ZfsDatasetInventory;

impl DatasetInventory for ZfsDatasetInventory {
    fn list(&self, pool: &str) -> Result<Vec<DatasetEntry>> {
        let output = ZfsCommand::new()
            .execute_output([
                "zfs",
                "list",
                "-H",
                "-p",
                "-r",
                "-t",
                "filesystem,volume",
                "-o",
                "name,org.diskless:type,creation",
                pool,
            ])
            .context("failed to list ZFS datasets")?;
        Ok(parse_dataset_listing(&output))
    }
}

/// Parse `zfs list -H -p -o name,org.diskless:type,creation` output.
///
/// Malformed lines are dropped rather than defaulted: an entry with an unknown
/// kind or creation time must never become a deletion candidate.
pub fn parse_dataset_listing(output: &str) -> Vec<DatasetEntry> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let name = fields.next()?.trim();
            let kind = fields.next().unwrap_or_default().trim();
            let created_at = fields.next()?.trim().parse::<i64>().ok();
            if name.is_empty() {
                return None;
            }
            Some(DatasetEntry {
                name: name.to_string(),
                kind: kind.to_string(),
                created_at,
            })
        })
        .collect()
}

/// A dataset the current configuration would reclaim.
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct DormantCandidate {
    pub dataset: String,
    pub client_name: String,
    pub age_hours: i64,
}

/// A dataset cleanup considered and refused, with the reason shown to the
/// administrator.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DormantSkip {
    pub dataset: String,
    pub reason: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DormantPlan {
    pub enabled: bool,
    pub dry_run: bool,
    pub min_age_hours: u32,
    pub candidates: Vec<DormantCandidate>,
    pub skipped: Vec<DormantSkip>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DormantRun {
    /// False when the run only recorded the plan (`dry_run`).
    pub executed: bool,
    pub plan: DormantPlan,
    pub deleted: usize,
    pub failures: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum DormantCleanupError {
    #[error("{0}")]
    Rejected(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

/// Classify a ZFS listing into reclaimable and refused datasets.
///
/// Pure: `clients` are the registered client names, `spares` the datasets the
/// spare table still owns, and `now` the epoch seconds the age is measured
/// against. Anything unexpected is a skip, never a candidate.
pub fn classify(
    entries: &[DatasetEntry],
    clients: &[String],
    spares: &HashSet<String>,
    min_age_hours: u32,
    now: i64,
) -> (Vec<DormantCandidate>, Vec<DormantSkip>) {
    let owners: HashSet<String> = clients
        .iter()
        .map(|name| name.trim().to_uppercase())
        .collect();
    let writeback_parents: Vec<&str> = entries
        .iter()
        .filter(|entry| entry.kind.trim() == "writeback")
        .map(|entry| entry.name.as_str())
        .collect();

    let mut candidates = Vec::new();
    let mut skipped = Vec::new();

    for entry in entries {
        if entry.kind.trim() == "writeback" {
            continue;
        }
        let Some(parent) = writeback_parents
            .iter()
            .find(|parent| entry.name.starts_with(&format!("{parent}/")))
        else {
            continue;
        };
        let relative = entry.name[parent.len() + 1..].to_string();
        if relative.contains('/') {
            skipped.push(skip(&entry.name, "not a direct writeback child"));
            continue;
        }
        let Some(client_name) = relative.strip_suffix("-disk") else {
            // Spares, game clones and unrelated volumes are never candidates.
            continue;
        };
        if client_name.is_empty() || relative.contains("-disk-spare-") {
            continue;
        }
        if owners.contains(&client_name.to_uppercase()) {
            skipped.push(skip(&entry.name, "a client with this name still exists"));
            continue;
        }
        if spares.contains(&entry.name) {
            skipped.push(skip(&entry.name, "a prewarmed spare still owns it"));
            continue;
        }
        let Some(created_at) = entry.created_at else {
            skipped.push(skip(&entry.name, "ZFS did not report a creation time"));
            continue;
        };
        let age_hours = (now - created_at) / 3600;
        if age_hours < i64::from(min_age_hours) {
            skipped.push(skip(
                &entry.name,
                &format!("younger than the {} hour minimum age", min_age_hours),
            ));
            continue;
        }
        candidates.push(DormantCandidate {
            dataset: entry.name.clone(),
            client_name: client_name.to_uppercase(),
            age_hours,
        });
    }

    (candidates, skipped)
}

fn skip(dataset: &str, reason: &str) -> DormantSkip {
    DormantSkip {
        dataset: dataset.to_string(),
        reason: reason.to_string(),
    }
}

pub struct DormantCleanupService {
    pool: sqlx::SqlitePool,
    settings: Arc<RwLock<Settings>>,
    backend: Arc<dyn ImageBackend>,
    iscsi: Arc<dyn IscsiProvisioner>,
    pool_name: String,
    inventory: Arc<dyn DatasetInventory>,
}

impl DormantCleanupService {
    pub fn new(
        pool: sqlx::SqlitePool,
        settings: Arc<RwLock<Settings>>,
        backend: Arc<dyn ImageBackend>,
        iscsi: Arc<dyn IscsiProvisioner>,
        pool_name: String,
    ) -> Self {
        Self {
            pool,
            settings,
            backend,
            iscsi,
            pool_name,
            inventory: Arc::new(ZfsDatasetInventory),
        }
    }

    #[cfg(test)]
    fn with_inventory(mut self, inventory: Arc<dyn DatasetInventory>) -> Self {
        self.inventory = inventory;
        self
    }

    /// Read-only: it lists ZFS, queries ownership and never destroys anything.
    pub async fn plan(&self) -> Result<DormantPlan> {
        let config = self.settings.read().await.storage.dormant_cleanup.clone();
        let target_prefix = self.settings.read().await.iscsi.target_prefix.clone();
        let entries = self.inventory_entries().await?;
        let clients = self.client_names().await?;
        let spares = self.spare_datasets().await?;
        let now = Utc::now().timestamp();
        let (candidates, mut skipped) =
            classify(&entries, &clients, &spares, config.min_age_hours, now);

        let mut confirmed = Vec::new();
        for candidate in candidates {
            match self
                .live_skip_reason(&candidate.dataset, &candidate.client_name, &target_prefix)
                .await
            {
                Ok(Some(reason)) => skipped.push(skip(&candidate.dataset, &reason)),
                Ok(None) => confirmed.push(candidate),
                Err(error) => skipped.push(skip(
                    &candidate.dataset,
                    &format!("ownership could not be verified: {error:#}"),
                )),
            }
        }

        Ok(DormantPlan {
            enabled: config.enabled,
            dry_run: config.dry_run,
            min_age_hours: config.min_age_hours,
            candidates: confirmed,
            skipped,
        })
    }

    /// Execute the current plan.
    ///
    /// Refused when disabled, and a destructive run additionally needs
    /// `confirm = true`. Every candidate is re-checked immediately before it is
    /// destroyed so a client created since planning keeps its storage.
    pub async fn run(&self, actor: &str, confirm: bool) -> Result<DormantRun, DormantCleanupError> {
        let config: DormantCleanupConfig =
            self.settings.read().await.storage.dormant_cleanup.clone();
        if !config.enabled {
            return Err(DormantCleanupError::Rejected(
                "dormant writeback cleanup is disabled".to_string(),
            ));
        }

        let plan = self.plan().await.map_err(DormantCleanupError::Internal)?;
        if config.dry_run {
            for candidate in &plan.candidates {
                self.record(actor, "planned", candidate, Some("dry run"), None)
                    .await
                    .map_err(DormantCleanupError::Internal)?;
            }
            return Ok(DormantRun {
                executed: false,
                plan,
                deleted: 0,
                failures: Vec::new(),
            });
        }
        if !confirm {
            return Err(DormantCleanupError::Rejected(
                "a destructive cleanup run requires confirm=true".to_string(),
            ));
        }

        let target_prefix = self.settings.read().await.iscsi.target_prefix.clone();
        let mut deleted = 0;
        let mut failures = Vec::new();

        for candidate in &plan.candidates {
            match self
                .live_skip_reason(&candidate.dataset, &candidate.client_name, &target_prefix)
                .await
            {
                Ok(None) => {}
                Ok(Some(reason)) => {
                    if let Err(error) = self
                        .record(actor, "skipped", candidate, Some(&reason), None)
                        .await
                    {
                        failures.push(format!("{}: {error:#}", candidate.dataset));
                    }
                    continue;
                }
                Err(error) => {
                    failures.push(format!("{}: {error:#}", candidate.dataset));
                    continue;
                }
            }

            match self.backend.destroy(&candidate.dataset) {
                Ok(()) => {
                    deleted += 1;
                    if let Err(error) = self
                        .record(
                            actor,
                            "deleted",
                            candidate,
                            Some("no client owns this dataset"),
                            None,
                        )
                        .await
                    {
                        failures.push(format!("{}: {error:#}", candidate.dataset));
                    }
                }
                Err(error) => {
                    let message = format!("{error:#}");
                    let _ = self
                        .record(actor, "failed", candidate, None, Some(&message))
                        .await;
                    failures.push(format!("{}: {message}", candidate.dataset));
                }
            }
        }

        Ok(DormantRun {
            executed: true,
            plan,
            deleted,
            failures,
        })
    }

    /// Re-verify ownership and export state from live sources.
    async fn live_skip_reason(
        &self,
        dataset: &str,
        client_name: &str,
        target_prefix: &str,
    ) -> Result<Option<String>> {
        if self.client_exists(client_name).await? {
            return Ok(Some("a client with this name still exists".to_string()));
        }
        if self.spare_owns(dataset).await? {
            return Ok(Some("a prewarmed spare still owns it".to_string()));
        }
        let target_iqn = format!(
            "{}:client.{}",
            target_prefix,
            client_name.trim().to_lowercase()
        );
        if self.iscsi.target_exists(&target_iqn)? {
            return Ok(Some(
                "an iSCSI target for this client name still exists".to_string(),
            ));
        }
        // `validate_unmounted` fails closed: an unreadable mount table is an
        // error, not an empty mount list, and both are treated as a skip.
        if let Err(error) = self.backend.validate_unmounted(dataset) {
            return Ok(Some(format!(
                "mount state is not safe to destroy (mounted or unverifiable): {error:#}"
            )));
        }
        Ok(None)
    }

    async fn inventory_entries(&self) -> Result<Vec<DatasetEntry>> {
        let inventory = self.inventory.clone();
        let pool = self.pool_name.clone();
        tokio::task::spawn_blocking(move || inventory.list(&pool))
            .await
            .context("dataset listing task failed")?
    }

    async fn client_names(&self) -> Result<Vec<String>> {
        sqlx::query_scalar::<_, String>("SELECT name FROM clients")
            .fetch_all(&self.pool)
            .await
            .context("failed to load client names")
    }

    async fn client_exists(&self, client_name: &str) -> Result<bool> {
        let exists: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM clients WHERE UPPER(name) = UPPER(?)")
                .bind(client_name)
                .fetch_one(&self.pool)
                .await
                .context("failed to re-check client ownership")?;
        Ok(exists != 0)
    }

    async fn spare_datasets(&self) -> Result<HashSet<String>> {
        let rows = sqlx::query_scalar::<_, String>("SELECT dataset FROM client_writeback_spares")
            .fetch_all(&self.pool)
            .await
            .context("failed to load spare datasets")?;
        Ok(rows.into_iter().collect())
    }

    async fn spare_owns(&self, dataset: &str) -> Result<bool> {
        let owned: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM client_writeback_spares WHERE dataset = ?")
                .bind(dataset)
                .fetch_one(&self.pool)
                .await
                .context("failed to re-check spare ownership")?;
        Ok(owned != 0)
    }

    async fn record(
        &self,
        actor: &str,
        action: &str,
        candidate: &DormantCandidate,
        reason: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO storage_dormant_cleanup_events \
             (id, actor, action, dataset, client_name, reason, error, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(actor)
        .bind(action)
        .bind(&candidate.dataset)
        .bind(&candidate.client_name)
        .bind(reason)
        .bind(error)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .context("failed to record dormant cleanup event")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::{
        image::{ImageBackend, ImageBackendInfo},
        iscsi::{ChapCredentials, IscsiLunState, IscsiTargetSpec, IscsiTargetState},
    };
    use std::sync::Mutex;

    fn entry(name: &str, kind: &str, created_at: Option<i64>) -> DatasetEntry {
        DatasetEntry {
            name: name.to_string(),
            kind: kind.to_string(),
            created_at,
        }
    }

    #[test]
    fn listing_parsing_drops_lines_without_a_creation_time() {
        let parsed = parse_dataset_listing(
            "diskless\t-\t100\ndiskless/clients\twriteback\t200\ndiskless/broken\n",
        );
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].created_at, Some(100));
        assert_eq!(parsed[1].kind, "writeback");
    }

    #[test]
    fn only_orphaned_client_disks_are_candidates() {
        let now = 1_000_000;
        let entries = vec![
            entry("diskless", "-", Some(0)),
            entry("diskless/clients", "writeback", Some(0)),
            entry("diskless/clients/PC001-disk", "-", Some(now - 200 * 3600)),
            entry("diskless/clients/PC002-disk", "-", Some(now - 200 * 3600)),
            entry("diskless/clients/PC003-disk", "-", Some(now - 3600)),
            entry("diskless/clients/PC001-disk-spare-abc", "-", Some(0)),
            entry("diskless/clients/PC001-games", "-", Some(0)),
            entry("diskless/images/PC009-disk", "-", Some(0)),
        ];
        let clients = vec!["pc001".to_string()];
        let spares = HashSet::from(["diskless/clients/PC002-disk-spare-abc".to_string()]);
        let (candidates, skipped) = classify(&entries, &clients, &spares, 24, now);

        let datasets: Vec<&str> = candidates.iter().map(|c| c.dataset.as_str()).collect();
        assert_eq!(datasets, vec!["diskless/clients/PC002-disk"]);
        assert!(skipped
            .iter()
            .any(|skip| skip.dataset == "diskless/clients/PC001-disk"
                && skip.reason.contains("client with this name")));
        assert!(skipped
            .iter()
            .any(|skip| skip.dataset == "diskless/clients/PC003-disk"
                && skip.reason.contains("minimum age")));
    }

    #[derive(Default)]
    struct FakeInventory(Vec<DatasetEntry>);

    impl DatasetInventory for FakeInventory {
        fn list(&self, _pool: &str) -> Result<Vec<DatasetEntry>> {
            Ok(self.0.clone())
        }
    }

    #[derive(Default)]
    struct FakeBackend {
        destroyed: Mutex<Vec<String>>,
        mounted: bool,
    }

    impl ImageBackend for FakeBackend {
        fn validate_unmounted(&self, _name: &str) -> Result<()> {
            anyhow::ensure!(!self.mounted, "volume is mounted");
            Ok(())
        }
        fn exists(&self, _name: &str) -> Result<bool> {
            Ok(true)
        }
        fn create_volume(&self, _: &str, _: u64) -> Result<()> {
            Ok(())
        }
        fn destroy(&self, name: &str) -> Result<()> {
            self.destroyed.lock().unwrap().push(name.to_string());
            Ok(())
        }
        fn rename(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn clone_image(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn create_snapshot(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn destroy_snapshot(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn rollback_snapshot(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn resize(&self, _: &str, _: u64) -> Result<()> {
            Ok(())
        }
        fn import_raw(&self, _: &std::path::Path, _: &str, _: u64) -> Result<()> {
            Ok(())
        }
        fn verify(&self, _: &str) -> Result<bool> {
            Ok(true)
        }
        fn info(&self, _: &str) -> Result<ImageBackendInfo> {
            anyhow::bail!("unused")
        }
        fn set_os_type(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn image_parent(&self) -> Result<String> {
            Ok("diskless".into())
        }
    }

    struct FakeIscsi {
        existing_targets: Vec<String>,
    }

    impl IscsiProvisioner for FakeIscsi {
        fn create_target(&self, _: &IscsiTargetSpec) -> Result<()> {
            Ok(())
        }
        fn remove_target(&self, _: &str) -> Result<()> {
            Ok(())
        }
        fn remove_target_with_backstores(&self, _: &str, _: &[String]) -> Result<()> {
            Ok(())
        }
        fn target_exists(&self, target_iqn: &str) -> Result<bool> {
            Ok(self
                .existing_targets
                .iter()
                .any(|existing| existing == target_iqn))
        }
        fn set_chap_auth(&self, _: &str, _: Option<&ChapCredentials>) -> Result<()> {
            Ok(())
        }
        fn list_target_luns(&self, _: &str) -> Result<Vec<IscsiLunState>> {
            Ok(Vec::new())
        }
        fn inspect_target(&self, _: &IscsiTargetSpec) -> Result<IscsiTargetState> {
            anyhow::bail!("unused")
        }
        fn reconcile(&self, _: &IscsiTargetSpec) -> Result<()> {
            Ok(())
        }
    }

    async fn pool() -> sqlx::SqlitePool {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::raw_sql(
            "CREATE TABLE clients(id TEXT PRIMARY KEY, name TEXT NOT NULL);\
             CREATE TABLE client_writeback_spares(client_id TEXT, dataset TEXT NOT NULL);\
             CREATE TABLE storage_dormant_cleanup_events(\
                id TEXT PRIMARY KEY, actor TEXT, action TEXT NOT NULL, dataset TEXT NOT NULL,\
                client_name TEXT NOT NULL, reason TEXT, error TEXT, created_at TEXT NOT NULL);",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    fn service(
        pool: sqlx::SqlitePool,
        settings: Settings,
        backend: Arc<FakeBackend>,
        iscsi: Arc<FakeIscsi>,
        entries: Vec<DatasetEntry>,
    ) -> DormantCleanupService {
        DormantCleanupService::new(
            pool,
            Arc::new(RwLock::new(settings)),
            backend,
            iscsi,
            "diskless".to_string(),
        )
        .with_inventory(Arc::new(FakeInventory(entries)))
    }

    fn entries(now: i64) -> Vec<DatasetEntry> {
        vec![
            entry("diskless", "-", Some(0)),
            entry("diskless/writeback", "writeback", Some(0)),
            entry("diskless/writeback/OLD-disk", "-", Some(now - 500 * 3600)),
            entry("diskless/writeback/LIVE-disk", "-", Some(now - 500 * 3600)),
            entry(
                "diskless/writeback/TARGETED-disk",
                "-",
                Some(now - 500 * 3600),
            ),
        ]
    }

    async fn seed(pool: &sqlx::SqlitePool) {
        sqlx::query("INSERT INTO clients (id, name) VALUES ('live', 'live')")
            .execute(pool)
            .await
            .unwrap();
    }

    fn iscsi() -> Arc<FakeIscsi> {
        Arc::new(FakeIscsi {
            existing_targets: vec!["iqn.2024-01.com.diskless:client.targeted".to_string()],
        })
    }

    #[tokio::test]
    async fn plan_only_proposes_unowned_and_untargeted_datasets() {
        let pool = pool().await;
        seed(&pool).await;
        let now = Utc::now().timestamp();
        let backend = Arc::new(FakeBackend::default());
        let service = service(pool, Settings::default(), backend, iscsi(), entries(now));

        let plan = service.plan().await.unwrap();
        let datasets: Vec<&str> = plan
            .candidates
            .iter()
            .map(|candidate| candidate.dataset.as_str())
            .collect();
        assert_eq!(datasets, vec!["diskless/writeback/OLD-disk"]);
        assert!(plan.skipped.iter().any(|skip| {
            skip.dataset == "diskless/writeback/TARGETED-disk"
                && skip.reason.contains("iSCSI target")
        }));
        assert!(plan.skipped.iter().any(|skip| {
            skip.dataset == "diskless/writeback/LIVE-disk"
                && skip.reason.contains("client with this name")
        }));
        assert!(!plan.enabled);
        assert!(plan.dry_run);
    }

    #[tokio::test]
    async fn a_mounted_dataset_is_never_proposed() {
        let pool = pool().await;
        seed(&pool).await;
        let now = Utc::now().timestamp();
        let service = service(
            pool,
            Settings::default(),
            Arc::new(FakeBackend {
                mounted: true,
                ..FakeBackend::default()
            }),
            iscsi(),
            entries(now),
        );

        let plan = service.plan().await.unwrap();
        assert!(plan.candidates.is_empty());
        assert!(plan.skipped.iter().any(|skip| {
            skip.dataset == "diskless/writeback/OLD-disk" && skip.reason.contains("mounted")
        }));
    }

    #[tokio::test]
    async fn disabled_cleanup_refuses_a_confirmed_run() {
        let pool = pool().await;
        seed(&pool).await;
        let backend = Arc::new(FakeBackend::default());
        let service = service(
            pool,
            Settings::default(),
            backend,
            iscsi(),
            entries(Utc::now().timestamp()),
        );

        let error = service.run("admin", true).await.unwrap_err();
        assert!(matches!(error, DormantCleanupError::Rejected(_)));
    }

    #[tokio::test]
    async fn dry_run_records_the_plan_without_destroying() {
        let pool = pool().await;
        seed(&pool).await;
        let mut settings = Settings::default();
        settings.storage.dormant_cleanup.enabled = true;
        let backend = Arc::new(FakeBackend::default());
        let service = service(
            pool.clone(),
            settings,
            backend.clone(),
            iscsi(),
            entries(Utc::now().timestamp()),
        );

        let run = service.run("admin", false).await.unwrap();
        assert!(!run.executed);
        assert_eq!(run.deleted, 0);
        assert!(backend.destroyed.lock().unwrap().is_empty());
        let recorded: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM storage_dormant_cleanup_events WHERE action = 'planned'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(recorded, 1);
    }

    #[tokio::test]
    async fn a_confirmed_run_destroys_only_the_orphan_and_audits_it() {
        let pool = pool().await;
        seed(&pool).await;
        let mut settings = Settings::default();
        settings.storage.dormant_cleanup.enabled = true;
        settings.storage.dormant_cleanup.dry_run = false;
        let backend = Arc::new(FakeBackend::default());
        let service = service(
            pool.clone(),
            settings,
            backend.clone(),
            iscsi(),
            entries(Utc::now().timestamp()),
        );

        let run = service.run("admin", true).await.unwrap();
        assert!(run.executed);
        assert_eq!(run.deleted, 1);
        assert!(run.failures.is_empty());
        assert_eq!(
            backend.destroyed.lock().unwrap().as_slice(),
            ["diskless/writeback/OLD-disk"]
        );
        let actions: Vec<String> =
            sqlx::query_scalar("SELECT action FROM storage_dormant_cleanup_events ORDER BY action")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(actions, vec!["deleted".to_string()]);
    }

    #[tokio::test]
    async fn a_client_created_between_plan_and_delete_keeps_its_dataset() {
        let pool = pool().await;
        seed(&pool).await;
        let mut settings = Settings::default();
        settings.storage.dormant_cleanup.enabled = true;
        settings.storage.dormant_cleanup.dry_run = false;
        let backend = Arc::new(FakeBackend::default());
        let service = service(
            pool.clone(),
            settings,
            backend.clone(),
            iscsi(),
            entries(Utc::now().timestamp()),
        );

        sqlx::query("INSERT INTO clients (id, name) VALUES ('old', 'old')")
            .execute(&pool)
            .await
            .unwrap();
        let run = service.run("admin", true).await.unwrap();
        assert_eq!(run.deleted, 0);
        assert!(backend.destroyed.lock().unwrap().is_empty());
    }
}
