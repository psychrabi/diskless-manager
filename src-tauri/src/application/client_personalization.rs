//! Per-client offline Windows personalization.
//!
//! A client may carry a personalization plan that is applied to its own
//! writeback clone while the clone is still offline, so the image's computer
//! name, static network configuration and inherited drive letters do not leak
//! onto hardware they do not describe.
//!
//! The stored plan is configuration, never proof of application: the row also
//! records when a plan was last applied and why the last attempt failed. A plan
//! that cannot be applied never blocks boot; the client simply boots with the
//! image identity and the failure is reported.
//!
//! Nothing here is applied to images. Only a job-owned client clone is edited,
//! and only while it has no iSCSI session.

use crate::infrastructure::{
    iscsi::{ChapCredentials, IscsiProvisioner, IscsiTargetSpec},
    pxe::{
        windows_image_preparation::WindowsImagePreparationRequest,
        windows_iscsi_servicing::{WindowsIscsiConnection, SERVICING_IQN_PREFIX},
        windows_personalization::WindowsPersonalizationPlan,
        RemoteWindowsServicer, RemoteWindowsServicingRequest, WindowsBootArmConfig,
        WindowsDriverInstallStrategy,
    },
};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::{path::PathBuf, sync::Arc};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClientPersonalization {
    pub client_id: String,
    pub plan: WindowsPersonalizationPlan,
    pub updated_at: i64,
    pub last_applied_at: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(sqlx::FromRow)]
struct PersonalizationRow {
    client_id: String,
    computer_name: Option<String>,
    convert_static_ipv4_to_dhcp: i64,
    clear_stale_drive_letters: i64,
    updated_at: i64,
    last_applied_at: Option<i64>,
    last_error: Option<String>,
}

impl From<PersonalizationRow> for ClientPersonalization {
    fn from(row: PersonalizationRow) -> Self {
        Self {
            client_id: row.client_id,
            plan: WindowsPersonalizationPlan {
                computer_name: row.computer_name,
                convert_static_ipv4_to_dhcp: row.convert_static_ipv4_to_dhcp != 0,
                clear_stale_drive_letters: row.clear_stale_drive_letters != 0,
            },
            updated_at: row.updated_at,
            last_applied_at: row.last_applied_at,
            last_error: row.last_error,
        }
    }
}

const COLUMNS: &str =
    "client_id, computer_name, convert_static_ipv4_to_dhcp, clear_stale_drive_letters, updated_at, last_applied_at, last_error";

pub async fn load(
    pool: &sqlx::SqlitePool,
    client_id: &str,
) -> Result<Option<ClientPersonalization>> {
    let row = sqlx::query_as::<_, PersonalizationRow>(&format!(
        "SELECT {COLUMNS} FROM client_windows_personalization WHERE client_id = ?"
    ))
    .bind(client_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(ClientPersonalization::from))
}

