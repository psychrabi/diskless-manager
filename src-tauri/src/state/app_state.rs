use crate::core::config::Settings;
use log::info;
use sqlx::sqlite::SqlitePool;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<RwLock<Settings>>,
    pub db_pool: SqlitePool,
    pub config_path: PathBuf,
}

impl AppState {
    pub async fn new() -> anyhow::Result<Self> {
        // Determine config directory - use same as existing diskless-manager
        let config_dir = dirs::config_dir()
            .map(|path| path.join("com.diskless.local"))
            .unwrap_or_else(|| PathBuf::from("./config"));

        std::fs::create_dir_all(&config_dir)?;

        let config_path = config_dir.join("config.json");

        let mut settings = Settings::load(&config_path.with_extension("toml"))?;

        // Initialize database in the same directory
        let db_path = config_dir.join("diskless.db");
        let db_url = format!("sqlite:{}?mode=rwc", db_path.display());

        let pool = SqlitePool::connect(&db_url).await?;

        // Run migrations
        Self::init_database(&pool).await?;

        // Load config from database and populate the cache
        if let Ok(config) = crate::config::read_config_db(&pool).await {
            crate::config::set_config(&config);
            // Sync settings from DB to the current settings struct if available
            if let Ok(db_settings) = serde_json::from_value::<Settings>(config.settings) {
                info!("Merged settings from database");
                settings = db_settings;
            }
        }

        info!("Database initialized at {}", db_path.display());

        Ok(Self {
            settings: Arc::new(RwLock::new(settings)),
            db_pool: pool,
            config_path,
        })
    }

    async fn init_database(pool: &SqlitePool) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS clients (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                mac TEXT NOT NULL UNIQUE,
                ip TEXT NOT NULL UNIQUE,
                master TEXT,
                enabled INTEGER NOT NULL DEFAULT 1,
                created_at TEXT,
                updated_at TEXT,
                snapshot TEXT,
                block_store TEXT,
                target_iqn TEXT,
                writeback TEXT,
                last_modified TEXT,
                block_device TEXT,
                status TEXT,
                mode TEXT,
                pxe_mode TEXT NOT NULL DEFAULT 'uefi',
                keep_writeback INTEGER NOT NULL DEFAULT 1,
                use_game_disk INTEGER NOT NULL DEFAULT 0
            )
            "#,
        )
        .execute(pool)
        .await?;

        // App configuration table (for masters, services, settings JSON blobs)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS app_config (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )
            "#,
        )
        .execute(pool)
        .await?;

        // Images table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS images (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                os_type TEXT NOT NULL,
                size_gb INTEGER NOT NULL,
                path TEXT NOT NULL,
                format TEXT NOT NULL DEFAULT 'raw',
                status TEXT NOT NULL DEFAULT 'ready',
                description TEXT,
                parent_id TEXT,
                tags TEXT,
                checksum TEXT,
                is_default INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            "#,
        )
        .execute(pool)
        .await?;

        // Image versions table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS image_versions (
                id TEXT PRIMARY KEY,
                base_name TEXT NOT NULL,
                version TEXT NOT NULL,
                image_id TEXT NOT NULL,
                parent_version_id TEXT,
                changelog TEXT,
                is_latest INTEGER NOT NULL DEFAULT 0,
                is_stable INTEGER NOT NULL DEFAULT 0,
                created_by TEXT,
                tags TEXT,
                created_at TEXT NOT NULL,
                UNIQUE(base_name, version)
            )
            "#,
        )
        .execute(pool)
        .await?;

        // Boot logs table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS boot_logs (
                id TEXT PRIMARY KEY,
                client_id TEXT NOT NULL,
                image_id TEXT,
                boot_time TEXT NOT NULL,
                success INTEGER NOT NULL,
                duration_ms INTEGER,
                message TEXT,
                FOREIGN KEY (client_id) REFERENCES clients(id)
            )
            "#,
        )
        .execute(pool)
        .await?;

        // Create indexes
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_clients_mac ON clients(mac)")
            .execute(pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_images_name ON images(name)")
            .execute(pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_boot_logs_client ON boot_logs(client_id)")
            .execute(pool)
            .await?;

        Ok(())
    }
}
