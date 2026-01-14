use crate::core::config::Settings;
use crate::ssh_executor::SshExecutor;
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
    pub client_ips: Arc<RwLock<Vec<String>>>,
    pub ssh_executor: Arc<SshExecutor>,
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

        // Load all client IPs from database
        let client_ips = Self::load_client_ips(&pool).await?;
        info!("Loaded {} client IPs from database", client_ips.len());

        Ok(Self {
            settings: Arc::new(RwLock::new(settings)),
            db_pool: pool,
            config_path,
            client_ips: Arc::new(RwLock::new(client_ips)),
            ssh_executor: Arc::new(SshExecutor::new()),
        })
    }

    /// Load all client IPs from the database
    async fn load_client_ips(pool: &SqlitePool) -> anyhow::Result<Vec<String>> {
        let ips: Vec<String> = sqlx::query_scalar("SELECT ip FROM clients WHERE enabled = 1")
            .fetch_all(pool)
            .await?;
        Ok(ips)
    }

    /// Update the client IPs cache by reloading from database
    pub async fn refresh_client_ips(&self) -> anyhow::Result<()> {
        let ips = Self::load_client_ips(&self.db_pool).await?;
        let mut cache = self.client_ips.write().await;
        *cache = ips;
        Ok(())
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

        // Control operations audit log
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS control_operations (
                id TEXT PRIMARY KEY,
                client_id TEXT NOT NULL,
                client_name TEXT NOT NULL,
                client_ip TEXT NOT NULL,
                os_type TEXT NOT NULL,
                operation_type TEXT NOT NULL,
                operation_mode TEXT NOT NULL,
                delay_minutes INTEGER,
                administrator TEXT,
                result TEXT NOT NULL,
                result_message TEXT,
                duration_ms INTEGER,
                timestamp TEXT NOT NULL,
                FOREIGN KEY (client_id) REFERENCES clients(id)
            )
            "#,
        )
        .execute(pool)
        .await?;

        // Error log
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS error_logs (
                id TEXT PRIMARY KEY,
                client_id TEXT,
                operation_type TEXT NOT NULL,
                error_type TEXT NOT NULL,
                error_message TEXT NOT NULL,
                error_code TEXT,
                stack_trace TEXT,
                timestamp TEXT NOT NULL,
                FOREIGN KEY (client_id) REFERENCES clients(id)
            )
            "#,
        )
        .execute(pool)
        .await?;

        // Scheduled operations
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS scheduled_operations (
                id TEXT PRIMARY KEY,
                client_id TEXT NOT NULL,
                operation_type TEXT NOT NULL,
                operation_mode TEXT NOT NULL,
                scheduled_time TEXT NOT NULL,
                created_at TEXT NOT NULL,
                cancelled_at TEXT,
                result TEXT,
                FOREIGN KEY (client_id) REFERENCES clients(id)
            )
            "#,
        )
        .execute(pool)
        .await?;

        // OS type cache
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS os_type_cache (
                client_id TEXT PRIMARY KEY,
                os_type TEXT NOT NULL,
                detected_at TEXT NOT NULL,
                FOREIGN KEY (client_id) REFERENCES clients(id)
            )
            "#,
        )
        .execute(pool)
        .await?;

        // Users table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                username TEXT NOT NULL UNIQUE,
                password_hash TEXT NOT NULL,
                role TEXT NOT NULL DEFAULT 'user',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                last_login TEXT
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

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_control_ops_client ON control_operations(client_id)")
            .execute(pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_control_ops_timestamp ON control_operations(timestamp)")
            .execute(pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_error_logs_client ON error_logs(client_id)")
            .execute(pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_scheduled_ops_client ON scheduled_operations(client_id)")
            .execute(pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_users_username ON users(username)")
            .execute(pool)
            .await?;

        // Seed default admin user if no users exist
        Self::seed_default_admin(pool).await?;

        Ok(())
    }

    /// Seed default admin user if no users exist
    async fn seed_default_admin(pool: &SqlitePool) -> anyhow::Result<()> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(pool)
            .await?;

        if count == 0 {
            use bcrypt::{hash, DEFAULT_COST};
            use chrono::Utc;
            use uuid::Uuid;

            let password_hash = hash("admin123", DEFAULT_COST)?;
            let now = Utc::now().to_rfc3339();
            let id = Uuid::new_v4().to_string();

            sqlx::query(
                r#"
                INSERT INTO users (id, username, password_hash, role, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&id)
            .bind("admin")
            .bind(&password_hash)
            .bind("admin")
            .bind(&now)
            .bind(&now)
            .execute(pool)
            .await?;

            info!("Default admin user created (username: admin, password: admin123)");
        }

        Ok(())
    }
}
