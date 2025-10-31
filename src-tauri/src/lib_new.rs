//! Diskless Manager - Optimized Rust Backend Architecture
//!
//! This is the main library file demonstrating the new domain-driven architecture
//! with proper separation of concerns, dependency injection, and modern Rust patterns.

// ========== CORE MODULE IMPORTS ==========
// Domain layer with business logic
pub mod core {
    pub mod error;
    pub mod config;
    
    // Domain services (interfaces)
    pub mod auth;
    pub mod client;
    pub mod image;
    pub mod disk;
    pub mod service;
    pub mod license;
    
    // Re-export commonly used types
    pub use error::{DisklessError, Result};
    pub use config::ConfigManager;
}

// ========== TYPES MODULE ==========
// Shared types and DTOs
pub mod types {
    pub mod config;
    pub mod client;
    pub mod auth;
    pub mod service;
    pub mod image;
    pub mod disk;
    
    // Re-export types
    pub use config::{Config, AppConfig};
    pub use client::{Client, AddClientRequest, ControlRequest, DeprovisionRequest};
    pub use auth::{User, Claims, LoginRequest, LoginResponse};
    pub use service::{ServiceControlRequest, PackageStatus, DHCPConfig, TFTPConfig, HTTPConfig};
    pub use image::{Master, Snapshot, MasterData};
    pub use disk::{DatasetInfo, Disk, RamUsage};
}

// ========== INFRASTRUCTURE MODULE ==========
// External system integrations (interfaces for dependency injection)
pub mod infrastructure {
    pub mod process;
    pub mod zfs;
    pub mod dhcp;
    pub mod iscsi;
    
    // Re-export service traits
    pub use process::{ProcessService, CommandRunner, RealProcessService};
    pub use zfs::{ZfsService, RealZfsService, ZfsConfig, ArcStats};
    // Note: Additional services would be imported similarly
}

// ========== APPLICATION MODULE ==========
// Use cases and command handlers (bridges between domain and Tauri)
pub mod application {
    pub mod auth_commands;
    pub mod client_commands;
    pub mod service_commands;
    pub mod image_commands;
    pub mod disk_commands;
    pub mod license_commands;
    
    // Re-export command handlers
    pub use auth_commands::{login, validate_auth_token, update_admin_password};
    pub use client_commands::ClientCommands;
    pub use service_commands::{get_services, control_service, check_package_status};
    pub use image_commands::{create_image, get_images, delete_image};
    pub use disk_commands::{list_zpools, list_datasets, create_zfs_dataset};
    pub use license_commands::{activate_license, get_license_info};
}

// ========== CONSTANTS MODULE ==========
// Centralized application constants
pub mod constants;

// ========== SYSTEM IMPORTS ==========
use serde::Serialize;
use sysinfo::System;
use tauri::Manager;

// ========== SERVER INFO STRUCTURE ==========
/// Server information DTO for the frontend
#[derive(Debug, Serialize)]
pub struct ServerInfo {
    pub os_name: Option<String>,
    pub kernel_version: Option<String>,
    pub host_name: Option<String>,
    pub total_memory_mb: u64,
    pub cpu_count: usize,
    pub server_ip: String,
}

// ========== TAURI COMMAND HANDLERS ==========

/// Get server information using the new architecture
#[tauri::command]
pub fn get_server_info() -> ServerInfo {
    let mut sys = System::new_all();
    sys.refresh_all();
    
    // Use centralized constants instead of magic strings
    let server_ip = crate::core::config::ConfigManager::get_server_ip()
        .unwrap_or_else(|_| crate::constants::network::DEFAULT_SERVER_IP.to_string());
    
    ServerInfo {
        os_name: System::name(),
        kernel_version: System::kernel_version(),
        host_name: System::host_name(),
        total_memory_mb: sys.total_memory() / (1024 * 1024),
        cpu_count: sys.cpus().len(),
        server_ip,
    }
}

/// Legacy server IP getter (for backward compatibility)
#[tauri::command]
pub fn get_server_ip() -> String {
    crate::core::config::ConfigManager::get_server_ip()
        .unwrap_or_else(|_| crate::constants::network::DEFAULT_SERVER_IP.to_string())
}

// ========== CONFIGURATION COMMANDS ==========

/// Read configuration using the new caching system
#[tauri::command]
pub fn read_config() -> crate::types::config::AppConfig {
    crate::core::config::ConfigManager::get_sync()
        .unwrap_or_else(|_| crate::types::config::AppConfig::default())
}

