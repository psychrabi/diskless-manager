//! Centralized configuration management with caching
//!
//! This module provides a unified way to manage application configuration
//! with proper caching and validation.

use crate::core::error::{DisklessError, Result};
use crate::types::config::{Config, AppConfig};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use once_cell::sync::Lazy;

const CONFIG_CACHE_TTL: Duration = Duration::from_secs(30); // 30 seconds
const CONFIG_FILE_NAME: &str = "config.json";
const APP_NAME: &str = "com.diskless.local";

/// Global configuration cache
static CONFIG_CACHE: Lazy<Arc<RwLock<ConfigCache>>> = Lazy::new(|| {
    Arc::new(RwLock::new(ConfigCache::new()))
});

/// Configuration cache with TTL and change detection
pub struct ConfigCache {
    config: Option<AppConfig>,
    last_load: Option<Instant>,
    config_path: std::path::PathBuf,
}

impl ConfigCache {
    /// Create a new configuration cache
    pub fn new() -> Self {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| dirs::home_dir().expect("No home directory found"))
            .join(APP_NAME);
        
        let config_path = config_dir.join(CONFIG_FILE_NAME);
        
        Self {
            config: None,
            last_load: None,
            config_path,
        }
    }

    /// Load configuration from disk
    pub async fn load_from_disk(&mut self) -> Result<AppConfig> {
        // Ensure config directory exists
        if let Some(parent) = self.config_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| DisklessError::Config(crate::core::error::ConfigError::WriteError(e.to_string())))?;
        }

        // Read configuration file
        match tokio::fs::read_to_string(&self.config_path).await {
            Ok(content) => {
                let config: AppConfig = serde_json::from_str(&content)
                    .map_err(|e| DisklessError::Config(crate::core::error::ConfigError::InvalidFormat(e.to_string())))?;
                
                Ok(config)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Create default configuration
                let default_config = AppConfig::default();
                self.save_to_disk(&default_config).await?;
                Ok(default_config)
            }
            Err(e) => Err(DisklessError::Config(crate::core::error::ConfigError::ReadError(e.to_string()))),
        }
    }

    /// Save configuration to disk
    pub async fn save_to_disk(&self, config: &AppConfig) -> Result<()> {
        let content = serde_json::to_string_pretty(config)
            .map_err(|e| DisklessError::Config(crate::core::error::ConfigError::WriteError(e.to_string())))?;

        tokio::fs::write(&self.config_path, content)
            .await
            .map_err(|e| DisklessError::Config(crate::core::error::ConfigError::WriteError(e.to_string())))?;

        Ok(())
    }

    /// Check if cache is stale
    fn is_stale(&self) -> bool {
        match self.last_load {
            Some(last_load) => last_load.elapsed() > CONFIG_CACHE_TTL,
            None => true,
        }
    }

    /// Force refresh the cache
    pub async fn refresh(&mut self) -> Result<AppConfig> {
        let config = self.load_from_disk().await?;
        self.config = Some(config.clone());
        self.last_load = Some(Instant::now());
        Ok(config)
    }
}

/// Configuration manager for the application
pub struct ConfigManager;

impl ConfigManager {
    /// Get the current configuration, loading from disk if needed
    pub async fn get() -> Result<AppConfig> {
        let mut cache = CONFIG_CACHE.write().await;
        
        // Check if we need to load or refresh
        if cache.config.is_none() || cache.is_stale() {
            let config = if cache.config.is_none() {
                cache.load_from_disk().await?
            } else {
                cache.refresh().await?
            };
            
            cache.config = Some(config.clone());
            cache.last_load = Some(Instant::now());
            
            Ok(config)
        } else {
            Ok(cache.config.as_ref().unwrap().clone())
        }
    }

    /// Get configuration without async (blocking version)
    pub fn get_sync() -> Result<AppConfig> {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| DisklessError::internal(format!("Failed to create runtime: {}", e)))?;
        
