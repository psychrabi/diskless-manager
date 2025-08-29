# Diskless Client Provisioning System Setup

This document outlines all the system changes and configurations made to set up an automated diskless client provisioning system using DHCP, ZFS, iSCSI, and PXE boot.

## Overview

The system automatically provisions diskless clients when they request an IP address via DHCP. It creates ZFS clones from a golden image, sets up iSCSI targets, and generates PXE boot configurations.

## System Components

- **DHCP Server**: Triggers provisioning on client requests
- **ZFS**: Manages storage pools and clones
- **iSCSI Target**: Exposes storage to clients
- **PXE/TFTP**: Network boot configuration
- **Provisioning Script**: `/usr/local/bin/provision_client.sh`
- **Web Management Interface**: Tauri + React application for system management

## Files Created/Modified

### 1. Provisioning Script
**File**: `/usr/local/bin/provision_client.sh`

**Purpose**: Main provisioning script that creates ZFS clones, iSCSI targets, and PXE configurations

**Key Features**:
- Accepts MAC address as argument (with optional IP)
- Creates ZFS clones from golden snapshot
- Sets up iSCSI targets with per-client IQNs
- Generates PXE boot configurations
- Handles concurrent provisioning with file locking
- Supports both root and non-root execution
- Comprehensive logging
- Can be called from web interface via Tauri commands

### 2. Deprovisioning Script
**File**: `/usr/local/bin/deprovision_client.sh`

**Purpose**: Complete client deprovisioning script that removes all traces of a client

**Key Features**:
- Removes iSCSI targets and backstores
- Deletes PXE boot configurations
- Destroys ZFS clones (optional)
- Cleans up DHCP reservations
- Checks client online status before deprovisioning
- Supports dry-run mode for testing
- Force mode for online clients
- Comprehensive error handling and logging
- Can be called from web interface via Tauri commands

**Usage**:
```bash
# Basic deprovisioning
sudo /usr/local/bin/deprovision_client.sh 70:5a:0f:07:6f:75

# Force deprovisioning (even if client is online)
sudo /usr/local/bin/deprovision_client.sh 70:5a:0f:07:6f:75 --force

# Keep ZFS clone (don't destroy storage)
sudo /usr/local/bin/deprovision_client.sh 70:5a:0f:07:6f:75 --keep-zfs

# Dry run (show what would be done)
sudo /usr/local/bin/deprovision_client.sh 70:5a:0f:07:6f:75 --dry-run

# Combine options
sudo /usr/local/bin/deprovision_client.sh 70:5a:0f:07:6f:75 --force --keep-zfs --dry-run
```

### 3. Web Management Interface
**Project**: Tauri + React application

**Purpose**: Web-based management interface for the diskless provisioning system

**Location**: Current project directory (`/home/rabistha/Documents/Projects/diskless-manager`)

**Features** (based on project structure):
- Client management interface
- Image management (master images, snapshots)
- Service management (DHCP, TFTP, iSCSI)
- ZFS pool monitoring
- System status dashboard

**Usage**:
```bash
# Manual execution
sudo /usr/local/bin/provision_client.sh 70:5a:0f:07:6f:75

# With size specification
sudo /usr/local/bin/provision_client.sh 70:5a:0f:07:6f:75 --size 80G

# With initiator IQN
sudo /usr/local/bin/provision_client.sh 70:5a:0f:07:6f:75 --initiator-iqn iqn.2025-01.com.client:initiator
```

### 4. Sudoers Configuration
**File**: `/etc/sudoers.d/dhcpd-provision`

**Purpose**: Allows dhcpd user to execute privileged commands without password

**Content**:
```
dhcpd ALL=(root) NOPASSWD: /usr/sbin/zfs, /usr/bin/targetcli, /usr/bin/udevadm, /usr/bin/flock, /usr/local/bin/provision_client.sh
```

**Permissions**: `440` (read-only for root)

### 5. DHCP Server Configuration
**File**: `/etc/dhcp/dhcpd.conf`

**Purpose**: Configure DHCP server to trigger provisioning on client requests

