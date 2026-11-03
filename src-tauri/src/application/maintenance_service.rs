//! Scheduled and manual ZFS maintenance (Phase 5).
//!
//! Scrub and TRIM are destructive to *performance*, never to data, but they
//! still must not collide with a resilver, a manual scrub an operator started in
//! another shell, or each other. Every run is recorded before the command is
//! issued and finalised by the same sweep, so a restart does not lose the
//! explanation for a scan that is still running.
//!
//! SMART is read through `smartctl -j` with a bounded timeout and a short
//! in-process cache: a dashboard refresh must never block on a slow disk.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::{DateTime, Timelike, Utc};
use tokio::sync::{Mutex, RwLock};

use crate::{
    core::config::{MaintenanceConfig, Settings},
    infrastructure::zfs::{list_pool_health, pool_free_bytes},
    state::AppState,
};

const PROBE_TIMEOUT: Duration = Duration::from_secs(15);
const SMART_TIMEOUT: Duration = Duration::from_secs(10);
const SMART_CACHE: Duration = Duration::from_secs(120);
const SWEEP_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MaintenanceOperation {
    Scrub,
    Trim,
}

impl MaintenanceOperation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Scrub => "scrub",
            Self::Trim => "trim",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "scrub" => Some(Self::Scrub),
            "trim" => Some(Self::Trim),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct VdevErrors {
    pub name: String,
    pub read: u64,
    pub write: u64,
    pub checksum: u64,
}

