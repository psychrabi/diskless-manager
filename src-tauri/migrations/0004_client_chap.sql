-- Per-client iSCSI CHAP credentials.
--
-- `chap_enabled` gates enforcement: existing rows default to 0 (open,
-- legacy behavior) while new clients are created with enforcement on.
-- The secret is server-generated; clients never supply it. The per-client
-- boot menu carries the credentials so unattended boot keeps working.
ALTER TABLE clients ADD COLUMN chap_user TEXT;
ALTER TABLE clients ADD COLUMN chap_secret TEXT;
ALTER TABLE clients ADD COLUMN chap_enabled INTEGER NOT NULL DEFAULT 0;
