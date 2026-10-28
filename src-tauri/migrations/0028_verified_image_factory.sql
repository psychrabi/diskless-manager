PRAGMA defer_foreign_keys = ON;

DROP TRIGGER installation_events_immutable_update;
DROP TRIGGER installation_client_assignment_reserved;
DROP TRIGGER installation_client_delete_reserved;
DROP TRIGGER installation_media_delete_reserved;
DROP TRIGGER installation_image_delete_reserved;
DROP INDEX installation_one_unfinished_client;

UPDATE clients SET installer_media_id = NULL WHERE installer_media_id IS NOT NULL;

CREATE TABLE installation_sessions_events_backup (
    sequence INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL,
    state TEXT NOT NULL,
    progress INTEGER NOT NULL,
    message TEXT NOT NULL,
    created_at TEXT NOT NULL
);
INSERT INTO installation_sessions_events_backup
SELECT sequence, session_id, state, progress, message, created_at FROM installation_events;
DROP TABLE installation_events;

CREATE TABLE installation_sessions_new (
    id TEXT PRIMARY KEY,
    client_id TEXT NOT NULL REFERENCES clients(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('factory', 'manual')),
    recipe TEXT NOT NULL CHECK (recipe IN ('windows', 'ubuntu', 'rhel', 'interactive')),
    media_id TEXT REFERENCES installer_media(id) ON DELETE RESTRICT,
    media_checksum TEXT,
    image_name TEXT,
    draft_image_id TEXT REFERENCES images(id) ON DELETE SET NULL,
    size_gb INTEGER CHECK (size_gb IS NULL OR size_gb >= 8),
    expected_bytes INTEGER CHECK (expected_bytes IS NULL OR expected_bytes > 0),
    prior_master TEXT,
    prior_snapshot TEXT,
    target_iqn TEXT,
    source_ip TEXT NOT NULL,
    firmware TEXT NOT NULL CHECK (firmware IN ('bios', 'uefi')),
    verification_target_iqn TEXT,
    verification_image_id TEXT REFERENCES images(id) ON DELETE SET NULL,
    verification_snapshot TEXT,
    attempt INTEGER NOT NULL DEFAULT 0 CHECK (attempt >= 0),
    published_image_id TEXT REFERENCES images(id) ON DELETE RESTRICT,
    assign_on_ready INTEGER NOT NULL DEFAULT 0 CHECK (assign_on_ready IN (0, 1)),
    assignment_state TEXT NOT NULL DEFAULT 'not_requested' CHECK (assignment_state IN ('not_requested', 'pending', 'applied', 'failed')),
    assignment_error TEXT,
    state TEXT NOT NULL CHECK (state IN ('preparing', 'waiting', 'installing', 'adapting', 'saving', 'verifying', 'published', 'verification_failed', 'failed', 'cancelled', 'manual', 'recovery_required', 'completed')),
    progress INTEGER NOT NULL DEFAULT 0 CHECK (progress BETWEEN 0 AND 100),
    error TEXT,
    phase_deadline TEXT,
    cleanup_state TEXT NOT NULL DEFAULT 'pending' CHECK (cleanup_state IN ('pending', 'complete', 'failed')),
    legacy INTEGER NOT NULL DEFAULT 0 CHECK (legacy IN (0, 1)),
    closed_at TEXT,
    token TEXT NOT NULL DEFAULT '',
    config_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (kind != 'manual' OR (verification_image_id IS NULL AND published_image_id IS NULL)),
    CHECK (state != 'published' OR published_image_id IS NOT NULL),
    CHECK (state != 'completed' OR closed_at IS NOT NULL)
);

INSERT INTO installation_sessions_new (
    id, client_id, kind, recipe, media_id, media_checksum, image_name, expected_bytes,
    prior_master, prior_snapshot, target_iqn, source_ip, firmware, published_image_id,
    state, progress, error, phase_deadline, cleanup_state, legacy, closed_at, token,
    config_json, created_at, updated_at
)
SELECT id, client_id,
       CASE recipe WHEN 'interactive' THEN 'manual' ELSE 'factory' END,
       recipe, media_id, media_checksum, NULL, expected_bytes, master, NULL, target_iqn,
       source_ip, firmware,
       CASE WHEN state = 'completed' AND recipe != 'interactive' THEN image_id ELSE NULL END,
       CASE WHEN state = 'completed' THEN 'completed' ELSE 'recovery_required' END,
       progress,
       CASE WHEN state = 'completed' THEN error ELSE 'Legacy installation needs administrator recovery; no storage cleanup is permitted' END,
       expires_at, CASE WHEN state = 'completed' THEN 'complete' ELSE 'pending' END,
       1, CASE WHEN state = 'completed' THEN updated_at ELSE NULL END,
       '', config_json, created_at, updated_at
FROM installation_sessions;

DROP TABLE installation_sessions;
ALTER TABLE installation_sessions_new RENAME TO installation_sessions;

