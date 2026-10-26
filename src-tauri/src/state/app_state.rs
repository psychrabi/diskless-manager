use super::ClientMutationLocks;
use crate::application::ApplicationServices;
use crate::core::config::Settings;
use crate::metrics::MetricsCollector;
use crate::ssh_executor::SshExecutor;
use log::{info, warn};
use sqlx::sqlite::SqlitePool;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub config: crate::config::RuntimeConfig,
    /// Serializes mutations per client and provides a barrier for fleet-wide operations.
    pub client_mutations: ClientMutationLocks,
    pub settings: Arc<RwLock<Settings>>,
    pub db_pool: SqlitePool,
    pub config_path: PathBuf,
    pub client_ips: Arc<RwLock<Vec<String>>>,
    /// Shared source of truth for REST and WebSocket traffic measurements.
    pub metrics_collector: Arc<MetricsCollector>,
    pub ssh_executor: Arc<SshExecutor>,
    pub application: Arc<ApplicationServices>,
}

impl AppState {
    pub fn hardware_identity_service(
        &self,
    ) -> crate::application::hardware_identity_service::HardwareIdentityService {
        crate::application::hardware_identity_service::HardwareIdentityService::new(
            crate::application::hardware_identity_service::HardwareIdentityDeps {
                repository: crate::persistence::repositories::hardware_identity::HardwareIdentityRepository::new(self.db_pool.clone()),
                clients: self.application.client_repository.clone(),
                backend: self.application.image_backend.clone(),
                storage: self.application.storage.clone(),
                iscsi: self.application.iscsi.clone(),
                mutation_locks: self.client_mutations.clone(),
            }
        )
    }
    /// Builds the installer-media service from the current configured boot root and upload cap.
    pub async fn installer_media_service(
        &self,
    ) -> crate::application::installer_media_service::InstallerMediaService {
        let settings = self.settings.read().await;
        crate::application::installer_media_service::InstallerMediaService::new(
            &settings.http.root_dir,
            settings.installer_media.max_upload_bytes as u64,
            self.application.installer_media.clone(),
            self.application.super_sessions.clone(),
        )
    }

    pub async fn super_client_service(
        &self,
    ) -> crate::application::super_client_service::SuperClientService {
        let settings = self.settings.read().await.clone();
        crate::application::super_client_service::SuperClientService::new(
            crate::application::super_client_service::SuperClientServiceDeps {
                clients: self.application.client_repository.clone(),
                media: self.application.installer_media.clone(),
                sessions: self.application.super_sessions.clone(),
                images: self.application.images.clone(),
                storage: self.application.storage.clone(),
                iscsi: self.application.iscsi.clone(),
                config: self.config.clone(),
                settings,
                media_service: self.installer_media_service().await,
                mutation_locks: self.client_mutations.clone(),
            },
        )
    }

