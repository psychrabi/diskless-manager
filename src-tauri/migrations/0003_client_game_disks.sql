-- Per-client game-disk selection.
--
-- `clients.use_game_disk` remains the master switch. This table stores the
-- explicit per-client selection of game master datasets. An empty selection
-- combined with `use_game_disk = 1` resolves to all discovered masters at
-- provision time (see StorageService::resolve_game_selection).
CREATE TABLE IF NOT EXISTS client_game_disks (
    client_id TEXT NOT NULL REFERENCES clients(id) ON DELETE CASCADE,
    master_dataset TEXT NOT NULL,
    PRIMARY KEY (client_id, master_dataset)
);
