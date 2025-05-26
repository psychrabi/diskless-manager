use std::fs;

use serde::{Deserialize, Serialize};

use crate::{client::Client, CONFIG_PATH};
use serde_json::{json, Value};

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    pub clients: Vec<Client>,
    pub masters: Value,
    pub services: Value,
    pub settings: Value,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            clients: Vec::new(),
            masters: json!({}),
            services: json!({}),
            settings: json!({}),
        }
    }
}

// Read config.json, or return default
pub fn read_config() -> Config {
    if let Ok(content) = fs::read_to_string(CONFIG_PATH) {
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Config::default()
    }
}

// Write config.json
pub fn write_config(cfg: &Config) -> Result<(), String> {
    let content = serde_json::to_string_pretty(cfg).unwrap();
    fs::write(CONFIG_PATH, content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_config() -> Result<Config, String> {
    Ok(read_config())
}

#[tauri::command]
pub fn save_config(pool_name: String) -> Result<(), String> {
    let mut cfg = read_config();
    // Ensure settings is an object
    let mut settings = cfg.settings.as_object().cloned().unwrap_or_default();
    settings.insert("zfsPool".to_string(), json!(pool_name));
    cfg.settings = json!(settings);
    write_config(&cfg)
}
