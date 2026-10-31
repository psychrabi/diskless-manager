//! Prewarmed replacement writebacks ("spares").
//!
//! A spare is a job-owned ZFS clone of a client's assigned immutable snapshot.
//! It is built while the client is offline and its writeback clone is already
//! absent, then adopted at PXE time through the same durable
//! [`OfflineReplacement`] journal used by every other replacement. A spare is
//! never proof that a client's active storage exists: it becomes boot storage
//! only after ownership, origin, session and export checks pass.

use crate::{
    application::storage_service::{OfflineReplacement, StorageService},
    core::config::SparePrewarmConfig,
    domain::storage::ClientStorageSpec,
    domain::Client,
    infrastructure::zfs::pool_free_bytes,
    persistence::ClientRepository,
    state::AppState,
};
use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

/// How long an interrupted build or adoption may stay unresolved before the
/// background sweep cleans it up.
const INTERRUPTED_GRACE_SECONDS: i64 = 3600;
/// Delay before retrying a failed spare build.
const FAILED_BUILD_RETRY_SECONDS: i64 = 3600;
/// Period of the background sweep.
const SWEEP_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpareStatus {
    Building,
    Ready,
    Invalid,
    Failed,
    Consuming,
}

impl SpareStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Building => "building",
            Self::Ready => "ready",
            Self::Invalid => "invalid",
            Self::Failed => "failed",
            Self::Consuming => "consuming",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "building" => Some(Self::Building),
            "ready" => Some(Self::Ready),
            "invalid" => Some(Self::Invalid),
            "failed" => Some(Self::Failed),
            "consuming" => Some(Self::Consuming),
            _ => None,
        }
    }
}

/// One persisted spare. `dataset` is a job-owned clone, never the client's
/// boot dataset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WritebackSpare {
    pub client_id: String,
    pub dataset: String,
    pub source_snapshot: String,
    pub fingerprint: String,
    pub status: SpareStatus,
    pub created_at: i64,
    pub validated_at: Option<i64>,
    pub retry_after: i64,
    pub last_error: Option<String>,
}

#[derive(sqlx::FromRow)]
struct SpareRow {
    client_id: String,
    dataset: String,
    source_snapshot: String,
    fingerprint: String,
    status: String,
    created_at: i64,
    validated_at: Option<i64>,
    retry_after: i64,
    last_error: Option<String>,
}

impl TryFrom<SpareRow> for WritebackSpare {
    type Error = anyhow::Error;

    fn try_from(row: SpareRow) -> Result<Self> {
        Ok(Self {
            client_id: row.client_id,
            dataset: row.dataset,
            source_snapshot: row.source_snapshot,
            fingerprint: row.fingerprint,
            status: SpareStatus::parse(&row.status)
                .with_context(|| format!("unknown writeback spare status '{}'", row.status))?,
            created_at: row.created_at,
            validated_at: row.validated_at,
            retry_after: row.retry_after,
            last_error: row.last_error,
        })
    }
}

const SPARE_COLUMNS: &str =
    "client_id, dataset, source_snapshot, fingerprint, status, created_at, validated_at, retry_after, last_error";

pub async fn load_spare(
    pool: &sqlx::SqlitePool,
    client_id: &str,
) -> Result<Option<WritebackSpare>> {
    let row = sqlx::query_as::<_, SpareRow>(&format!(
        "SELECT {SPARE_COLUMNS} FROM client_writeback_spares WHERE client_id = ?"
    ))
    .bind(client_id)
    .fetch_optional(pool)
    .await?;
    row.map(WritebackSpare::try_from).transpose()
}

pub async fn list_spares(pool: &sqlx::SqlitePool) -> Result<Vec<WritebackSpare>> {
    sqlx::query_as::<_, SpareRow>(&format!(
        "SELECT {SPARE_COLUMNS} FROM client_writeback_spares ORDER BY created_at"
    ))
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(WritebackSpare::try_from)
    .collect()
}

async fn insert_building_spare(pool: &sqlx::SqlitePool, spare: &WritebackSpare) -> Result<()> {
    sqlx::query(
        "INSERT INTO client_writeback_spares(client_id, dataset, source_snapshot, fingerprint, status, created_at, validated_at, retry_after, last_error) VALUES (?, ?, ?, ?, ?, ?, NULL, 0, NULL) ON CONFLICT(client_id) DO UPDATE SET dataset = excluded.dataset, source_snapshot = excluded.source_snapshot, fingerprint = excluded.fingerprint, status = excluded.status, created_at = excluded.created_at, validated_at = NULL, retry_after = 0, last_error = NULL",
    )
    .bind(&spare.client_id)
    .bind(&spare.dataset)
    .bind(&spare.source_snapshot)
    .bind(&spare.fingerprint)
    .bind(spare.status.as_str())
    .bind(spare.created_at)
    .execute(pool)
    .await?;
    Ok(())
}