**Key Sections**:
```dhcpd
# MAC address formatting function
option space pxelinux;
option pxelinux.mac code 208 = string;

# Execute provisioning script on commit
on commit {
  set clmac = concat(
    binary-to-ascii(16, 8, ":", substring(hardware, 1, 1)), ":",
    binary-to-ascii(16, 8, ":", substring(hardware, 2, 1)), ":",
    suffix(concat("0", binary-to-ascii(16, 8, "", substring(hardware, 3, 1))), 2), ":",
    suffix(concat("0", binary-to-ascii(16, 8, "", substring(hardware, 4, 1))), 2), ":",
    suffix(concat("0", binary-to-ascii(16, 8, "", substring(hardware, 5, 1))), 2), ":",
    suffix(concat("0", binary-to-ascii(16, 8, "", substring(hardware, 6, 1))), 2)
  );
  execute("/usr/local/bin/provision_client.sh", clmac);
}
```

## Directory Structure Created

### 1. Lock Directory
**Path**: `/var/lock/diskless/`

**Purpose**: Stores lock files for concurrent provisioning

**Setup Commands**:
```bash
sudo mkdir -p /var/lock/diskless
sudo chown dhcpd:dhcpd /var/lock/diskless
sudo chmod 775 /var/lock/diskless
```

**Files Created**:
- `provision_<mac>.lock` - Per-client provisioning locks
- `golden_snapshot.lock` - Golden snapshot creation lock
- `targetcli.lock` - iSCSI target configuration lock

### 2. Log Directory
**Path**: `/var/log/provision.log`

**Purpose**: Centralized logging for all provisioning operations

**Setup Commands**:
```bash
sudo touch /var/log/provision.log
sudo chown dhcpd:dhcpd /var/log/provision.log
```

### 3. PXE Configuration Directory
**Path**: `/srv/tftp/pxelinux.cfg/`

**Purpose**: Stores PXE boot configurations for each client

**Setup Commands**:
```bash
sudo mkdir -p /srv/tftp/pxelinux.cfg
sudo chown -R dhcpd:dhcpd /srv/tftp/pxelinux.cfg
```

**Files Created**:
- `01-<mac>` - PXE configuration for each client MAC

## ZFS Configuration

### 1. Storage Pool
**Pool Name**: `diskless`

**Structure**:
```
diskless/
├── win11-master@golden-boot    # Golden snapshot
├── client-70-5a-0f-07-6f-75    # Client clone
├── client-<mac2>               # Additional client clones
└── ...
```

### 2. Golden Snapshot
**Name**: `golden-boot`

**Purpose**: Base snapshot for all client clones

**Creation**: Automatically created by the script on first run

## iSCSI Configuration

### 1. Target Naming Convention
**Format**: `iqn.2025-04.local.diskless:<mac>`

**Example**: `iqn.2025-04.local.diskless:70-5a-0f-07-6f-75`

### 2. Target Attributes
- `generate_node_acls=1` - Dynamic ACL generation
- `cache_dynamic_acls=1` - Cache ACLs for performance
- `demo_mode_write_protect=0` - Allow writes
- `authentication=0` - No authentication required

### 3. Portal Configuration
**Default Portal**: `192.168.1.250:3260`

## PXE Boot Configuration

### 1. Configuration Format
**File**: `/srv/tftp/pxelinux.cfg/01-<mac>`

**Content**:
```
DEFAULT linux
LABEL linux
  KERNEL ipxe.lkrn
  APPEND dhcp && sanboot iscsi:192.168.1.250:3260::::iqn.2025-04.local.diskless:<mac>
```

### 2. Boot Process
1. Client PXE boots
2. Loads iPXE kernel
3. Gets DHCP lease
4. Sanboots to iSCSI target

## Integration with Web Interface

### 1. Tauri Integration
The provisioning script can be integrated with the Tauri web interface to provide a complete management solution.

