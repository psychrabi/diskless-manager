CREATE TABLE IF NOT EXISTS windows_image_driver_revisions (
    id TEXT PRIMARY KEY,
    source_image_id TEXT NOT NULL,
    source_snapshot TEXT NOT NULL,
    revision_image_id TEXT,
    driver_package_ids TEXT NOT NULL DEFAULT '[]',
    driver_set_hash TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'preparing' CHECK (status IN ('preparing', 'awaiting-test', 'ready', 'failed')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (source_image_id) REFERENCES images(id)
);
CREATE INDEX IF NOT EXISTS idx_driver_revisions_source ON windows_image_driver_revisions(source_image_id);
CREATE INDEX IF NOT EXISTS idx_driver_revisions_hash ON windows_image_driver_revisions(driver_set_hash);
CREATE INDEX IF NOT EXISTS idx_driver_revisions_status ON windows_image_driver_revisions(status);
CREATE UNIQUE INDEX IF NOT EXISTS idx_driver_revisions_source_hash ON windows_image_driver_revisions(source_image_id, driver_set_hash);
