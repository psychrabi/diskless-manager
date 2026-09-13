use once_cell::sync::OnceCell;
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::RwLock;

use crate::domain::{BootMode, ClientId, ClientStatus, MacAddress, PxeMode};
use crate::persistence::ClientRepository;
use crate::types::AppConfig;
use log::{info, warn};
use serde_json::{json, Value};

static CONFIG_CACHE: OnceCell<RwLock<AppConfig>> = OnceCell::new();

pub fn get_config() -> AppConfig {
    let cache = CONFIG_CACHE.get_or_init(|| {
        // Initialize with default config - the actual config should be loaded from DB
        // and cached when the application starts up via read_config_db
        RwLock::new(AppConfig::default())
    });
    cache.read().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn set_config(config: &AppConfig) {
    let cache = CONFIG_CACHE.get_or_init(|| RwLock::new(config.clone()));
    let mut w = cache.write().unwrap_or_else(|e| e.into_inner());
    *w = config.clone();
}

/// Helper function to insert or replace a key-value pair in app_config table
async fn upsert_config_value(
    pool: &sqlx::SqlitePool,
    key: &str,
    value: &str,
) -> anyhow::Result<()> {
    sqlx::query("INSERT OR REPLACE INTO app_config (key, value) VALUES (?, ?)")
        .bind(key)
        .bind(value)
        .execute(pool)
        .await?;
    Ok(())
}

fn parse_legacy_datetime(value: &str) -> anyhow::Result<chrono::DateTime<chrono::Utc>> {
    if let Ok(value) = chrono::DateTime::parse_from_rfc3339(value) {
        return Ok(value.with_timezone(&chrono::Utc));
    }

    if let Ok(value) = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S") {
        return Ok(value.and_utc());
    }

    anyhow::bail!("invalid legacy datetime: {value}")
}

fn legacy_client_to_domain(client: &crate::core::client::Client) -> anyhow::Result<crate::domain::Client> {
    let status = match client.status.as_deref().map(str::to_ascii_lowercase).as_deref() {
        None | Some("provisioning") => ClientStatus::Provisioning,
        Some("ready") => ClientStatus::Ready,
        Some("online") => ClientStatus::Online,
        Some("offline") => ClientStatus::Offline,
        Some("error") => ClientStatus::Error,
        Some("disabled") => ClientStatus::Disabled,
        Some(value) => anyhow::bail!("unsupported legacy client status: {value}"),
    };

    let mode = match client.mode.as_deref().map(str::to_ascii_lowercase).as_deref() {
        None | Some("normal") => BootMode::Normal,
        Some("super") => BootMode::Super,
        Some(value) => anyhow::bail!("unsupported legacy client mode: {value}"),
    };

    let pxe_mode = match client
        .pxe_mode
        .as_deref()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        None | Some("uefi") => PxeMode::Uefi,
        Some("bios") | Some("legacy") => PxeMode::Bios,
        Some(value) => anyhow::bail!("unsupported legacy PXE mode: {value}"),
    };

    let last_modified = client
        .last_modified
        .as_deref()
        .map(parse_legacy_datetime)
        .transpose()?;

    Ok(crate::domain::Client {
        id: ClientId::from_string(client.id.clone())?,
        name: client.name.clone(),
        mac: MacAddress::parse(&client.mac)?,
        ip: IpAddr::from_str(client.ip.trim())
            .map_err(|_| anyhow::anyhow!("invalid legacy client IP: {}", client.ip))?,
        master: client.master.clone(),
        enabled: client.enabled,
        created_at: client.created_at,
        updated_at: client.updated_at,
        snapshot: client.snapshot.clone(),
        block_store: client.block_store.clone(),
        target_iqn: client.target_iqn.clone(),
        writeback: client.writeback.clone(),
        last_modified,
        block_device: client.block_device.clone(),
        status,
        mode,
        pxe_mode,
        keep_writeback: client.keep_writeback.unwrap_or(true),
        use_game_disk: client.use_game_disk.unwrap_or(false),
        game_disks: Vec::new(),
        chap_user: client.chap_user.clone(),
        chap_secret: client.chap_secret.clone(),
        chap_enabled: client.chap_enabled.unwrap_or(false),
    })
}

