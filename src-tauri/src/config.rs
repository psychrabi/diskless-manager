use once_cell::sync::OnceCell;
use std::sync::RwLock;

use std::fs;

extern crate dirs;
use crate::types::Config;
use serde_json::json;
use crate::utils::append_log;

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

#[tauri::command]
// Read config.json, or return default
pub fn read_config() -> Config {
    append_log("DEBUG", "read_config called");
    dirs::config_dir()
        .map(|path| {
            let config_path = path.join("com.diskless.local").join("config.json");
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
    append_log("INFO", "write_config called");
    dirs
        ::config_dir()
        .ok_or("Could not find config directory".to_string())
        .and_then(|path| {
            let config_dir = path.join("com.diskless.local");
            fs::create_dir_all(&config_dir).map_err(|e| e.to_string())?;
            Ok(config_dir.join("config.json"))
        })
        .and_then(|config_path| {
            fs::write(
                config_path,
                serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?
            ).map_err(|e| e.to_string())
        })?;
    reload_config_from_disk();
    append_log("INFO", "write_config success");
    Ok(())
}

#[tauri::command]
pub fn save_config(pool_name: String) -> Result<(), String> {
    let mut cfg = get_config();
    // Ensure settings is an object
    let mut settings = cfg.settings.as_object().cloned().unwrap_or_default();
    settings.insert("zpool_name".to_string(), json!(pool_name.clone()));
    settings.insert("zfsPool".to_string(), json!(pool_name));
    cfg.settings = json!(settings);
    write_config(&cfg)
}

/// Returns the configured ZFS pool name from config.settings.
/// Prefers 'zpool_name' and falls back to legacy 'zfsPool'. Defaults to 'diskless'.
pub fn get_zpool_name() -> String {
    let cfg = get_config();
    let settings = cfg.settings.as_object();
    let from_new = settings
        .and_then(|s| s.get("zpool_name"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let from_legacy = settings
        .and_then(|s| s.get("zfsPool"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    from_new
        .or(from_legacy)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "diskless".to_string())
}
