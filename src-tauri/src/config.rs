use once_cell::sync::OnceCell;
use std::sync::RwLock;

extern crate dirs;
use crate::state::AppState;
use crate::types::AppConfig;
use log::info;
use serde_json::{json, Value};
use tauri::State;

static CONFIG_CACHE: OnceCell<RwLock<AppConfig>> = OnceCell::new();

pub fn get_config() -> AppConfig {
    let cache = CONFIG_CACHE.get_or_init(|| {
        // Initialize with default config - the actual config should be loaded from DB
        // and cached when the application starts up via read_config_db
        RwLock::new(AppConfig::default())
    });
    cache
        .read()
        .expect("Failed to acquire read lock on config cache")
        .clone()
}

pub fn set_config(config: &AppConfig) {
    let cache = CONFIG_CACHE.get_or_init(|| RwLock::new(config.clone()));
    let mut w = cache
        .write()
        .expect("Failed to acquire write lock on config cache");
    *w = config.clone();
}

/// Helper function to insert or replace a key-value pair in app_config table
async fn upsert_config_value(
    pool: &sqlx::SqlitePool,
    key: &str,
    value: &str,
) -> Result<(), String> {
    sqlx::query("INSERT OR REPLACE INTO app_config (key, value) VALUES (?, ?)")
        .bind(key)
        .bind(value)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn write_config(pool: &sqlx::SqlitePool, config: &AppConfig) -> Result<(), String> {
    set_config(config);

    // 1. Persist masters and services to app_config
    upsert_config_value(
        pool,
        "masters",
        &serde_json::to_string(&config.masters).map_err(|e| e.to_string())?,
    )
    .await?;

    upsert_config_value(
        pool,
        "services",
        &serde_json::to_string(&config.services).map_err(|e| e.to_string())?,
    )
    .await?;

    // 2. Persist each setting under its own key in the app_config table
    if let Some(obj) = config.settings.as_object() {
        for (k, v) in obj {
            upsert_config_value(
                pool,
                k,
                &serde_json::to_string(v).map_err(|e| e.to_string())?,
            )
            .await?;
        }
    }

    // 3. Persist clients using ClientManager's upsert function
    for client in &config.clients {
        crate::core::client::ClientManager::upsert_client(pool, client)
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub async fn read_config(state: State<'_, AppState>) -> Result<AppConfig, String> {
    read_config_db(&state.db_pool).await
}

pub async fn read_config_db(pool: &sqlx::SqlitePool) -> Result<AppConfig, String> {
    info!("read_config_db called");

    let mut config = AppConfig::default();

    // Fetch all configuration keys from app_config
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT key, value FROM app_config")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

    let mut settings_map = serde_json::Map::new();

    for (key, value) in rows {
        match key.as_str() {
            "masters" => {
                config.masters = serde_json::from_str(&value).unwrap_or(json!({}));
            }
            "services" => {
                config.services = serde_json::from_str(&value).unwrap_or(json!({}));
            }
            "settings_legacy" => {
                // Merge legacy settings if they exist and haven't been overridden by individual keys
                if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&value) {
                    for (k, v) in map {
                        settings_map.entry(k).or_insert(v);
                    }
                }
            }
            _ => {
                // Treat every other key as an individual setting
                if let Ok(v) = serde_json::from_str::<Value>(&value) {
                    settings_map.insert(key, v);
                }
            }
        }
    }

    config.settings = Value::Object(settings_map);

    #[derive(sqlx::FromRow)]
    struct ClientRow {
        id: String,
        name: String,
        mac: String,
        ip: String,
        master: Option<String>,
        snapshot: Option<String>,
        block_store: Option<String>,
        target_iqn: Option<String>,
        writeback: Option<String>,
        block_device: Option<String>,
        status: Option<String>,
        mode: Option<String>,
        pxe_mode: String,
        keep_writeback: bool,
        use_game_disk: bool,
        created_at: Option<String>,
        last_modified: Option<String>,
    }

    // Get clients from DB
    let clients = sqlx::query_as::<_, ClientRow>(
        r#"
        SELECT id, name, mac, ip, master, snapshot, block_store, target_iqn,
               writeback, block_device, status, mode, pxe_mode, keep_writeback,
               use_game_disk, created_at, last_modified
        FROM clients
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    for c in clients {
        config.clients.push(crate::core::client::Client {
            id: c.id,
            name: c.name,
            mac: c.mac,
            ip: c.ip,
            master: c.master.unwrap_or_default(),
            enabled: true, // Default to enabled
            created_at: c.created_at
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(chrono::Utc::now),
            updated_at: c.last_modified
                .as_ref()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(chrono::Utc::now),
            snapshot: c.snapshot,
            block_store: c.block_store,
            target_iqn: c.target_iqn,
            writeback: c.writeback,
            last_modified: c.last_modified,
            block_device: c.block_device,
            status: c.status,
            mode: c.mode,
            pxe_mode: Some(c.pxe_mode),
            keep_writeback: Some(c.keep_writeback),
            use_game_disk: Some(c.use_game_disk),
        });
    }

    // Update cache with the loaded config
    set_config(&config);
    Ok(config)
}

#[tauri::command]
pub async fn save_config(state: State<'_, AppState>, pool_name: String) -> Result<(), String> {
    let mut cfg = get_config();
    // Ensure settings is an object
    let mut settings = cfg.settings.as_object().cloned().unwrap_or_else(|| {
        // Create an empty object if settings is not an object
        serde_json::Map::new()
    });
    settings.insert("zpool_name".to_string(), json!(pool_name.clone()));
    settings.insert("zfsPool".to_string(), json!(pool_name));
    cfg.settings = json!(settings);

    // Write to DB and update cache using the unified write_config
    write_config(&state.db_pool, &cfg).await?;
    Ok(())
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
