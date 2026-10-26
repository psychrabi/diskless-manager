CREATE TABLE IF NOT EXISTS windows_revision_boot_tests (
    id TEXT PRIMARY KEY,
    revision_id TEXT NOT NULL UNIQUE,
    test_client_id TEXT NOT NULL,
    expected_nic_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'candidate' CHECK (status IN ('candidate', 'passed', 'failed')),
    reported_revision TEXT,
    reported_nic_id TEXT,
    result_at TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (revision_id) REFERENCES windows_image_driver_revisions(id),
    FOREIGN KEY (test_client_id) REFERENCES clients(id)
);
CREATE INDEX IF NOT EXISTS idx_boot_tests_revision ON windows_revision_boot_tests(revision_id);
CREATE INDEX IF NOT EXISTS idx_boot_tests_client ON windows_revision_boot_tests(test_client_id);
