//! Centralized application constants and configuration
//!
//! This module contains all application constants, configuration paths,
//! and system-specific settings to eliminate magic strings and numbers.

use std::time::Duration;

/// Application metadata
pub mod app {
    /// Application name
    pub const NAME: &str = "Diskless Manager";
    /// Application version
    pub const VERSION: &str = env!("CARGO_PKG_VERSION");
    /// Application authors
    pub const AUTHORS: &str = "Rabi Shrestha";
    /// Application identifier for configuration directories
    pub const IDENTIFIER: &str = "com.diskless.local";
    /// Default ZFS pool name
    pub const DEFAULT_ZPOOL: &str = "diskless";
}

/// Configuration paths
pub mod paths {
    use super::app;
    
    /// Configuration directory path (platform-dependent)
    pub fn config_dir() -> std::path::PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| dirs::home_dir().expect("No home directory found"))
            .join(app::IDENTIFIER)
    }
    
    /// Main configuration file path
    pub fn config_file() -> std::path::PathBuf {
        config_dir().join("config.json")
    }
    
    /// Log file path
    pub fn log_file() -> std::path::PathBuf {
        config_dir().join("diskless-manager.log")
    }
    
    /// Backup directory for configuration files
    pub fn backup_dir() -> std::path::PathBuf {
        "/srv/tftp/backups".into()
    }
    
    /// DHCP configuration files
    pub const DHCP_CONFIG: &str = "/etc/dhcp/dhcpd.conf";
    pub const DHCP_CLIENTS: &str = "/etc/dhcp/clients.conf";
    
    /// TFTP configuration files
    pub const TFTP_AUTOEXEC: &str = "/srv/tftp/autoexec.ipxe";
    pub const TFTP_CONFIG: &str = "/etc/default/tftpd-hpa";
    
    /// Apache configuration files
    pub const APACHE_CONFIG: &str = "/etc/apache2/sites-available/diskless-server.conf";
    
    /// Samba configuration files
    pub const SAMBA_CONFIG: &str = "/etc/samba/smb.conf";
    
    /// TargetCLI configuration files
    pub const TARGETCLI_CONFIG: &str = "/etc/rtslib-fb-target/saveconfig.json";
    
    /// System service files
    pub const SERVICES: phf::Map<&'static str, &'static str> = phf::phf_map! {
        "iscsi" => "rtslib-fb-targetctl.service",
        "dhcp" => "isc-dhcp-server.service",
        "tftp" => "tftpd-hpa.service",
        "http" => "apache2.service",
        "share" => "smbd.service",
    };
}

/// Network and IP configuration
pub mod network {
    /// Default server IP address
    pub const DEFAULT_SERVER_IP: &str = "192.168.1.200";
    
    /// iSCSI target base IQN
    pub const IQN_BASE: &str = "iqn.2025-04.local.diskless";
    
    /// Default iSCSI port
    pub const ISCSI_PORT: u16 = 3260;
    
    /// Default HTTP port
    pub const HTTP_PORT: u16 = 80;
    
    /// Default HTTPS port
    pub const HTTPS_PORT: u16 = 443;
    
    /// TFTP default port
    pub const TFTP_PORT: u16 = 69;
}

/// Authentication and security
pub mod auth {
    use super::app;
    
    /// JWT secret key (should be loaded from environment)
    pub fn jwt_secret() -> String {
        std::env::var("DISKLESS_JWT_SECRET")
            .unwrap_or_else(|_| format!("{}_secret_key_2025", app::IDENTIFIER))
    }
    
    /// Default admin username
    pub const ADMIN_USERNAME: &str = "admin";
    
    /// Default admin password (should be changed in production)
    pub const ADMIN_PASSWORD: &str = "admin123";
    
    /// Token expiration time (24 hours)
    pub const TOKEN_EXPIRATION_HOURS: i64 = 24;
    
    /// License server URL (placeholder)
    pub const LICENSE_SERVER_URL: &str = "https://license.example.com/api/verify";
    
