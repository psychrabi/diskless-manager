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
pub mod core;
pub mod platform;
mod services;
pub mod ssh_executor;
pub mod state;
pub mod utils;
pub mod validation;

pub mod api;

use log::info;

use tauri::Manager;

use state::AppState;

const DHCP_CONFIG_PATH: &str = "/etc/dhcp/dhcpd.conf";
const DHCP_CLIENTS_PATH: &str = "/etc/dhcp/clients.conf";

/// Resolve the canonical log file path used by both the Tauri GUI and CLI.
pub fn log_file_path() -> std::path::PathBuf {
    let base = dirs::config_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let dir = base.join("com.diskless.local");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("diskless-manager.log")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub async fn run() -> anyhow::Result<()> {
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
    let lifecycle_task = tokio::spawn(crate::application::client_lifecycle::run(state.clone()));

    // Dedicated PXE client enrollment listener. Bound to the LAN on its own
    // port (default 4237) so iPXE clients can register an unprovisioned
    // machine without exposing the management API on the network.
    let enroll_state = state.clone();
    let configured_enroll_addr = std::env::var("DISKLESS_ENROLL_ADDR").ok();
    let enroll_addr = crate::api::enroll::enroll_address(configured_enroll_addr.as_deref())?;
    let enroll_server = crate::api::enroll::EnrollServer::new(enroll_state, enroll_addr)
        .bind()
        .await?;
    let enroll_task = tokio::spawn(enroll_server.serve());

    // Start Tauri application
    let app = tauri::Builder::default()
        .plugin({
            let log_dir = log_file_path()
                .parent()
                .expect("log file must have a parent directory")
                .to_path_buf();
            tauri_plugin_log::Builder::default()
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Folder {
                        path: log_dir,
                        file_name: Some("diskless-manager.log".into()),
                    }),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Webview),
                ])
                .level(log::LevelFilter::Info)
                .build()
        })
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                window.hide().expect("Failed to hide window");
                api.prevent_close();
            }
        })
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let _ = app
                .get_webview_window("main")
                .expect("no main window")
                .set_focus();
        }))
        // All frontend communication now uses HTTP API instead of Tauri invoke calls
        .invoke_handler(tauri::generate_handler![])
        .setup(|app| {
            info!("Application startup");
            // config.json creation and migration is now handled inside AppState::new() during initialization.

            app.manage(state);

            // Setup system tray
            #[cfg(desktop)]
            {
                use tauri::{
                    menu::{Menu, MenuItem},
                    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
                };

                let show_i = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
                let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

                let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

                let _tray = TrayIconBuilder::new()
                    .icon(
                        app.default_window_icon()
                            .expect("Failed to get default window icon")
                            .clone(),
                    )
                    .menu(&menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(move |app, event| match event.id.as_ref() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    })
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            let app = tray.app_handle();
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                    })
                    .build(app)?;
            }

            info!("Tauri setup completed");
            Ok(())
        })
        .build(tauri::generate_context!())?;

    let mut api_shutdown_tx = Some(api_shutdown_tx);
    app.run(move |_app_handle, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            if let Some(sender) = api_shutdown_tx.take() {
                let _ = sender.send(());
            }
        }
    });

    lifecycle_task.abort();
    enroll_task.abort();
    api_task
        .await
        .map_err(|error| anyhow::anyhow!("API server task failed: {error}"))??;
    Ok(())
}
