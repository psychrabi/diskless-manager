-- Administrative audit trail for authenticated mutations and sign-in events.
-- Only routing metadata is stored; request bodies may contain credentials.
CREATE TABLE IF NOT EXISTS audit_events (
    id TEXT PRIMARY KEY,
    actor TEXT NOT NULL,
    actor_role TEXT,
    ip TEXT,
    module TEXT NOT NULL,
    action TEXT NOT NULL,
    method TEXT NOT NULL,
    path TEXT NOT NULL,
    status TEXT NOT NULL,
    status_code INTEGER NOT NULL,
    details TEXT,
    timestamp TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_events_timestamp ON audit_events(timestamp);
CREATE INDEX IF NOT EXISTS idx_audit_events_actor ON audit_events(actor);
CREATE INDEX IF NOT EXISTS idx_audit_events_module ON audit_events(module);
CREATE INDEX IF NOT EXISTS idx_audit_events_status ON audit_events(status);
