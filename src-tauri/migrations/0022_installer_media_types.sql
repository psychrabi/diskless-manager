-- Replace the checked column without dropping the referenced media table.
-- SQLx keeps the schema change, catalog data, and migration record atomic.
DROP INDEX idx_installer_media_default_type;
ALTER TABLE installer_media RENAME COLUMN media_type TO legacy_media_type;
ALTER TABLE installer_media ADD COLUMN media_type TEXT NOT NULL DEFAULT 'winpe'
    CHECK (media_type IN ('windows_iso', 'winpe', 'linux_iso', 'other_iso'));
UPDATE installer_media
SET media_type = CASE legacy_media_type
    WHEN 'linux_live' THEN 'linux_iso'
    ELSE legacy_media_type
END;
ALTER TABLE installer_media DROP COLUMN legacy_media_type;

-- Preserve the existing single-default selection rule when upgrading catalogs
-- that previously allowed one default per type.
UPDATE installer_media SET is_default = 0
WHERE (SELECT COUNT(*) FROM installer_media WHERE is_default = 1) > 1;
CREATE UNIQUE INDEX idx_installer_media_default
    ON installer_media(is_default) WHERE is_default = 1;
