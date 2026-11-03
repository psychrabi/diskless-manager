use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::{FromRow, SqlitePool};
use std::collections::HashSet;

use crate::{
    core::{
        alarm::{Alarm, AlarmKind, AlarmSeverity, AlarmSummary},
        config::{Settings, SparePrewarmConfig},
        service::ServiceInfo,
    },
    infrastructure::zfs::ZpoolHealth,
    services::ServiceManager,
};

/// Read model for operational alarms.
///
/// Alarms are derived from existing state on every read so a fixed problem
/// disappears without a second write path. Only acknowledgements are stored.
pub struct AlarmService {
    pool: SqlitePool,
}

#[derive(Debug, FromRow)]
struct BootFailureRow {
    client_id: String,
    client_name: String,
    failures: i64,
    last_failed_at: i64,
    last_error: String,
}

#[derive(Debug, FromRow)]
struct StorageRecoveryRow {
    client_id: String,
    client_name: String,
    failures: i64,
    retry_after: i64,
    last_error: Option<String>,
}

/// Consecutive build failures that mean prewarming is broken for a client,
/// not that one attempt failed while the pool was busy.
const SPARE_FAILURE_THRESHOLD: i64 = 3;

/// How long a full ready-spare limit may sit untouched before it is worth an
/// alarm. Spares expire at `ttl_hours`, so a limit that stays full for this
/// long means no eligible client has rebooted into one.
const SPARE_DRAIN_STALL_SECONDS: i64 = 6 * 3600;

#[derive(Debug, FromRow)]
struct SpareFailureRow {
    client_id: String,
    client_name: String,
    failures: i64,
    last_error: Option<String>,
}

#[derive(Debug, FromRow)]
struct ReadySpareRow {
    ready: i64,
    newest: Option<i64>,
}

#[derive(Debug, FromRow)]
struct MaintenanceFailureRow {
    pool: String,
    operation: String,
    detail: Option<String>,
    finished_at: String,
}

#[derive(Debug, FromRow)]
struct AcknowledgementRow {
    alarm_id: String,
    acknowledged_at: i64,
    acknowledged_by: String,
}

impl AlarmService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list(&self, settings: Settings) -> Result<AlarmSummary> {
        let mut alarms = self.database_alarms().await?;
        alarms.extend(service_alarms(&probe_services(&settings, &self.pool).await));
        alarms.extend(pool_alarms(&probe_pools().await));
        if settings.storage.spare_prewarm.enabled {
            alarms.extend(self.spare_alarms(&settings.storage.spare_prewarm).await?);
        }

