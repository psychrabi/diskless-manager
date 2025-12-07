//! Application configuration module
//!
//! Centralizes all configuration paths and settings to avoid hardcoded values
//! scattered throughout the codebase. Configuration can be loaded from a TOML
//! file or use sensible defaults.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::OnceLock;

static APP_CONFIG: OnceLock<AppConfig> = OnceLock::new();

/// Main application configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// TFTP server root directory
    pub tftp_root: PathBuf,

    /// DHCP server configuration file path
    pub dhcp_config_path: PathBuf,

    /// iSCSI IQN prefix for targets
    pub iscsi_iqn_prefix: String,

    /// Application log file path
    pub log_file_path: PathBuf,

    /// Default ZFS pool name
    pub zfs_pool: String,

    /// Apache/HTTP server root directory
    pub http_root: PathBuf,

    /// Configuration directory
    pub config_dir: PathBuf,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            tftp_root: PathBuf::from("/srv/tftp"),
            dhcp_config_path: PathBuf::from("/etc/dhcp/dhcpd.conf"),
            iscsi_iqn_prefix: "iqn.2025-04.local.diskless".to_string(),
            log_file_path: PathBuf::from("/var/log/diskless-manager.log"),
            zfs_pool: "diskless".to_string(),
            http_root: PathBuf::from("/var/www/html"),
            config_dir: PathBuf::from("/etc/diskless-manager"),
        }
    }
}

impl AppConfig {
    /// Load configuration from file or use defaults
    ///
    /// Attempts to load from `/etc/diskless-manager/config.toml`
    /// Falls back to defaults if file doesn't exist or can't be parsed
    pub fn load() -> Self {
        let config_path = PathBuf::from("/etc/diskless-manager/config.toml");

        if config_path.exists() {
            match std::fs::read_to_string(&config_path) {
                Ok(contents) => match toml::from_str(&contents) {
                    Ok(config) => {
                        eprintln!("Loaded configuration from {:?}", config_path);
                        return config;
                    }
                    Err(e) => {
                        eprintln!(
                            "Warning: Failed to parse config file {:?}: {}",
                            config_path, e
                        );
                        eprintln!("Using default configuration");
                    }
                },
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to read config file {:?}: {}",
                        config_path, e
                    );
                    eprintln!("Using default configuration");
                }
            }
        }

        Self::default()
    }

    /// Get the global application configuration
    ///
    /// Lazily loads configuration on first access
    pub fn get() -> &'static AppConfig {
        APP_CONFIG.get_or_init(Self::load)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.tftp_root, PathBuf::from("/srv/tftp"));
        assert_eq!(
            config.dhcp_config_path,
            PathBuf::from("/etc/dhcp/dhcpd.conf")
        );
        assert_eq!(config.iscsi_iqn_prefix, "iqn.2025-04.local.diskless");
    }

    #[test]
    fn test_config_get() {
        let config = AppConfig::get();
        assert!(!config.tftp_root.as_os_str().is_empty());
    }
}