async fn set_spare_status(
    pool: &sqlx::SqlitePool,
    client_id: &str,
    status: SpareStatus,
    retry_after: i64,
    last_error: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "UPDATE client_writeback_spares SET status = ?, retry_after = ?, last_error = ?, validated_at = CASE WHEN ? THEN ? ELSE validated_at END WHERE client_id = ?",
    )
    .bind(status.as_str())
    .bind(retry_after)
    .bind(last_error)
    .bind(status == SpareStatus::Ready)
    .bind(chrono::Utc::now().timestamp())
    .bind(client_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_spare(pool: &sqlx::SqlitePool, client_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM client_writeback_spares WHERE client_id = ?")
        .bind(client_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// The deterministic identity a spare must match to remain usable.
///
/// Both the background builder and the boot path compute this from persisted
/// client state, so any change to the source revision, boot configuration,
/// persistence choice or game selection invalidates an existing spare.
pub async fn spare_fingerprint(pool: &sqlx::SqlitePool, client: &Client) -> Result<String> {
    let mut games =
        crate::core::reconciliation::stored_game_selection(pool, client.id.as_str()).await?;
    games.sort();
    games.dedup();
    // A hardware-identity cache can override the assigned snapshot; it also
    // reserves storage, so such clients never receive a spare. Including it
    // keeps the fingerprint honest if that ever changes.
    let effective_source =
        crate::application::hardware_identity_service::cached_source_for_client(pool, client)
            .await?;
    Ok(serde_json::to_string(&(
        client.id.to_string(),
        &client.name,
        &client.master,
        &client.snapshot,
        &effective_source,
        &client.writeback,
        &client.block_device,
        &client.block_store,
        &client.target_iqn,
        client.keep_writeback,
        client.use_game_disk,
        games,
    ))?)
}

/// Reject a spare that does not belong to this exact client configuration.
fn validate_spare_identity(
    spare: &WritebackSpare,
    client: &Client,
    spec: &ClientStorageSpec,
    fingerprint: &str,
) -> Result<()> {
    anyhow::ensure!(
        spare.client_id == client.id.as_str() && spare.client_id == spec.client_id,
        "spare belongs to a different client"
    );
    anyhow::ensure!(spare.status == SpareStatus::Ready, "spare is not ready");
    anyhow::ensure!(
        spare.fingerprint == fingerprint,
        "spare does not match the client's current storage configuration"
    );
    anyhow::ensure!(
        spare.source_snapshot == spec.source.value(),
        "spare was built from a different source snapshot"
    );
    anyhow::ensure!(
        spare.dataset != spec.dataset,
        "spare dataset must not be the client's boot dataset"
    );
    anyhow::ensure!(
        spare.dataset.starts_with(spec.dataset.as_str()),
        "spare dataset is outside the client's storage namespace"
    );
    Ok(())
}

fn spare_dataset_name(writeback: &str) -> String {
    let token = uuid::Uuid::new_v4().simple().to_string();
    format!("{writeback}-spare-{}", &token[..12])
}

/// Free-space provider for the capacity gate. Errors mean "unknown", which
/// blocks new builds rather than assuming the pool has room.
pub trait CapacityProbe: Send + Sync {
    fn free_bytes(&self) -> Result<u64>;
}

/// Session visibility for preprocessing. Errors mean "unknown", which also
/// blocks a build instead of treating the client as offline.
pub trait SessionProbe: Send + Sync {
    fn target_connected(&self, target_iqn: &str) -> Result<bool>;
}

pub struct SystemSessions {
    storage: Arc<StorageService>,
}

impl SystemSessions {
    pub fn new(storage: Arc<StorageService>) -> Self {
        Self { storage }
    }
}

impl SessionProbe for SystemSessions {
    fn target_connected(&self, target_iqn: &str) -> Result<bool> {
        self.storage.target_connected_if_present(target_iqn)
    }
}

pub struct SystemCapacity {
    config: crate::config::RuntimeConfig,
}

impl SystemCapacity {
    pub fn new(config: crate::config::RuntimeConfig) -> Self {
        Self { config }
    }
}

impl CapacityProbe for SystemCapacity {
    fn free_bytes(&self) -> Result<u64> {
        pool_free_bytes(&crate::config::get_zpool_name(&self.config))
    }
}

/// Clients whose storage is reserved elsewhere must never receive a spare.
async fn reserved_client_ids(pool: &sqlx::SqlitePool) -> Result<HashSet<String>> {
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT client_id FROM client_offline_resets WHERE operation IS NOT NULL \
         UNION SELECT client_id FROM client_identity_caches \
         UNION SELECT client_id FROM installation_sessions WHERE closed_at IS NULL",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().collect())
}

async fn journaled_client_ids(pool: &sqlx::SqlitePool) -> Result<HashSet<String>> {
    Ok(sqlx::query_scalar(
        "SELECT client_id FROM client_offline_resets WHERE operation IS NOT NULL",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect())
}

/// A client that can safely own a spare clone right now.
fn client_can_own_spare(client: &Client) -> bool {
    client.enabled
        && !client.keep_writeback
        && client.has_snapshot()
        && !client.is_super_mode()
        && !client.master.trim().is_empty()
        && client.master != "pending"
        && client
            .writeback
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
}

fn safe_dataset_pair<'a>(client: &'a Client, snapshot: &'a str) -> Option<(&'a str, &'a str)> {
    let dataset = client.writeback.as_deref()?.trim();
    if dataset.is_empty()
        || dataset == client.master
        || dataset.contains('@')
        || dataset.contains("..")
        || snapshot.split('@').next() == Some(dataset)
    {
        return None;
    }
    Some((dataset, snapshot))
}

pub async fn run(state: AppState) {
    let capacity: Arc<dyn CapacityProbe> = Arc::new(SystemCapacity::new(state.config.clone()));
    let sessions: Arc<dyn SessionProbe> =
        Arc::new(SystemSessions::new(state.application.storage.clone()));
    let mut interval = tokio::time::interval(SWEEP_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        if let Err(error) = maintain(&state, capacity.clone(), sessions.clone()).await {
            tracing::warn!(%error, "writeback spare sweep failed");
        }
    }
}

/// One sweep: expire superseded/stale spares, then build replacements for
/// eligible offline clients up to the configured limits.
pub async fn maintain(
    state: &AppState,
    capacity: Arc<dyn CapacityProbe>,
    sessions: Arc<dyn SessionProbe>,
) -> Result<()> {
    let settings = state.settings.read().await.clone();
    let config = settings.storage.spare_prewarm.clone();
    expire_spares(state, &config).await?;
    if !config.enabled || config.max_ready_spares == 0 {
        return Ok(());
    }

    let probe = capacity.clone();
    let free_bytes = match tokio::task::spawn_blocking(move || probe.free_bytes()).await {
        Ok(Ok(free)) => free,
        Ok(Err(error)) => {
            tracing::warn!(%error, "skipping spare builds: pool free space is unavailable");
            return Ok(());
        }
        Err(error) => {
            tracing::warn!(%error, "skipping spare builds: capacity probe failed");
            return Ok(());
        }
    };
    if free_bytes < config.min_free_bytes {
        tracing::debug!(
            free_bytes,
            min_free_bytes = config.min_free_bytes,
            "skipping spare builds: pool free space below the prewarm floor"
        );
        return Ok(());
    }

    let spares = list_spares(&state.db_pool).await?;
    let occupied = spares
        .iter()
        .filter(|spare| matches!(spare.status, SpareStatus::Ready | SpareStatus::Building))
        .count();
    let slots = (config.max_ready_spares as usize).saturating_sub(occupied);
    if slots == 0 {
        return Ok(());
    }

    for (client, fingerprint) in plan_builds(state, &spares, slots).await? {
        if let Err(error) = build_spare(state, &client, &fingerprint, sessions.clone()).await {
            tracing::warn!(client_id = %client.id, %error, "spare build failed");
        }
    }
    Ok(())
}

/// Offline, unserved clients whose writeback is already absent.
async fn plan_builds(
    state: &AppState,
    spares: &[WritebackSpare],
    slots: usize,
) -> Result<Vec<(Client, String)>> {
    let clients = ClientRepository::new(state.db_pool.clone())
        .find_all()
        .await?;
    let existing: HashSet<&str> = spares
        .iter()
        .map(|spare| spare.client_id.as_str())
        .collect();
    let reserved = reserved_client_ids(&state.db_pool).await?;

    let mut planned: Vec<(Client, String)> = Vec::new();
    for client in clients {
        if planned.len() >= slots {
            break;
        }
        if !client_can_own_spare(&client)
            || reserved.contains(client.id.as_str())
            || existing.contains(client.id.as_str())
        {
            continue;
        }
        let fingerprint = spare_fingerprint(&state.db_pool, &client).await?;
        planned.push((client, fingerprint));
    }
    if planned.is_empty() {
        return Ok(planned);
    }

    // One batched presence query; a failed query must not be read as "absent".
    let datasets: Vec<String> = planned
        .iter()
        .filter_map(|(client, _)| client.writeback.clone())
        .collect();
    let storage = state.application.storage.clone();
    let present =
        tokio::task::spawn_blocking(move || storage.check_volume_presence(&datasets)).await??;

    Ok(planned
        .into_iter()
        .zip(present)
        .filter(|(_, present)| !present)
        .map(|((client, fingerprint), _)| (client, fingerprint))
        .collect())
}

async fn build_spare(
    state: &AppState,
    client: &Client,
    expected: &str,
    sessions: Arc<dyn SessionProbe>,
) -> Result<()> {
    // Preprocessing never queues behind an active boot or mutation.
    let Some(_guard) = state
        .client_mutations
        .try_lock_client(client.id.as_str())
        .await
    else {
        return Ok(());
    };
    let Some(current) = ClientRepository::new(state.db_pool.clone())
        .find_by_id(&client.id)
        .await?
    else {
        return Ok(());
    };
    if !client_can_own_spare(&current)
        || reserved_client_ids(&state.db_pool)
            .await?
            .contains(current.id.as_str())
    {
        return Ok(());
    }
    let snapshot = current
        .snapshot
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("client has no snapshot")?;
    let Some((dataset, snapshot)) = safe_dataset_pair(&current, snapshot) else {
        return Ok(());
    };
    if spare_fingerprint(&state.db_pool, &current).await? != expected {
        return Ok(());
    }
    let dataset = dataset.to_string();
    let snapshot = snapshot.to_string();
    let storage = state.application.storage.clone();

    let exists = {
        let dataset = dataset.clone();
        let storage = storage.clone();
        tokio::task::spawn_blocking(move || storage.volume_exists(&dataset)).await??
    };
    if exists {
        return Ok(());
    }
    let connected = match current.target_iqn.clone() {
        // Session inspection shells out; keep it off the async worker.
        Some(target) => {
            tokio::task::spawn_blocking(move || sessions.target_connected(&target)).await?
        }
        None => Ok(false),
    };
    // Unknown session state blocks preprocessing instead of being read as
    // offline; the normal boot path remains available.
    match connected {
        Ok(false) => {}
        Ok(true) => return Ok(()),
        Err(error) => {
            tracing::debug!(
                client_id = %current.id,
                %error,
                "skipping spare build: session state is unknown"
            );
            return Ok(());
        }
    }

    let spare_dataset = spare_dataset_name(&dataset);
    let spare = WritebackSpare {
        client_id: current.id.to_string(),
        dataset: spare_dataset.clone(),
        source_snapshot: snapshot.clone(),
        fingerprint: expected.to_string(),
        status: SpareStatus::Building,
        created_at: chrono::Utc::now().timestamp(),
        validated_at: None,
        retry_after: 0,
        last_error: None,
    };
    insert_building_spare(&state.db_pool, &spare).await?;

    let client_id = current.id.to_string();
    let built =
        tokio::task::spawn_blocking(move || storage.prepare_spare_clone(&snapshot, &spare_dataset))
            .await?;
    match built {
        Ok(()) => {
            set_spare_status(&state.db_pool, &client_id, SpareStatus::Ready, 0, None).await?;
            tracing::info!(
                client_id,
                dataset = %spare.dataset,
                "writeback spare is ready"
            );
        }
        Err(error) => {
            set_spare_status(
                &state.db_pool,
                &client_id,
                SpareStatus::Failed,
                chrono::Utc::now().timestamp() + FAILED_BUILD_RETRY_SECONDS,
                Some(&format!("{error:#}")),
            )
            .await?;
            return Err(error);
        }
    }
    Ok(())
}

/// Drop spares that are expired, stale, superseded or owned by a client that
/// may no longer use one. Journals always win: a spare referenced by a pending
/// recovery is left for that recovery to resolve.
async fn expire_spares(state: &AppState, config: &SparePrewarmConfig) -> Result<()> {
    let spares = list_spares(&state.db_pool).await?;
    if spares.is_empty() {
        return Ok(());
    }
    let clients: HashMap<String, Client> = ClientRepository::new(state.db_pool.clone())
        .find_all()
        .await?
        .into_iter()
        .map(|client| (client.id.to_string(), client))
        .collect();

    for spare in spares {
        // The adoption path holds this lock from journal write through commit,
        // so an in-flight replacement can never be swept underneath it.
        let Some(_guard) = state
            .client_mutations
            .try_lock_client(&spare.client_id)
            .await
        else {
            continue;
        };
        let Some(spare) = load_spare(&state.db_pool, &spare.client_id).await? else {
            continue;
        };
        if journaled_client_ids(&state.db_pool)
            .await?
            .contains(&spare.client_id)
        {
            continue;
        }
        if let Err(error) = expire_one(state, config, &clients, &spare).await {
            tracing::warn!(
                client_id = %spare.client_id,
                dataset = %spare.dataset,
                %error,
                "writeback spare cleanup failed"
            );
        }
    }
    Ok(())
}

async fn expire_one(
    state: &AppState,
    config: &SparePrewarmConfig,
    clients: &HashMap<String, Client>,
    spare: &WritebackSpare,
) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    let ttl = i64::from(config.ttl_hours) * 3600;
    let Some(client) = clients.get(&spare.client_id) else {
        return discard_spare(state, spare).await;
    };
    // Retry a failed build or cleanup once its backoff has elapsed; the row
    // is removed so a later sweep can build a fresh spare.
    if matches!(spare.status, SpareStatus::Failed | SpareStatus::Invalid)
        && now >= spare.retry_after
    {
        return discard_spare(state, spare).await;
    }
    if matches!(spare.status, SpareStatus::Building | SpareStatus::Consuming) {
        if now.saturating_sub(spare.created_at) > INTERRUPTED_GRACE_SECONDS {
            tracing::warn!(
                client_id = %spare.client_id,
                dataset = %spare.dataset,
                "discarding interrupted writeback spare"
            );
            return discard_spare(state, spare).await;
        }
        return Ok(());
    }

    let expired = now.saturating_sub(spare.created_at) > ttl;
    let unusable = !client_can_own_spare(client);
    let stale = match spare_fingerprint(&state.db_pool, client).await {
        Ok(fingerprint) => fingerprint != spare.fingerprint,
        Err(error) => {
            tracing::warn!(client_id = %spare.client_id, %error, "cannot fingerprint client");
            true
        }
    };
    // A spare is superseded when the client has boot storage again: the
    // lazy lifecycle has not removed it, so a rebuild is not wanted yet.
    let boot_dataset = client.writeback.clone();
    let spare_dataset = spare.dataset.clone();
    let storage = state.application.storage.clone();
    let (spare_present, superseded) = tokio::task::spawn_blocking(move || {
        let superseded = match boot_dataset {
            Some(dataset) => storage.volume_exists(&dataset)?,
            None => false,
        };
        Ok::<_, anyhow::Error>((storage.volume_exists(&spare_dataset)?, superseded))
    })
    .await??;

    if expired || unusable || stale || superseded || !spare_present {
        tracing::info!(
            client_id = %spare.client_id,
            dataset = %spare.dataset,
            expired,
            unusable,
            stale,
            superseded,
            spare_present,
            "discarding writeback spare"
        );
        discard_spare(state, spare).await?;
    }
    Ok(())
}

async fn discard_spare(state: &AppState, spare: &WritebackSpare) -> Result<()> {
    let storage = state.application.storage.clone();
    let dataset = spare.dataset.clone();
    let outcome =
        tokio::task::spawn_blocking(move || storage.discard_spare_clone(&dataset)).await?;
    match outcome {
        Ok(()) => delete_spare(&state.db_pool, &spare.client_id).await,
        Err(error) => {
            set_spare_status(
                &state.db_pool,
                &spare.client_id,
                SpareStatus::Invalid,
                chrono::Utc::now().timestamp() + FAILED_BUILD_RETRY_SECONDS,
                Some(&format!("{error:#}")),
            )
            .await?;
            Err(error)
        }
    }
}

/// Adopt a validated spare as boot storage through the replacement journal.
///
/// Returns `true` only when the client's boot dataset is now the verified
/// spare. Every other outcome leaves the normal boot path to run: a bad or
/// missing spare must never prevent a safe boot.
pub async fn try_consume_for_boot(
    state: &AppState,
    client: &Client,
    spec: &ClientStorageSpec,
) -> Result<bool> {
    let enabled = state.settings.read().await.storage.spare_prewarm.enabled;
    if !enabled {
        return Ok(false);
    }
    let Some(spare) = load_spare(&state.db_pool, client.id.as_str()).await? else {
        return Ok(false);
    };
    let fingerprint = spare_fingerprint(&state.db_pool, client).await?;
    if validate_spare_identity(&spare, client, spec, &fingerprint).is_err() {
        return Ok(false);
    }

    let storage = state.application.storage.clone();
    let check = {
        let storage = storage.clone();
        let spec = spec.clone();
        let spare_dataset = spare.dataset.clone();
        tokio::task::spawn_blocking(move || storage.spare_is_consumable(&spec, &spare_dataset))
            .await?
    };
    if let Err(reason) = check {
        tracing::warn!(
            client_id = %client.id,
            dataset = %spare.dataset,
            %reason,
            "writeback spare is not consumable; using the normal boot path"
        );
        return Ok(false);
    }

    let operation = OfflineReplacement::from_spare(spec.clone(), spare.dataset.clone());
    sqlx::query("INSERT INTO client_offline_resets(client_id, fingerprint, operation) VALUES (?, '', ?) ON CONFLICT(client_id) DO UPDATE SET operation = excluded.operation")
        .bind(client.id.as_str())
        .bind(serde_json::to_string(&operation)?)
        .execute(&state.db_pool)
        .await?;
    set_spare_status(
        &state.db_pool,
        client.id.as_str(),
        SpareStatus::Consuming,
        0,
        None,
    )
    .await?;

    let chap = spec.chap.clone();
    let consume = {
        let storage = storage.clone();
        let operation = operation.clone();
        tokio::task::spawn_blocking(move || {
            storage.consume_spare_offline(&operation, chap.as_ref())
        })
        .await?
    };
    if consume.is_ok() {
        // Clear the journal before forgetting the spare: a crash in between
        // leaves a spare row whose dataset is gone, which the next attempt
        // rejects and the sweep discards.
        clear_spare_journal(&state.db_pool, client.id.as_str()).await?;
        delete_spare(&state.db_pool, client.id.as_str()).await?;
        tracing::info!(
            client_id = %client.id,
            dataset = %spec.dataset,
            "adopted a prewarmed writeback spare"
        );
        return Ok(true);
    }

    let error = consume.expect_err("adoption failure is handled here");
    tracing::warn!(
        client_id = %client.id,
        dataset = %spare.dataset,
        %error,
        "writeback spare adoption failed; restoring a consistent state"
    );
    let recovered = {
        let storage = storage.clone();
        let operation = operation.clone();
        tokio::task::spawn_blocking(move || storage.recover_offline(&operation)).await?
    };
    match recovered {
        Ok(()) => {
            clear_spare_journal(&state.db_pool, client.id.as_str()).await?;
            delete_spare(&state.db_pool, client.id.as_str()).await?;
            Ok(false)
        }
        Err(recovery_error) => {
            set_spare_status(
                &state.db_pool,
                client.id.as_str(),
                SpareStatus::Failed,
                chrono::Utc::now().timestamp() + FAILED_BUILD_RETRY_SECONDS,
                Some(&format!("{error:#}")),
            )
            .await?;
            Err(recovery_error)
                .context("spare adoption failed and recovery could not restore client storage")
        }
    }
}

async fn clear_spare_journal(pool: &sqlx::SqlitePool, client_id: &str) -> Result<()> {
    sqlx::query(
        "UPDATE client_offline_resets SET operation = NULL WHERE client_id = ? AND operation IS NOT NULL",
    )
    .bind(client_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::storage::StorageSource;
    use chrono::Utc;

    async fn pool() -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, snapshot, writeback, block_device, block_store, target_iqn, keep_writeback, use_game_disk, enabled, created_at, updated_at) VALUES ('client', 'pc001', '00:11:22:33:44:55', '192.0.2.10', 'pool/master', 'pool/master@ready', 'pool/clients/pc001', '/dev/zvol/pool/clients/pc001', 'block_pc001', 'iqn.test:pc001', 0, 0, 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    fn client() -> Client {
        Client {
            installer_media_id: None,
            id: crate::domain::ClientId::from_string("client").unwrap(),
            name: "pc001".into(),
            mac: crate::domain::MacAddress::parse("00:11:22:33:44:55").unwrap(),
            ip: "192.0.2.10".parse().unwrap(),
            master: "pool/master".into(),
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            snapshot: Some("pool/master@ready".into()),
            block_store: Some("block_pc001".into()),
            target_iqn: Some("iqn.test:pc001".into()),
            writeback: Some("pool/clients/pc001".into()),
            last_modified: None,
            block_device: Some("/dev/zvol/pool/clients/pc001".into()),
            status: crate::domain::ClientStatus::Offline,
            mode: crate::domain::BootMode::Normal,
            pxe_mode: crate::domain::PxeMode::Uefi,
            keep_writeback: false,
            use_game_disk: false,
            game_disks: Vec::new(),
            chap_user: None,
            chap_secret: None,
            chap_enabled: false,
        }
    }

    fn spec() -> ClientStorageSpec {
        ClientStorageSpec {
            installer_iso: None,
            client_id: "client".into(),
            client_name: "pc001".into(),
            source: StorageSource::Snapshot("pool/master@ready".into()),
            dataset: "pool/clients/pc001".into(),
            backstore: "block_pc001".into(),
            target_iqn: "iqn.test:pc001".into(),
            lun: 0,
            use_game_disk: false,
            game_disks: Vec::new(),
            chap: None,
        }
    }

    fn spare(fingerprint: &str) -> WritebackSpare {
        WritebackSpare {
            client_id: "client".into(),
            dataset: "pool/clients/pc001-spare-abc123".into(),
            source_snapshot: "pool/master@ready".into(),
            fingerprint: fingerprint.into(),
            status: SpareStatus::Ready,
            created_at: 0,
            validated_at: Some(0),
            retry_after: 0,
            last_error: None,
        }
    }

    #[tokio::test]
    async fn spare_store_round_trips_status_and_error() {
        let pool = pool().await;
        let mut record = spare("fp");
        record.status = SpareStatus::Building;
        record.validated_at = None;
        insert_building_spare(&pool, &record).await.unwrap();
        let loaded = load_spare(&pool, "client").await.unwrap().unwrap();
        assert_eq!(loaded, record);

        set_spare_status(&pool, "client", SpareStatus::Ready, 0, None)
            .await
            .unwrap();
        let loaded = load_spare(&pool, "client").await.unwrap().unwrap();
        assert_eq!(loaded.status, SpareStatus::Ready);
        assert!(loaded.validated_at.is_some());

        set_spare_status(&pool, "client", SpareStatus::Failed, 42, Some("boom"))
            .await
            .unwrap();
        let loaded = load_spare(&pool, "client").await.unwrap().unwrap();
        assert_eq!(
            (
                loaded.status,
                loaded.retry_after,
                loaded.last_error.as_deref()
            ),
            (SpareStatus::Failed, 42, Some("boom"))
        );

        delete_spare(&pool, "client").await.unwrap();
        assert!(load_spare(&pool, "client").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn deleting_the_client_removes_its_spare_record() {
        let pool = pool().await;
        insert_building_spare(&pool, &spare("fp")).await.unwrap();
        sqlx::query("DELETE FROM clients WHERE id = 'client'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(load_spare(&pool, "client").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn fingerprint_changes_with_source_and_persistence_but_not_storage_order() {
        let pool = pool().await;
        let client = client();
        let first = spare_fingerprint(&pool, &client).await.unwrap();
        assert_eq!(first, spare_fingerprint(&pool, &client).await.unwrap());

        sqlx::query("INSERT INTO client_game_disks(client_id, master_dataset) VALUES ('client', 'pool/games/b'), ('client', 'pool/games/a')")
            .execute(&pool)
            .await
            .unwrap();
        let with_games = spare_fingerprint(&pool, &client).await.unwrap();
        assert_ne!(with_games, first, "game selection must invalidate spares");

        let mut resaved = client.clone();
        resaved.keep_writeback = true;
        assert_ne!(
            spare_fingerprint(&pool, &resaved).await.unwrap(),
            with_games,
            "persistence choice must invalidate spares"
        );

        sqlx::query("UPDATE clients SET snapshot = 'pool/master@v2' WHERE id = 'client'")
            .execute(&pool)
            .await
            .unwrap();
        let mut newer = client.clone();
        newer.snapshot = Some("pool/master@v2".into());
        assert_ne!(
            spare_fingerprint(&pool, &newer).await.unwrap(),
            with_games,
            "source revision must invalidate spares"
        );
    }

    #[test]
    fn spare_identity_rejects_wrong_client_source_slot_and_dataset() {
        let client = client();
        let spec = spec();
        let fingerprint = "fp";
        assert!(validate_spare_identity(&spare(fingerprint), &client, &spec, fingerprint).is_ok());

        let mut wrong_client = spare(fingerprint);
        wrong_client.client_id = "other".into();
        assert!(validate_spare_identity(&wrong_client, &client, &spec, fingerprint).is_err());

        let mut wrong_source = spare(fingerprint);
        wrong_source.source_snapshot = "pool/master@old".into();
        assert!(validate_spare_identity(&wrong_source, &client, &spec, fingerprint).is_err());

        let mut not_ready = spare(fingerprint);
        not_ready.status = SpareStatus::Failed;
        assert!(validate_spare_identity(&not_ready, &client, &spec, fingerprint).is_err());

        let mut not_a_spare = spare(fingerprint);
        not_a_spare.dataset = "pool/clients/pc001".into();
        assert!(validate_spare_identity(&not_a_spare, &client, &spec, fingerprint).is_err());

        let mut foreign_namespace = spare(fingerprint);
        foreign_namespace.dataset = "pool/other/pc002-spare-abc123".into();
        assert!(validate_spare_identity(&foreign_namespace, &client, &spec, fingerprint).is_err());

        assert!(validate_spare_identity(&spare("other"), &client, &spec, fingerprint).is_err());
    }

    #[test]
    fn only_disposable_snapshot_clients_are_prewarmed() {
        let client = client();
        assert!(client_can_own_spare(&client));

        let mut kept = client.clone();
        kept.keep_writeback = true;
        assert!(!client_can_own_spare(&kept));

        let mut disabled = client.clone();
        disabled.enabled = false;
        assert!(!client_can_own_spare(&disabled));

        let mut super_mode = client.clone();
        super_mode.mode = crate::domain::BootMode::Super;
        assert!(!client_can_own_spare(&super_mode));

        let mut no_snapshot = client.clone();
        no_snapshot.snapshot = None;
        assert!(!client_can_own_spare(&no_snapshot));

        let mut unprovisioned = client.clone();
        unprovisioned.master = "pending".into();
        assert!(!client_can_own_spare(&unprovisioned));
    }

    #[test]
    fn dataset_pairs_reject_masters_and_ambiguous_names() {
        let client = client();
        assert_eq!(
            safe_dataset_pair(&client, "pool/master@ready"),
            Some(("pool/clients/pc001", "pool/master@ready"))
        );
        let mut master_self = client.clone();
        master_self.writeback = Some("pool/master".into());
        assert!(safe_dataset_pair(&master_self, "pool/master@ready").is_none());
        assert!(safe_dataset_pair(&client, "pool/clients/pc001@ready").is_none());
        let mut traversal = client.clone();
        traversal.writeback = Some("pool/clients/../pc001".into());
        assert!(safe_dataset_pair(&traversal, "pool/master@ready").is_none());
    }

    #[test]
    fn spare_names_are_unique_and_inside_the_client_namespace() {
        let first = spare_dataset_name("pool/clients/pc001");
        let second = spare_dataset_name("pool/clients/pc001");
        assert_ne!(first, second);
        for name in [first, second] {
            assert!(name.starts_with("pool/clients/pc001-spare-"));
            assert!(!name.contains('@'));
        }
    }

    #[tokio::test]
    async fn spare_store_rejects_unknown_status_values() {
        let pool = pool().await;
        insert_building_spare(&pool, &spare("fp")).await.unwrap();
        sqlx::query(
            "UPDATE client_writeback_spares SET status = 'half-built' WHERE client_id = 'client'",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(load_spare(&pool, "client").await.is_err());
    }

    // ---- adoption and sweep through the real services with fake backends ----

    use crate::application::ApplicationServices;
    use crate::infrastructure::image::ImageBackendInfo;
    use crate::infrastructure::iscsi::{IscsiLunState, IscsiProvisioner, IscsiTargetSpec};
    use crate::state::ClientMutationLocks;
    use anyhow::bail;
    use sqlx::SqlitePool;
    use std::path::Path;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeDisks {
        volumes: Mutex<HashMap<String, String>>,
    }

    impl FakeDisks {
        fn add(&self, name: &str, origin: &str) {
            self.volumes
                .lock()
                .unwrap()
                .insert(name.into(), origin.into());
        }

        fn content(&self, name: &str) -> Option<String> {
            self.volumes.lock().unwrap().get(name).cloned()
        }
    }

    impl ImageBackend for FakeDisks {
        fn validate_unmounted(&self, _: &str) -> Result<()> {
            Ok(())
        }
        fn settle_device_changes(&self) -> Result<()> {
            Ok(())
        }
        fn clone_origin(&self, name: &str) -> Result<Option<String>> {
            Ok(self
                .volumes
                .lock()
                .unwrap()
                .get(name)
                .filter(|origin| !origin.is_empty())
                .cloned())
        }
        fn exists(&self, name: &str) -> Result<bool> {
            Ok(self.volumes.lock().unwrap().contains_key(name))
        }
        fn verify(&self, name: &str) -> Result<bool> {
            self.exists(name)
        }
        fn clone_image(&self, source: &str, destination: &str) -> Result<()> {
            let mut volumes = self.volumes.lock().unwrap();
            if !volumes.contains_key(source) || volumes.contains_key(destination) {
                bail!("invalid clone request");
            }
            volumes.insert(destination.into(), source.into());
            Ok(())
        }
        fn destroy(&self, name: &str) -> Result<()> {
            self.volumes.lock().unwrap().remove(name);
            Ok(())
        }
        fn rename(&self, source: &str, destination: &str) -> Result<()> {
            let mut volumes = self.volumes.lock().unwrap();
            let origin = volumes.remove(source).context("missing source")?;
            volumes.insert(destination.into(), origin);
            Ok(())
        }
        fn create_volume(&self, _: &str, _: u64) -> Result<()> {
            bail!("unexpected create")
        }
        fn create_snapshot(&self, _: &str, _: &str) -> Result<()> {
            bail!("unexpected snapshot")
        }
        fn destroy_snapshot(&self, _: &str, _: &str) -> Result<()> {
            bail!("unexpected destroy snapshot")
        }
        fn rollback_snapshot(&self, _: &str, _: &str) -> Result<()> {
            bail!("unexpected rollback")
        }
        fn resize(&self, _: &str, _: u64) -> Result<()> {
            bail!("unexpected resize")
        }
        fn import_raw(&self, _: &Path, _: &str, _: u64) -> Result<()> {
            bail!("unexpected import")
        }
        fn info(&self, _: &str) -> Result<ImageBackendInfo> {
            bail!("unexpected info")
        }
        fn set_os_type(&self, _: &str, _: &str) -> Result<()> {
            bail!("unexpected OS")
        }
        fn image_parent(&self) -> Result<String> {
            Ok("pool/clients".into())
        }
    }

    use crate::infrastructure::image::ImageBackend;
    use crate::infrastructure::iscsi::IscsiTargetState;

    #[derive(Default)]
    struct FakeIscsi {
        target: Mutex<Option<IscsiTargetSpec>>,
    }

    impl IscsiProvisioner for FakeIscsi {
        fn create_target(&self, spec: &IscsiTargetSpec) -> Result<()> {
            *self.target.lock().unwrap() = Some(spec.clone());
            Ok(())
        }
        fn remove_target(&self, _: &str) -> Result<()> {
            *self.target.lock().unwrap() = None;
            Ok(())
        }
        fn remove_target_with_backstores(&self, _: &str, backstores: &[String]) -> Result<()> {
            anyhow::ensure!(backstores == ["block_pc001"], "unexpected backstores");
            *self.target.lock().unwrap() = None;
            Ok(())
        }
        fn target_exists(&self, _: &str) -> Result<bool> {
            Ok(self.target.lock().unwrap().is_some())
        }
        fn set_chap_auth(
            &self,
            _: &str,
            _: Option<&crate::infrastructure::iscsi::ChapCredentials>,
        ) -> Result<()> {
            Ok(())
        }
        fn list_target_luns(&self, _: &str) -> Result<Vec<IscsiLunState>> {
            Ok(Vec::new())
        }
        fn inspect_target(&self, spec: &IscsiTargetSpec) -> Result<IscsiTargetState> {
            let ready = self.target.lock().unwrap().as_ref().is_some_and(|current| {
                spec.luns.iter().all(|wanted| {
                    current.luns.iter().any(|actual| {
                        actual.lun == wanted.lun && actual.backstore == wanted.backstore
                    })
                })
            });
            Ok(IscsiTargetState::from_luns(
                spec.target_iqn.clone(),
                ready,
                spec.luns
                    .iter()
                    .map(|lun| IscsiLunState {
                        lun: lun.lun,
                        backstore: lun.backstore.clone(),
                        exists: ready,
                        backstore_exists: ready,
                        block_device_matches: ready,
                    })
                    .collect(),
                ready,
            ))
        }
        fn reconcile(&self, spec: &IscsiTargetSpec) -> Result<()> {
            self.create_target(spec)
        }
    }

    #[derive(Default)]
    struct FakeCapacity {
        free: u64,
    }

    impl CapacityProbe for FakeCapacity {
        fn free_bytes(&self) -> Result<u64> {
            Ok(self.free)
        }
    }

    #[derive(Default)]
    struct FakeSessions {
        connected: bool,
        unknown: bool,
    }

    impl SessionProbe for FakeSessions {
        fn target_connected(&self, _: &str) -> Result<bool> {
            if self.unknown {
                bail!("session state unavailable");
            }
            Ok(self.connected)
        }
    }

    async fn spare_state(enabled: bool) -> (AppState, Arc<FakeDisks>, Arc<FakeIscsi>) {
        let pool = pool().await;
        let disks = Arc::new(FakeDisks::default());
        disks.add("pool/master@ready", "");
        let iscsi = Arc::new(FakeIscsi::default());
        let application = Arc::new(ApplicationServices::with_backends(
            pool.clone(),
            crate::config::RuntimeConfig::default(),
            disks.clone(),
            iscsi.clone(),
            crate::core::config::IscsiBackend::Targetcli,
        ));
        let mut settings = crate::core::config::Settings::default();
        settings.storage.spare_prewarm.enabled = enabled;
        let state = AppState {
            config: crate::config::RuntimeConfig::default(),
            client_mutations: ClientMutationLocks::default(),
            settings: Arc::new(tokio::sync::RwLock::new(settings)),
            db_pool: pool,
            config_path: std::path::PathBuf::new(),
            client_ips: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            metrics_collector: Arc::new(crate::metrics::MetricsCollector::default()),
            ssh_executor: Arc::new(crate::ssh_executor::SshExecutor::new()),
            application,
        };
        (state, disks, iscsi)
    }

    async fn ready_spare(state: &AppState, disks: &FakeDisks) -> WritebackSpare {
        let fingerprint = spare_fingerprint(&state.db_pool, &client()).await.unwrap();
        let record = spare(&fingerprint);
        insert_building_spare(&state.db_pool, &record)
            .await
            .unwrap();
        set_spare_status(&state.db_pool, "client", SpareStatus::Ready, 0, None)
            .await
            .unwrap();
        disks.add(&record.dataset, "pool/master@ready");
        record
    }

    async fn journal(pool: &SqlitePool) -> Option<Option<String>> {
        sqlx::query_scalar("SELECT operation FROM client_offline_resets WHERE client_id = 'client'")
            .fetch_optional(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn boot_adopts_a_ready_spare_and_clears_the_journal() {
        let (state, disks, iscsi) = spare_state(true).await;
        let record = ready_spare(&state, &disks).await;

        assert!(try_consume_for_boot(&state, &client(), &spec())
            .await
            .unwrap());

        assert_eq!(
            disks.content("pool/clients/pc001").as_deref(),
            Some("pool/master@ready")
        );
        assert!(disks.content(&record.dataset).is_none());
        assert!(disks.content("pool/master@ready").is_some());
        assert!(load_spare(&state.db_pool, "client")
            .await
            .unwrap()
            .is_none());
        assert_eq!(journal(&state.db_pool).await, Some(None));
        assert!(iscsi.target.lock().unwrap().is_some());
    }

    #[tokio::test]
    async fn adoption_is_skipped_when_disabled_stale_or_superseded() {
        // Feature flag off.
        let (state, disks, _) = spare_state(false).await;
        let record = ready_spare(&state, &disks).await;
        assert!(!try_consume_for_boot(&state, &client(), &spec())
            .await
            .unwrap());
        assert!(disks.content(&record.dataset).is_some());
        assert!(load_spare(&state.db_pool, "client")
            .await
            .unwrap()
            .is_some());

        // Configuration changed after the spare was built.
        let (state, disks, _) = spare_state(true).await;
        let record = ready_spare(&state, &disks).await;
        sqlx::query("UPDATE clients SET snapshot = 'pool/master@v2' WHERE id = 'client'")
            .execute(&state.db_pool)
            .await
            .unwrap();
        let mut newer = client();
        newer.snapshot = Some("pool/master@v2".into());
        assert!(!try_consume_for_boot(&state, &newer, &spec()).await.unwrap());
        assert!(disks.content(&record.dataset).is_some());
        assert_eq!(disks.content("pool/clients/pc001"), None);

        // The client still owns a live writeback (the reset has not run).
        let (state, disks, _) = spare_state(true).await;
        let record = ready_spare(&state, &disks).await;
        disks.add("pool/clients/pc001", "pool/master@ready");
        assert!(!try_consume_for_boot(&state, &client(), &spec())
            .await
            .unwrap());
        assert!(disks.content(&record.dataset).is_some());
        assert_eq!(
            disks.content("pool/clients/pc001").as_deref(),
            Some("pool/master@ready")
        );
    }

    #[tokio::test]
    async fn sweep_builds_one_spare_respects_capacity_and_expires_it() {
        let (state, disks, _) = spare_state(true).await;
        let sessions: Arc<dyn SessionProbe> = Arc::new(FakeSessions::default());
        let capacity = Arc::new(FakeCapacity { free: u64::MAX / 2 });

        maintain(&state, capacity.clone(), sessions.clone())
            .await
            .unwrap();
        let record = load_spare(&state.db_pool, "client")
            .await
            .unwrap()
            .expect("eligible offline clients get a spare");
        assert_eq!(record.status, SpareStatus::Ready);
        assert_eq!(
            disks.content(&record.dataset).as_deref(),
            Some("pool/master@ready")
        );
        assert_eq!(disks.content("pool/clients/pc001"), None);

        // A second sweep neither duplicates the spare nor touches boot storage.
        maintain(&state, capacity.clone(), sessions.clone())
            .await
            .unwrap();
        assert_eq!(list_spares(&state.db_pool).await.unwrap().len(), 1);

        // Below the free-space floor nothing new is built.
        let spent = std::sync::Arc::new(FakeCapacity { free: 0 });
        sqlx::query("DELETE FROM client_writeback_spares WHERE client_id = 'client'")
            .execute(&state.db_pool)
            .await
            .unwrap();
        disks.destroy(&record.dataset).unwrap();
        maintain(&state, spent, sessions.clone()).await.unwrap();
        assert!(load_spare(&state.db_pool, "client")
            .await
            .unwrap()
            .is_none());

        // An expired spare is destroyed and forgotten (a fresh spare may be
        // built in the same sweep, but never the expired dataset).
        let record = ready_spare(&state, &disks).await;
        sqlx::query("UPDATE client_writeback_spares SET created_at = 0 WHERE client_id = 'client'")
            .execute(&state.db_pool)
            .await
            .unwrap();
        maintain(&state, capacity, sessions.clone()).await.unwrap();
        assert!(disks.content(&record.dataset).is_none());
        if let Some(rebuilt) = load_spare(&state.db_pool, "client").await.unwrap() {
            assert_ne!(rebuilt.dataset, record.dataset);
            assert_eq!(rebuilt.status, SpareStatus::Ready);
        }

        // A ready record whose clone disappeared is cleaned up, not kept
        // forever as an unusable "ready" spare.
        let record = ready_spare(&state, &disks).await;
        disks.destroy(&record.dataset).unwrap();
        let no_capacity = Arc::new(FakeCapacity { free: 0 });
        maintain(&state, no_capacity, sessions).await.unwrap();
        assert!(load_spare(&state.db_pool, "client")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn sweep_skips_clients_whose_session_state_is_unknown_or_active() {
        for (connected, unknown) in [(true, false), (false, true)] {
            let (state, disks, _) = spare_state(true).await;
            let sessions: Arc<dyn SessionProbe> = Arc::new(FakeSessions { connected, unknown });
            let capacity = Arc::new(FakeCapacity { free: u64::MAX / 2 });
            maintain(&state, capacity, sessions).await.unwrap();
            assert!(
                load_spare(&state.db_pool, "client")
                    .await
                    .unwrap()
                    .is_none(),
                "connected={connected} unknown={unknown} must not be prewarmed"
            );
            assert_eq!(list_spares(&state.db_pool).await.unwrap().len(), 0);
            assert!(disks.content("pool/clients/pc001").is_none());
        }
    }

    #[tokio::test]
    async fn sweep_never_prewarms_persistent_or_live_clients() {
        let (state, disks, _) = spare_state(true).await;
        let sessions: Arc<dyn SessionProbe> = Arc::new(FakeSessions::default());
        let capacity = Arc::new(FakeCapacity { free: u64::MAX / 2 });
        sqlx::query("UPDATE clients SET keep_writeback = 1 WHERE id = 'client'")
            .execute(&state.db_pool)
            .await
            .unwrap();
        maintain(&state, capacity.clone(), sessions.clone())
            .await
            .unwrap();
        assert!(load_spare(&state.db_pool, "client")
            .await
            .unwrap()
            .is_none());

        sqlx::query("UPDATE clients SET keep_writeback = 0 WHERE id = 'client'")
            .execute(&state.db_pool)
            .await
            .unwrap();
        disks.add("pool/clients/pc001", "pool/master@ready");
        maintain(&state, capacity, sessions).await.unwrap();
        assert!(
            load_spare(&state.db_pool, "client")
                .await
                .unwrap()
                .is_none(),
            "a live writeback must not be prewarmed over"
        );
        assert_eq!(
            disks.content("pool/clients/pc001").as_deref(),
            Some("pool/master@ready")
        );
    }
}
