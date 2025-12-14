mod app_config;
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
mod utils;
pub mod validation;
mod zfs;
use dirs;

mod commands;
mod core;
mod state;

use serde::Serialize;
use std::sync::{Arc, RwLock};
use sysinfo::System;

use tauri::Manager;

use crate::utils::{append_log, get_server_ip};

// Legacy constants for backward compatibility - prefer using AppConfig
const DHCP_CONFIG_PATH: &str = "/etc/dhcp/dhcpd.conf";
const DHCP_CLIENTS_PATH: &str = "/etc/dhcp/clients.conf";
pub const TFTP_AUTOEXEC_PATH: &str = "/srv/tftp/autoexec.ipxe";

// Cache for server info to avoid frequent system calls
use once_cell::sync::Lazy;

static SERVER_INFO_CACHE: Lazy<Arc<RwLock<ServerInfoCache>>> =
    Lazy::new(|| Arc::new(RwLock::new(ServerInfoCache::new())));

#[derive(Debug, Clone)]
struct ServerInfoCache {
    info: ServerInfo,
    last_updated: std::time::SystemTime,
    ttl: std::time::Duration,
}

impl ServerInfoCache {
    fn new() -> Self {
        ServerInfoCache {
            info: ServerInfo {
                os_name: None,
                kernel_version: None,
                host_name: None,
                total_memory_mb: 0,
                cpu_count: 0,
                server_ip: String::new(),
            },
            last_updated: std::time::SystemTime::UNIX_EPOCH,
            ttl: std::time::Duration::from_secs(60), // 60 second cache TTL
        }
    }

    fn is_fresh(&self) -> bool {
        self.last_updated
            .elapsed()
            .map(|elapsed| elapsed < self.ttl)
            .unwrap_or(false)
    }

    fn needs_refresh(&self) -> bool {
        !self.is_fresh()
    }
}

#[derive(Debug, Clone, Serialize)]
struct ServerInfo {
    os_name: Option<String>,
    kernel_version: Option<String>,
    host_name: Option<String>,
    total_memory_mb: u64,
    cpu_count: usize,
    server_ip: String,
}

#[tauri::command]
fn get_server_info() -> ServerInfo {
    // Try to get cached info first
    let cache = SERVER_INFO_CACHE.read().unwrap();

    if !cache.needs_refresh() {
        // Return cached data
        return cache.info.clone();
    }

    // Drop read lock before acquiring write lock
    drop(cache);

    let mut cache = SERVER_INFO_CACHE.write().unwrap();

    // Double-check after acquiring write lock
    if !cache.needs_refresh() {
        // Another thread may have updated the cache while we were waiting
        return cache.info.clone();
    }

    // Refresh system info
    let mut sys = System::new_all();
    sys.refresh_all();

    let new_info = ServerInfo {
        os_name: System::name(),
        kernel_version: System::kernel_version(),
        host_name: System::host_name(),
        total_memory_mb: sys.total_memory() / (1024 * 1024), // bytes -> MB
        cpu_count: sys.cpus().len(),
        server_ip: get_server_ip(),
    };

    // Update cache
    cache.info = new_info.clone();
    cache.last_updated = std::time::SystemTime::now();

    new_info
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize tracing with fixed log file
    let log_path = utils::log_file_path();
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .expect("Failed to open log file");

    let (non_blocking, _guard) = tracing_appender::non_blocking(file);

    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_max_level(tracing::Level::INFO)
        .init();

    tauri::Builder::default()
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                window.hide().unwrap();
                api.prevent_close();
            }
            _ => {}
        })
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let _ = app
                .get_webview_window("main")
                .expect("no main window")
                .set_focus();
        }))
        .invoke_handler(tauri::generate_handler![
            get_server_info,
            auth::login,
            auth::validate_auth_token,
            auth::update_admin_password,
            middleware::authenticate,
            client::get_clients,
            client::add_client,
            client::edit_client,
            client::delete_client,
            client::control_client,
            client::remote_client,
            client::reset_client,
            client::reset_client_to_clean,
            client::deprovision_client,
            client::deprovision_client_by_id,
            client::get_deprovision_status,
            client::get_client_overview,
            config::read_config,
            config::save_config,
            disks::list_zpools,
            disks::list_datasets,
            disks::create_zfs_dataset,
            disks::delete_zfs_dataset,
            disks::rename_zfs_dataset,
            service::get_services,
            service::control_service,
            service::install_service,
            service::get_service_config,
            service::save_service_config,
            service::check_package_status,
            service::install_packages,
            // service::restart_service,
            service::configure_dhcp_server,
            service::configure_tftp_server,
            service::configure_apache_server,
            service::configure_samba_server,
            utils::list_disks,
            utils::get_ram_usage,
            utils::clear_ram_cache,
            utils::get_service_logs,
            logs::get_logs,
            logs::clear_logs,
            license::activate_license,
            license::get_license_info,
            zfs::get_zfs_arcstat,
            zfs::get_images,
            zfs::create_zfs_pool,
            zfs::get_zpool_list,
            zfs::create_image,
            zfs::create_game_disk,
            zfs::delete_image,
            zfs::rename_image,
            zfs::delete_snapshot,
            zfs::zfs_pool_exists,
            zfs::create_snapshot,
            zfs::set_default_image,
            zfs::rollback_image_snapshot,
            zfs::get_default_image_overview,
            commands::services::list_services,
            commands::services::get_service_status,
            commands::services::start_service,
            commands::services::stop_service,
            commands::services::restart_service,
            commands::system::get_system_info,
            commands::system::get_server_status,
            commands::system::initialize_server,
            commands::system::check_dependencies,
            commands::system::get_settings,
            commands::system::save_settings,
        ])
        .setup(|app| {
            append_log("INFO", "Application startup");
            // Ensure config.json exists on first run
            if let Some(base) = dirs::config_dir() {
                let config_dir = base.join("com.diskless.local");
                let config_path = config_dir.join("config.json");
                if !config_path.exists() {
                    if let Err(e) = std::fs::create_dir_all(&config_dir) {
                        eprintln!("[WARN] Failed to create config directory: {}", e);
                    } else if let Err(e) = config::write_config(&types::AppConfig::default()) {
                        eprintln!("[WARN] Failed to create default config.json: {}", e);
                    } else {
                        println!("Created default config at {}", config_path.display());
                    }
                }
            }

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
                    .icon(app.default_window_icon().unwrap().clone())
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

            // app.get_webview_window("main").unwrap().open_devtools();
            if cfg!(debug_assertions) {
                if let Err(e) = app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                ) {
                    eprintln!("[WARN] Failed to initialize logging plugin: {}", e);
                }
            }
            append_log("INFO", "Tauri setup completed");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
