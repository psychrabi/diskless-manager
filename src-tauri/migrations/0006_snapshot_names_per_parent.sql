-- Snapshot labels belong to a parent volume, not the entire image library.
CREATE TABLE images_new (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'master',
    os_type TEXT NOT NULL,
    size_gb INTEGER NOT NULL,
    path TEXT NOT NULL,
    format TEXT NOT NULL DEFAULT 'raw',
    status TEXT NOT NULL DEFAULT 'ready',
    description TEXT,
    parent_id TEXT,
    source_snapshot TEXT,
    tags TEXT,
    checksum TEXT,
    is_default INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

INSERT INTO images_new (
    id, name, kind, os_type, size_gb, path, format, status, description,
    parent_id, source_snapshot, tags, checksum, is_default, created_at, updated_at
)
SELECT id, name, kind, os_type, size_gb, path, format, status, description,
    parent_id, source_snapshot, tags, checksum, is_default, created_at, updated_at
FROM images;

DROP TABLE images;
ALTER TABLE images_new RENAME TO images;
CREATE INDEX idx_images_name ON images(name);
CREATE UNIQUE INDEX idx_images_volume_name ON images(name) WHERE kind != 'snapshot';
CREATE UNIQUE INDEX idx_images_snapshot_name ON images(parent_id, name) WHERE kind = 'snapshot';
