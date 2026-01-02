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

pub mod validation;
mod zfs;

mod cmd;
mod commands;
mod core;
mod services;
pub mod state;

use log::info;

use tauri::Manager;

use state::AppState;

// Legacy constants for backward compatibility - prefer using AppConfig
const DHCP_CONFIG_PATH: &str = "/etc/dhcp/dhcpd.conf";
const DHCP_CLIENTS_PATH: &str = "/etc/dhcp/clients.conf";
pub const TFTP_AUTOEXEC_PATH: &str = "/srv/tftp/autoexec.ipxe";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::default()
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("diskless-manager".into()),
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
                window.hide().unwrap();
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
        .invoke_handler(tauri::generate_handler![
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
            client::get_client_overview,
            config::read_config,
            config::save_config,
            disks::list_zpools,
            disks::list_datasets,
            disks::create_zfs_dataset,
            disks::delete_zfs_dataset,
            disks::rename_zfs_dataset,
            // Old service commands (to be replaced gradually)
            service::install_service,
            service::save_service_config,
            service::configure_dhcp_server,
            service::configure_tftp_server,
            service::configure_apache_server,
            service::configure_samba_server,
            service::configure_nfs_server,
            // New services architecture commands
            commands::services::list_services,
            commands::services::get_service_status,
            commands::services::get_service_config,
            commands::services::start_service,
            commands::services::stop_service,
            commands::services::restart_service,
            commands::services::start_all_services,
            commands::services::stop_all_services,
            commands::services::restart_all_services,
            commands::services::configure_service,
            cmd::list_disks,
            cmd::get_ram_usage,
            cmd::clear_ram_cache,
            cmd::get_service_logs,
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
            // System commands
            commands::system::get_system_info,
            commands::system::get_server_status,
            commands::system::initialize_server,
            commands::system::check_dependencies,
            commands::system::get_settings,
            commands::system::save_settings,
            // Client commands
            commands::clients::list_clients,
            commands::clients::get_client,
            commands::clients::add_client_command,
            commands::clients::update_client_command,
            commands::clients::delete_client_command,
            commands::clients::get_client_boot_history,
            // Image commands
            commands::images::list_images,
            commands::images::get_image,
            commands::images::create_image_command,
            commands::images::import_image,
            commands::images::delete_image_command,
            commands::images::clone_image,
            commands::images::create_snapshot_command,
            commands::images::get_image_info,
            commands::images::resize_image,
            commands::images::verify_image,
            // Version commands
            commands::versions::list_versions,
            commands::versions::get_version_history,
        ])
        .setup(|app| {
            info!("Application startup");
            // config.json creation and migration is now handled inside AppState::new() during initialization.

            tauri::async_runtime::block_on(async {
                match AppState::new().await {
                    Ok(state) => {
                        app.manage(state);
                        tracing::info!("Application state initialized");
                        Ok::<(), Box<dyn std::error::Error>>(())
                    }
                    Err(e) => {
                        let error_msg = format!("Failed to initialize state: {}", e);
                        tracing::error!("{}", error_msg);
                        Err(error_msg.into())
                    }
                }
            })?;

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

            info!("Tauri setup completed");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
