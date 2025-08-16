# FileIO Master Images

## Overview

The Diskless Boot Manager now supports creating master images using fileIO (file-based storage) as an alternative to ZFS volumes. This feature is useful when ZFS is not available or for testing purposes.

## Features

### FileIO vs ZFS Comparison

| Feature | ZFS Volume | FileIO |
|---------|------------|--------|
| **Storage Type** | ZFS volume | Regular file |
| **Location** | ZFS pool | `/var/lib/diskless/fileio/` |
| **Snapshots** | ✅ Supported | ❌ Not supported |
| **Performance** | ✅ Optimized | ⚠️ Slower |
| **Space Efficiency** | ✅ Copy-on-write | ❌ Full copy |
| **Dependencies** | Requires ZFS | No special requirements |

### Creating FileIO Images

1. Navigate to **Image Management** in the web interface
2. Click **Create Image**
3. Select **FileIO (File-based)** as the image type
4. Enter the image name and size
5. Click **Create Master**

### FileIO Image Structure

FileIO images are stored as sparse files in `/var/lib/diskless/fileio/` with the naming convention:
```
/var/lib/diskless/fileio/{name}-master.img
```

## Client Provisioning with FileIO

### How Clients Use FileIO Master Images

When a client is provisioned using a fileIO master image, the system:

1. **Creates a Copy**: Instead of creating a ZFS clone, the system copies the master fileIO image to create a client-specific copy
2. **Client Image Location**: Client images are stored as:
   ```
   /var/lib/diskless/fileio/{client-id}-{mac}-client.img
   ```
3. **iSCSI Target Setup**: The iSCSI target points directly to the copied file (not a ZFS volume)
4. **PXE Boot**: Clients boot using the same PXE/iSCSI process as ZFS-based clients

### Client Provisioning Process

1. **Web Interface**: Add a new client through the web interface
2. **Master Selection**: Choose a fileIO master image from the dropdown
3. **Automatic Copying**: The system automatically copies the master image to create a client-specific copy
4. **iSCSI Setup**: An iSCSI target is created pointing to the client's copy
5. **DHCP Configuration**: DHCP reservation is created for the client
6. **PXE Configuration**: PXE boot configuration is generated

### Client Management Operations

#### Adding a Client
- **ZFS**: Creates a ZFS clone from master snapshot
- **FileIO**: Copies the master file to create a client-specific image

#### Editing a Client
- **ZFS**: Can change master, snapshot, or use master directly
- **FileIO**: Can change master (no snapshot support), creates new copy

#### Resetting a Client
- **ZFS**: Destroys clone and creates new one from snapshot
- **FileIO**: Deletes client copy and creates new copy from master

#### Deleting a Client
- **ZFS**: Destroys ZFS clone
- **FileIO**: Deletes client-specific image file

### FileIO Client Image Naming

Client images follow this naming convention:
```
/var/lib/diskless/fileio/{client-id}-{mac-address}-client.img
```

Example:
```
/var/lib/diskless/fileio/client1-70-5a-0f-07-6f-75-client.img
```

### iSCSI Configuration for FileIO

FileIO images are exposed via iSCSI using fileio backstores (not block backstores):

- **Backstore Type**: `fileio` (not `block`)
- **Backstore Path**: Points to the client's fileIO image file
- **Target IQN**: `iqn.2025-04.com.nsboot:{mac-address}`
- **LUN**: Maps the fileio backstore to the target
- **Portal**: Standard iSCSI portal (3260)

**Example targetcli commands for fileIO:**
```bash
# Create fileio backstore
sudo targetcli backstores/fileio create block_pc103 /var/lib/diskless/fileio/pc103-70-5a-0f-07-6f-75-client.img

# Create LUN mapping
sudo targetcli iscsi/iqn.2025-04.com.nsboot:70-5a-0f-07-6f-75/tpg1/luns create /backstores/fileio/block_pc103
```

### PXE Boot Process

The PXE boot process is identical for both ZFS and fileIO clients:

1. Client PXE boots
2. Loads iPXE kernel
3. Gets DHCP lease
4. Sanboots to iSCSI target
5. Mounts the fileIO image as a block device

## Limitations

- **No Snapshots**: FileIO images do not support ZFS snapshots
- **Slower Performance**: File-based storage is generally slower than ZFS volumes
- **No Copy-on-Write**: Each client requires a full copy of the master image
- **Manual Management**: File cleanup and management must be done manually
- **Space Usage**: Each client uses the full size of the master image

### Use Cases

FileIO images are recommended for:
- Testing environments where ZFS is not available
- Development and experimentation
- Systems without ZFS support
- Quick prototyping
- Small-scale deployments

### System Requirements

- Linux system with `truncate` and `cp` commands
- Write permissions to `/var/lib/diskless/fileio/`
- Sufficient disk space for image files
- iSCSI target support (targetcli)

### File Permissions

FileIO images are created with permissions `644` (readable by all, writable by owner) and owned by root.

### Troubleshooting

**Permission Denied Errors**
```bash
sudo chown -R diskless:diskless /var/lib/diskless/fileio/
sudo chmod 755 /var/lib/diskless/fileio/
```

**Disk Space Issues**
```bash
df -h /var/lib/diskless/fileio/
```

**File Corruption**
```bash
ls -la /var/lib/diskless/fileio/
file /var/lib/diskless/fileio/*.img
```

**iSCSI Target Issues**
```bash
sudo targetcli ls
sudo targetcli /backstores/block ls
```

## Migration

To migrate from FileIO to ZFS:
1. Create a new ZFS master image
2. Copy data from FileIO image to ZFS image
3. Update client configurations
4. Delete old FileIO image

## Future Enhancements

- FileIO snapshot support using file-based snapshots
- Compression for FileIO images
- Backup and restore functionality
- Performance optimizations
- Integration with external storage systems
