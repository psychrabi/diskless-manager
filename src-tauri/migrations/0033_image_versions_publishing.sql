-- Add the published-version record from ADR 0005 to image_versions.
--
-- Migration 0001 introduced image_versions for an image pipeline that never
-- shipped: no code path ever inserted or read it. Publishing a version needs
-- the asset it points at, provenance, pin/lock and base/version identity, so
-- the missing columns are added in place. The legacy table is not renamed or
-- rebuilt: renaming forces SQLite to recompile every trigger in the schema,
-- including triggers on tables this migration does not touch.
--
-- Existing rows (only reachable if an external tool wrote them) keep their
-- base name and version and inherit the OS type of their image asset.
ALTER TABLE image_versions ADD COLUMN name TEXT;
ALTER TABLE image_versions ADD COLUMN image_asset_id TEXT;
ALTER TABLE image_versions ADD COLUMN os_type TEXT;
ALTER TABLE image_versions ADD COLUMN source_description TEXT;
ALTER TABLE image_versions ADD COLUMN comment TEXT;
ALTER TABLE image_versions ADD COLUMN is_pinned INTEGER NOT NULL DEFAULT 0;
ALTER TABLE image_versions ADD COLUMN is_locked INTEGER NOT NULL DEFAULT 0;
ALTER TABLE image_versions ADD COLUMN ordering INTEGER NOT NULL DEFAULT 0;
ALTER TABLE image_versions ADD COLUMN published_by TEXT;
ALTER TABLE image_versions ADD COLUMN published_at TEXT;
ALTER TABLE image_versions ADD COLUMN last_used_at TEXT;
ALTER TABLE image_versions ADD COLUMN last_used_by_client_id TEXT;
ALTER TABLE image_versions ADD COLUMN updated_at TEXT;

UPDATE image_versions
SET name = base_name || '/' || version,
    image_asset_id = image_id,
    os_type = COALESCE(
        (SELECT images.os_type FROM images WHERE images.id = image_versions.image_id),
        'windows'
    ),
    source_description = changelog,
    published_at = created_at,
    updated_at = created_at
WHERE name IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_image_versions_identity
    ON image_versions(base_name, version);

CREATE INDEX IF NOT EXISTS idx_image_versions_asset
    ON image_versions(image_asset_id);

CREATE INDEX IF NOT EXISTS idx_image_versions_parent
    ON image_versions(parent_version_id);