    pub async fn new() -> anyhow::Result<Self> {
        // Determine config directory - use same as existing diskless-manager
        let config_dir = dirs::config_dir()
            .map(|path| path.join("com.diskless.local"))
            .unwrap_or_else(|| PathBuf::from("./config"));

        std::fs::create_dir_all(&config_dir)?;
        #[cfg(unix)]
        std::fs::set_permissions(
            &config_dir,
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )?;
        crate::api::handlers::backup::apply_pending_restore(&config_dir).await?;
        crate::auth::initialize_jwt_secret(&config_dir)?;

        let config_path = config_dir.join("config.json");

        let mut settings = Settings::load(&config_path.with_extension("toml"))?;

        // Initialize database in the same directory
        let db_path = config_dir.join("diskless.db");
        let db_url = format!("sqlite:{}?mode=rwc", db_path.display());

        let pool = SqlitePool::connect(&db_url).await?;
        #[cfg(unix)]
        std::fs::set_permissions(
            &db_path,
            std::os::unix::fs::PermissionsExt::from_mode(0o600),
        )?;

        // Run migrations
        Self::init_database(&pool).await?;
        let runtime_config = crate::config::RuntimeConfig::default();
        // Load the instance-owned configuration snapshot.
        if let Ok(config) = crate::config::read_config_db(&pool).await {
            crate::config::set_config(&runtime_config, &config);
            // Sync settings from DB to the current settings struct if available
            if let Ok(db_settings) = serde_json::from_value::<Settings>(config.settings) {
                if db_settings.validate().is_ok() {
                    info!("Merged settings from database");
                    settings = db_settings;
                } else {
                    log::warn!("Ignored unsafe settings loaded from database");
                }
            }
        }

        let mut config = crate::config::get_config(&runtime_config);
        let typed_settings = serde_json::to_value(&settings)?;
        if let (Some(existing), Some(typed)) =
            (config.settings.as_object_mut(), typed_settings.as_object())
        {
            existing.extend(typed.clone());
        } else {
            config.settings = typed_settings;
        }
        crate::config::set_config(&runtime_config, &config);

        let application = Arc::new(ApplicationServices::with_iscsi_backend(
            pool.clone(),
            settings.iscsi.backend,
            runtime_config.clone(),
        ));
        crate::application::installer_media_service::InstallerMediaService::new(
            &settings.http.root_dir,
            settings.installer_media.max_upload_bytes as u64,
            application.installer_media.clone(),
            application.super_sessions.clone(),
        )
        .reconcile_storage()
        .await?;

        info!("Database initialized at {}", db_path.display());

        // Load all client IPs from database
        let client_ips = Self::load_client_ips(&pool).await?;
        info!("Loaded {} client IPs from database", client_ips.len());

        let state = Self {
            config: runtime_config,
            client_mutations: ClientMutationLocks::default(),
            settings: Arc::new(RwLock::new(settings)),
            db_pool: pool,
            config_path,
            client_ips: Arc::new(RwLock::new(client_ips)),
            metrics_collector: Arc::new(MetricsCollector::default()),
            ssh_executor: Arc::new(SshExecutor::new()),
            application,
        };
        state
            .super_client_service()
            .await
            .recover_incomplete()
            .await?;
        crate::persistence::repositories::installation::InstallationRepository::new(
            state.db_pool.clone(),
        )
        .recover_interrupted()
        .await?;
        state.hardware_identity_service().recover_pending().await?;
        let interrupted =
            crate::application::windows_servicing::recover_interrupted_revisions(&state.db_pool)
                .await?;
        if interrupted > 0 {
            warn!("Abandoned {interrupted} interrupted Windows driver revision(s)");
        }
        Ok(state)
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
        Self::ensure_legacy_columns(pool).await?;

        static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
        MIGRATOR.run(pool).await?;

        Self::ensure_clients_columns(pool).await?;
        Self::migrate_legacy_images(pool).await?;
        Self::create_support_tables(pool).await?;
        Self::create_indexes(pool).await?;

        Ok(())
    }

