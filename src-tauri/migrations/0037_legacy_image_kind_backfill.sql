-- Unversioned image rows predate ImageKind and encoded snapshots/clones in
-- their free-text description. Reclassify those rows once, at the schema
-- layer, instead of replaying the backfill on every server start.
UPDATE images
SET kind = 'snapshot'
WHERE
    parent_id IS NOT NULL
    AND description LIKE 'Snapshot of %'
    AND (kind IS NULL OR kind = 'master');

UPDATE images
SET
    kind = 'clone',
    source_snapshot = CASE
        WHEN instr(description, '@') > 0
        THEN substr(description, instr(description, '@') + 1)
        ELSE source_snapshot
    END
WHERE
    parent_id IS NOT NULL
    AND description LIKE 'Clone of %@%'
    AND (kind IS NULL OR kind = 'master');
