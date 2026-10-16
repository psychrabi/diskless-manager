CREATE TABLE client_boot_menu_backups (
    client_id TEXT PRIMARY KEY REFERENCES clients(id) ON DELETE CASCADE,
    contents TEXT NOT NULL
);
