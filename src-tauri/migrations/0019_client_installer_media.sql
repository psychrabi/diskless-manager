ALTER TABLE clients
    ADD COLUMN installer_media_id TEXT REFERENCES installer_media(id) ON DELETE RESTRICT;
