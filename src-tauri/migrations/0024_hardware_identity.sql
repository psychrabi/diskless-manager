CREATE TABLE hardware_profiles (
    client_id TEXT PRIMARY KEY REFERENCES clients(id) ON DELETE CASCADE,
    profile_key TEXT NOT NULL,
    boot_nic_ids TEXT NOT NULL,
    architecture TEXT NOT NULL,
    firmware TEXT NOT NULL,
    observed_at TEXT NOT NULL
);
CREATE TABLE client_identity_caches (
    client_id TEXT PRIMARY KEY REFERENCES clients(id),
    id TEXT NOT NULL UNIQUE,
    profile_key TEXT NOT NULL,
    source_revision TEXT NOT NULL,
    dataset TEXT NOT NULL,
    snapshot TEXT NOT NULL,
    replacement TEXT NOT NULL,
    prior_dataset TEXT,
    state TEXT NOT NULL CHECK(state IN ('preparing','ready','applying','failed','stale','deleting')),
    last_error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
