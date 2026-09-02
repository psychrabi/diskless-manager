CREATE TABLE IF NOT EXISTS clients (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    mac TEXT NOT NULL UNIQUE,
    ip TEXT NOT NULL UNIQUE,
    master TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    snapshot TEXT,
    block_store TEXT,
    target_iqn TEXT,
    writeback TEXT,
    last_modified TEXT,
    block_device TEXT,
    status TEXT DEFAULT 'Offline',
    mode TEXT DEFAULT 'read-only',
    pxe_mode TEXT DEFAULT 'uefi',
    keep_writeback INTEGER NOT NULL DEFAULT 1,
    use_game_disk INTEGER NOT NULL DEFAULT 0
);

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
);

CREATE TABLE IF NOT EXISTS app_config (key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS image_versions (
    id TEXT PRIMARY KEY, base_name TEXT NOT NULL, version TEXT NOT NULL,
    image_id TEXT NOT NULL, parent_version_id TEXT, changelog TEXT,
    is_latest INTEGER NOT NULL DEFAULT 0, is_stable INTEGER NOT NULL DEFAULT 0,
    created_by TEXT, tags TEXT, created_at TEXT NOT NULL,
    UNIQUE(base_name, version)
);

CREATE TABLE IF NOT EXISTS boot_logs (
    id TEXT PRIMARY KEY, client_id TEXT NOT NULL, image_id TEXT,
    boot_time TEXT NOT NULL, success INTEGER NOT NULL, duration_ms INTEGER,
    message TEXT, FOREIGN KEY (client_id) REFERENCES clients(id)
);

CREATE TABLE IF NOT EXISTS control_operations (
    id TEXT PRIMARY KEY, client_id TEXT NOT NULL, client_name TEXT NOT NULL,
    client_ip TEXT NOT NULL, os_type TEXT NOT NULL, operation_type TEXT NOT NULL,
    operation_mode TEXT NOT NULL, delay_minutes INTEGER, administrator TEXT,
    result TEXT NOT NULL, result_message TEXT, duration_ms INTEGER,
    timestamp TEXT NOT NULL, FOREIGN KEY (client_id) REFERENCES clients(id)
);

CREATE TABLE IF NOT EXISTS error_logs (
    id TEXT PRIMARY KEY, client_id TEXT, operation_type TEXT NOT NULL,
    error_type TEXT NOT NULL, error_message TEXT NOT NULL, error_code TEXT,
    stack_trace TEXT, timestamp TEXT NOT NULL,
    FOREIGN KEY (client_id) REFERENCES clients(id)
);

CREATE TABLE IF NOT EXISTS scheduled_operations (
    id TEXT PRIMARY KEY, client_id TEXT NOT NULL, operation_type TEXT NOT NULL,
    operation_mode TEXT NOT NULL, scheduled_time TEXT NOT NULL, created_at TEXT NOT NULL,
    cancelled_at TEXT, result TEXT, FOREIGN KEY (client_id) REFERENCES clients(id)
);

CREATE TABLE IF NOT EXISTS os_type_cache (
    client_id TEXT PRIMARY KEY, os_type TEXT NOT NULL, detected_at TEXT NOT NULL,
    FOREIGN KEY (client_id) REFERENCES clients(id)
);

CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY, username TEXT NOT NULL UNIQUE, password_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'user', created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
    last_login TEXT
);

CREATE INDEX IF NOT EXISTS idx_clients_mac ON clients(mac);
CREATE INDEX IF NOT EXISTS idx_images_name ON images(name);
CREATE INDEX IF NOT EXISTS idx_boot_logs_client ON boot_logs(client_id);
CREATE INDEX IF NOT EXISTS idx_control_ops_client ON control_operations(client_id);
CREATE INDEX IF NOT EXISTS idx_control_ops_timestamp ON control_operations(timestamp);
CREATE INDEX IF NOT EXISTS idx_error_logs_client ON error_logs(client_id);
CREATE INDEX IF NOT EXISTS idx_scheduled_ops_client ON scheduled_operations(client_id);
CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