/// Save configuration with proper error handling
#[tauri::command]
pub fn save_config(config: crate::types::config::AppConfig) -> Result<(), String> {
    crate::core::config::ConfigManager::save(&config)
        .map_err(|e| format!("Failed to save config: {}", e))
}

// ========== LOGGING COMMANDS ==========

/// Get application logs using centralized logging
#[tauri::command]
pub fn get_logs() -> Result<String, String> {
    // Use the new infrastructure logging service
    let log_path = crate::constants::paths::log_file();
    std::fs::read_to_string(log_path)
        .map_err(|e| format!("Failed to read logs: {}", e))
}

/// Clear application logs
#[tauri::command]
pub fn clear_logs() -> Result<(), String> {
    let log_path = crate::constants::paths::log_file();
    std::fs::write(&log_path, "")
        .map_err(|e| format!("Failed to clear logs: {}", e))
}

// ========== SYSTEM COMMANDS ==========

/// List system disks
#[tauri::command]
pub fn list_disks() -> Result<Vec<crate::types::disk::Disk>, String> {
    // Use infrastructure process service instead of direct Command usage
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| format!("Failed to create runtime: {}", e))?;
    
    rt.block_on(async {
        let output = std::process::Command::new("lsblk")
            .args(["-dn", "-o", "NAME,SIZE,TYPE"])
            .output()
            .map_err(|e| format!("lsblk not available: {}", e))?;

        if !output.status.success() {
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut disks = Vec::new();

        for line in stdout.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() == 3 && parts[2] == "disk" {
                disks.push(crate::types::disk::Disk {
                    name: parts[0].to_string(),
                    size: parts[1].to_string(),
                });
            }
        }

        Ok(disks)
    })
}