        self.attach_acknowledgements(alarms).await
    }

    pub async fn acknowledge(&self, alarm_id: &str, actor: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO alarm_acknowledgements (alarm_id, acknowledged_at, acknowledged_by)
             VALUES (?, ?, ?)
             ON CONFLICT(alarm_id) DO UPDATE SET
                 acknowledged_at = excluded.acknowledged_at,
                 acknowledged_by = excluded.acknowledged_by",
        )
        .bind(alarm_id)
        .bind(Utc::now().timestamp())
        .bind(actor)
        .execute(&self.pool)
        .await
        .context("failed to store alarm acknowledgement")?;
        Ok(())
    }

    async fn database_alarms(&self) -> Result<Vec<Alarm>> {
        let boot_failures = sqlx::query_as::<_, BootFailureRow>(
            "SELECT b.client_id, c.name AS client_name, b.failures,
                    b.last_failed_at, b.last_error
             FROM client_boot_failures b
             JOIN clients c ON c.id = b.client_id",
        )
        .fetch_all(&self.pool)
        .await
        .context("failed to load boot failures for alarms")?;

        let storage_recoveries = sqlx::query_as::<_, StorageRecoveryRow>(
            "SELECT r.client_id, c.name AS client_name, r.failures,
                    r.retry_after, r.last_error
             FROM client_offline_resets r
             JOIN clients c ON c.id = r.client_id
             WHERE r.operation IS NOT NULL",
        )
        .fetch_all(&self.pool)
        .await
        .context("failed to load storage recoveries for alarms")?;

        // Only the newest run per pool/operation counts, so a later successful
        // run clears the alarm without deleting history.
        let maintenance_failures = sqlx::query_as::<_, MaintenanceFailureRow>(
            "SELECT r.pool, r.operation, r.detail, r.finished_at
             FROM storage_maintenance_runs r
             WHERE r.state = 'failed' AND r.finished_at = (
                 SELECT MAX(latest.finished_at) FROM storage_maintenance_runs latest
                 WHERE latest.pool = r.pool AND latest.operation = r.operation)",
        )
        .fetch_all(&self.pool)
        .await
        .context("failed to load maintenance failures for alarms")?;

        let mut alarms = Vec::with_capacity(boot_failures.len() + storage_recoveries.len());

        alarms.extend(boot_failures.into_iter().map(|row| Alarm {
            id: format!("boot:{}:{}", row.client_id, row.failures),
            kind: AlarmKind::BootFailure,
            severity: if row.failures >= 3 {
                AlarmSeverity::Critical
            } else {
                AlarmSeverity::Warning
            },
            title: format!("Boot failure on {}", row.client_name),
            detail: format!(
                "{} consecutive failure(s). Last error: {}",
                row.failures, row.last_error
            ),
            source: row.client_name,
            client_id: Some(row.client_id),
            observed_at: DateTime::from_timestamp(row.last_failed_at, 0),
            acknowledged_at: None,
            acknowledged_by: None,
        }));

        alarms.extend(storage_recoveries.into_iter().map(|row| Alarm {
            id: format!("storage:{}:{}", row.client_id, row.failures),
            kind: AlarmKind::StorageRecovery,
            severity: AlarmSeverity::Critical,
            title: format!("Storage recovery pending for {}", row.client_name),
            detail: format!(
                "{} failed attempt(s); retry after {}. {}",
                row.failures,
                DateTime::from_timestamp(row.retry_after, 0)
                    .map(|time| time.to_rfc3339())
                    .unwrap_or_else(|| "an unknown time".to_string()),
                row.last_error.unwrap_or_else(|| "no error recorded".to_string())
            ),
            source: row.client_name,
            client_id: Some(row.client_id),
            observed_at: DateTime::from_timestamp(row.retry_after, 0),
            acknowledged_at: None,
            acknowledged_by: None,
        }));

        alarms.extend(maintenance_failures.into_iter().map(|row| {
            Alarm {
                id: format!(
                    "maintenance:{}:{}:{}",
                    row.pool, row.operation, row.finished_at
                ),
                kind: AlarmKind::Maintenance,
                severity: AlarmSeverity::Warning,
                title: format!("Storage {} failed on {}", row.operation, row.pool),
                detail: row
                    .detail
                    .unwrap_or_else(|| "no diagnostic was recorded".to_string()),
                source: row.pool,
                client_id: None,
                observed_at: DateTime::parse_from_rfc3339(&row.finished_at)
                    .ok()
                    .map(|time| time.with_timezone(&Utc)),
                acknowledged_at: None,
                acknowledged_by: None,
            }
        }));

        Ok(alarms)
    }

    /// Prewarm health: builds that keep failing, and a ready limit that no
    /// client ever drains. Both are derived from stored spare rows, so the
    /// alarm clears by itself once a build succeeds or a spare is adopted.
    async fn spare_alarms(&self, config: &SparePrewarmConfig) -> Result<Vec<Alarm>> {
        let failures = sqlx::query_as::<_, SpareFailureRow>(
            "SELECT s.client_id, c.name AS client_name, s.failures, s.last_error
             FROM client_writeback_spares s
             JOIN clients c ON c.id = s.client_id
             WHERE s.status IN ('failed', 'invalid') AND s.failures >= ?",
        )
        .bind(SPARE_FAILURE_THRESHOLD)
        .fetch_all(&self.pool)
        .await
        .context("failed to load spare build failures for alarms")?;

        let mut alarms: Vec<Alarm> = failures
            .into_iter()
            .map(|row| Alarm {
                id: format!("spare:{}:{}", row.client_id, row.failures),
                kind: AlarmKind::SparePrewarm,
                severity: AlarmSeverity::Critical,
                title: format!("Writeback prewarm keeps failing for {}", row.client_name),
                detail: format!(
                    "{} consecutive replacement clone build or cleanup attempt(s) failed. Last error: {}",
                    row.failures,
                    row.last_error
                        .unwrap_or_else(|| "no error recorded".to_string())
                ),
                source: row.client_name,
                client_id: Some(row.client_id),
                observed_at: None,
                acknowledged_at: None,
                acknowledged_by: None,
            })
            .collect();

        if config.max_ready_spares == 0 {
            return Ok(alarms);
        }
        let ready = sqlx::query_as::<_, ReadySpareRow>(
            "SELECT COUNT(*) AS ready, MAX(created_at) AS newest
             FROM client_writeback_spares WHERE status = 'ready'",
        )
        .fetch_one(&self.pool)
        .await
        .context("failed to count ready spares for alarms")?;
        let now = Utc::now().timestamp();
        if ready.ready >= i64::from(config.max_ready_spares)
            && ready
                .newest
                .is_some_and(|newest| now.saturating_sub(newest) > SPARE_DRAIN_STALL_SECONDS)
        {
            alarms.push(Alarm {
                id: format!("spare:stalled:{}:{}", ready.ready, ready.newest.unwrap_or(0)),
                kind: AlarmKind::SparePrewarm,
                severity: AlarmSeverity::Warning,
                title: "Prewarmed replacements are not being used".to_string(),
                detail: format!(
                    "{} of {} permitted replacement clone(s) are ready and the newest is over {} hours old. \
                     No eligible client has booted into one; check that non-persistent clients still reboot \
                     and that their assigned image has not changed.",
                    ready.ready,
                    config.max_ready_spares,
                    SPARE_DRAIN_STALL_SECONDS / 3600
                ),
                source: "prewarm".to_string(),
                client_id: None,
                observed_at: ready.newest.and_then(|newest| DateTime::from_timestamp(newest, 0)),
                acknowledged_at: None,
                acknowledged_by: None,
            });
        }
        Ok(alarms)
    }

    async fn attach_acknowledgements(&self, mut alarms: Vec<Alarm>) -> Result<AlarmSummary> {
        let rows = sqlx::query_as::<_, AcknowledgementRow>(
            "SELECT alarm_id, acknowledged_at, acknowledged_by FROM alarm_acknowledgements",
        )
        .fetch_all(&self.pool)
        .await
        .context("failed to load alarm acknowledgements")?;

        let active: HashSet<&str> = alarms.iter().map(|alarm| alarm.id.as_str()).collect();
        let mut acknowledgements = std::collections::HashMap::new();
        for row in rows {
            let alarm_id = row.alarm_id.clone();
            if active.contains(alarm_id.as_str()) {
                acknowledgements.insert(alarm_id, row);
            } else {
                // Once an alarm clears, a later occurrence must alert again.
                sqlx::query("DELETE FROM alarm_acknowledgements WHERE alarm_id = ?")
                    .bind(&alarm_id)
                    .execute(&self.pool)
                    .await
                    .context("failed to prune a cleared alarm acknowledgement")?;
            }
        }

        for alarm in &mut alarms {
            if let Some(row) = acknowledgements.remove(&alarm.id) {
                alarm.acknowledged_at = DateTime::from_timestamp(row.acknowledged_at, 0);
                alarm.acknowledged_by = Some(row.acknowledged_by);
            }
        }

        Ok(AlarmSummary::from_alarms(alarms))
    }
}

