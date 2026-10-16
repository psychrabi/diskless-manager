CREATE TABLE legacy_super_exits (
    client_id TEXT PRIMARY KEY REFERENCES clients(id) ON DELETE RESTRICT,
    prior_master TEXT NOT NULL,
    prior_snapshot TEXT,
    selected_snapshot TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('pending', 'recovery_required')),
    last_error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
