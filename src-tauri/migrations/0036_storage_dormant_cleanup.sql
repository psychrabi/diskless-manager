-- Audit for reclaiming writeback datasets whose client no longer exists.
--
-- The table deliberately has no foreign key: the dataset and its client are
-- gone by the time the row is read, but the explanation for a destructive run
-- has to survive.
CREATE TABLE IF NOT EXISTS storage_dormant_cleanup_events (
    id TEXT PRIMARY KEY,
    actor TEXT,
    action TEXT NOT NULL CHECK (action IN ('planned', 'deleted', 'skipped', 'failed')),
    dataset TEXT NOT NULL,
    client_name TEXT NOT NULL,
    reason TEXT,
    error TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_storage_dormant_cleanup_events_created
    ON storage_dormant_cleanup_events(created_at);
