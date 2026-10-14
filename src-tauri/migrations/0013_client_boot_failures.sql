CREATE TABLE IF NOT EXISTS client_boot_failures (
    client_id TEXT PRIMARY KEY REFERENCES clients(id) ON DELETE CASCADE,
    failures INTEGER NOT NULL DEFAULT 0,
    last_failed_at INTEGER NOT NULL,
    last_error TEXT NOT NULL
);
