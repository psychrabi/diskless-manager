//! Configuration types for the application
//!
//! This module contains all configuration-related types and structures.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub clients: Vec<super::client::Client>,
    pub masters: serde_json::Value,
    pub services: serde_json::Value,
    pub settings: serde_json::Value,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            clients: Vec::new(),
            masters: serde_json::json!({}),
            services: serde_json::json!({}),
            settings: serde_json::json!({}),
        }
    }
}

/// Legacy configuration structure (for backward compatibility)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub clients: Vec<super::client::Client>,
    pub masters: serde_json::Value,
    pub services: serde_json::Value,
    pub settings: serde_json::Value,
}

impl Config {
    /// Convert to new AppConfig format
    pub fn into_app_config(self) -> AppConfig {
        AppConfig {
            clients: self.clients,
            masters: self.masters,
            services: self.services,
            settings: self.settings,
        }
    }
}

impl From<AppConfig> for Config {
    fn from(app_config: AppConfig) -> Self {
        Self {
            clients: app_config.clients,
            masters: app_config.masters,
            services: app_config.services,
            settings: app_config.settings,
        }
    }
}