async fn probe_services(settings: &Settings, pool: &SqlitePool) -> Vec<ServiceInfo> {
    let manager = ServiceManager::new(settings.clone(), pool.clone());
    let configured = [
        ("dhcp", "DHCP Server", settings.dhcp.enabled),
        ("tftp", "TFTP Server", settings.tftp.enabled),
        ("iscsi", "iSCSI Target", settings.iscsi.enabled),
        ("nfs", "NFS Server", settings.nfs.enabled),
        ("http", "HTTP Server", settings.http.enabled),
        ("samba", "Samba Server", settings.samba.enabled),
    ];

    let mut failing = Vec::new();
    for (name, display_name, enabled) in configured {
        if !enabled {
            continue;
        }
        match manager.status(name).await {
            Ok(status) if !status.running => failing.push(ServiceInfo {
                name: name.to_string(),
                display_name: display_name.to_string(),
                running: false,
                enabled,
                pid: status.pid,
                starts_on_boot: false,
            }),
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(service = name, error = %error, "service status unavailable for alarm inspection");
            }
        }
    }
    failing
}

async fn probe_pools() -> Vec<ZpoolHealth> {
    match tokio::task::spawn_blocking(crate::infrastructure::zfs::list_pool_health).await {
        Ok(Ok(pools)) => pools,
        Ok(Err(error)) => {
            tracing::warn!(error = %error, "ZFS pool health unavailable for alarm inspection");
            Vec::new()
        }
        Err(error) => {
            tracing::warn!(error = %error, "ZFS pool health task failed");
            Vec::new()
        }
    }
}

