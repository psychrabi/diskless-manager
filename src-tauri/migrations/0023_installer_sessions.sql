CREATE TABLE installation_sessions (
 id TEXT PRIMARY KEY,
 client_id TEXT NOT NULL REFERENCES clients(id) ON DELETE CASCADE,
 media_id TEXT NOT NULL REFERENCES installer_media(id) ON DELETE CASCADE,
 image_id TEXT NOT NULL REFERENCES images(id) ON DELETE CASCADE,
 master TEXT NOT NULL,
 media_checksum TEXT NOT NULL,
 target_iqn TEXT NOT NULL,
 expected_bytes INTEGER NOT NULL CHECK(expected_bytes > 0),
 source_ip TEXT NOT NULL,
 firmware TEXT NOT NULL CHECK(firmware IN ('bios','uefi')),
 recipe TEXT NOT NULL CHECK(recipe IN ('interactive','windows','ubuntu','rhel')),
 token TEXT NOT NULL,
 config_json TEXT NOT NULL DEFAULT '{}',
 state TEXT NOT NULL CHECK(state IN ('ready','booting','installing','completed','failed','cancelled','recovery_required')),
 progress INTEGER NOT NULL DEFAULT 0 CHECK(progress BETWEEN 0 AND 100),
 error TEXT,
 created_at TEXT NOT NULL,
 updated_at TEXT NOT NULL,
 expires_at TEXT NOT NULL
);
-- Failed/cancelled operations retain their ownership until an administrator closes recovery.
CREATE UNIQUE INDEX installation_one_unfinished_client ON installation_sessions(client_id)
 WHERE state != 'completed';
CREATE TABLE installation_events (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 session_id TEXT NOT NULL REFERENCES installation_sessions(id) ON DELETE CASCADE,
 state TEXT NOT NULL,
 progress INTEGER NOT NULL,
 message TEXT NOT NULL,
 created_at TEXT NOT NULL
);
CREATE TRIGGER installation_events_immutable_update BEFORE UPDATE ON installation_events BEGIN SELECT RAISE(ABORT,'installation events are immutable'); END;

CREATE TRIGGER installation_client_assignment_reserved BEFORE UPDATE OF master,snapshot,writeback,installer_media_id,target_iqn,ip,mac,block_device,block_store,chap_user,chap_secret,chap_enabled,pxe_mode,mode,use_game_disk ON clients
WHEN EXISTS(SELECT 1 FROM installation_sessions WHERE client_id=OLD.id AND state != 'completed')
AND (NEW.master IS NOT OLD.master OR NEW.snapshot IS NOT OLD.snapshot OR NEW.writeback IS NOT OLD.writeback OR NEW.installer_media_id IS NOT OLD.installer_media_id OR NEW.target_iqn IS NOT OLD.target_iqn OR NEW.ip IS NOT OLD.ip OR NEW.mac IS NOT OLD.mac OR NEW.block_device IS NOT OLD.block_device OR NEW.block_store IS NOT OLD.block_store OR NEW.chap_user IS NOT OLD.chap_user OR NEW.chap_secret IS NOT OLD.chap_secret OR NEW.chap_enabled IS NOT OLD.chap_enabled OR NEW.pxe_mode IS NOT OLD.pxe_mode OR NEW.mode IS NOT OLD.mode OR NEW.use_game_disk IS NOT OLD.use_game_disk)
BEGIN SELECT RAISE(ABORT,'client has a reserved installation session; close offline recovery first'); END;
CREATE TRIGGER installation_client_delete_reserved BEFORE DELETE ON clients
WHEN EXISTS(SELECT 1 FROM installation_sessions WHERE client_id=OLD.id AND state != 'completed')
BEGIN SELECT RAISE(ABORT,'client has a reserved installation session'); END;
CREATE TRIGGER installation_media_delete_reserved BEFORE DELETE ON installer_media
WHEN EXISTS(SELECT 1 FROM installation_sessions WHERE media_id=OLD.id AND state != 'completed')
BEGIN SELECT RAISE(ABORT,'installer media is reserved by an unfinished client installation'); END;
CREATE TRIGGER installation_image_delete_reserved BEFORE DELETE ON images
WHEN EXISTS(SELECT 1 FROM installation_sessions WHERE image_id=OLD.id AND state != 'completed')
BEGIN SELECT RAISE(ABORT,'image storage is reserved by an unfinished client installation'); END;
