DROP INDEX IF EXISTS idx_driver_revisions_source_hash;
CREATE UNIQUE INDEX idx_driver_revisions_source_snapshot_hash
    ON windows_image_driver_revisions(source_image_id, source_snapshot, driver_set_hash)
    WHERE status != 'failed';
