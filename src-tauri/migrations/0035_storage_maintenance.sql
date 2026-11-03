-- Restart-safe storage maintenance history (Phase 5).
--
-- A run row is written before the command starts and finalised by the sweep, so
-- a server restart during a scrub/TRIM leaves an explainable record instead of
-- an in-memory only operation. Rows are never rewritten in place except to
-- finalise state.
CREATE TABLE IF NOT EXISTS storage_maintenance_runs (
    id TEXT PRIMARY KEY,
    pool TEXT NOT NULL,
    operation TEXT NOT NULL CHECK (operation IN ('scrub', 'trim')),
    state TEXT NOT NULL CHECK (state IN ('running', 'completed', 'failed', 'cancelled')),
    actor TEXT,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    detail TEXT
);

CREATE INDEX IF NOT EXISTS idx_storage_maintenance_runs_started
    ON storage_maintenance_runs(started_at);

CREATE INDEX IF NOT EXISTS idx_storage_maintenance_runs_active
    ON storage_maintenance_runs(state, pool);
