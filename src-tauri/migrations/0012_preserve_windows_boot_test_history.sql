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
    FOREIGN KEY (candidate_image_id) REFERENCES images(id)
);

INSERT INTO windows_revision_boot_tests_v2
    (id, revision_id, test_client_id, expected_nic_id, candidate_image_id,
     candidate_snapshot, status, reported_revision, reported_nic_id, result_at, created_at)
SELECT id, revision_id, test_client_id, expected_nic_id, candidate_image_id,
       candidate_snapshot, status, reported_revision, reported_nic_id, result_at, created_at
FROM windows_revision_boot_tests;

DROP TABLE windows_revision_boot_tests;
ALTER TABLE windows_revision_boot_tests_v2 RENAME TO windows_revision_boot_tests;
CREATE INDEX idx_boot_tests_revision ON windows_revision_boot_tests(revision_id);
CREATE INDEX idx_boot_tests_client ON windows_revision_boot_tests(test_client_id);
CREATE INDEX idx_boot_tests_candidate ON windows_revision_boot_tests(test_client_id, status, created_at);
CREATE UNIQUE INDEX idx_boot_tests_one_candidate_per_revision
    ON windows_revision_boot_tests(revision_id) WHERE status = 'candidate';