async fn persist_config_client(
    pool: &sqlx::SqlitePool,
    client: &crate::core::client::Client,
) -> anyhow::Result<()> {
    let domain_client = match legacy_client_to_domain(client) {
        Ok(client) => client,
        Err(error) => {
            warn!(
                "legacy config client '{}' is not domain-compatible ({}); using compatibility upsert",
                client.id, error
            );
            return crate::core::client::ClientManager::upsert_client(pool, client).await;
        }
    };

    let repository = ClientRepository::new(pool.clone());
    match repository.find_by_id(&domain_client.id).await {
        Ok(Some(_)) => repository.update(&domain_client).await,
        Ok(None) => repository.insert(&domain_client).await,
        Err(error) => {
            // An existing historical row can still contain placeholder values
            // that the strict domain mapper intentionally rejects. Preserve the
            // upgrade path instead of making config writes fail on those rows.
            warn!(
                "client '{}' could not be loaded through ClientRepository ({}); using compatibility upsert",
                client.id, error
            );
            crate::core::client::ClientManager::upsert_client(pool, client).await
        }
    }
}

pub async fn write_config(pool: &sqlx::SqlitePool, config: &AppConfig) -> anyhow::Result<()> {
    set_config(config);

    // 1. Persist masters and services to app_config
    upsert_config_value(pool, "masters", &serde_json::to_string(&config.masters)?).await?;

    upsert_config_value(pool, "services", &serde_json::to_string(&config.services)?).await?;

    // 2. Persist each setting under its own key in the app_config table
    if let Some(obj) = config.settings.as_object() {
        for (k, v) in obj {
            upsert_config_value(pool, k, &serde_json::to_string(v)?).await?;
        }
    }

    // 3. Persist domain-compatible clients through the authoritative repository.
    // Historical rows that cannot yet be represented by the strict domain model
    // retain the legacy upsert as an explicit compatibility fallback.
    for client in &config.clients {
        persist_config_client(pool, client).await?;
    }

    Ok(())
}

pub async fn read_config_db(pool: &sqlx::SqlitePool) -> anyhow::Result<AppConfig> {
    info!("read_config_db called");

    let mut config = AppConfig::default();

    // Fetch all configuration keys from app_config
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT key, value FROM app_config")
        .fetch_all(pool)
        .await?;

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
        enabled: bool,
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
        chap_user: Option<String>,
        chap_secret: Option<String>,
        chap_enabled: Option<bool>,
        created_at: Option<String>,
        last_modified: Option<String>,
    }

    // Get clients from DB. This remains intentionally compatibility-oriented:
    // config export must still be able to surface historical rows containing
    // placeholder values that the strict domain repository would reject.
    let clients = sqlx::query_as::<_, ClientRow>(
        r#"
        SELECT id, name, mac, ip, master, enabled, snapshot, block_store, target_iqn,
               writeback, block_device, status, mode, pxe_mode, keep_writeback,
               use_game_disk, chap_user, chap_secret, chap_enabled,
               created_at, last_modified
        FROM clients
        "#,
    )
    .fetch_all(pool)
    .await?;

    for c in clients {
        config.clients.push(crate::core::client::Client {
            id: c.id,
            name: c.name,
            mac: c.mac,
            ip: c.ip,
            master: c.master.unwrap_or_default(),
            enabled: c.enabled,
            created_at: c
                .created_at
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(chrono::Utc::now),
            updated_at: c
                .last_modified
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
            chap_user: c.chap_user,
            chap_secret: c.chap_secret,
            chap_enabled: c.chap_enabled,
        });
    }

    // Update cache with the loaded config
    set_config(&config);
    Ok(config)
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
