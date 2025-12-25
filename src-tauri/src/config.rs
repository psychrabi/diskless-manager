use once_cell::sync::OnceCell;
use std::sync::RwLock;

extern crate dirs;
use crate::state::AppState;
use crate::types::AppConfig;
use log::info;
use serde_json::json;
use tauri::State;

static CONFIG_CACHE: OnceCell<RwLock<AppConfig>> = OnceCell::new();

pub fn get_config() -> AppConfig {
    let cache = CONFIG_CACHE.get_or_init(|| {
        // Initialize with default config - the actual config should be loaded from DB
        // and cached when the application starts up via read_config_db
        RwLock::new(AppConfig::default())
    });
    cache.read().unwrap().clone()
}

pub fn set_config(config: &AppConfig) {
    let cache = CONFIG_CACHE.get_or_init(|| RwLock::new(config.clone()));
    let mut w = cache.write().unwrap();
    *w = config.clone();
}

pub async fn write_config(pool: &sqlx::SqlitePool, config: &AppConfig) -> Result<(), String> {
    set_config(config);

    // 1. Persist masters, services, settings to app_config
    sqlx::query("INSERT OR REPLACE INTO app_config (key, value) VALUES (?, ?)")
        .bind("masters")
        .bind(serde_json::to_string(&config.masters).map_err(|e| e.to_string())?)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    sqlx::query("INSERT OR REPLACE INTO app_config (key, value) VALUES (?, ?)")
        .bind("services")
        .bind(serde_json::to_string(&config.services).map_err(|e| e.to_string())?)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    sqlx::query("INSERT OR REPLACE INTO app_config (key, value) VALUES (?, ?)")
        .bind("settings_legacy")
        .bind(serde_json::to_string(&config.settings).map_err(|e| e.to_string())?)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    // 2. Persist clients (this is harder as we need to sync)
    // For simplicity in this first pass, we'll clear and re-insert or use UPSERT
    for client in &config.clients {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO clients (
                id, name, mac, ip, master, snapshot, block_store, target_iqn,
                writeback, block_device, status, mode, pxe_mode, keep_writeback,
                use_game_disk, created_at, last_modified
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&client.id)
        .bind(&client.name)
        .bind(&client.mac)
        .bind(&client.ip)
        .bind(&client.master)
        .bind(&client.snapshot)
        .bind(&client.block_store)
        .bind(&client.target_iqn)
        .bind(&client.writeback)
        .bind(&client.block_device)
        .bind(&client.status)
        .bind(&client.mode)
        .bind(client.pxe_mode.as_ref().unwrap_or(&"uefi".to_string()))
        .bind(client.keep_writeback.unwrap_or(true))
        .bind(client.use_game_disk.unwrap_or(false))
        .bind(&client.created_at)
        .bind(&client.last_modified)
        .execute(pool)
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

    // Get masters from DB
    let masters_row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM app_config WHERE key = 'masters'")
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;

    if let Some((masters_json,)) = masters_row {
        config.masters = serde_json::from_str(&masters_json).unwrap_or_default();
    }

    // Get services from DB
    let services_row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM app_config WHERE key = 'services'")
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;

    if let Some((services_json,)) = services_row {
        config.services = serde_json::from_str(&services_json).unwrap_or_default();
    }

    // Get settings from DB
    let settings_row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM app_config WHERE key = 'settings_legacy'")
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;

    if let Some((settings_json,)) = settings_row {
        config.settings = serde_json::from_str(&settings_json).unwrap_or_default();
    }

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
        config.clients.push(crate::types::client::Client {
            id: c.id,
            name: c.name,
            mac: c.mac,
            ip: c.ip,
            master: c.master.unwrap_or_default(),
            snapshot: c.snapshot,
            block_store: c.block_store,
            target_iqn: c.target_iqn,
            writeback: c.writeback,
            block_device: c.block_device,
            status: c.status,
            mode: c.mode,
            pxe_mode: Some(c.pxe_mode),
            keep_writeback: Some(c.keep_writeback),
            use_game_disk: Some(c.use_game_disk),
            created_at: c.created_at,
            last_modified: c.last_modified,
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
    let mut settings = cfg.settings.as_object().cloned().unwrap_or_default();
    settings.insert("zpool_name".to_string(), json!(pool_name.clone()));
    settings.insert("zfsPool".to_string(), json!(pool_name));
    cfg.settings = json!(settings);

    // Write to DB
    sqlx::query("INSERT OR REPLACE INTO app_config (key, value) VALUES (?, ?)")
        .bind("settings_legacy")
        .bind(serde_json::to_string(&cfg.settings).map_err(|e| e.to_string())?)
        .execute(&state.db_pool)
        .await
        .map_err(|e| e.to_string())?;

    // Update cache
    set_config(&cfg);
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
