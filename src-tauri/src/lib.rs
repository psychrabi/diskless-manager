mod auth;
pub mod client;
mod config;
mod dhcp;
mod disks;
mod error;
mod iscsi;
mod license;
mod logs;
mod middleware;
mod service;
pub mod types;

pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod persistence;

pub mod audit_logger;
pub mod command_builder;
pub mod control_handler;
pub mod error_logger;
pub mod os_detector;
pub mod remote_desktop_launcher;
pub mod ssh_executor;
pub mod validation;
mod zfs;

mod cmd;
mod commands;
pub mod core;
mod services;
pub mod state;
pub mod utils;

pub mod api;

use log::info;

use tauri::Manager;

use state::AppState;

// Legacy constants for backward compatibility - prefer using AppConfig
const DHCP_CONFIG_PATH: &str = "/etc/dhcp/dhcpd.conf";
const DHCP_CLIENTS_PATH: &str = "/etc/dhcp/clients.conf";
pub const TFTP_AUTOEXEC_PATH: &str = "/srv/tftp/autoexec.ipxe";

#[expect(dead_code, reason = "Reserved for future init use - currently unused")]
fn setup_logging() -> Result<(), Box<dyn std::error::Error>> {
    // Get the log file path
    let mut log_dir = dirs::config_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    log_dir.push("com.diskless.local");
    let _ = std::fs::create_dir_all(&log_dir);

    // Create file appender
    let file_appender = tracing_appender::rolling::never(&log_dir, "diskless-manager.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // Set up tracing subscriber with both file and stdout
    use tracing_subscriber::fmt::format::FmtSpan;
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_span_events(FmtSpan::CLOSE)
        .with_level(true)
        .with_target(true)
        .with_thread_ids(true)
        .init();

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub async fn run() {
    // Initialize application state
    let state = AppState::new()
        .await
        .expect("Failed to initialize AppState");

    // Start Axum API server in a separate task
    let api_state = state.clone();
    tokio::spawn(async move {
        let addr = "127.0.0.1:8080"
            .parse()
            .expect("Invalid API server address");
        let api_server = crate::api::server::ApiServer::new(api_state, addr);

        if let Err(e) = api_server.start().await {
            tracing::error!("API server error: {}", e);
        }
    });

    // Start Tauri application
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::default()
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("diskless-manager.log".into()),
                    }),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Webview),
                ])
                .level(log::LevelFilter::Info)
                .build(),
        )
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
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
