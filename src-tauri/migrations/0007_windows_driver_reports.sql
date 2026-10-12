CREATE TABLE IF NOT EXISTS windows_driver_reports (
    client_id TEXT PRIMARY KEY,
    windows_version TEXT NOT NULL,
    architecture TEXT NOT NULL,
    boot_nic_ids TEXT NOT NULL DEFAULT '[]',
    package_ids TEXT NOT NULL DEFAULT '[]',
    peer_address TEXT NOT NULL,
    reported_at TEXT NOT NULL,
    FOREIGN KEY (client_id) REFERENCES clients(id)
);
