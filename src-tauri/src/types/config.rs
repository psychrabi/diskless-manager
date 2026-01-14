//! Configuration types for the application
//!
//! This module contains all configuration-related types and structures.

use serde::{Deserialize, Serialize};

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub clients: Vec<crate::core::client::Client>,
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
