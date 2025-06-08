mod client;
mod config;
mod dhcp;
mod iscsi;
mod service;
mod utils;
mod zfs;
use once_cell::sync::Lazy;
use tauri::Manager;

const ZFS_POOL: &str = "diskless"; // Adjust to your ZFS pool name
const DHCP_CONFIG_PATH: &str = "/etc/dhcp/dhcpd.conf"; // Adjust as needed

pub static SERVER_IP: Lazy<String> = Lazy::new(|| {
    let ip = utils::get_server_ip();
    println!("Using server IP: {}", ip);
    ip
});

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Handle single instance logic here
            let _ = app
                .get_webview_window("main")
                .expect("no main window")
                .set_focus();
        }))
        .invoke_handler(tauri::generate_handler![
            client::get_clients,
            client::add_client,
            client::edit_client,
            client::delete_client,
            client::control_client,
            client::remote_client,
            client::reset_client,
            config::get_config,
            config::save_config,
            service::get_services,
            service::control_service,
            service::check_services,
            service::install_service,
            service::get_service_config,
            service::save_service_config,
            utils::list_disks,
            utils::get_ram_usage,
            utils::clear_ram_cache,
            utils::get_zfs_arcstat,
            zfs::get_masters,
            zfs::create_zfs_pool,
            zfs::get_zpool_stats,
            zfs::create_master,
            zfs::delete_master,
            zfs::create_snapshot,
            zfs::delete_snapshot,
            zfs::zfs_pool_exists,
            zfs::set_default_master
        ])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