        rt.block_on(Self::get())
    }

    /// Update configuration
    pub async fn update<F>(updater: F) -> Result<AppConfig>
    where
        F: FnOnce(&mut AppConfig),
    {
        let mut cache = CONFIG_CACHE.write().await;
        
        // Load current config if not cached
        if cache.config.is_none() || cache.is_stale() {
            let config = cache.load_from_disk().await?;
            cache.config = Some(config);
            cache.last_load = Some(Instant::now());
        }

        // Apply update
        if let Some(config) = &mut cache.config {
            updater(config);
            
            // Save to disk
            cache.save_to_disk(config).await?;
            
            Ok(config.clone())
        } else {
            Err(DisklessError::internal("Configuration cache is empty"))
        }
    }

    /// Save configuration explicitly
    pub async fn save(config: &AppConfig) -> Result<()> {
        let mut cache = CONFIG_CACHE.write().await;
        cache.save_to_disk(config).await?;
        cache.config = Some(config.clone());
        cache.last_load = Some(Instant::now());
        Ok(())
    }

    /// Force refresh configuration from disk
    pub async fn refresh() -> Result<AppConfig> {
        let mut cache = CONFIG_CACHE.write().await;
        cache.refresh().await
    }

    /// Get ZFS pool name with fallback
    pub async fn get_zpool_name() -> Result<String> {
        let config = Self::get().await?;
        let settings = &config.settings;
        
        // Try new key first, then legacy key
        let zpool_name = settings
            .get("zpool_name")
            .and_then(|v| v.as_str())
            .or_else(|| {
                settings
                    .get("zfsPool")
                    .and_then(|v| v.as_str())
            })
            .unwrap_or("diskless");

        Ok(zpool_name.to_string())
    }

    /// Get server IP address
    pub async fn get_server_ip() -> Result<String> {
        let config = Self::get().await?;
        let settings = &config.settings;
        
        let server_ip = settings
            .get("server_ip")
            .and_then(|v| v.as_str())
            .unwrap_or("192.168.1.200");

        Ok(server_ip.to_string())
    }

    /// Check if a setting exists and is valid
    pub async fn has_setting(&self, key: &str) -> bool {
        match Self::get().await {
            Ok(config) => config.settings.get(key).is_some(),
            Err(_) => false,
        }
    }

    /// Get a specific setting value
    pub async fn get_setting(&self, key: &str) -> Option<serde_json::Value> {
        match Self::get().await {
            Ok(config) => config.settings.get(key).cloned(),
            Err(_) => None,
        }
    }

    /// Set a specific setting value
    pub async fn set_setting(&self, key: String, value: serde_json::Value) -> Result<()> {
        Self::update(|config| {
            config.settings[key] = value;
        }).await
    }

    /// Remove a setting
    pub async fn remove_setting(&self, key: &str) -> Result<()> {
        Self::update(|config| {
            if let Some(settings_obj) = config.settings.as_object_mut() {
                settings_obj.remove(key);
            }
        }).await
    }

    /// Get the configuration file path
    pub fn get_config_path() -> std::path::PathBuf {
        let mut cache = CONFIG_CACHE.blocking_write();
        if cache.config_path.as_os_str().is_empty() {
            let config_dir = dirs::config_dir()
                .unwrap_or_else(|| dirs::home_dir().expect("No home directory found"))
                .join(APP_NAME);
            cache.config_path = config_dir.join(CONFIG_FILE_NAME);
        }
        cache.config_path.clone()
    }

    /// Check if configuration file exists
    pub async fn config_exists() -> bool {
        Self::get_config_path().exists()
    }

    /// Create default configuration file
    pub async fn create_default() -> Result<AppConfig> {
        let default_config = AppConfig::default();
        Self::save(&default_config).await?;
        Ok(default_config)
    }

    /// Validate configuration
    pub async fn validate() -> Result<Vec<String>> {
        let mut errors = Vec::new();
        let config = Self::get().await?;
        
        // Validate required settings
        if !config.settings.is_object() {
            errors.push("Settings must be an object".to_string());
        }

        // Validate ZFS pool configuration
        let zpool_name = config
            .settings
            .get("zpool_name")
            .or_else(|| config.settings.get("zfsPool"))
            .and_then(|v| v.as_str());
            
        if zpool_name.is_none() {
            errors.push("ZFS pool name not configured".to_string());
        }

        // Validate license configuration if present
        if let Some(license_status) = config
            .settings
            .get("license_status")
            .and_then(|v| v.as_str())
        {
            if license_status == "valid" {
                if let Some(expires) = config
                    .settings
                    .get("license_expires")
                    .and_then(|v| v.as_str())
                {
                    // Basic date format validation
                    if !expires.chars().all(|c| c.is_ascii_digit() || c == '-') {
                        errors.push("Invalid license expiry date format".to_string());
                    }
                }
            }
        }

        Ok(errors)
    }
}

/// Convenience functions for common configuration operations
impl ConfigManager {
    /// Get clients list
    pub async fn get_clients() -> Result<Vec<crate::types::client::Client>> {
        let config = Self::get().await?;
        Ok(config.clients)
    }

    /// Get client by ID
    pub async fn get_client_by_id(&self, client_id: &str) -> Result<Option<crate::types::client::Client>> {
        let clients = Self::get_clients().await?;
        let client = clients
            .into_iter()
            .find(|c| c.id.eq_ignore_ascii_case(client_id));
        Ok(client)
    }

    /// Add or update client
    pub async fn save_client(&self, client: &crate::types::client::Client) -> Result<()> {
        Self::update(|config| {
            // Find and replace existing client
            let mut found = false;
            for existing_client in &mut config.clients {
                if existing_client.id.eq_ignore_ascii_case(&client.id) {
                    *existing_client = client.clone();
                    found = true;
                    break;
                }
            }
            
            // Add new client if not found
            if !found {
                config.clients.push(client.clone());
            }
        }).await
    }

    /// Remove client by ID
    pub async fn remove_client(&self, client_id: &str) -> Result<bool> {
        let mut removed = false;
        Self::update(|config| {
            let before_len = config.clients.len();
            config.clients.retain(|c| !c.id.eq_ignore_ascii_case(client_id));
            removed = config.clients.len() < before_len;
        }).await?;
        Ok(removed)
    }

    /// Get masters/images configuration
    pub async fn get_masters(&self) -> Result<serde_json::Value> {
        let config = Self::get().await?;
        Ok(config.masters)
    }

    /// Save masters configuration
    pub async fn set_masters(&self, masters: serde_json::Value) -> Result<()> {
        Self::update(|config| {
            config.masters = masters;
        }).await
    }
}