/// Store a plan. An empty plan clears the row instead of storing a no-op,
/// so "no plan" has exactly one representation.
pub async fn save(
    pool: &sqlx::SqlitePool,
    client_id: &str,
    plan: &WindowsPersonalizationPlan,
    now: i64,
) -> Result<()> {
    if plan.is_empty() {
        return clear(pool, client_id).await;
    }
    plan.validate()?;
    let name = plan.normalized_computer_name()?;
    sqlx::query(
        "INSERT INTO client_windows_personalization(client_id, computer_name, convert_static_ipv4_to_dhcp, clear_stale_drive_letters, updated_at, last_applied_at, last_error) \
         VALUES (?, ?, ?, ?, ?, NULL, NULL) \
         ON CONFLICT(client_id) DO UPDATE SET computer_name = excluded.computer_name, convert_static_ipv4_to_dhcp = excluded.convert_static_ipv4_to_dhcp, clear_stale_drive_letters = excluded.clear_stale_drive_letters, updated_at = excluded.updated_at, last_applied_at = NULL, last_error = NULL",
    )
    .bind(client_id)
    .bind(name)
    .bind(i64::from(plan.convert_static_ipv4_to_dhcp))
    .bind(i64::from(plan.clear_stale_drive_letters))
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn clear(pool: &sqlx::SqlitePool, client_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM client_windows_personalization WHERE client_id = ?")
        .bind(client_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Record a successful application, as observed on the clone itself.
pub async fn record_applied(pool: &sqlx::SqlitePool, client_id: &str, now: i64) -> Result<()> {
    sqlx::query(
        "UPDATE client_windows_personalization SET last_applied_at = ?, last_error = NULL WHERE client_id = ?",
    )
    .bind(now)
    .bind(client_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record a failed application. The plan is kept: an operator fixes the worker
/// or the plan, and the next build retries.
pub async fn record_failure(pool: &sqlx::SqlitePool, client_id: &str, error: &str) -> Result<()> {
    sqlx::query(
        "UPDATE client_windows_personalization SET last_error = ?, last_applied_at = NULL WHERE client_id = ?",
    )
    .bind(error)
    .bind(client_id)
    .execute(pool)
    .await
    .with_context(|| format!("failed to record a personalization failure for '{client_id}'"))?;
    Ok(())
}

/// Fingerprint contribution used by spare validation.
///
/// A stored plan changes what a ready spare contains, so changing it must
/// invalidate spares built under the previous plan.
pub async fn fingerprint_component(
    pool: &sqlx::SqlitePool,
    client_id: &str,
) -> Result<Option<String>> {
    load(pool, client_id)
        .await?
        .map(|stored| serde_json::to_string(&stored.plan).map_err(anyhow::Error::from))
        .transpose()
}

/// Personalize one offline clone through the trusted Windows worker.
///
/// The clone is exported only to the worker on a job-owned servicing target
/// with generated CHAP credentials, and the target is removed on every path,
/// including a worker failure. The caller must own the clone and must have
/// confirmed that no session or mount references it.
pub async fn apply_to_clone(
    iscsi: Arc<dyn IscsiProvisioner>,
    worker: &crate::core::config::WindowsServicingConfig,
    dataset: &str,
    plan: &WindowsPersonalizationPlan,
    expected_size_bytes: u64,
) -> Result<()> {
    plan.validate()?;
    anyhow::ensure!(
        worker.is_configured(),
        "no Windows servicing worker is configured"
    );
    anyhow::ensure!(expected_size_bytes > 0, "clone size is unknown");
    anyhow::ensure!(
        !dataset.trim().is_empty() && !dataset.contains('@') && !dataset.contains(".."),
        "clone dataset name is invalid"
    );

    let job = uuid::Uuid::new_v4();
    let target = format!("{SERVICING_IQN_PREFIX}{job}");
    let backstore = format!("service_{}", job.simple());
    let chap = ChapCredentials::generate(&format!("servicing-{job}"))?;
    let connection = WindowsIscsiConnection {
        portal: worker.iscsi_portal.clone(),
        target_iqn: target.clone(),
        chap_username: chap.username.clone(),
        chap_password: chap.password.clone(),
        expected_size_bytes,
    };
    connection.validate()?;
    let mut spec = IscsiTargetSpec::new(&target, &backstore, format!("/dev/zvol/{dataset}"), 0);
    spec.chap = Some(chap);

    {
        let iscsi = iscsi.clone();
        let target = target.clone();
        let spec = spec.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            if iscsi.target_exists(&target)? {
                bail!("personalization target already exists");
            }
            iscsi.create_target_transaction(&spec)?;
            Ok(())
        })
        .await
        .context("personalization target setup task failed")??;
    }

    let preparation = WindowsImagePreparationRequest {
        image_path: PathBuf::new(),
        driver_root: PathBuf::new(),
        mount_root: None,
        image_index: 1,
        recursive: true,
        commit: true,
        boot_arm: WindowsBootArmConfig::default(),
        iscsi: Some(connection),
        reporter: None,
        driver_install_strategy: WindowsDriverInstallStrategy::OfflineDism,
        personalization: Some(plan.clone()),
    };
    let serviced = RemoteWindowsServicer::new()
        .prepare(RemoteWindowsServicingRequest {
            host: worker.worker_host.clone(),
            username: worker.worker_username.clone(),
            // The worker profile authenticates with the service account's SSH key.
            password: None,
            executable_path: Some(worker.helper_executable_path.clone()),
            preparation,
        })
        .await;

    let teardown = {
        let iscsi = iscsi.clone();
        let target = target.clone();
        let backstore = backstore.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            iscsi.remove_target_with_backstores(&target, std::slice::from_ref(&backstore))?;
            iscsi.remove_target(&target)?;
            Ok(())
        })
        .await
        .context("personalization target teardown task failed")?
    };

    let result = match (serviced, teardown) {
        (Ok(result), Ok(())) => Ok(result),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(teardown_error)) => Err(error.context(format!(
            "additionally failed to remove personalization target '{target}': {teardown_error:#}"
        ))),
    }?;
    let outcome = result
        .personalization
        .context("worker did not report a personalization outcome")?;
    anyhow::ensure!(
        outcome.applied,
        "worker did not apply the personalization plan"
    );
    for warning in &outcome.warnings {
        tracing::warn!(dataset, warning, "personalization warning");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worker_profile() -> crate::core::config::WindowsServicingConfig {
        crate::core::config::WindowsServicingConfig {
            worker_host: "192.0.2.50".into(),
            worker_username: "Administrator".into(),
            helper_executable_path: "C:\\helper.exe".into(),
            iscsi_portal: "192.0.2.1".into(),
            report_server_url: "http://192.0.2.1:8080".into(),
            driver_install_strategy: "offline_dism".into(),
        }
    }

    #[tokio::test]
    async fn personalization_refuses_unconfigured_workers_and_bad_clones() {
        let iscsi: Arc<dyn IscsiProvisioner> = Arc::new(
            crate::infrastructure::iscsi::SafeIscsiProvisioner::with_backend(
                crate::core::config::IscsiBackend::default(),
            ),
        );
        let plan = WindowsPersonalizationPlan {
            computer_name: Some("pc-01".into()),
            ..Default::default()
        };
        let unconfigured = crate::core::config::WindowsServicingConfig::default();
        let error = apply_to_clone(
            iscsi.clone(),
            &unconfigured,
            "pool/clients/pc001-spare-abc",
            &plan,
            1024,
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("no Windows servicing worker"));

        let error = apply_to_clone(
            iscsi.clone(),
            &worker_profile(),
            "pool/clients/pc001@installed",
            &plan,
            1024,
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("clone dataset name is invalid"));

        let error = apply_to_clone(
            iscsi.clone(),
            &worker_profile(),
            "pool/clients/pc001-spare-abc",
            &plan,
            0,
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("clone size is unknown"));

        let error = apply_to_clone(
            iscsi,
            &worker_profile(),
            "pool/clients/pc001-spare-abc",
            &WindowsPersonalizationPlan::default(),
            1024,
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("no change"));
    }

    async fn pool() -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, enabled, created_at, updated_at) VALUES ('client', 'pc001', '00:11:22:33:44:55', '192.0.2.10', 'pool/master', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn saving_normalizes_and_an_empty_plan_clears() {
        let pool = pool().await;
        let plan = WindowsPersonalizationPlan {
            computer_name: Some(" pc-lab-07 ".into()),
            convert_static_ipv4_to_dhcp: true,
            clear_stale_drive_letters: false,
        };
        save(&pool, "client", &plan, 10).await.unwrap();
        let stored = load(&pool, "client").await.unwrap().unwrap();
        assert_eq!(stored.plan.computer_name.as_deref(), Some("PC-LAB-07"));
        assert!(stored.plan.convert_static_ipv4_to_dhcp);
        assert!(!stored.plan.clear_stale_drive_letters);
        assert_eq!(stored.updated_at, 10);

        save(&pool, "client", &WindowsPersonalizationPlan::default(), 20)
            .await
            .unwrap();
        assert!(load(&pool, "client").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn invalid_names_are_rejected_before_storage() {
        let pool = pool().await;
        let plan = WindowsPersonalizationPlan {
            computer_name: Some("pc_lab".into()),
            ..Default::default()
        };
        assert!(save(&pool, "client", &plan, 1).await.is_err());
        assert!(load(&pool, "client").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn outcomes_are_recorded_and_a_new_plan_resets_them() {
        let pool = pool().await;
        let plan = WindowsPersonalizationPlan {
            computer_name: Some("pc-01".into()),
            ..Default::default()
        };
        save(&pool, "client", &plan, 10).await.unwrap();
        record_failure(&pool, "client", "worker unreachable")
            .await
            .unwrap();
        let stored = load(&pool, "client").await.unwrap().unwrap();
        assert_eq!(stored.last_error.as_deref(), Some("worker unreachable"));
        assert_eq!(stored.last_applied_at, None);

        record_applied(&pool, "client", 12).await.unwrap();
        let stored = load(&pool, "client").await.unwrap().unwrap();
        assert_eq!(stored.last_applied_at, Some(12));
        assert_eq!(stored.last_error, None);

        // Editing the plan clears the previous outcome: it is unproven again.
        save(&pool, "client", &plan, 13).await.unwrap();
        let stored = load(&pool, "client").await.unwrap().unwrap();
        assert_eq!(stored.last_applied_at, None);
        assert_eq!(stored.last_error, None);
    }

    #[tokio::test]
    async fn fingerprint_tracks_the_plan_and_is_none_without_one() {
        let pool = pool().await;
        assert_eq!(fingerprint_component(&pool, "client").await.unwrap(), None);
        let plan = WindowsPersonalizationPlan {
            computer_name: Some("pc-01".into()),
            ..Default::default()
        };
        save(&pool, "client", &plan, 1).await.unwrap();
        let first = fingerprint_component(&pool, "client").await.unwrap();
        assert!(first.is_some());

        let renamed = WindowsPersonalizationPlan {
            computer_name: Some("pc-02".into()),
            ..Default::default()
        };
        save(&pool, "client", &renamed, 2).await.unwrap();
        assert_ne!(first, fingerprint_component(&pool, "client").await.unwrap());

        clear(&pool, "client").await.unwrap();
        assert_eq!(fingerprint_component(&pool, "client").await.unwrap(), None);
    }
}