/// One parsed `zpool status -P` result.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PoolProbe {
    pub pool: String,
    pub state: String,
    pub scan: Option<String>,
    pub scan_kind: Option<String>,
    pub scan_in_progress: bool,
    pub scan_percent: Option<f64>,
    pub vdev_errors: Vec<VdevErrors>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SmartDevice {
    pub device: String,
    pub model: Option<String>,
    pub serial: Option<String>,
    /// `passed`, `failed` or `unknown`.
    pub health: String,
    pub temperature_c: Option<i64>,
    pub power_on_hours: Option<u64>,
    /// NVMe wear indicator (`percentage_used`), when the drive reports it.
    pub wear_percent: Option<i64>,
    pub media_errors: Option<u64>,
    pub available: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct MaintenanceRun {
    pub id: String,
    pub pool: String,
    pub operation: String,
    pub state: String,
    pub actor: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MaintenanceStatus {
    pub enabled: bool,
    pub scrub_interval_days: u32,
    pub trim_interval_days: u32,
    pub window_start_hour: u32,
    pub window_end_hour: u32,
    pub free_bytes: Option<u64>,
    pub pool: Option<PoolProbe>,
    pub smart: Vec<SmartDevice>,
    pub runs: Vec<MaintenanceRun>,
}

#[derive(Debug, thiserror::Error)]
pub enum MaintenanceError {
    #[error("{0}")]
    Rejected(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

pub struct MaintenanceService {
    db_pool: sqlx::SqlitePool,
    settings: Arc<RwLock<Settings>>,
    smart_cache: Mutex<Option<(Instant, Vec<SmartDevice>)>>,
}

impl MaintenanceService {
    pub fn new(db_pool: sqlx::SqlitePool, settings: Arc<RwLock<Settings>>) -> Self {
        Self {
            db_pool,
            settings,
            smart_cache: Mutex::new(None),
        }
    }

    pub async fn status(&self, refresh_smart: bool) -> Result<MaintenanceStatus> {
        let config = self.settings.read().await.storage.maintenance.clone();
        let pool = self.probe_pool().await?;
        let smart = match pool.as_ref() {
            Some(probe) => self.smart_devices(&probe.1, refresh_smart).await?,
            None => Vec::new(),
        };
        let free_bytes = pool
            .as_ref()
            .and_then(|(probe, _)| pool_free_bytes(&probe.pool).ok());
        let runs = self.recent_runs(20).await?;

        Ok(MaintenanceStatus {
            enabled: config.enabled,
            scrub_interval_days: config.scrub_interval_days,
            trim_interval_days: config.trim_interval_days,
            window_start_hour: config.window_start_hour,
            window_end_hour: config.window_end_hour,
            free_bytes,
            pool: pool.map(|(probe, _)| probe),
            smart,
            runs,
        })
    }

    /// Start a scrub or TRIM now. Manual runs are allowed while scheduling is
    /// disabled; they are still refused while any scan is already active.
    pub async fn start(
        &self,
        actor: &str,
        operation: MaintenanceOperation,
    ) -> Result<MaintenanceRun, MaintenanceError> {
        let (probe, _) = self
            .probe_pool()
            .await
            .map_err(MaintenanceError::Internal)?
            .ok_or_else(|| MaintenanceError::Rejected("no ZFS pool was found".to_string()))?;
        if probe.scan_in_progress {
            return Err(MaintenanceError::Rejected(format!(
                "a {} is already in progress on {}",
                probe.scan_kind.as_deref().unwrap_or("scan"),
                probe.pool
            )));
        }
        if probe.state != "ONLINE" {
            return Err(MaintenanceError::Rejected(format!(
                "pool {} is {} and must be healthy before maintenance",
                probe.pool, probe.state
            )));
        }

        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO storage_maintenance_runs (id, pool, operation, state, actor, started_at) \
             VALUES (?, ?, ?, 'running', ?, ?)",
        )
        .bind(&id)
        .bind(&probe.pool)
        .bind(operation.as_str())
        .bind(actor)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.db_pool)
        .await
        .context("failed to record maintenance run")?;

        let pool = probe.pool.clone();
        let command = operation.as_str().to_string();
        let mut zpool = tokio::process::Command::new("zpool");
        zpool.arg(&command).arg(&pool).kill_on_drop(true);
        let output = tokio::time::timeout(PROBE_TIMEOUT, zpool.output())
            .await
            .context("zpool maintenance command timed out")?
            .context("failed to start zpool maintenance")?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
            self.finish(
                &id,
                "failed",
                Some(if detail.is_empty() {
                    "zpool exited without a diagnostic"
                } else {
                    detail.as_str()
                }),
            )
            .await?;
            return Err(MaintenanceError::Internal(anyhow::anyhow!(
                "zpool {command} failed: {detail}"
            )));
        }

        self.run(&id).await?.ok_or_else(|| {
            MaintenanceError::Internal(anyhow::anyhow!("maintenance run disappeared"))
        })
    }

    /// Cancel the active scrub/TRIM, if the running kernel supports `-s`.
    pub async fn stop(&self, _actor: &str) -> Result<MaintenanceRun, MaintenanceError> {
        let (probe, _) = self
            .probe_pool()
            .await
            .map_err(MaintenanceError::Internal)?
            .ok_or_else(|| MaintenanceError::Rejected("no ZFS pool was found".to_string()))?;
        if !probe.scan_in_progress {
            return Err(MaintenanceError::Rejected(
                "no maintenance scan is running".to_string(),
            ));
        }
        let operation = match probe.scan_kind.as_deref() {
            Some("trim") => MaintenanceOperation::Trim,
            _ => MaintenanceOperation::Scrub,
        };

        let pool = probe.pool.clone();
        let command = operation.as_str().to_string();
        let mut zpool = tokio::process::Command::new("zpool");
        zpool.arg(&command).arg("-s").arg(&pool).kill_on_drop(true);
        let output = tokio::time::timeout(PROBE_TIMEOUT, zpool.output())
            .await
            .context("zpool cancel command timed out")?
            .context("failed to cancel zpool maintenance")?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(MaintenanceError::Internal(anyhow::anyhow!(
                "zpool {command} -s failed: {detail}"
            )));
        }

        let running = self
            .running_runs()
            .await
            .map_err(MaintenanceError::Internal)?;
        let mut latest = None;
        for mut run in running {
            if run.pool == pool {
                self.finish(&run.id, "cancelled", Some("stopped by an administrator"))
                    .await
                    .map_err(MaintenanceError::Internal)?;
                run.state = "cancelled".to_string();
                run.finished_at = Some(Utc::now().to_rfc3339());
                latest = Some(run);
            }
        }
        latest.ok_or_else(|| {
            MaintenanceError::Rejected("no recorded maintenance run was running".to_string())
        })
    }

    /// One sweep: finalise interrupted runs, then schedule a due operation when
    /// the off-hours window is open.
    pub async fn tick(&self) -> Result<()> {
        self.reconcile().await?;
        let config = self.settings.read().await.storage.maintenance.clone();
        if !config.enabled || !within_window(Utc::now().hour(), &config) {
            return Ok(());
        }
        let Some((probe, _)) = self.probe_pool().await? else {
            return Ok(());
        };
        if probe.scan_in_progress || probe.state != "ONLINE" {
            return Ok(());
        }
        for operation in [MaintenanceOperation::Scrub, MaintenanceOperation::Trim] {
            if !self.due(&probe.pool, operation, &config).await? {
                continue;
            }
            match self.start("scheduler", operation).await {
                Ok(_) => break,
                Err(MaintenanceError::Rejected(reason)) => {
                    tracing::info!(%reason, "scheduled maintenance skipped");
                    break;
                }
                Err(MaintenanceError::Internal(error)) => {
                    tracing::warn!(%error, "scheduled maintenance failed to start");
                    break;
                }
            }
        }
        Ok(())
    }

    async fn due(
        &self,
        pool: &str,
        operation: MaintenanceOperation,
        config: &MaintenanceConfig,
    ) -> Result<bool> {
        let interval = match operation {
            MaintenanceOperation::Scrub => config.scrub_interval_days,
            MaintenanceOperation::Trim => config.trim_interval_days,
        };
        let last: Option<String> = sqlx::query_scalar(
            "SELECT finished_at FROM storage_maintenance_runs \
             WHERE pool = ? AND operation = ? AND state = 'completed' \
             ORDER BY finished_at DESC LIMIT 1",
        )
        .bind(pool)
        .bind(operation.as_str())
        .fetch_optional(&self.db_pool)
        .await
        .context("failed to read the last maintenance run")?;
        let Some(last) = last else {
            return Ok(true);
        };
        let finished = DateTime::parse_from_rfc3339(&last)?;
        Ok(
            Utc::now() - finished.with_timezone(&Utc)
                >= chrono::Duration::days(i64::from(interval)),
        )
    }

    /// Finalise runs whose scan is no longer visible. A restart during an
    /// operation leaves the row `running`; this is where it becomes explainable.
    async fn reconcile(&self) -> Result<()> {
        let running = self.running_runs().await?;
        if running.is_empty() {
            return Ok(());
        }
        let Some((probe, _)) = self.probe_pool().await? else {
            return Ok(());
        };
        for run in running {
            if probe.scan_in_progress && probe.scan_kind.as_deref() == Some(run.operation.as_str())
            {
                continue;
            }
            let (state, detail) = if probe.state == "ONLINE" {
                ("completed", probe.scan.clone())
            } else {
                ("failed", Some(format!("pool state is {}", probe.state)))
            };
            self.finish(&run.id, state, detail.as_deref()).await?;
        }
        Ok(())
    }

    async fn finish(&self, id: &str, state: &str, detail: Option<&str>) -> Result<()> {
        sqlx::query(
            "UPDATE storage_maintenance_runs SET state = ?, finished_at = ?, detail = ? \
             WHERE id = ? AND state = 'running'",
        )
        .bind(state)
        .bind(Utc::now().to_rfc3339())
        .bind(detail)
        .bind(id)
        .execute(&self.db_pool)
        .await
        .context("failed to finalise maintenance run")?;
        Ok(())
    }

    async fn run(&self, id: &str) -> Result<Option<MaintenanceRun>> {
        sqlx::query_as::<_, MaintenanceRun>("SELECT * FROM storage_maintenance_runs WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.db_pool)
            .await
            .context("failed to reload maintenance run")
    }

    async fn running_runs(&self) -> Result<Vec<MaintenanceRun>> {
        sqlx::query_as::<_, MaintenanceRun>(
            "SELECT * FROM storage_maintenance_runs WHERE state = 'running' ORDER BY started_at",
        )
        .fetch_all(&self.db_pool)
        .await
        .context("failed to read running maintenance runs")
    }

    async fn recent_runs(&self, limit: i64) -> Result<Vec<MaintenanceRun>> {
        sqlx::query_as::<_, MaintenanceRun>(
            "SELECT * FROM storage_maintenance_runs ORDER BY started_at DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.db_pool)
        .await
        .context("failed to read maintenance history")
    }

    /// Returns the parsed probe and the raw status text (used for device paths).
    async fn probe_pool(&self) -> Result<Option<(PoolProbe, String)>> {
        let probe = tokio::task::spawn_blocking(|| -> Result<Option<(String, String)>> {
            let health = list_pool_health()?;
            let Some(pool) = health.first() else {
                return Ok(None);
            };
            let status = crate::infrastructure::command::run_command_output_no_sudo([
                "zpool",
                "status",
                "-P",
                pool.name.as_str(),
            ])
            .map_err(anyhow::Error::from)
            .with_context(|| format!("failed to read status for pool '{}'", pool.name))?;
            Ok(Some((pool.name.clone(), status)))
        })
        .await??;

        Ok(probe.map(|(pool, status)| (parse_zpool_status(&pool, &status), status)))
    }

    async fn smart_devices(&self, status_text: &str, refresh: bool) -> Result<Vec<SmartDevice>> {
        if !refresh {
            let cached = self.smart_cache.lock().await;
            if let Some((at, devices)) = cached.as_ref() {
                if at.elapsed() < SMART_CACHE {
                    return Ok(devices.clone());
                }
            }
        }

        let devices = devices_from_status(status_text);
        let mut collected = Vec::with_capacity(devices.len());
        for device in devices {
            let mut smartctl = tokio::process::Command::new("smartctl");
            smartctl.args(["-j", "-a"]).arg(&device).kill_on_drop(true);
            let probed = tokio::time::timeout(SMART_TIMEOUT, smartctl.output()).await;
            collected.push(match probed {
                Ok(Ok(output)) if output.status.success() => {
                    match serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                        Ok(value) => parse_smart(&device, &value),
                        Err(error) => {
                            unavailable(&device, format!("unreadable smartctl JSON: {error}"))
                        }
                    }
                }
                Ok(Ok(_)) => unavailable(&device, "smartctl reported an error".to_string()),
                Ok(Err(error)) => unavailable(&device, format!("smartctl is unavailable: {error}")),
                Err(_) => unavailable(&device, "smartctl timed out".to_string()),
            });
        }

        *self.smart_cache.lock().await = Some((Instant::now(), collected.clone()));
        Ok(collected)
    }
}

fn unavailable(device: &str, note: String) -> SmartDevice {
    SmartDevice {
        device: device.to_string(),
        model: None,
        serial: None,
        health: "unknown".to_string(),
        temperature_c: None,
        power_on_hours: None,
        wear_percent: None,
        media_errors: None,
        available: false,
        note: Some(note),
    }
}

fn parse_smart(device: &str, value: &serde_json::Value) -> SmartDevice {
    let model = value
        .get("model_name")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            value
                .get("device")
                .and_then(|device| device.get("name"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        });
    let nvme = value.get("nvme_smart_health_information_log");
    SmartDevice {
        device: device.to_string(),
        model,
        serial: value
            .get("serial_number")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        health: match value
            .pointer("/smart_status/passed")
            .and_then(serde_json::Value::as_bool)
        {
            Some(true) => "passed".to_string(),
            Some(false) => "failed".to_string(),
            None => "unknown".to_string(),
        },
        temperature_c: value
            .pointer("/temperature/current")
            .and_then(serde_json::Value::as_i64),
        power_on_hours: value
            .pointer("/power_on_time/hours")
            .and_then(serde_json::Value::as_u64),
        wear_percent: nvme
            .and_then(|log| log.get("percentage_used"))
            .and_then(serde_json::Value::as_i64),
        media_errors: nvme
            .and_then(|log| log.get("media_errors"))
            .and_then(serde_json::Value::as_u64),
        available: true,
        note: None,
    }
}

/// Pool member paths from `zpool status -P`. ZFS zvols are not physical disks
/// and are skipped so `smartctl` is never pointed at them.
fn devices_from_status(status: &str) -> Vec<String> {
    status
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|token| token.starts_with("/dev/") && !token.starts_with("/dev/zd"))
        .map(str::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .take(64)
        .collect()
}

pub fn parse_zpool_status(pool: &str, output: &str) -> PoolProbe {
    let mut state = "UNKNOWN".to_string();
    let mut scan_text: Option<String> = None;
    let mut in_scan = false;
    let mut vdev_errors = Vec::new();
    let mut in_config = false;
    let mut past_header = false;

    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("state:") {
            state = value.trim().to_ascii_uppercase();
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("scan:") {
            scan_text = Some(value.trim().to_string());
            in_scan = true;
            continue;
        }
        // `scan:` continues on indented lines; join them before parsing so the
        // percentage and kind seen on the second line are not lost.
        if in_scan && !trimmed.is_empty() && line.starts_with(char::is_whitespace) {
            if let Some(text) = scan_text.as_mut() {
                text.push(' ');
                text.push_str(trimmed);
            }
            continue;
        }
        in_scan = false;
        if trimmed.starts_with("config:") {
            in_config = true;
            continue;
        }
        if trimmed.starts_with("errors:") {
            in_config = false;
            continue;
        }
        if !in_config {
            continue;
        }
        let fields = trimmed.split_whitespace().collect::<Vec<_>>();
        if fields.first().is_some_and(|value| *value == "NAME") {
            past_header = true;
            continue;
        }
        if !past_header || fields.len() < 4 {
            continue;
        }
        let (read, write, checksum) = (
            fields[fields.len() - 3].parse::<u64>(),
            fields[fields.len() - 2].parse::<u64>(),
            fields[fields.len() - 1].parse::<u64>(),
        );
        if let (Ok(read), Ok(write), Ok(checksum)) = (read, write, checksum) {
            vdev_errors.push(VdevErrors {
                // The last four columns are STATE, READ, WRITE and CKSUM; the
                // vdev name is everything before them and may be a path.
                name: fields[..fields.len() - 4].join(" "),
                read,
                write,
                checksum,
            });
        }
    }

    let scan = scan_text;
    let scan_in_progress = scan
        .as_deref()
        .is_some_and(|text| text.contains("in progress"));
    let scan_kind = scan.as_deref().and_then(|text| {
        if text.contains("trim") {
            Some("trim".to_string())
        } else if text.contains("scrub") {
            Some("scrub".to_string())
        } else if text.contains("resilver") {
            Some("resilver".to_string())
        } else {
            None
        }
    });
    let scan_percent = scan.as_deref().and_then(scan_percent_of);

    PoolProbe {
        pool: pool.to_string(),
        state,
        scan,
        scan_kind,
        scan_in_progress,
        scan_percent,
        vdev_errors,
    }
}

fn scan_percent_of(scan: &str) -> Option<f64> {
    let (before, _) = scan.split_once("% done")?;
    before
        .split_whitespace()
        .next_back()?
        .trim_matches(|character: char| !character.is_ascii_digit() && character != '.')
        .parse::<f64>()
        .ok()
}

/// Off-hours window, including windows that wrap past midnight.
pub fn within_window(hour: u32, config: &MaintenanceConfig) -> bool {
    let (start, end) = (config.window_start_hour, config.window_end_hour);
    if start == end {
        return true;
    }
    if start < end {
        (start..end).contains(&hour)
    } else {
        hour >= start || hour < end
    }
}

/// Background sweep; a no-op while scheduling is disabled.
pub async fn run(state: AppState) {
    let service = MaintenanceService::new(state.db_pool.clone(), state.settings.clone());
    let mut interval = tokio::time::interval(SWEEP_INTERVAL);
    loop {
        interval.tick().await;
        if let Err(error) = service.tick().await {
            tracing::warn!(%error, "storage maintenance sweep failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRUBBING: &str = r#"
  pool: tank
 state: ONLINE
  scan: scrub in progress since Sun Nov  2 02:00:00 2026
	1.2T scanned at 500M/s, 30.0% done, 0 days 01:00:00 to go
config:

	NAME        STATE     READ WRITE CKSUM
	tank        ONLINE       0     0     0
	  mirror-0  ONLINE       1     2     3
	    /dev/sda ONLINE      0     0     0

errors: No known data errors
"#;

    #[test]
    fn parses_an_active_scrub_and_vdev_errors() {
        let probe = parse_zpool_status("tank", SCRUBBING);
        assert_eq!(probe.state, "ONLINE");
        assert!(probe.scan_in_progress);
        assert_eq!(probe.scan_kind.as_deref(), Some("scrub"));
        assert_eq!(probe.scan_percent, Some(30.0));
        assert!(probe
            .vdev_errors
            .iter()
            .any(|vdev| vdev.name == "mirror-0" && vdev.write == 2 && vdev.checksum == 3));
    }

    #[test]
    fn a_finished_scrub_is_not_in_progress() {
        let finished = SCRUBBING
            .replace("scrub in progress since Sun Nov  2 02:00:00 2026\n\t1.2T scanned at 500M/s, 30.0% done, 0 days 01:00:00 to go", "scrub repaired 0B in 00:12:34 with 0 errors on Sun Nov  2 02:12:34 2026");
        let probe = parse_zpool_status("tank", &finished);
        assert!(!probe.scan_in_progress);
        assert_eq!(probe.scan_kind.as_deref(), Some("scrub"));
        assert_eq!(probe.scan_percent, None);
    }

    #[test]
    fn trim_is_identified_separately_from_scrub() {
        let trim = SCRUBBING.replace("scrub in progress", "trim in progress");
        let probe = parse_zpool_status("tank", &trim);
        assert_eq!(probe.scan_kind.as_deref(), Some("trim"));
    }

    #[test]
    fn a_degraded_pool_reports_its_state() {
        let degraded = SCRUBBING.replace(" state: ONLINE", " state: DEGRADED");
        assert_eq!(parse_zpool_status("tank", &degraded).state, "DEGRADED");
    }

    #[test]
    fn only_physical_device_paths_are_probed() {
        let devices = devices_from_status(SCRUBBING);
        assert_eq!(devices, vec!["/dev/sda".to_string()]);
    }

    #[test]
    fn parses_smart_status_for_ata_and_nvme() {
        let ata = serde_json::json!({
            "model_name": "Samsung SSD 870",
            "serial_number": "S123",
            "smart_status": {"passed": true},
            "temperature": {"current": 32},
            "power_on_time": {"hours": 500}
        });
        let parsed = parse_smart("/dev/sda", &ata);
        assert_eq!(parsed.health, "passed");
        assert_eq!(parsed.temperature_c, Some(32));
        assert_eq!(parsed.power_on_hours, Some(500));

        let nvme = serde_json::json!({
            "smart_status": {"passed": false},
            "nvme_smart_health_information_log": {"percentage_used": 87, "media_errors": 2}
        });
        let parsed = parse_smart("/dev/nvme0n1", &nvme);
        assert_eq!(parsed.health, "failed");
        assert_eq!(parsed.wear_percent, Some(87));
        assert_eq!(parsed.media_errors, Some(2));
    }

    #[test]
    fn the_off_hours_window_wraps_midnight() {
        let config = MaintenanceConfig {
            enabled: true,
            scrub_interval_days: 30,
            trim_interval_days: 7,
            window_start_hour: 22,
            window_end_hour: 5,
        };
        assert!(within_window(23, &config));
        assert!(within_window(4, &config));
        assert!(!within_window(12, &config));
    }
}