    /// Trial license expiration date
    pub const TRIAL_LICENSE_EXPIRES: &str = "2027-10-12";
}

/// System commands
pub mod commands {
    /// Essential system commands
    pub const ZPOOL: &str = "zpool";
    pub const ZFS: &str = "zfs";
    pub const SYSTEMCTL: &str = "systemctl";
    pub const SUDO: &str = "sudo";
    pub const TARGETCLI: &str = "targetcli";
    pub const PING: &str = "ping";
    pub const WAKEONLAN: &str = "wakeonlan";
    pub const LSOF: &str = "lsof";
    pub const IP: &str = "ip";
    pub const FREE: &str = "free";
    pub const LSBLK: &str = "lsblk";
    pub const APT: &str = "apt";
    pub const DPKG: &str = "dpkg-query";
    pub const CAT: &str = "cat";
    pub const MV: &str = "mv";
    
    /// Network commands
    pub const NET_RPC: &str = "net";
    pub const DHCP_LEASE_LIST: &str = "dhcp-lease-list";
}

/// iSCSI configuration
pub mod iscsi {
    /// Default target portal group
    pub const DEFAULT_TPG: &str = "tpg1";
    
    /// Default portal address
    pub const DEFAULT_PORTAL: &str = "0.0.0.0";
    
    /// Default portal port
    pub const DEFAULT_PORT: &str = "3260";
    
    /// iSCSI backstore types
    pub const BACKSTORE_BLOCK: &str = "backstores/block";
    pub const BACKSTORE_FILE: &str = "backstores/file";
    pub const BACKSTORE_RAMDISK: &str = "backstores/ramdisk";
    
    /// TargetCLI path prefixes
    pub const ISCSI_PATH: &str = "iscsi/";
    pub const LUNS_PATH: &str = "luns";
    pub const PORTALS_PATH: &str = "portals/";
}

/// ZFS configuration
pub mod zfs {
    /// ZFS custom properties
    pub const TYPE_PROPERTY: &str = "org.diskless:type";
    
    /// ZFS dataset types
    pub const DATASET_TYPE_IMAGE: &str = "image";
    pub const DATASET_TYPE_WRITEBACK: &str = "writeback";
    pub const DATASET_TYPE_GAMES: &str = "games";
    
    /// Default dataset names
    pub const IMAGES_DATASET: &str = "images";
    pub const GAMES_DATASET: &str = "games";
    pub const CLIENTS_DATASET: &str = "clients";
    
    /// ZFS volume properties
    pub const DEFAULT_VOLBLOCKSIZE: &str = "128K";
    pub const DEFAULT_VOLBLOCKSIZE_ZVOL: &str = "4K";
    
    /// ZFS compression algorithms
    pub const COMPRESSION_LZ4: &str = "lz4";
    pub const COMPRESSION_GZIP: &str = "gzip";
    
    /// Snapshot naming convention
    pub const BASE_SNAPSHOT_SUFFIX: &str = "base";
    pub const SNAPSHOT_DATE_FORMAT: &str = "%Y%m%d_%H%M%S";
}

/// DHCP configuration
pub mod dhcp {
    /// Default lease times (seconds)
    pub const DEFAULT_LEASE_TIME: u32 = 86400; // 24 hours
    pub const MAX_LEASE_TIME: u32 = 86400; // 24 hours
    
    /// DHCP options
    pub const OPTION_PXEEXT: u16 = 16;
    pub const OPTION_ISCSI: u16 = 17;
    pub const OPTION_AOE: u16 = 18;
    pub const OPTION_HTTP: u16 = 19;
    pub const OPTION_HTTPS: u16 = 20;
    pub const OPTION_TFTP: u16 = 21;
    pub const OPTION_FTP: u16 = 22;
    pub const OPTION_DNS: u16 = 23;
    pub const OPTION_CLIENT_ARCH: u16 = 93;
    