fn service_alarms(services: &[ServiceInfo]) -> Vec<Alarm> {
    services
        .iter()
        .filter(|service| service.enabled && !service.running)
        .map(|service| {
            let critical = matches!(service.name.as_str(), "dhcp" | "tftp" | "iscsi");
            Alarm {
                id: format!("service:{}", service.name),
                kind: AlarmKind::Service,
                severity: if critical {
                    AlarmSeverity::Critical
                } else {
                    AlarmSeverity::Warning
                },
                title: format!("{} is stopped", service.display_name),
                detail: if critical {
                    "Clients cannot complete network boot while this service is stopped."
                        .to_string()
                } else {
                    "The service is enabled but is not running.".to_string()
                },
                source: service.name.clone(),
                client_id: None,
                observed_at: None,
                acknowledged_at: None,
                acknowledged_by: None,
            }
        })
        .collect()
}

fn pool_alarms(pools: &[ZpoolHealth]) -> Vec<Alarm> {
    pools
        .iter()
        .filter(|pool| !pool.health.eq_ignore_ascii_case("ONLINE"))
        .map(|pool| {
            let severity = if pool.health.eq_ignore_ascii_case("DEGRADED") {
                AlarmSeverity::Warning
            } else {
                AlarmSeverity::Critical
            };
            Alarm {
                id: format!("pool:{}:{}", pool.name, pool.health),
                kind: AlarmKind::Pool,
                severity,
                title: format!("ZFS pool {} is {}", pool.name, pool.health),
                detail: "Check pool status and device health before starting new client sessions."
                    .to_string(),
                source: pool.name.clone(),
                client_id: None,
                observed_at: None,
                acknowledged_at: None,
                acknowledged_by: None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn database() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    fn service(name: &str, running: bool, enabled: bool) -> ServiceInfo {
        ServiceInfo {
            name: name.to_string(),
            display_name: name.to_ascii_uppercase(),
            running,
            enabled,
            pid: None,
            starts_on_boot: false,
        }
    }

    #[tokio::test]
    async fn derives_boot_and_storage_alarms_from_stored_state() {
        let pool = database().await;
        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, created_at, updated_at) VALUES ('client', 'PC-01', '02:00:00:00:00:01', '192.0.2.1', 'pool/master', 'now', 'now')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO client_boot_failures (client_id, failures, last_failed_at, last_error) VALUES ('client', 3, 100, 'iSCSI target unavailable')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO client_offline_resets (client_id, fingerprint, failures, retry_after, last_error, operation) VALUES ('client', '', 2, 200, 'cleanup interrupted', 'reset')")
            .execute(&pool)
            .await
            .unwrap();

        let alarms = AlarmService::new(pool).database_alarms().await.unwrap();

        assert_eq!(alarms.len(), 2);
        let boot = alarms
            .iter()
            .find(|alarm| alarm.kind == AlarmKind::BootFailure)
            .unwrap();
        assert_eq!(boot.severity, AlarmSeverity::Critical);
        assert_eq!(boot.id, "boot:client:3");
        assert!(boot.detail.contains("3 consecutive"));

        let storage = alarms
            .iter()
            .find(|alarm| alarm.kind == AlarmKind::StorageRecovery)
            .unwrap();
        assert_eq!(storage.severity, AlarmSeverity::Critical);
        assert_eq!(storage.id, "storage:client:2");
        assert!(storage.detail.contains("cleanup interrupted"));
    }

    fn prewarm(enabled: bool, max_ready_spares: u32) -> SparePrewarmConfig {
        SparePrewarmConfig {
            enabled,
            max_ready_spares,
            ..SparePrewarmConfig::default()
        }
    }

    /// Two clients: the spare table allows exactly one spare per client.
    async fn spare_clients(pool: &SqlitePool) {
        for (id, name, mac, ip) in [
            ("client", "PC-01", "02:00:00:00:00:01", "192.0.2.1"),
            ("client2", "PC-02", "02:00:00:00:00:02", "192.0.2.2"),
        ] {
            sqlx::query("INSERT INTO clients (id, name, mac, ip, master, created_at, updated_at) VALUES (?, ?, ?, ?, 'pool/master', 'now', 'now')")
                .bind(id)
                .bind(name)
                .bind(mac)
                .bind(ip)
                .execute(pool)
                .await
                .unwrap();
        }
    }

    async fn spare_row(
        pool: &SqlitePool,
        dataset: &str,
        status: &str,
        failures: i64,
        created_at: i64,
    ) {
        let client = if dataset.ends_with("b") {
            "client2"
        } else {
            "client"
        };
        sqlx::query(
            "INSERT INTO client_writeback_spares(client_id, dataset, source_snapshot, fingerprint, status, created_at, retry_after, last_error, failures)
             VALUES (?, ?, 'pool/master@ready', 'fp', ?, ?, 0, 'zfs clone failed', ?)",
        )
        .bind(client)
        .bind(dataset)
        .bind(status)
        .bind(created_at)
        .bind(failures)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn alarms_only_after_repeated_spare_build_failures() {
        let pool = database().await;
        spare_clients(&pool).await;
        spare_row(&pool, "pool/clients/pc-spare-a", "failed", 2, 100).await;
        let service = AlarmService::new(pool.clone());

        assert!(service
            .spare_alarms(&prewarm(true, 4))
            .await
            .unwrap()
            .is_empty());

        spare_row(&pool, "pool/clients/pc-spare-b", "failed", 3, 200).await;
        let alarms = service.spare_alarms(&prewarm(true, 4)).await.unwrap();
        assert_eq!(alarms.len(), 1);
        assert_eq!(alarms[0].kind, AlarmKind::SparePrewarm);
        assert_eq!(alarms[0].severity, AlarmSeverity::Critical);
        assert_eq!(alarms[0].id, "spare:client2:3");
        assert!(alarms[0].detail.contains("zfs clone failed"));

        sqlx::query(
            "UPDATE client_writeback_spares SET status = 'invalid' WHERE client_id = 'client2'",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            service.spare_alarms(&prewarm(true, 4)).await.unwrap().len(),
            1
        );

        // Recovery clears the alarm without deleting history.
        sqlx::query("UPDATE client_writeback_spares SET status = 'ready', failures = 0 WHERE dataset = 'pool/clients/pc-spare-b'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(service
            .spare_alarms(&prewarm(true, 4))
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn alarms_when_a_full_ready_limit_stops_draining() {
        let pool = database().await;
        spare_clients(&pool).await;
        let now = Utc::now().timestamp();
        spare_row(&pool, "pool/clients/pc-spare-a", "ready", 0, now - 7 * 3600).await;
        spare_row(&pool, "pool/clients/pc-spare-b", "ready", 0, now - 7 * 3600).await;
        let service = AlarmService::new(pool.clone());

        let alarms = service.spare_alarms(&prewarm(true, 2)).await.unwrap();
        assert_eq!(alarms.len(), 1);
        assert_eq!(alarms[0].severity, AlarmSeverity::Warning);
        assert!(alarms[0].title.contains("not being used"));

        // Below the limit, or freshly built, is normal operation.
        assert!(service
            .spare_alarms(&prewarm(true, 3))
            .await
            .unwrap()
            .is_empty());
        sqlx::query(
            "DELETE FROM client_writeback_spares WHERE dataset = 'pool/clients/pc-spare-b'",
        )
        .execute(&pool)
        .await
        .unwrap();
        spare_row(&pool, "pool/clients/pc-spare-b", "ready", 0, now).await;
        assert!(service
            .spare_alarms(&prewarm(true, 2))
            .await
            .unwrap()
            .is_empty());
    }

    #[test]
    fn derives_stopped_enabled_services_only() {
        let alarms = service_alarms(&[
            service("dhcp", false, true),
            service("nfs", false, true),
            service("samba", false, false),
            service("tftp", true, true),
        ]);

        assert_eq!(alarms.len(), 2);
        assert_eq!(alarms[0].id, "service:dhcp");
        assert_eq!(alarms[0].severity, AlarmSeverity::Critical);
        assert_eq!(alarms[1].id, "service:nfs");
        assert_eq!(alarms[1].severity, AlarmSeverity::Warning);
    }

    #[test]
    fn derives_pool_alarms_for_non_online_health() {
        let alarms = pool_alarms(&[
            ZpoolHealth {
                name: "tank".into(),
                health: "ONLINE".into(),
            },
            ZpoolHealth {
                name: "backup".into(),
                health: "DEGRADED".into(),
            },
            ZpoolHealth {
                name: "archive".into(),
                health: "FAULTED".into(),
            },
        ]);

        assert_eq!(alarms.len(), 2);
        assert_eq!(alarms[0].severity, AlarmSeverity::Warning);
        assert_eq!(alarms[0].id, "pool:backup:DEGRADED");
        assert_eq!(alarms[1].severity, AlarmSeverity::Critical);
    }

    #[tokio::test]
    async fn acknowledgement_applies_until_the_alarm_clears() {
        let pool = database().await;
        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, created_at, updated_at) VALUES ('client', 'PC-01', '02:00:00:00:00:01', '192.0.2.1', 'pool/master', 'now', 'now')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO client_boot_failures (client_id, failures, last_failed_at, last_error) VALUES ('client', 1, 100, 'timeout')")
            .execute(&pool)
            .await
            .unwrap();
        let service = AlarmService::new(pool.clone());

        service.acknowledge("boot:client:1", "admin").await.unwrap();
        let summary = service
            .attach_acknowledgements(service.database_alarms().await.unwrap())
            .await
            .unwrap();
        assert_eq!(summary.acknowledged, 1);
        assert_eq!(summary.warning, 0);
        assert_eq!(summary.alarms[0].acknowledged_by.as_deref(), Some("admin"));

        // A new failure is a new occurrence and must alert again.
        sqlx::query("UPDATE client_boot_failures SET failures = 2 WHERE client_id = 'client'")
            .execute(&pool)
            .await
            .unwrap();
        let summary = service
            .attach_acknowledgements(service.database_alarms().await.unwrap())
            .await
            .unwrap();
        assert_eq!(summary.acknowledged, 0);
        assert_eq!(summary.warning, 1);

        // The old acknowledgement is removed once its alarm is gone.
        sqlx::query("DELETE FROM client_boot_failures")
            .execute(&pool)
            .await
            .unwrap();
        let summary = service
            .attach_acknowledgements(service.database_alarms().await.unwrap())
            .await
            .unwrap();
        assert!(summary.alarms.is_empty());
        let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM alarm_acknowledgements")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(remaining, 0);
    }
}
