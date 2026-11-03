-- Per-client offline personalization applied to that client's own Windows
-- writeback clone. The plan is a configuration input, not proof that it was
-- applied: `last_applied_at` and `last_error` record the outcome of the last
-- attempt so a client detail view can show a stale or failing plan.
--
-- The table holds no secrets. Removing the row (or every option being empty)
-- means the client inherits the image identity unchanged.
CREATE TABLE IF NOT EXISTS client_windows_personalization (
    client_id TEXT PRIMARY KEY REFERENCES clients(id) ON DELETE CASCADE,
    computer_name TEXT,
    convert_static_ipv4_to_dhcp INTEGER NOT NULL DEFAULT 0,
    clear_stale_drive_letters INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL,
    last_applied_at INTEGER,
    last_error TEXT
);