    /// Add columns that must exist before the versioned migration runner executes.
    async fn ensure_legacy_columns(pool: &SqlitePool) -> anyhow::Result<()> {
        // Legacy unversioned images need these columns before the table-rebuild migration.
        let images_exist: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'images'",
        )
        .fetch_one(pool)
        .await?;
        if images_exist != 0 {
            Self::ensure_column(pool, "images", "kind", "TEXT NOT NULL DEFAULT 'master'").await?;
            Self::ensure_column(pool, "images", "source_snapshot", "TEXT").await?;
        }
        // Migration 0011 rebuilds boot-test rows and reads clients.snapshot.
        // Older unversioned databases can have clients before that column was
        // introduced, so add it before the migration runner reaches 0011.
        let clients_exist: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'clients'",
        )
        .fetch_one(pool)
        .await?;
        if clients_exist != 0 {
            Self::ensure_column(pool, "clients", "snapshot", "TEXT").await?;
        }
        Ok(())
    }
    async fn ensure_clients_columns(pool: &SqlitePool) -> anyhow::Result<()> {
        for (column, definition) in [
            ("snapshot", "TEXT"),
            ("block_store", "TEXT"),
            ("target_iqn", "TEXT"),
            ("writeback", "TEXT"),
            ("last_modified", "TEXT"),
            ("block_device", "TEXT"),
            ("status", "TEXT DEFAULT 'Offline'"),
            ("mode", "TEXT DEFAULT 'read-only'"),
            ("pxe_mode", "TEXT DEFAULT 'uefi'"),
            ("keep_writeback", "INTEGER NOT NULL DEFAULT 1"),
            ("use_game_disk", "INTEGER NOT NULL DEFAULT 0"),
            ("chap_user", "TEXT"),
            ("chap_secret", "TEXT"),
            ("chap_enabled", "INTEGER NOT NULL DEFAULT 0"),
            (
                "installer_media_id",
                "TEXT REFERENCES installer_media(id) ON DELETE RESTRICT",
            ),
        ] {
            Self::ensure_column(pool, "clients", column, definition).await?;
        }
        Ok(())
    }
    /// Backfill the ImageKind columns and reclassify legacy image records.
    async fn migrate_legacy_images(pool: &SqlitePool) -> anyhow::Result<()> {
        sqlx::query(
            r#"
        CREATE TABLE IF NOT EXISTS images (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            kind TEXT NOT NULL DEFAULT 'master',
            os_type TEXT NOT NULL,
            size_gb INTEGER NOT NULL,
            path TEXT NOT NULL,
            format TEXT NOT NULL DEFAULT 'raw',
            status TEXT NOT NULL DEFAULT 'ready',
            description TEXT,
            parent_id TEXT,
            source_snapshot TEXT,
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

        let has_kind: i64 = sqlx::query_scalar(
            r#"
        SELECT COUNT(*)
        FROM pragma_table_info('images')
        WHERE name = 'kind'
        "#,
        )
        .fetch_one(pool)
        .await?;

        if has_kind == 0 {
            sqlx::query(
                r#"
            ALTER TABLE images
            ADD COLUMN kind TEXT NOT NULL DEFAULT 'master'
            "#,
            )
            .execute(pool)
            .await?;
        }

        let has_source_snapshot: i64 = sqlx::query_scalar(
            r#"
        SELECT COUNT(*)
        FROM pragma_table_info('images')
        WHERE name = 'source_snapshot'
        "#,
        )
        .fetch_one(pool)
        .await?;

        if has_source_snapshot == 0 {
            sqlx::query(
                r#"
            ALTER TABLE images
            ADD COLUMN source_snapshot TEXT
            "#,
            )
            .execute(pool)
            .await?;
        }

        // ---------------------------------------------------------------------
        // Migrate legacy image records.
        //
        // Before ImageKind existed, parent_id was used for both snapshots
        // and clones. Detect the old records from their descriptions.
        // ---------------------------------------------------------------------

        sqlx::query(
            r#"
        UPDATE images
        SET kind = 'snapshot'
        WHERE
            parent_id IS NOT NULL
            AND description LIKE 'Snapshot of %'
            AND (kind IS NULL OR kind = 'master')
        "#,
        )
        .execute(pool)
        .await?;

        sqlx::query(
            r#"
        UPDATE images
        SET
            kind = 'clone',
            source_snapshot = CASE
                WHEN instr(description, '@') > 0
                THEN substr(description, instr(description, '@') + 1)
                ELSE source_snapshot
            END
        WHERE
            parent_id IS NOT NULL
            AND description LIKE 'Clone of %@%'
            AND (kind IS NULL OR kind = 'master')
        "#,
        )
        .execute(pool)
        .await?;
        Ok(())
    }
    async fn create_support_tables(pool: &SqlitePool) -> anyhow::Result<()> {
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
        Ok(())
    }
    async fn create_indexes(pool: &SqlitePool) -> anyhow::Result<()> {
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_clients_mac ON clients(mac)")
            .execute(pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_images_name ON images(name)")
            .execute(pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_boot_logs_client ON boot_logs(client_id)")
            .execute(pool)
            .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_control_ops_client ON control_operations(client_id)",
        )
        .execute(pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_control_ops_timestamp ON control_operations(timestamp)",
        )
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
        Ok(())
    }

    async fn ensure_column(
        pool: &SqlitePool,
        table: &str,
        column: &str,
        definition: &str,
    ) -> anyhow::Result<()> {
        let query = format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = ?");
        let exists: i64 = sqlx::query_scalar(&query)
            .bind(column)
            .fetch_one(pool)
            .await?;
        if exists == 0 {
            sqlx::query(&format!(
                "ALTER TABLE {table} ADD COLUMN {column} {definition}"
            ))
            .execute(pool)
            .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod migration_tests {
    use super::AppState;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn memory_pool() -> sqlx::SqlitePool {
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory database should open")
    }

    #[tokio::test]
    async fn fresh_database_contains_the_complete_runtime_schema() {
        let pool = memory_pool().await;

        AppState::init_database(&pool)
            .await
            .expect("fresh database should migrate");

        let required = [
            "clients",
            "images",
            "users",
            "app_config",
            "_sqlx_migrations",
        ];
        for table in required {
            let exists: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?",
            )
            .bind(table)
            .fetch_one(&pool)
            .await
            .expect("schema should be readable");
            assert_eq!(exists, 1, "missing table: {table}");
        }
    }

    #[tokio::test]
    async fn snapshot_names_are_unique_per_parent_after_migration() {
        let pool = memory_pool().await;
        sqlx::raw_sql(include_str!("../../migrations/0001_base_schema.sql"))
            .execute(&pool)
            .await
            .unwrap();
        let insert = "INSERT INTO images (id, name, kind, os_type, size_gb, path, parent_id, tags, created_at, updated_at) VALUES (?, ?, ?, 'linux', 50, '/dev/zvol/test', ?, 'preserve-me', 'now', 'now')";
        for (id, name, kind, parent) in [
            ("fedora", "fedora44", "master", None),
            ("kubuntu", "kubuntu", "master", None),
            ("fedora-base", "base", "snapshot", Some("fedora")),
        ] {
            sqlx::query(insert)
                .bind(id)
                .bind(name)
                .bind(kind)
                .bind(parent)
                .execute(&pool)
                .await
                .unwrap();
        }
        AppState::init_database(&pool).await.unwrap();
        sqlx::query(insert)
            .bind("kubuntu-base")
            .bind("base")
            .bind("snapshot")
            .bind("kubuntu")
            .execute(&pool)
            .await
            .expect("different images may both have a base snapshot");
        for (id, name, kind, parent) in [
            ("duplicate-snapshot", "base", "snapshot", Some("kubuntu")),
            ("duplicate-master", "kubuntu", "master", None),
        ] {
            assert!(sqlx::query(insert)
                .bind(id)
                .bind(name)
                .bind(kind)
                .bind(parent)
                .execute(&pool)
                .await
                .is_err());
        }
        let tags: String = sqlx::query_scalar("SELECT tags FROM images WHERE id = 'fedora-base'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(tags, "preserve-me");
        AppState::init_database(&pool)
            .await
            .expect("migration is restartable");
    }

    #[tokio::test]
    async fn migration_preserves_existing_clients_and_image_identity() {
        let pool = memory_pool().await;
        sqlx::query(
            r#"
            CREATE TABLE clients (
                id TEXT PRIMARY KEY, name TEXT NOT NULL, mac TEXT NOT NULL, ip TEXT NOT NULL,
                master TEXT NOT NULL, enabled INTEGER NOT NULL, created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("legacy clients table should be created");
        sqlx::query(
            "INSERT INTO clients (id, name, mac, ip, master, enabled, created_at, updated_at) VALUES ('client-1', 'PC-01', '00:11:22:33:44:55', '192.168.1.101', 'win11', 1, 'now', 'now')",
        )
        .execute(&pool)
        .await
        .expect("legacy client should be inserted");
        sqlx::query(
            r#"
            CREATE TABLE images (
                id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE, os_type TEXT NOT NULL,
                size_gb INTEGER NOT NULL, path TEXT NOT NULL, format TEXT NOT NULL,
                status TEXT NOT NULL, description TEXT, parent_id TEXT, tags TEXT,
                checksum TEXT, is_default INTEGER NOT NULL, created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("legacy images table should be created");
        sqlx::query("INSERT INTO images VALUES ('image-1', 'Windows 11', 'windows', 64, '/tank/win11', 'raw', 'ready', NULL, NULL, NULL, NULL, 1, 'now', 'now')")
            .execute(&pool)
            .await
            .expect("legacy image should be inserted");

        AppState::init_database(&pool)
            .await
            .expect("legacy database should migrate");

        let client_name: String =
            sqlx::query_scalar("SELECT name FROM clients WHERE id = 'client-1'")
                .fetch_one(&pool)
                .await
                .expect("client should remain");
        let image_name: String = sqlx::query_scalar("SELECT name FROM images WHERE id = 'image-1'")
            .fetch_one(&pool)
            .await
            .expect("image should remain");
        assert_eq!(client_name, "PC-01");
        assert_eq!(image_name, "Windows 11");

        for column in [
            "snapshot",
            "target_iqn",
            "pxe_mode",
            "keep_writeback",
            "use_game_disk",
            "chap_user",
            "chap_secret",
            "chap_enabled",
            "installer_media_id",
        ] {
            let exists: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM pragma_table_info('clients') WHERE name = ?",
            )
            .bind(column)
            .fetch_one(&pool)
            .await
            .expect("client schema should be readable");
            assert_eq!(exists, 1, "missing migrated client column: {column}");
        }

        let game_table: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'client_game_disks'",
        )
        .fetch_one(&pool)
        .await
        .expect("schema should be readable");
        assert_eq!(game_table, 1, "missing client_game_disks table");
    }
}
