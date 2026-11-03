//! Bounded iPXE to operating-system boot handoffs.
//!
//! iPXE logs in to the client's LUN for `sanboot` and disconnects the moment
//! the kernel or Windows opens its own session to the same LUN. The client is
//! neither online nor offline during that gap, and a long installer or first
//! Windows boot can outlast the non-persistent reset delay, which used to
//! delete the storage the client was booting from.
//!
//! A handoff is a persisted, bounded window armed before the boot script
//! leaves the server. While it is live and the session is confirmed absent,
//! the lifecycle suspends the offline deadline instead of arming it. LIO
//! reports the same initiator IQN for the iPXE and the OS session, so the
//! phases stop at [`HandoffPhase::SessionObserved`]: an OS-level confirmation
//! needs the optional client agent and is never inferred from a session.
//!
//! Unknown session state is not offline. Only a positive "no session" answer
//! can expire a handoff; probe errors leave the attempt untouched.

use anyhow::{Context, Result};
use std::sync::Arc;

/// Shortest accepted handoff window.
pub const MIN_TIMEOUT_SECONDS: i64 = 30;
/// Longest accepted handoff window; a boot beyond this is treated as failed.
pub const MAX_TIMEOUT_SECONDS: i64 = 900;

/// Why a handoff expired; surfaced through the boot-failure diagnostics.
const EXPIRED_MESSAGE: &str = "iPXE boot handoff expired before the client opened an OS session";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandoffPhase {
    /// Storage is ready and the boot script is about to be served.
    ScriptServed,
    /// An iSCSI session was observed inside the window (iPXE or the OS).
    SessionObserved,
    /// The window closed with no session: the boot never reached the OS.
    Expired,
}

