use crate::core::config::Settings;
use sqlx::sqlite::SqlitePool;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct AppState {
    pub settings: Arc<RwLock<Settings>>,
    pub db_pool: SqlitePool,
    pub config_path: PathBuf,
}

impl AppState {
    pub async fn new() -> anyhow::Result<Self> {
        // Determine config directory
        let config_dir = directories::ProjectDirs::from("com", "diskless", "boot-server")
            .map(|dirs| dirs.config_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/etc/diskless-boot-server"));

        std::fs::create_dir_all(&config_dir)?;

        let config_path = config_dir.join("config.toml");
        let settings = Settings::load(&config_path)?;

        // Initialize database
        let data_dir = directories::ProjectDirs::from("com", "diskless", "boot-server")
            .map(|dirs| dirs.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/var/lib/diskless"));

        std::fs::create_dir_all(&data_dir)?;

        let db_path = data_dir.join("diskless.db");
        let db_url = format!("sqlite:{}?mode=rwc", db_path.display());

        let pool = SqlitePool::connect(&db_url).await?;

        // Run migrations
        Self::init_database(&pool).await?;

        tracing::info!("Database initialized at {}", db_path.display());

        Ok(Self {
            settings: Arc::new(RwLock::new(settings)),
            db_pool: pool,
            config_path,
        })
    }

    async fn init_database(pool: &SqlitePool) -> anyhow::Result<()> {
        // Clients table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS clients (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                mac_address TEXT NOT NULL UNIQUE,
                ip_address TEXT,
                image_id TEXT NOT NULL,
                boot_mode TEXT NOT NULL DEFAULT 'uefi',
                enabled INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
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
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_clients_mac ON clients(mac_address)")
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