**Tauri Commands** (implemented in `src-tauri/src/client.rs`):
```rust
// Provisioning commands
#[tauri::command]
async fn provision_client(mac: String, size: Option<String>) -> Result<String, String> {
    let output = Command::new("/usr/local/bin/provision_client.sh")
        .arg(&mac)
        .args(size.map(|s| vec!["--size".to_string(), s]).unwrap_or_default())
        .output()
        .map_err(|e| e.to_string())?;
    
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

// Deprovisioning commands
#[tauri::command]
async fn deprovision_client(req: DeprovisionRequest) -> Result<serde_json::Value, String> {
    let mac = req.mac;
    let force = req.force.unwrap_or(false);
    let keep_zfs = req.keep_zfs.unwrap_or(false);
    let dry_run = req.dry_run.unwrap_or(false);

    let mut args = vec!["/usr/local/bin/deprovision_client.sh", &mac];
    if force { args.push("--force"); }
    if keep_zfs { args.push("--keep-zfs"); }
    if dry_run { args.push("--dry-run"); }

    let output = Command::new("sudo")
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to execute deprovision script: {}", e))?;

    if output.status.success() {
        Ok(serde_json::json!({
            "success": true,
            "message": "Client deprovisioned successfully",
            "output": String::from_utf8_lossy(&output.stdout).trim()
        }))
    } else {
        Err(format!("Deprovisioning failed: {}", String::from_utf8_lossy(&output.stderr)))
    }
}

#[tauri::command]
async fn deprovision_client_by_id(client_id: String, force: Option<bool>, keep_zfs: Option<bool>) -> Result<serde_json::Value, String> {
    // Get client by ID and call deprovision_client
}

#[tauri::command]
async fn get_deprovision_status(mac: String) -> Result<serde_json::Value, String> {
    // Check client status across all systems (ZFS, iSCSI, PXE, DHCP)
}

// Utility commands
#[tauri::command]
async fn get_provisioning_logs() -> Result<String, String> {
    std::fs::read_to_string("/var/log/provision.log")
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_provisioned_clients() -> Result<Vec<String>, String> {
    let output = Command::new("zfs")
        .args(&["list", "-H", "-o", "name", "-r", "diskless"])
        .output()
        .map_err(|e| e.to_string())?;
    
    let clients: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains("client-"))
        .map(|s| s.to_string())
        .collect();
    
    Ok(clients)
}
```

**Frontend Integration** (React hooks):
```javascript
// hooks/useProvisioning.js
import { invoke } from '@tauri-apps/api/tauri';

export const useProvisioning = () => {
  const provisionClient = async (mac, size) => {
    try {
      const result = await invoke('provision_client', { mac, size });
      return { success: true, data: result };
    } catch (error) {
      return { success: false, error: error.toString() };
    }
  };

  const getLogs = async () => {
    try {
      return await invoke('get_provisioning_logs');
    } catch (error) {
      throw new Error(`Failed to get logs: ${error}`);
    }
  };

  const listClients = async () => {
    try {
      return await invoke('list_provisioned_clients');
    } catch (error) {
      throw new Error(`Failed to list clients: ${error}`);
    }
  };

  return { provisionClient, getLogs, listClients };
};

// hooks/useDeprovisioning.js
import { useState } from 'react';
import { invoke } from '@tauri-apps/api/tauri';

export const useDeprovisioning = () => {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);

  const deprovisionClient = async (mac, options = {}) => {
    setLoading(true);
    setError(null);
    
    try {
      const result = await invoke('deprovision_client', {
        req: {
          mac,
          force: options.force || false,
          keep_zfs: options.keep_zfs || false,
          dry_run: options.dry_run || false,
        }
      });
      
      return { success: true, data: result };
    } catch (err) {
      const errorMsg = err.toString();
      setError(errorMsg);
      return { success: false, error: errorMsg };
    } finally {
      setLoading(false);
    }
  };

  const deprovisionClientById = async (clientId, options = {}) => {
    setLoading(true);
    setError(null);
    
    try {
      const result = await invoke('deprovision_client_by_id', {
        clientId,
        force: options.force || false,
        keep_zfs: options.keep_zfs || false,
      });
      
      return { success: true, data: result };
    } catch (err) {
      const errorMsg = err.toString();
      setError(errorMsg);
      return { success: false, error: errorMsg };
    } finally {
      setLoading(false);
    }
  };

  const getDeprovisionStatus = async (mac) => {
    try {
      const status = await invoke('get_deprovision_status', { mac });
      return { success: true, data: status };
    } catch (err) {
      return { success: false, error: err.toString() };
    }
  };

  return {
    deprovisionClient,
    deprovisionClientById,
    getDeprovisionStatus,
    loading,
    error,
    clearError: () => setError(null),
  };
};
```

### 2. Web Interface Features
The existing Tauri application can be enhanced to include:

- **Client Provisioning Panel**: Manual client provisioning with MAC input
- **Client Deprovisioning Panel**: Complete client removal with safety checks
- **Provisioning Logs Viewer**: Real-time log monitoring
- **Client Status Dashboard**: Overview of all provisioned clients
- **ZFS Pool Management**: Monitor and manage storage pools
- **iSCSI Target Management**: View and manage iSCSI targets
- **PXE Configuration Management**: View and edit PXE configurations
- **Client Health Monitoring**: Check client online status and system health