    /// PXE boot architectures
    pub const ARCH_X86_PC: &str = "00:00";
    pub const ARCH_EFI_X86_32: &str = "00:06";
    pub const ARCH_EFI_X86_64: &str = "00:07";
    
    /// Default filenames
    pub const DEFAULT_BOOT_FILE: &str = "pxelinux.0";
    pub const EFI_BOOT_FILE: &str = "grubx64.efi";
}

/// Timeouts and limits
pub mod timeouts {
    /// Command execution timeouts
    pub const DEFAULT_COMMAND: Duration = Duration::from_secs(30);
    pub const ZFS_COMMAND: Duration = Duration::from_secs(60);
    pub const NETWORK_COMMAND: Duration = Duration::from_secs(5);
    pub const SERVICE_COMMAND: Duration = Duration::from_secs(10);
    pub const HTTP_REQUEST: Duration = Duration::from_secs(10);
    
    /// Configuration cache TTL
    pub const CONFIG_CACHE_TTL: Duration = Duration::from_secs(30);
    
    /// Log file rotation threshold (bytes)
    pub const LOG_ROTATION_SIZE: u64 = 10 * 1024 * 1024; // 10MB
    
    /// Maximum log lines to keep
    pub const MAX_LOG_LINES: usize = 10000;
}

/// Validation patterns
pub mod validation {
    use regex::Regex;
    
    /// MAC address regex pattern
    pub fn mac_pattern() -> Regex {
        Regex::new(r"^([0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}$").unwrap()
    }
    
    /// IP address regex pattern
    pub fn ip_pattern() -> Regex {
        Regex::new(r"^(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)$").unwrap()
    }
    
    /// Dataset name regex pattern
    pub fn dataset_pattern() -> Regex {
        Regex::new(r"^[a-zA-Z0-9/_-]+$").unwrap()
    }
    
    /// Client name regex pattern
    pub fn client_name_pattern() -> Regex {
        Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap()
    }
    
    /// Size format regex pattern (e.g., 50G, 1T)
    pub fn size_pattern() -> Regex {
        Regex::new(r"^\d+[KMGTP]$").unwrap()
    }
}

/// Package information
pub mod packages {
    /// Required packages
    pub const REQUIRED_PACKAGES: &[&str] = &[
        "isc-dhcp-server",
        "tftpd-hpa", 
        "targetcli-fb",
        "apache2",
        "samba",
        "samba-common-bin",
        "wakeonlan",
        "zfsutils-linux",
    ];
    
    /// Package to service mapping
    pub const PACKAGE_SERVICES: phf::Map<&'static str, &'static str> = phf::phf_map! {
        "isc-dhcp-server" => "isc-dhcp-server",
        "tftpd-hpa" => "tftpd-hpa",
        "targetcli-fb" => "rtslib-fb-targetctl",
        "apache2" => "apache2",
        "samba" => "smbd",
        "wakeonlan" => "wakeonlan",
        "zfsutils-linux" => "zfs",
    };
}

/// Error messages
pub mod errors {
    /// Common error messages
    pub const CONFIG_FILE_NOT_FOUND: &str = "Configuration file not found";
    pub const INVALID_CONFIG_FORMAT: &str = "Invalid configuration format";
    pub const PERMISSION_DENIED: &str = "Permission denied";
    pub const COMMAND_NOT_FOUND: &str = "Command not found";
    pub const SERVICE_NOT_FOUND: &str = "Service not found";
    pub const DATASET_NOT_FOUND: &str = "Dataset not found";
    pub const POOL_NOT_FOUND: &str = "Pool not found";
    pub const SNAPSHOT_NOT_FOUND: &str = "Snapshot not found";
    pub const CLIENT_NOT_FOUND: &str = "Client not found";
    pub const INVALID_MAC_ADDRESS: &str = "Invalid MAC address format";
    pub const INVALID_IP_ADDRESS: &str = "Invalid IP address format";
    pub const LICENSE_EXPIRED: &str = "License has expired";
    pub const LICENSE_NOT_ACTIVATED: &str = "License not activated";
}