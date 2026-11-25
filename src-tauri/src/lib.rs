mod auth;
pub mod client;
mod config;
mod disks;
mod error;
mod license;
pub mod metrics;
pub mod types;

pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod persistence;

pub mod audit_logger;
mod commands;
pub use commands::system::authorize_from_terminal;
pub mod core;
pub mod platform;
mod services;
pub mod ssh_executor;
pub mod state;
pub mod utils;
pub mod validation;

pub mod api;

use log::{info, warn};

use state::AppState;
use std::time::Duration;

const DHCP_CONFIG_PATH: &str = "/etc/dhcp/dhcpd.conf";
const DHCP_CLIENTS_PATH: &str = "/etc/dhcp/clients.conf";

/// Resolve the canonical log file path used by both the Tauri GUI and CLI.
pub fn log_file_path() -> std::path::PathBuf {
    let base = dirs::config_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let dir = base.join("com.diskless.local");
    let _ = std::fs::create_dir_all(&dir);
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    dir.join(format!("diskless-manager.log.{today}"))
}

pub async fn run() -> anyhow::Result<()> {
    // Only one server may own the API port and kernel state. The guard
    // lives until shutdown; a second instance exits with a clear error.
    let _instance_lock = lock_single_instance()?;
    // Initialize application state
    let state = AppState::new().await?;

    // Bind before opening the UI so startup cannot appear successful when the
    // configured API address is unavailable.
    let api_state = state.clone();
    let configured_addr = std::env::var("DISKLESS_API_ADDR").ok();
    let addr = crate::api::server::api_address(configured_addr.as_deref())?;
    let api_server = crate::api::server::ApiServer::new(api_state, addr)
        .bind()
        .await?;
    let (api_shutdown_tx, api_shutdown_rx) = tokio::sync::oneshot::channel();
    let api_task = tokio::spawn(api_server.serve_with_shutdown(api_shutdown_rx));
    let (background_shutdown_tx, background_shutdown_rx) = tokio::sync::watch::channel(false);
    let lifecycle_task = tokio::spawn(crate::application::client_lifecycle::run(
        state.clone(),
        background_shutdown_rx.clone(),
    ));
    // Prewarms replacement writebacks while clients are offline; disabled by
    // default in settings, and it only ever creates job-owned spare clones.
    let spare_task = tokio::spawn(crate::application::writeback_spare_service::run(
        state.clone(),
        background_shutdown_rx.clone(),
    ));
    // Scheduled scrub/TRIM, disabled by default. Manual runs are on demand.
    let maintenance_task = tokio::spawn(crate::application::maintenance_service::run(
        state.clone(),
        background_shutdown_rx.clone(),
    ));

    // Dedicated PXE client enrollment listener. Bound to the LAN on its own
    // port (default 4237) so iPXE clients can register an unprovisioned
    // machine without exposing the management API on the network.
    let enroll_state = state.clone();
    let configured_enroll_addr = std::env::var("DISKLESS_ENROLL_ADDR").ok();
    let enroll_addr = crate::api::enroll::enroll_address(configured_enroll_addr.as_deref())?;
    let enroll_server = crate::api::enroll::EnrollServer::new(enroll_state, enroll_addr)
        .bind()
        .await?;
    let enroll_task = tokio::spawn(enroll_server.serve(background_shutdown_rx.clone()));

    info!("Diskless manager running; press Ctrl-C to stop");
    shutdown_signal().await;
    info!("Shutdown signal received");
    let _ = api_shutdown_tx.send(());
    let _ = background_shutdown_tx.send(true);

    // Let in-flight sweeps finish their current step before process exit.
    // Recovery records remain the fallback if the grace period expires.
    let background_tasks = async {
        let _ = lifecycle_task.await;
        let _ = spare_task.await;
        let _ = maintenance_task.await;
        let _ = enroll_task.await;
    };
    if tokio::time::timeout(Duration::from_secs(30), background_tasks)
        .await
        .is_err()
    {
        warn!("background tasks did not stop within 30s; exiting");
    }

    // Open WebSocket dashboards can delay hyper's graceful shutdown. Bound the
    // wait so a slow client cannot keep the process alive indefinitely.
    match tokio::time::timeout(Duration::from_secs(30), api_task).await {
        Ok(result) => {
            result.map_err(|error| anyhow::anyhow!("API server task failed: {error}"))??;
        }
        Err(_) => warn!("API server did not stop within 30s; exiting"),
    }
    Ok(())
}

/// Exclusive instance lock, held until shutdown.
fn lock_single_instance() -> anyhow::Result<std::fs::File> {
    use fs2::FileExt;
    // /run is root-only; fall back to the temp dir for unprivileged runs.
    let path = std::path::PathBuf::from("/run/diskless-manager.lock");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .or_else(|_| {
            std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(std::env::temp_dir().join("diskless-manager.lock"))
        })
        .map_err(|error| anyhow::anyhow!("cannot create instance lock: {error}"))?;
    file.try_lock_exclusive()
        .map_err(|_| anyhow::anyhow!("another instance is already running"))?;
    Ok(file)
}

/// Wait for Ctrl-C (plus SIGTERM on Unix) before graceful shutdown.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("SIGTERM handler must install");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = terminate.recv() => {},
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
