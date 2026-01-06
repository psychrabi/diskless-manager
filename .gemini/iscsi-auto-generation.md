# iSCSI Target Details Auto-Generation

## Overview

Implemented automatic generation and storage of iSCSI target details when clients are created or updated with snapshots. This allows the system to regenerate iSCSI targets later if needed without losing the configuration.

## Implementation Details

### What Gets Generated

When a client is created or updated with a snapshot, the system automatically generates:

1. **Block Store Path**: `/dev/zvol/{clone_dataset}`
   - Example: `/dev/zvol/diskless/PC001-disk`
2. **Target IQN**: `{iscsi.target_prefix}:client.{client_name}`
   - Example: `iqn.2023-01.com.diskless:client.PC001`

### Where It's Stored

The generated details are stored in the `clients` table in the database:

- `block_store` column: Stores the path to the ZFS ZVOL block device
- `target_iqn` column: Stores the iSCSI target IQN

### When It Happens

#### Client Creation (`create_client`)

- Triggered when: A new client is created with a snapshot
- Logic:
  1. Checks if `snapshot` field is provided and not empty
  2. Generates `clone_dataset` using `get_writeback_or_default_dataset()`
  3. Generates `block_store_path` as `/dev/zvol/{clone_dataset}`
  4. Generates `target_iqn` using the iSCSI target prefix from settings
  5. Stores both values in the database

#### Client Update (`update_client`)

- Triggered when: An existing client is updated with a snapshot
- Logic:
  1. Fetches existing client to get current name
  2. Uses new name if provided in update, otherwise uses existing name
  3. Generates the same details as creation
  4. Updates the database with new values

## Benefits

1. **Persistence**: iSCSI configuration is stored in the database, not just in targetcli
2. **Regeneration**: Can recreate iSCSI targets from database if targetcli config is lost
3. **Consistency**: Ensures naming conventions are applied uniformly
4. **Traceability**: Logs show exactly what was generated for each client

## Usage Example

### Creating a Client with Snapshot

```json
POST /api/clients
{
  "name": "PC001",
  "mac": "d8:43:ae:a7:8e:a7",
  "ip": "192.168.1.101",
  "master": "diskless/image-disk/win11",
  "snapshot": "diskless/image-disk/win11@base",
  "keep_writeback": false,
  "use_game_disk": false
}
```

**Result**: Client is created with:

- `block_store`: `/dev/zvol/diskless/PC001-disk`
- `target_iqn`: `iqn.2023-01.com.diskless:client.PC001`

### Updating a Client with Snapshot

```json
PUT /api/clients/{id}
{
  "snapshot": "diskless/image-disk/win11@base"
}
```

**Result**: Client is updated with generated iSCSI details

## Future Enhancements

Potential improvements:

1. Add a command/API endpoint to regenerate all iSCSI targets from database
2. Validate that generated paths/IQNs don't conflict with existing ones
3. Add cleanup logic to remove iSCSI targets when snapshot is removed from client
4. Implement automatic ZFS clone creation based on stored details

## Files Modified

- `src-tauri/src/api/handlers/clients.rs`: Added auto-generation logic to `create_client` and `update_client`
- `src-tauri/src/core/client.rs`: Already had `block_store` and `target_iqn` fields in structs
- `src-tauri/src/services/iscsi.rs`: Updated to use block backstores instead of fileio

## Testing

To test the implementation:

1. Create a client with a snapshot via the UI or API
2. Check the database to verify `block_store` and `target_iqn` are populated
3. Check logs for the "Generated iSCSI details" message
4. Update a client to add/change snapshot and verify details are regenerated
