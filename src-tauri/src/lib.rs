mod client;
mod config;
mod dhcp;
mod iscsi;
mod service;
mod utils;
mod zfs;
use once_cell::sync::Lazy;

const ZFS_POOL: &str = "diskless"; // Adjust to your ZFS pool name
const DHCP_CONFIG_PATH: &str = "/etc/dhcp/dhcpd.conf"; // Adjust as needed
const CONFIG_PATH: &str = "./config.json"; // Adjust as needed

pub static SERVER_IP: Lazy<String> = Lazy::new(|| {
    let ip = utils::get_server_ip();
    println!("Using server IP: {}", ip);
    ip
});

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            client::get_clients,
            service::get_services,
            zfs::get_masters,
            client::control_client,
            client::remote_client,
            service::control_service,
            zfs::get_zpool_stats,
            service::get_service_config,
            client::add_client,
            client::edit_client,
            client::delete_client,
            zfs::create_master,
            zfs::delete_master,
            zfs::create_snapshot,
            zfs::delete_snapshot,
            config::get_config,
            config::save_config,
            utils::list_disks,
            zfs::create_zfs_pool,
            service::check_services,
            service::install_service,
            zfs::zfs_pool_exists,
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