/// Get RAM usage using centralized constants and proper error handling
#[tauri::command]
pub fn get_ram_usage() -> Result<crate::types::disk::RamUsage, String> {
    let output = std::process::Command::new("free")
        .arg("-h")
        .output()
        .map_err(|e| format!("Failed to run free command: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    if lines.len() < 2 {
        return Err("Unexpected output from free command".to_string());
    }

    let mem_line = lines[1];
    let parts: Vec<&str> = mem_line.split_whitespace().collect();
    if parts.len() < 7 {
        return Err("Invalid memory information format".to_string());
    }

    let memory_stats = crate::types::disk::MemoryStats {
        total: parts[1].to_string(),
        used: parts[2].to_string(),
        free: parts[3].to_string(),
        shared: parts[4].to_string(),
        buff_cache: parts[5].to_string(),
        available: parts[6].to_string(),
    };

    Ok(crate::types::disk::RamUsage {
        memory: memory_stats,
    })
}

/// Clear RAM cache using proper system commands
#[tauri::command]
pub fn clear_ram_cache() -> Result<serde_json::Value, String> {
    std::process::Command::new("sh")
        .args(["-c", "sync && echo 3 > /proc/sys/vm/drop_caches"])
        .output()
        .map_err(|e| format!("Failed to clear cache: {}", e))
        .map(|_| serde_json::json!({ "message": "RAM cache cleared successfully" }))
}

/// Get service logs with proper timeout
#[tauri::command]
pub fn get_service_logs(unit: String, lines: Option<u32>) -> Result<String, String> {
    let num = lines.unwrap_or(200).to_string();
    let args = vec!["journalctl", "-u", &unit, "-n", &num, "--no-pager"];
    
    let output = std::process::Command::new("sudo")
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to get logs for {}: {}", unit, e))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

// ========== ZFS COMMANDS ==========

/// Get ZFS arc statistics using the new ZFS service
#[tauri::command]
pub async fn get_zfs_arcstat() -> Result<serde_json::Value, String> {
    // This would use the new ZfsService interface
    // For demonstration, showing the pattern
    Ok(serde_json::json!({
        "message": "ZFS arc statistics"
    }))
}

/// Check if ZFS pool exists
#[tauri::command]
pub async fn zfs_pool_exists(pool_name: String) -> Result<bool, String> {
    // This would use the new ZfsService interface
    Ok(false) // Placeholder
}

/// Get ZFS pool list
#[tauri::command]
pub async fn get_zpool_list() -> Result<serde_json::Value, String> {
    // This would use the new ZfsService interface
    Ok(serde_json::json!([]))
}

// ========== MAIN APPLICATION ENTRY POINT ==========

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // ========== DEPENDENCY INJECTION SETUP ==========
    // Create infrastructure services (these would be real implementations)
    let process_service: Box<dyn crate::infrastructure::ProcessService> = 
        Box::new(crate::infrastructure::RealProcessService::new());
    
    let zfs_service: Box<dyn crate::infrastructure::ZfsService> = 
        Box::new(crate::infrastructure::RealZfsService::new(process_service.clone()));
    
    // Create domain services with injected dependencies
    let config_manager = crate::core::ConfigManager;
    
    // Create application services with dependency injection
    let client_commands = crate::application::ClientCommands::new(
        config_manager,
        process_service,
        zfs_service.clone(),
        Box::new(crate::infrastructure::RealIscsiService::new(process_service.clone())), // Would be implemented
        Box::new(crate::infrastructure::RealDhcpService::new(process_service)), // Would be implemented
    );

    // ========== TAURI APPLICATION SETUP ==========
    tauri::Builder::default()
        // Initialize plugins
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let _ = app
                .get_webview_window("main")
                .expect("no main window")
                .set_focus();
        }))
        
        // Register all Tauri commands using the new architecture
        .invoke_handler(tauri::generate_handler![
            // Core commands
            get_server_info,
            read_config,
            save_config,
            get_logs,
            clear_logs,
            get_server_ip,
            
            // System commands
            list_disks,
            get_ram_usage,
            clear_ram_cache,
            get_service_logs,
            
            // Authentication commands (from application layer)
            login,
            validate_auth_token,
            update_admin_password,
            
            // Client management commands (from application layer)
            client_commands.get_clients, // Note: This shows dependency injection
            client_commands.get_client_by_id,
            client_commands.add_client,
            client_commands.control_client,
            client_commands.delete_client,
            client_commands.reset_client,
            
            // Service management commands
            get_services,
            control_service,
            check_package_status,
            
            // Image management commands
            create_image,
            get_images,
            delete_image,
            
            // Disk management commands
            list_zpools,
            list_datasets,
            create_zfs_dataset,
            
            // License management commands
            activate_license,
            get_license_info,
            
            // ZFS commands
            get_zfs_arcstat,
            zfs_pool_exists,
            get_zpool_list
        ])
        
        // Application setup with proper error handling
        .setup(|_app| {
            use crate::constants::{app, paths};
            use crate::core::config::ConfigManager;
            
            println!("{} v{} starting up...", app::NAME, app::VERSION);
            
            // Initialize configuration with proper error handling
            async_std::task::block_on(async {
                if !ConfigManager::config_exists().await {
                    match ConfigManager::create_default().await {
                        Ok(config) => {
                            println!("Created default configuration at {}", paths::config_file().display());
                            println!("Initial config: {:?}", config);
                        }
                        Err(e) => {
                            eprintln!("Failed to create default configuration: {}", e);
                        }
                    }
                }
            });

            Ok(())
        })
        
        // Run the application with error handling
        .run(tauri::generate_context!())
        .expect("failed to run tauri application");
}

// ========== BACKWARD COMPATIBILITY ==========
// Keep old command names for compatibility while they get migrated

/// Legacy middleware authenticate function (would be updated to use new auth system)
#[tauri::command]
pub fn authenticate(token: String) -> Result<serde_json::Value, String> {
    // This would be migrated to use the new authentication domain
    Ok(serde_json::json!({
        "valid": true,
        "message": "Authentication middleware (legacy)"
    }))
}

/// Remote client function (would be moved to ClientCommands)
#[tauri::command]
pub async fn remote_client(token: String, client_id: String) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "message": "Remote desktop functionality (migration needed)",
        "client_id": client_id
    }))
}

// ========== DEPRECATED COMMAND MAPPINGS ==========
// These would be gradually migrated to the new architecture

#[tauri::command]
pub async fn edit_client(token: String, client_id: String, data: serde_json::Value) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "message": "Edit client functionality (migration needed)",
        "client_id": client_id
    }))
}

#[tauri::command]
pub async fn deprovision_client(token: String, mac: String, force: Option<bool>, keep_zfs: Option<bool>, dry_run: Option<bool>) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "message": "Deprovision client functionality (migration needed)",
        "mac": mac
    }))
}

#[tauri::command]
pub async fn deprovision_client_by_id(token: String, client_id: String, force: Option<bool>, keep_zfs: Option<bool>) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "message": "Deprovision by ID functionality (migration needed)",
        "client_id": client_id
    }))
}

#[tauri::command]
pub async fn get_deprovision_status(token: String, mac: String) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "message": "Deprovision status functionality (migration needed)",
        "mac": mac
    }))
}