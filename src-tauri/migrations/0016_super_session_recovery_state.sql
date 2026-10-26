CREATE TABLE super_sessions_new (
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
    updated_at TEXT NOT NULL,
    CHECK (state = 'completed' OR installer_media_id IS NOT NULL)
);

INSERT INTO super_sessions_new
    (id, client_id, prior_image_id, prior_snapshot, working_clone, installer_media_id, state, created_at, updated_at)
SELECT id, client_id, prior_image_id, prior_snapshot, working_clone, installer_media_id, state, created_at, updated_at
FROM super_sessions;

DROP TABLE super_sessions;
ALTER TABLE super_sessions_new RENAME TO super_sessions;

CREATE UNIQUE INDEX idx_super_sessions_one_incomplete_per_client
    ON super_sessions(client_id) WHERE state != 'completed';
