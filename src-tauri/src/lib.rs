mod client;
mod config;
mod dhcp;
mod iscsi;
mod service;
mod utils;
mod zfs;
use once_cell::sync::Lazy;
use serde::Serialize;
use sysinfo::System;
use tauri::Manager;
use tauri::tray::TrayIconBuilder;


const ZFS_POOL: &str = "diskless"; // Adjust to your ZFS pool name
const DHCP_CONFIG_PATH: &str = "/etc/dhcp/dhcpd.conf"; // Adjust as needed

pub static SERVER_IP: Lazy<String> = Lazy::new(|| {
    let ip = utils::get_server_ip();
    println!("Using server IP: {}", ip);
    ip
});

#[derive(Debug, Serialize)]
struct ServerInfo {
  os_name: Option<String>,
  kernel_version: Option<String>,
  host_name: Option<String>,
  total_memory_mb: u64,
  cpu_count: usize,
}

#[tauri::command]
fn get_server_info() -> ServerInfo {
  let mut sys = System::new_all();
  sys.refresh_all();
  ServerInfo {
    os_name: System::name(),
    kernel_version: System::kernel_version(),
    host_name: System::host_name(),
    total_memory_mb: sys.total_memory() / 1024, // KiB -> GiB
    cpu_count: sys.cpus().len(),    
  }
}

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
            get_server_info,
            client::get_clients,
            client::add_client,
            client::edit_client,
            client::delete_client,
            client::control_client,
            client::remote_client,
            client::reset_client,
            client::deprovision_client,
            client::deprovision_client_by_id,
            client::get_deprovision_status,
            config::read_config,
            config::save_config,
            service::get_services,
            service::control_service,
            service::check_services,
            service::install_service,
            service::get_service_config,
            service::save_service_config,
            service::check_package_status,
            service::install_packages,
            service::restart_service,
            service::configure_dhcp_server,
            service::configure_tftp_server,
            service::configure_apache_server,
            service::configure_samba_server,            
            utils::list_disks,
            utils::get_ram_usage,
            utils::clear_ram_cache,
            zfs::get_zfs_arcstat,
            zfs::get_masters,
            zfs::create_zfs_pool,
            zfs::get_zpool_stats,
            zfs::create_master,
            zfs::create_fileio_master,
            zfs::delete_master,
            zfs::rename_master,
            zfs::create_snapshot,
            zfs::delete_snapshot,
            zfs::zfs_pool_exists,
            zfs::set_default_master,
            zfs::rollback_master_snapshot,
            zfs::get_client_overview,
            zfs::get_master_image_overview
        ])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
let _tray = TrayIconBuilder::new()
  .icon(app.default_window_icon().unwrap().clone())
  .build(app)?;        
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