CREATE TABLE installation_events (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES installation_sessions(id) ON DELETE CASCADE,
    state TEXT NOT NULL,
    progress INTEGER NOT NULL CHECK (progress BETWEEN 0 AND 100),
    message TEXT NOT NULL CHECK (length(message) <= 1024),
    actor TEXT,
    created_at TEXT NOT NULL
);
INSERT INTO installation_events (sequence, session_id, state, progress, message, actor, created_at)
SELECT sequence, session_id, state, progress, message, NULL, created_at FROM installation_sessions_events_backup;
DROP TABLE installation_sessions_events_backup;

CREATE UNIQUE INDEX installation_one_unfinished_client
    ON installation_sessions(client_id) WHERE closed_at IS NULL;
CREATE INDEX installation_sessions_sweep ON installation_sessions(state, phase_deadline);
-- A verification IQN belongs to exactly one session/attempt; repeated reports
-- must never be ambiguous between two sessions.
CREATE UNIQUE INDEX installation_verification_target_unique
    ON installation_sessions(verification_target_iqn) WHERE verification_target_iqn IS NOT NULL;
CREATE TRIGGER installation_events_immutable_update BEFORE UPDATE ON installation_events BEGIN SELECT RAISE(ABORT,'installation events are immutable'); END;
-- Session-owned assignment is the only mutation allowed while a published
-- session still reserves the client: master moves to the published image and
-- the old snapshot/writeback are cleared. Every other watched field must stay
-- put, and the session must still be waiting for its opt-in assignment.
CREATE TRIGGER installation_client_assignment_reserved BEFORE UPDATE OF master,snapshot,writeback,installer_media_id,target_iqn,ip,mac,block_device,block_store,chap_user,chap_secret,chap_enabled,pxe_mode,mode,use_game_disk ON clients
WHEN EXISTS(SELECT 1 FROM installation_sessions WHERE client_id=OLD.id AND closed_at IS NULL)
AND NOT (
    NEW.master IS NOT OLD.master
    AND NEW.snapshot IS NULL
    AND NEW.writeback IS NULL
    AND NEW.installer_media_id IS OLD.installer_media_id
    AND NEW.target_iqn IS OLD.target_iqn
    AND NEW.ip IS OLD.ip
    AND NEW.mac IS OLD.mac
    AND NEW.block_store IS OLD.block_store
    AND NEW.chap_user IS OLD.chap_user
    AND NEW.chap_secret IS OLD.chap_secret
    AND NEW.chap_enabled IS OLD.chap_enabled
    AND NEW.pxe_mode IS OLD.pxe_mode
    AND NEW.mode IS OLD.mode
    AND NEW.use_game_disk IS OLD.use_game_disk
    AND EXISTS(SELECT 1 FROM installation_sessions WHERE client_id=OLD.id AND closed_at IS NULL AND state='published' AND assign_on_ready=1 AND assignment_state='pending')
)
AND (NEW.master IS NOT OLD.master OR NEW.snapshot IS NOT OLD.snapshot OR NEW.writeback IS NOT OLD.writeback OR NEW.installer_media_id IS NOT OLD.installer_media_id OR NEW.target_iqn IS NOT OLD.target_iqn OR NEW.ip IS NOT OLD.ip OR NEW.mac IS NOT OLD.mac OR NEW.block_device IS NOT OLD.block_device OR NEW.block_store IS NOT OLD.block_store OR NEW.chap_user IS NOT OLD.chap_user OR NEW.chap_secret IS NOT OLD.chap_secret OR NEW.chap_enabled IS NOT OLD.chap_enabled OR NEW.pxe_mode IS NOT OLD.pxe_mode OR NEW.mode IS NOT OLD.mode OR NEW.use_game_disk IS NOT OLD.use_game_disk)
BEGIN SELECT RAISE(ABORT,'client has an unfinished installation session'); END;
CREATE TRIGGER installation_client_delete_reserved BEFORE DELETE ON clients
WHEN EXISTS(SELECT 1 FROM installation_sessions WHERE client_id=OLD.id AND closed_at IS NULL)
BEGIN SELECT RAISE(ABORT,'client has an unfinished installation session'); END;
CREATE TRIGGER installation_media_delete_reserved BEFORE DELETE ON installer_media
WHEN EXISTS(SELECT 1 FROM installation_sessions WHERE media_id=OLD.id AND closed_at IS NULL)
BEGIN SELECT RAISE(ABORT,'installer media is reserved by an unfinished installation'); END;
CREATE TRIGGER installation_image_delete_reserved BEFORE DELETE ON images
WHEN EXISTS(SELECT 1 FROM installation_sessions WHERE closed_at IS NULL AND OLD.id=published_image_id)
 OR EXISTS(SELECT 1 FROM installation_sessions WHERE closed_at IS NULL AND verification_image_id=OLD.id AND state NOT IN ('saving','failed','cancelled','verification_failed','recovery_required'))
 OR EXISTS(SELECT 1 FROM installation_sessions WHERE closed_at IS NULL AND draft_image_id=OLD.id AND state NOT IN ('preparing','manual','failed','cancelled','verification_failed','recovery_required'))
BEGIN SELECT RAISE(ABORT,'image is reserved by an unfinished installation'); END;