### 3. Real-time Monitoring
Implement WebSocket or polling for real-time updates:
```javascript
// Real-time log monitoring
const useLogMonitor = () => {
  const [logs, setLogs] = useState([]);
  
  useEffect(() => {
    const interval = setInterval(async () => {
      try {
        const newLogs = await invoke('get_provisioning_logs');
        setLogs(newLogs.split('\n').filter(line => line.trim()));
      } catch (error) {
        console.error('Failed to fetch logs:', error);
      }
    }, 5000); // Poll every 5 seconds
    
    return () => clearInterval(interval);
  }, []);
  
  return logs;
};
```

## System Dependencies

### 1. Required Packages
```bash
# ZFS support
sudo apt install zfsutils-linux

# iSCSI target
sudo apt install targetcli-fb

# DHCP server
sudo apt install isc-dhcp-server

# TFTP server (for PXE)
sudo apt install tftpd-hpa
```

### 2. Required Binaries
- `/usr/sbin/zfs` - ZFS management
- `/usr/bin/targetcli` - iSCSI target configuration
- `/usr/bin/udevadm` - Device management
- `/usr/bin/flock` - File locking
- `/usr/bin/sudo` - Privilege escalation

## Service Configuration

### 1. DHCP Server
**Service**: `isc-dhcp-server`

**Status**: Enabled and running

**Configuration**: `/etc/dhcp/dhcpd.conf`

### 2. TFTP Server
**Service**: `tftpd-hpa`

**Status**: Enabled and running

**Root Directory**: `/srv/tftp`

## Security Considerations

### 1. File Permissions
- Lock directory: `775` (dhcpd:dhcpd)
- Log file: `644` (dhcpd:dhcpd)
- PXE directory: `755` (dhcpd:dhcpd)
- Script: `755` (root:root)

### 2. Sudoers Configuration
- Limited to specific commands
- No password required for dhcpd
- Restricted to root execution

### 3. Network Security
- iSCSI targets accessible on local network
- No authentication (demo mode)
- Consider implementing CHAP authentication for production

## Monitoring and Troubleshooting

### 1. Log Files
**Primary Log**: `/var/log/provision.log`

**DHCP Logs**: `/var/log/syslog` (filter for dhcpd)

**System Logs**: `journalctl -u isc-dhcp-server`

### 2. Common Issues

#### Permission Denied
```bash
# Check lock directory permissions
ls -la /var/lock/diskless/

# Check sudoers configuration
sudo -u dhcpd sudo -n /usr/local/bin/provision_client.sh 70:5a:0f:07:6f:75
```

#### ZFS Errors
```bash
# Check ZFS pool status
sudo zpool status diskless

# Check ZFS datasets
sudo zfs list -r diskless
```

#### iSCSI Target Issues
```bash
# Check targetcli configuration
sudo targetcli ls

# Check target status
sudo targetcli /iscsi ls
```

### 3. Testing Commands
```bash
# Test script manually
sudo /usr/local/bin/provision_client.sh 70:5a:0f:07:6f:75

# Test as dhcpd user
sudo -u dhcpd sudo -n /usr/local/bin/provision_client.sh 70:5a:0f:07:6f:75

# Check DHCP configuration
sudo dhcpd -t -cf /etc/dhcp/dhcpd.conf
```

## Maintenance

### 1. Regular Tasks
- Monitor log files for errors
- Clean up old client clones if needed
- Update golden snapshot periodically
- Check ZFS pool health

### 2. Backup Considerations
- Backup golden snapshot
- Backup DHCP configuration
- Backup sudoers configuration
- Backup provisioning script

### 3. Scaling Considerations
- Monitor lock file contention
- Consider separate ZFS pools for different client types
- Implement client cleanup procedures
- Monitor iSCSI target performance

## Future Enhancements

### 1. Potential Improvements
- ✅ Add client deprovisioning script (implemented)
- Implement client metadata storage
- ✅ Integrate provisioning with existing web interface (implemented)
- ✅ Implement client health monitoring (implemented)
- Add support for multiple golden images
- ✅ Add real-time provisioning status updates (implemented)
- Implement provisioning templates/profiles
- Add bulk client provisioning capabilities
- Add client backup and restore functionality
- Implement client migration between pools
- Add client performance monitoring
- Implement automated client cleanup policies

### 2. Security Enhancements
- Implement iSCSI CHAP authentication
- Add client certificate validation
- Implement network segmentation
- Add audit logging

## Support

For issues or questions:
1. Check the log files first
2. Verify all dependencies are installed
3. Test manual execution of the script
4. Check DHCP server configuration
5. Verify file permissions and ownership

---

**Last Updated**: August 10, 2025  
**Version**: 1.0  
**Author**: System Administrator
