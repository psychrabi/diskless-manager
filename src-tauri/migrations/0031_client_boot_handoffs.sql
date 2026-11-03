-- One active iPXE-to-OS handoff per client. iPXE closes its iSCSI session
-- before the kernel or Windows opens its own to the same LUN, so a missing
-- session inside this window means "still booting" and must not start the
-- non-persistent reset timer. The window is bounded from `started_at` so
-- repeated PXE requests cannot extend it, and `expiry_reported` makes the
-- diagnostic fire once per attempt.
CREATE TABLE IF NOT EXISTS client_boot_handoffs (
    client_id TEXT PRIMARY KEY REFERENCES clients(id) ON DELETE CASCADE,
    attempt_id TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    phase TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    expiry_reported INTEGER NOT NULL DEFAULT 0,
    last_error TEXT
);

CREATE INDEX IF NOT EXISTS idx_client_boot_handoffs_expiry
    ON client_boot_handoffs(expiry_reported, expires_at);
