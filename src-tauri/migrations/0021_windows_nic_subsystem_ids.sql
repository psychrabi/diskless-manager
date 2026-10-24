ALTER TABLE windows_nic_inventory
    ADD COLUMN subvendor_id TEXT NOT NULL DEFAULT '';
ALTER TABLE windows_nic_inventory
    ADD COLUMN subdevice_id TEXT NOT NULL DEFAULT '';
