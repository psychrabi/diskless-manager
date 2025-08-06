use once_cell::sync::OnceCell;
use std::sync::RwLock;

static CONFIG_CACHE: OnceCell<RwLock<Config>> = OnceCell::new();

pub fn get_config() -> Config {
    let cache = CONFIG_CACHE.get_or_init(|| {
        let config = read_config();
        RwLock::new(config)
    });
    cache.read().unwrap().clone()
}

pub fn set_config(new_config: &Config) {
    let cache = CONFIG_CACHE.get_or_init(|| {
        let config = read_config();
        RwLock::new(config)
    });
    *cache.write().unwrap() = new_config.clone();
}

pub fn reload_config_from_disk() {
    let config = read_config();
    set_config(&config);
}
use std::fs;

use serde::{Deserialize, Serialize};
extern crate dirs;
use crate::client::Client;
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

#[tauri::command]
// Read config.json, or return default
pub fn read_config() -> Config {
    dirs::config_dir()
        .map(|path| {
            let config_path = path.join("com.diskless.local").join("config.json");
            print!("Reading config from: {}\n", config_path.display());
            if let Ok(content) = fs::read_to_string(config_path) {
                serde_json::from_str(&content).unwrap_or_default()
            } else {
                Config::default()
            }
        })
        .unwrap_or_default()
}

// Write config.json
pub fn write_config(cfg: &Config) -> Result<(), String> {
    dirs::config_dir()
        .ok_or("Could not find config directory".to_string())
        .and_then(|path| {
            let config_dir = path.join("com.diskless.local");
            fs::create_dir_all(&config_dir).map_err(|e| e.to_string())?;
            Ok(config_dir.join("config.json"))
        })
        .and_then(|config_path| {
            fs::write(
                config_path,
                serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())
        })?;
        reload_config_from_disk();
    Ok(())
}

#[tauri::command]
pub fn save_config(pool_name: String) -> Result<(), String> {
    let mut cfg = get_config();
    // Ensure settings is an object
    let mut settings = cfg.settings.as_object().cloned().unwrap_or_default();
    settings.insert("zfsPool".to_string(), json!(pool_name));
    cfg.settings = json!(settings);
    write_config(&cfg)
}
