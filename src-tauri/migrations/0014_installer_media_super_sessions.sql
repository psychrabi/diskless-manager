CREATE TABLE installer_media (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    media_type TEXT NOT NULL CHECK (media_type IN ('winpe', 'linux_live')),
    managed_path TEXT NOT NULL UNIQUE,
    byte_size INTEGER NOT NULL CHECK (byte_size > 0),
    checksum TEXT NOT NULL,
    validation_state TEXT NOT NULL CHECK (validation_state IN ('pending', 'valid', 'invalid')),
    is_default INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_installer_media_default_type
    ON installer_media(media_type) WHERE is_default = 1;

CREATE TABLE installer_media_boot_verifications (
    id TEXT PRIMARY KEY,
    installer_media_id TEXT NOT NULL,
    checksum TEXT NOT NULL,
    firmware_mode TEXT NOT NULL CHECK (firmware_mode IN ('uefi', 'bios')),
    test_client_id TEXT NOT NULL,
    assigned_image_revision TEXT NOT NULL,
    observed_luns TEXT NOT NULL,
    boot_result TEXT NOT NULL CHECK (boot_result IN ('passed', 'failed')),
    verified_at TEXT NOT NULL
);

CREATE INDEX idx_installer_media_boot_verifications_lookup
    ON installer_media_boot_verifications(installer_media_id, checksum, firmware_mode);

CREATE TRIGGER installer_media_boot_verifications_no_update
BEFORE UPDATE ON installer_media_boot_verifications
BEGIN
    SELECT RAISE(ABORT, 'installer-media boot verifications are append-only');
END;

CREATE TRIGGER installer_media_boot_verifications_no_delete
BEFORE DELETE ON installer_media_boot_verifications
BEGIN
    SELECT RAISE(ABORT, 'installer-media boot verifications are append-only');
END;

CREATE TABLE super_sessions (
    id TEXT PRIMARY KEY,
    client_id TEXT NOT NULL UNIQUE REFERENCES clients(id) ON DELETE RESTRICT,
    prior_image_id TEXT NOT NULL REFERENCES images(id) ON DELETE RESTRICT,
    prior_snapshot TEXT,
    working_clone TEXT NOT NULL UNIQUE,
    installer_media_id TEXT NOT NULL REFERENCES installer_media(id) ON DELETE RESTRICT,
    state TEXT NOT NULL CHECK (state IN ('preparing', 'active', 'saving', 'discarding', 'recovery_required', 'completed')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