impl HandoffPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ScriptServed => "script_served",
            Self::SessionObserved => "session_observed",
            Self::Expired => "expired",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "script_served" => Some(Self::ScriptServed),
            "session_observed" => Some(Self::SessionObserved),
            "expired" => Some(Self::Expired),
            _ => None,
        }
    }

    /// A live phase can still suspend the offline deadline.
    fn is_live(self) -> bool {
        matches!(self, Self::ScriptServed | Self::SessionObserved)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootHandoff {
    pub client_id: String,
    pub attempt_id: String,
    /// Lifecycle fingerprint of the source revision that was served.
    pub fingerprint: String,
    pub phase: HandoffPhase,
    pub started_at: i64,
    pub updated_at: i64,
    pub expires_at: i64,
    pub expiry_reported: bool,
}

impl BootHandoff {
    /// Whether this attempt still suspends the offline deadline at `now`.
    pub fn is_live_at(&self, now: i64) -> bool {
        self.phase.is_live() && now < self.expires_at
    }
}

#[derive(sqlx::FromRow)]
struct HandoffRow {
    client_id: String,
    attempt_id: String,
    fingerprint: String,
    phase: String,
    started_at: i64,
    updated_at: i64,
    expires_at: i64,
    expiry_reported: bool,
}

impl TryFrom<HandoffRow> for BootHandoff {
    type Error = anyhow::Error;

    fn try_from(row: HandoffRow) -> Result<Self> {
        Ok(Self {
            client_id: row.client_id,
            attempt_id: row.attempt_id,
            fingerprint: row.fingerprint,
            phase: HandoffPhase::parse(&row.phase)
                .with_context(|| format!("unknown boot handoff phase '{}'", row.phase))?,
            started_at: row.started_at,
            updated_at: row.updated_at,
            expires_at: row.expires_at,
            expiry_reported: row.expiry_reported,
        })
    }
}

const HANDOFF_COLUMNS: &str =
    "client_id, attempt_id, fingerprint, phase, started_at, updated_at, expires_at, expiry_reported";

pub async fn load(pool: &sqlx::SqlitePool, client_id: &str) -> Result<Option<BootHandoff>> {
    let row = sqlx::query_as::<_, HandoffRow>(&format!(
        "SELECT {HANDOFF_COLUMNS} FROM client_boot_handoffs WHERE client_id = ?"
    ))
    .bind(client_id)
    .fetch_optional(pool)
    .await?;
    row.map(BootHandoff::try_from).transpose()
}

/// Whether a boot attempt is still inside its handoff window.
///
/// The lifecycle uses this to decide that a confirmed-absent session belongs to
/// a boot in progress rather than to an offline client.
pub async fn active(pool: &sqlx::SqlitePool, client_id: &str, now: i64) -> Result<bool> {
    Ok(load(pool, client_id)
        .await?
        .is_some_and(|handoff| handoff.is_live_at(now)))
}

/// Arm (or refresh) the handoff for one boot script.
///
/// Repeated PXE requests for the same client are idempotent: they keep the
/// original attempt and its deadline so a boot loop cannot hold storage
/// forever by requesting the script again. A closed window starts a new
/// attempt. The timeout is clamped to the accepted range.
pub async fn arm(
    pool: &sqlx::SqlitePool,
    client_id: &str,
    fingerprint: &str,
    timeout_seconds: i64,
    now: i64,
) -> Result<String> {
    let timeout = timeout_seconds.clamp(MIN_TIMEOUT_SECONDS, MAX_TIMEOUT_SECONDS);
    let reuse = load(pool, client_id)
        .await?
        .filter(|handoff| handoff.is_live_at(now));
    let (attempt_id, started_at) = match reuse {
        Some(handoff) => (handoff.attempt_id, handoff.started_at),
        None => (
            format!("boot-{}", &uuid::Uuid::new_v4().simple().to_string()[..16]),
            now,
        ),
    };
    sqlx::query(
        "INSERT INTO client_boot_handoffs(client_id, attempt_id, fingerprint, phase, started_at, updated_at, expires_at, expiry_reported, last_error) \
         VALUES (?, ?, ?, ?, ?, ?, ?, 0, NULL) \
         ON CONFLICT(client_id) DO UPDATE SET attempt_id = excluded.attempt_id, fingerprint = excluded.fingerprint, phase = excluded.phase, started_at = excluded.started_at, updated_at = excluded.updated_at, expires_at = excluded.expires_at, expiry_reported = 0, last_error = NULL",
    )
    .bind(client_id)
    .bind(&attempt_id)
    .bind(fingerprint)
    .bind(HandoffPhase::ScriptServed.as_str())
    .bind(started_at)
    .bind(now)
    .bind(started_at.saturating_add(timeout))
    .execute(pool)
    .await?;
    Ok(attempt_id)
}

/// Record that a session was observed for this attempt.
///
/// A later attempt's row must never be marked by an earlier attempt's probe,
/// so the update is matched on both identifiers.
pub async fn observe_session(
    pool: &sqlx::SqlitePool,
    client_id: &str,
    attempt_id: &str,
    now: i64,
) -> Result<()> {
    sqlx::query(
        "UPDATE client_boot_handoffs SET phase = ?, updated_at = ?, expiry_reported = 1 \
         WHERE client_id = ? AND attempt_id = ?",
    )
    .bind(HandoffPhase::SessionObserved.as_str())
    .bind(now)
    .bind(client_id)
    .bind(attempt_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Forget the handoff for a client that is confirmed online or deleted.
pub async fn clear(pool: &sqlx::SqlitePool, client_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM client_boot_handoffs WHERE client_id = ?")
        .bind(client_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Session visibility for expiry checks. `Ok(None)` means "unknown" and must
/// never be read as disconnected.
pub trait SessionProbe: Send + Sync {
    fn connected(&self, target_iqn: &str) -> Result<Option<bool>>;
}

pub struct SystemSessionProbe {
    storage: Arc<crate::application::storage_service::StorageService>,
}

impl SystemSessionProbe {
    pub fn new(storage: Arc<crate::application::storage_service::StorageService>) -> Self {
        Self { storage }
    }
}

impl SessionProbe for SystemSessionProbe {
    fn connected(&self, target_iqn: &str) -> Result<Option<bool>> {
        // A failed LIO query stays an error: unknown state never expires a boot.
        self.storage
            .target_connected_if_present(target_iqn)
            .map(Some)
    }
}

#[derive(sqlx::FromRow)]
struct DueAttempt {
    client_id: String,
    attempt_id: String,
    target_iqn: Option<String>,
}

/// Close every window whose deadline has passed.
///
/// A client that is still connected when the window closes simply keeps its
/// attempt marked as observed; the lifecycle already cancels the offline
/// deadline for connected clients. A client with no session gets exactly one
/// boot-failure diagnostic, which the pending-recovery page and the alarm read
/// model already surface. Probe failures leave the attempt open, because
/// treating an unknown session as idle is what this module exists to prevent.
pub async fn expire_due(
    pool: &sqlx::SqlitePool,
    probe: Arc<dyn SessionProbe>,
    now: i64,
) -> Result<usize> {
    let due = sqlx::query_as::<_, DueAttempt>(
        "SELECT h.client_id, h.attempt_id, c.target_iqn FROM client_boot_handoffs h \
         JOIN clients c ON c.id = h.client_id \
         WHERE h.expiry_reported = 0 AND h.expires_at <= ?",
    )
    .bind(now)
    .fetch_all(pool)
    .await?;

    let mut expired = 0;
    for attempt in due {
        let connected = match attempt.target_iqn {
            // No persisted target cannot be connected to anything.
            None => Ok(Some(false)),
            Some(target_iqn) => {
                let probe = probe.clone();
                match tokio::task::spawn_blocking(move || probe.connected(&target_iqn)).await {
                    Ok(result) => result,
                    Err(error) => Err(anyhow::anyhow!("session probe task failed: {error}")),
                }
            }
        };
        match connected {
            Ok(Some(true)) => {
                observe_session(pool, &attempt.client_id, &attempt.attempt_id, now).await?
            }
            Ok(Some(false)) => {
                sqlx::query(
                    "UPDATE client_boot_handoffs SET phase = ?, updated_at = ?, expiry_reported = 1, last_error = ? \
                     WHERE client_id = ? AND attempt_id = ?",
                )
                .bind(HandoffPhase::Expired.as_str())
                .bind(now)
                .bind(EXPIRED_MESSAGE)
                .bind(&attempt.client_id)
                .bind(&attempt.attempt_id)
                .execute(pool)
                .await?;
                crate::application::client_boot::record_boot_failure_row(
                    pool,
                    &attempt.client_id,
                    EXPIRED_MESSAGE,
                    now,
                )
                .await?;
                tracing::warn!(
                    client_id = %attempt.client_id,
                    attempt_id = %attempt.attempt_id,
                    "boot handoff expired without a client session"
                );
                expired += 1;
            }
            Ok(None) => {
                tracing::debug!(
                    client_id = %attempt.client_id,
                    "boot handoff expiry deferred: session state is unknown"
                );
            }
            Err(error) => {
                tracing::warn!(
                    client_id = %attempt.client_id,
                    %error,
                    "boot handoff expiry deferred: session probe failed"
                );
            }
        }
    }
    Ok(expired)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLIENT: &str = "client";

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

    struct FixedSession(Result<Option<bool>, &'static str>);

    impl SessionProbe for FixedSession {
        fn connected(&self, _target_iqn: &str) -> Result<Option<bool>> {
            match &self.0 {
                Ok(value) => Ok(*value),
                Err(message) => Err(anyhow::anyhow!(*message)),
            }
        }
    }

    async fn failures(pool: &sqlx::SqlitePool) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM client_boot_failures WHERE client_id = 'client'")
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn repeated_requests_keep_one_bounded_attempt() {
        let pool = pool().await;
        let first = arm(&pool, CLIENT, "fp", 300, 1_000).await.unwrap();
        let second = arm(&pool, CLIENT, "fp", 300, 1_050).await.unwrap();

        assert_eq!(first, second, "a repeated PXE request is not a new attempt");
        let handoff = load(&pool, CLIENT).await.unwrap().unwrap();
        assert_eq!(handoff.started_at, 1_000);
        assert_eq!(
            handoff.expires_at, 1_300,
            "a boot loop must not extend the window"
        );
        assert_eq!(handoff.updated_at, 1_050);
        assert!(active(&pool, CLIENT, 1_299).await.unwrap());
        assert!(!active(&pool, CLIENT, 1_300).await.unwrap());
    }

    #[tokio::test]
    async fn window_is_clamped_to_the_accepted_range() {
        let pool = pool().await;
        arm(&pool, CLIENT, "fp", 1, 0).await.unwrap();
        assert_eq!(
            load(&pool, CLIENT).await.unwrap().unwrap().expires_at,
            MIN_TIMEOUT_SECONDS
        );
        clear(&pool, CLIENT).await.unwrap();
        arm(&pool, CLIENT, "fp", i64::MAX, 0).await.unwrap();
        assert_eq!(
            load(&pool, CLIENT).await.unwrap().unwrap().expires_at,
            MAX_TIMEOUT_SECONDS
        );
    }

    #[tokio::test]
    async fn unknown_session_state_never_expires_a_boot() {
        let pool = pool().await;
        arm(&pool, CLIENT, "fp", 300, 0).await.unwrap();

        let unknown: Arc<dyn SessionProbe> = Arc::new(FixedSession(Ok(None)));
        assert_eq!(expire_due(&pool, unknown, 10_000).await.unwrap(), 0);
        let failed: Arc<dyn SessionProbe> = Arc::new(FixedSession(Err("lio unavailable")));
        assert_eq!(expire_due(&pool, failed, 10_000).await.unwrap(), 0);

        let handoff = load(&pool, CLIENT).await.unwrap().unwrap();
        assert_eq!(handoff.phase, HandoffPhase::ScriptServed);
        assert_eq!(failures(&pool).await, 0);
        assert!(!active(&pool, CLIENT, 10_000).await.unwrap());
    }

    #[tokio::test]
    async fn expiry_reports_once_and_allows_a_new_attempt() {
        let pool = pool().await;
        let attempt = arm(&pool, CLIENT, "fp", 300, 0).await.unwrap();

        let absent: Arc<dyn SessionProbe> = Arc::new(FixedSession(Ok(Some(false))));
        assert_eq!(expire_due(&pool, absent.clone(), 400).await.unwrap(), 1);
        assert_eq!(
            expire_due(&pool, absent, 500).await.unwrap(),
            0,
            "an expired attempt must not report again"
        );
        assert_eq!(failures(&pool).await, 1);
        assert!(!active(&pool, CLIENT, 500).await.unwrap());

        let next = arm(&pool, CLIENT, "fp", 300, 900).await.unwrap();
        assert_ne!(next, attempt, "a closed window starts a new attempt");
        assert!(active(&pool, CLIENT, 901).await.unwrap());
    }

    #[tokio::test]
    async fn a_connected_session_inside_the_window_keeps_the_boot_alive() {
        let pool = pool().await;
        let attempt = arm(&pool, CLIENT, "fp", 300, 0).await.unwrap();

        let present: Arc<dyn SessionProbe> = Arc::new(FixedSession(Ok(Some(true))));
        assert_eq!(expire_due(&pool, present, 400).await.unwrap(), 0);

        let handoff = load(&pool, CLIENT).await.unwrap().unwrap();
        assert_eq!(handoff.attempt_id, attempt);
        assert_eq!(handoff.phase, HandoffPhase::SessionObserved);
        assert_eq!(failures(&pool).await, 0);
    }
}
