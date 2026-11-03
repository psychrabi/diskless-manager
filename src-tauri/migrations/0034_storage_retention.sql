-- Retention audit for image-snapshot cleanup (Phase 4A).
--
-- Retention is preview-first: a plan is recorded before any snapshot is
-- destroyed, so a destructive run can be reconstructed afterwards. The table
-- deliberately has no foreign key to images: the snapshot it describes is
-- usually gone by the time the row is read.
CREATE TABLE IF NOT EXISTS storage_retention_events (
    id TEXT PRIMARY KEY,
    actor TEXT,
    action TEXT NOT NULL CHECK (action IN ('planned', 'deleted', 'skipped', 'failed')),
    dataset TEXT NOT NULL,
    snapshot TEXT NOT NULL,
    reason TEXT,
    error TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_storage_retention_events_created
    ON storage_retention_events(created_at);
