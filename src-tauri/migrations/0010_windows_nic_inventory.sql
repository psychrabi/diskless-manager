CREATE TABLE IF NOT EXISTS windows_nic_inventory (
    mac TEXT PRIMARY KEY,
    client_id TEXT,
    boot_nic_ids TEXT NOT NULL DEFAULT '[]',
    secondary_ids TEXT NOT NULL DEFAULT '[]',
    architecture TEXT NOT NULL DEFAULT '',
    firmware TEXT NOT NULL DEFAULT '',
    observed_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_nic_inventory_client ON windows_nic_inventory(client_id);
ALTER TABLE clients ADD COLUMN inventory_boot_requested INTEGER NOT NULL DEFAULT 0;
