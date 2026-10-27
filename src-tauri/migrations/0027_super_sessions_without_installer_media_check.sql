-- Normal Super sessions edit an isolated working clone and do not attach
-- installer media, so installer_media_id must be nullable for every state.
-- Verification-only sessions still record the media they boot tested.
--
-- The table is copied through a scratch table instead of ALTER TABLE RENAME:
-- renaming reparses every trigger in the schema, and pre-0001 databases can
-- still contain installer triggers written before a column existed.
CREATE TABLE super_sessions_rebuild (
    id TEXT PRIMARY KEY,
    client_id TEXT NOT NULL REFERENCES clients(id) ON DELETE RESTRICT,
    prior_image_id TEXT NOT NULL REFERENCES images(id) ON DELETE RESTRICT,
    prior_snapshot TEXT,
    working_origin_snapshot TEXT,
    working_clone TEXT NOT NULL UNIQUE,
    installer_media_id TEXT REFERENCES installer_media(id) ON DELETE SET NULL,
    verification_only INTEGER NOT NULL DEFAULT 0 CHECK (verification_only IN (0, 1)),
    resume_state TEXT CHECK (resume_state IN ('preparing', 'active', 'saving', 'discarding')),
    state TEXT NOT NULL CHECK (state IN ('preparing', 'active', 'saving', 'discarding', 'recovery_required', 'completed')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

INSERT INTO super_sessions_rebuild
    (id, client_id, prior_image_id, prior_snapshot, working_origin_snapshot, working_clone,
     installer_media_id, verification_only, resume_state, state, created_at, updated_at)
SELECT id, client_id, prior_image_id, prior_snapshot, working_origin_snapshot, working_clone,
       installer_media_id, verification_only, resume_state, state, created_at, updated_at
FROM super_sessions;

DROP TABLE super_sessions;

CREATE TABLE super_sessions (
    id TEXT PRIMARY KEY,
    client_id TEXT NOT NULL REFERENCES clients(id) ON DELETE RESTRICT,
    prior_image_id TEXT NOT NULL REFERENCES images(id) ON DELETE RESTRICT,
    prior_snapshot TEXT,
    working_origin_snapshot TEXT,
    working_clone TEXT NOT NULL UNIQUE,
    installer_media_id TEXT REFERENCES installer_media(id) ON DELETE SET NULL,
    verification_only INTEGER NOT NULL DEFAULT 0 CHECK (verification_only IN (0, 1)),
    resume_state TEXT CHECK (resume_state IN ('preparing', 'active', 'saving', 'discarding')),
    state TEXT NOT NULL CHECK (state IN ('preparing', 'active', 'saving', 'discarding', 'recovery_required', 'completed')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

INSERT INTO super_sessions
    (id, client_id, prior_image_id, prior_snapshot, working_origin_snapshot, working_clone,
     installer_media_id, verification_only, resume_state, state, created_at, updated_at)
SELECT id, client_id, prior_image_id, prior_snapshot, working_origin_snapshot, working_clone,
       installer_media_id, verification_only, resume_state, state, created_at, updated_at
FROM super_sessions_rebuild;

DROP TABLE super_sessions_rebuild;

CREATE UNIQUE INDEX idx_super_sessions_one_incomplete_per_client
    ON super_sessions(client_id) WHERE state != 'completed';
