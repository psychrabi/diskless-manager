-- Prewarmed, job-owned replacement clones for a client's non-persistent
-- writeback. A spare is never a client's active storage: it becomes boot
-- storage only after the journaled adoption path renames and verifies it.
CREATE TABLE IF NOT EXISTS client_writeback_spares (
    client_id TEXT PRIMARY KEY REFERENCES clients(id) ON DELETE CASCADE,
    dataset TEXT NOT NULL UNIQUE,
    source_snapshot TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    validated_at INTEGER,
    retry_after INTEGER NOT NULL DEFAULT 0,
    last_error TEXT
);

CREATE INDEX IF NOT EXISTS idx_client_writeback_spares_status
    ON client_writeback_spares(status);
