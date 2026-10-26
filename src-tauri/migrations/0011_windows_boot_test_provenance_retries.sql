CREATE TABLE windows_revision_boot_tests_v2 (
    id TEXT PRIMARY KEY,
    revision_id TEXT NOT NULL,
    test_client_id TEXT NOT NULL,
    expected_nic_id TEXT NOT NULL,
    candidate_image_id TEXT,
    candidate_snapshot TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'candidate' CHECK (status IN ('candidate', 'passed', 'failed')),
    reported_revision TEXT,
    reported_nic_id TEXT,
    result_at TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (revision_id) REFERENCES windows_image_driver_revisions(id),
    FOREIGN KEY (test_client_id) REFERENCES clients(id),
    FOREIGN KEY (candidate_image_id) REFERENCES images(id)
);
INSERT INTO windows_revision_boot_tests_v2
    (id, revision_id, test_client_id, expected_nic_id, candidate_image_id,
     candidate_snapshot, status, reported_revision, reported_nic_id, result_at, created_at)
SELECT tests.id, tests.revision_id, tests.test_client_id, tests.expected_nic_id,
       revisions.revision_image_id,
       CASE WHEN revisions.revision_image_id IS NOT NULL AND (
           SELECT COUNT(*) FROM images AS snapshots
           WHERE snapshots.parent_id = revisions.revision_image_id
             AND snapshots.kind = 'snapshot' AND snapshots.name LIKE 'serviced-%'
             AND snapshots.status = 'ready'
       ) = 1 THEN COALESCE((
           SELECT images.name || '@' || snapshots.name
           FROM images JOIN images AS snapshots ON snapshots.parent_id = images.id
           WHERE images.id = revisions.revision_image_id
             AND snapshots.kind = 'snapshot' AND snapshots.name LIKE 'serviced-%'
             AND snapshots.status = 'ready'
             AND EXISTS (
                 SELECT 1 FROM clients
                 WHERE clients.id = tests.test_client_id
                   AND clients.master = images.name
                   AND clients.snapshot = images.name || '@' || snapshots.name
             )
       ), '') ELSE '' END,
       tests.status, tests.reported_revision,
       tests.reported_nic_id, tests.result_at, tests.created_at
FROM windows_revision_boot_tests AS tests
JOIN windows_image_driver_revisions AS revisions ON revisions.id = tests.revision_id;
DROP TABLE windows_revision_boot_tests;
ALTER TABLE windows_revision_boot_tests_v2 RENAME TO windows_revision_boot_tests;
UPDATE windows_revision_boot_tests
SET status = 'failed', result_at = COALESCE(result_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
WHERE status = 'candidate'
  AND (candidate_image_id IS NULL OR candidate_snapshot = '');
CREATE INDEX idx_boot_tests_revision ON windows_revision_boot_tests(revision_id);
CREATE INDEX idx_boot_tests_client ON windows_revision_boot_tests(test_client_id);
CREATE INDEX idx_boot_tests_candidate ON windows_revision_boot_tests(test_client_id, status, created_at);
CREATE UNIQUE INDEX idx_boot_tests_one_candidate_per_revision
    ON windows_revision_boot_tests(revision_id) WHERE status = 'candidate';
