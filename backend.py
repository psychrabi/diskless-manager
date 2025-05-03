#!/usr/bin/env python3

import subprocess
import json
from flask import Flask, request, jsonify, abort
import shlex # Used for safer command splitting
import re
import os

# --- Configuration ---
# !! SECURITY WARNING !!: Running Flask as root is highly discouraged.
# Configure sudoers to allow the user running Flask to execute *specific*
# required commands without a password. Example sudoers entry:
# flaskuser ALL=(ALL) NOPASSWD: /usr/sbin/zfs, /usr/bin/targetcli, /bin/systemctl, /usr/sbin/dhcpd, /usr/bin/wakeonlan
# Adapt the paths and commands based on your system.
SUDO_CMD = "/usr/bin/sudo" # Adjust path if needed

# Base path for ZFS operations (e.g., your main pool)
ZFS_POOL = "nsboot0" # !!! IMPORTANT: Change this to your actual ZFS pool name !!!

# DHCP Configuration - Choose ONE method:
DHCP_CONFIG_METHOD = "include_files" # or "main_config"
DHCP_CONFIG_PATH = "/etc/dhcp/dhcpd.conf"
DHCP_INCLUDE_DIR = "/etc/dhcp/clients.d" # Used if method is "include_files"

# Ensure include directory exists if using that method
if DHCP_CONFIG_METHOD == "include_files" and not os.path.exists(DHCP_INCLUDE_DIR):
    try:
        # Requires appropriate permissions or running as root/sudo
        os.makedirs(DHCP_INCLUDE_DIR)
        print(f"Created DHCP include directory: {DHCP_INCLUDE_DIR}")
        # Make sure dhcpd.conf includes this directory, e.g.:
        # include "/etc/dhcp/clients.d/*.conf";
    except OSError as e:
        print(f"Error creating DHCP include directory {DHCP_INCLUDE_DIR}: {e}")
        # Consider aborting startup if directory creation fails
        # exit(1)


app = Flask(__name__)

# --- Helper Functions ---

def run_command(command_list, check=True, capture_output=True, text=True, use_sudo=True):
    """
    Runs a system command securely using subprocess.run.
    Handles sudo execution and basic error checking.

    Args:
        command_list (list): The command and its arguments as a list.
        check (bool): If True, raise CalledProcessError on non-zero exit code.
        capture_output (bool): If True, capture stdout and stderr.
        text (bool): If True, decode stdout/stderr as text.
        use_sudo (bool): If True, prepend sudo command from config.

    Returns:
        subprocess.CompletedProcess: The result object.

    Raises:
        subprocess.CalledProcessError: If check is True and command fails.
        FileNotFoundError: If the command (or sudo) is not found.
        Exception: For other potential errors.
    """
    if not isinstance(command_list, list):
        raise ValueError("command_list must be a list")

    cmd = command_list
    if use_sudo:
        if not SUDO_CMD:
             raise ValueError("SUDO_CMD is not configured.")
        cmd = [SUDO_CMD] + command_list

    print(f"Running command: {' '.join(shlex.quote(c) for c in cmd)}") # Log command execution attempt
    try:
        result = subprocess.run(
            cmd,
            check=check,
            capture_output=capture_output,
            text=text,
            # Consider adding a timeout
            # timeout=30
        )
        # Log output even on success for debugging, but maybe less verbosely
        if result.returncode != 0:
             print(f"Command failed with code {result.returncode}: {cmd}")
             if result.stderr:
                 print(f"Stderr: {result.stderr.strip()}")
             if result.stdout:
                 print(f"Stdout: {result.stdout.strip()}")
        # else:
        #     # Optionally log success differently or skip stdout unless debugging
        #     print(f"Command succeeded: {cmd}")
        #     # if result.stdout:
        #     #      print(f"Stdout: {result.stdout.strip()}")


        return result
    except subprocess.CalledProcessError as e:
        print(f"Error running command: {e}")
        print(f"Stderr: {e.stderr}")
        print(f"Stdout: {e.stdout}")
        raise # Re-raise the exception after logging
    except FileNotFoundError as e:
        print(f"Error: Command or sudo not found: {e}")
        raise
    except Exception as e:
        print(f"An unexpected error occurred: {e}")
        raise

def parse_zfs_list(output, type_filter=None):
    """ Parses output of 'zfs list -H -o name,origin,creation,used' """
    items = []
    for line in output.strip().split('\n'):
        if not line:
            continue
        parts = line.split('\t')
        if len(parts) == 4:
            name, origin, creation, used = parts
            # Basic filtering by name prefix if needed
            # if name_prefix and not name.startswith(name_prefix):
            #     continue
            items.append({
                "name": name,
                "origin": origin if origin != '-' else None,
                "created": creation, # Keep original format for now
                "size": used, # 'used' property
                "id": name # Use name as ID for simplicity
            })
    return items

def parse_targetcli_ls(output):
    """ Very basic parser for targetcli ls output - fragile! """
    # NOTE: Using targetcli's JSON config is more robust: /etc/target/saveconfig.json
    # This parser is just a placeholder demonstration.
    targets = []
    current_target = None
    try:
        lines = output.strip().split('\n')
        # This parsing logic is highly dependent on targetcli output format
        # and likely needs significant refinement or replacement.
        for line in lines:
            line = line.strip()
            # Look for target IQNs
            iqn_match = re.search(r'o- iscsi .*(iqn\..*)', line)
            if iqn_match:
                 current_target = iqn_match.group(1).strip()
                 # Try to extract associated block device (very fragile)
                 # This assumes a simple structure and might break easily
                 # A better approach involves parsing the JSON config or more detailed ls output
                 # Placeholder: Extracting client name from IQN
                 client_name_match = re.search(r':([^:]+)$', current_target)
                 client_name = client_name_match.group(1) if client_name_match else None
                 if client_name:
                     # Assume clone name convention matches client name
                     clone_path = f"{ZFS_POOL}/{client_name}-disk" # Assumed convention
                     targets.append({
                         "name": client_name, # Extracted name
                         "target": current_target,
                         "clone": clone_path # Assumed clone path
                         # Add more details if parsable: LUNs, ACLs etc.
                     })

    except Exception as e:
        print(f"Error parsing targetcli output: {e}")
    return targets


def get_client_status(client_ip):
    """ Placeholder for checking client status (e.g., ping) """
    # In a real system, check iSCSI connections, ping, etc.
    # For now, simulate based on IP
    # Example: Use ping command
    if not client_ip or client_ip == "N/A":
        return "Unknown"
    try:
        # Run ping command (adjust count/timeout as needed)
        # Use check=False as ping might fail for offline clients
        result = run_command(['ping', '-c', '1', '-W', '1', client_ip], check=False, use_sudo=False)
        if result.returncode == 0:
            return "Online"
        else:
            return "Offline"
    except Exception as e:
        print(f"Error pinging client {client_ip}: {e}")
        return "Error"


def get_client_dhcp_info():
    """ Parses DHCP leases or config files to get client IPs/MACs """
    # This is complex and depends heavily on your DHCP server setup.
    # Option 1: Parse lease file (e.g., /var/lib/dhcp/dhcpd.leases) - format varies
    # Option 2: Parse static host entries from dhcpd.conf or include files
    clients = {}
    if DHCP_CONFIG_METHOD == "include_files":
        if not os.path.isdir(DHCP_INCLUDE_DIR):
             print(f"DHCP include directory not found: {DHCP_INCLUDE_DIR}")
             return {}
        try:
            for filename in os.listdir(DHCP_INCLUDE_DIR):
                if filename.endswith(".conf"):
                    filepath = os.path.join(DHCP_INCLUDE_DIR, filename)
                    try:
                        with open(filepath, 'r') as f:
                            content = f.read()
                            # Simple parsing - needs improvement for robustness
                            host_match = re.search(r'host\s+([\w-]+)\s*{', content, re.IGNORECASE)
                            mac_match = re.search(r'hardware\s+ethernet\s+([\w:]+);', content, re.IGNORECASE)
                            ip_match = re.search(r'fixed-address\s+([\d.]+);', content, re.IGNORECASE)
                            if host_match and mac_match and ip_match:
                                hostname = host_match.group(1)
                                mac = mac_match.group(1).upper()
                                ip = ip_match.group(1)
                                clients[hostname] = {"mac": mac, "ip": ip}
                    except Exception as e:
                         print(f"Error reading or parsing DHCP file {filepath}: {e}")
        except Exception as e:
            print(f"Error listing DHCP include directory {DHCP_INCLUDE_DIR}: {e}")
    elif DHCP_CONFIG_METHOD == "main_config":
         print("Parsing main dhcpd.conf is not implemented in this example.")
         # Implement parsing logic for main config file if needed
         pass

    # Add lease file parsing here if needed for dynamic clients

    return clients


# --- API Endpoints ---

@app.route('/api/status', methods=['GET'])
def get_api_status():
    return jsonify({"status": "ok", "message": "Backend is running"})

@app.route('/api/services', methods=['GET'])
def get_services_status():
    services_to_check = {
        'iscsi': 'target.service', # LIO target service name
        'dhcp': 'isc-dhcp-server.service', # Adjust if using a different DHCP server
        'tftp': 'tftpd-hpa.service',
        # 'zfs': 'zfs-zed.service' # ZFS event daemon, 'active' doesn't guarantee pool health
    }
    statuses = {}
    for key, service_name in services_to_check.items():
        try:
            # Use 'is-active' for a simple status check
            result = run_command(['systemctl', 'is-active', service_name], check=False, use_sudo=False) # systemctl often runnable by user
            status = result.stdout.strip() if result.returncode == 0 else 'inactive' # or 'failed' etc.
            statuses[key] = {"name": service_name.replace('.service', ''), "status": status}
        except Exception as e:
            print(f"Error checking service {service_name}: {e}")
            statuses[key] = {"name": service_name.replace('.service', ''), "status": "error"}

    # Add a basic ZFS pool health check
    zfs_status = 'error' # Default status
    try:
         # Remove -H flag, parse standard output
         result = run_command(['zpool', 'status', ZFS_POOL], use_sudo=True, check=False) # Use check=False to parse output even on non-ONLINE states
         if result.returncode == 0:
             pool_state = 'unknown'
             for line in result.stdout.strip().split('\n'):
                 if line.strip().startswith('state:'):
                     pool_state = line.split(':')[1].strip()
                     break # Found the state line
             if pool_state == 'ONLINE':
                 zfs_status = 'active'
             elif pool_state in ['DEGRADED', 'FAULTED', 'UNAVAIL', 'REMOVING']:
                 zfs_status = 'degraded' # Or map specific states if needed
             else:
                 zfs_status = 'unknown' # Or 'error' if state parsing failed
         else:
             print(f"zpool status command failed with code {result.returncode}")
             zfs_status = 'error'

    except FileNotFoundError:
        print(f"Error: 'zpool' command not found.")
        zfs_status = 'error'
    except Exception as e:
         print(f"Error checking ZFS pool status for {ZFS_POOL}: {e}")
         zfs_status = 'error' # Keep default error status

    statuses['zfs'] = {"name": f"ZFS Pool ({ZFS_POOL})", "status": zfs_status}


    return jsonify(statuses)

@app.route('/api/masters', methods=['GET'])
def get_masters():
    masters_data = []
    try:
        # List ZVOLs in the pool - assuming masters are ZVOLs named *-master
        # Also list filesystems which might be used as masters
        result = run_command(['zfs', 'list', '-H', '-t', 'filesystem,volume', '-o', 'name,creation,used', '-r', ZFS_POOL], use_sudo=True)
        all_datasets = parse_zfs_list(result.stdout)

        # Adjust the filtering logic if your master naming convention differs
        master_names = [ds['name'] for ds in all_datasets if ds['name'].endswith('-master') and ds['name'].count('/') == 1] # Simple convention: pool/name-master

        for master_name in master_names:
             # Get snapshots for this master
             # Use check=False because a master might not have snapshots yet
             snap_result = run_command(['zfs', 'list', '-H', '-t', 'snapshot', '-o', 'name,creation,used', '-r', master_name], use_sudo=True, check=False)
             snapshots = []
             if snap_result.returncode == 0: # Only parse if command succeeded
                 snapshots = parse_zfs_list(snap_result.stdout)

             masters_data.append({
                 "id": master_name,
                 "name": master_name,
                 "snapshots": sorted(snapshots, key=lambda s: s['created']) # Sort by creation time
             })

    except Exception as e:
        print(f"Error getting masters: {e}")
        return jsonify({"error": "Failed to retrieve master images"}), 500
    return jsonify(masters_data)


@app.route('/api/clients', methods=['GET'])
def get_clients():
    clients_data = []
    try:
        # 1. Get ZFS clones (assuming naming convention like pool/clientname-disk)
        # Only list volumes, assuming clones are always volumes
        zfs_result = run_command(['zfs', 'list', '-H', '-t', 'volume', '-o', 'name,origin', '-r', ZFS_POOL], use_sudo=True)
        zfs_clones = {}
        if zfs_result.returncode == 0:
            for line in zfs_result.stdout.strip().split('\n'):
                 if not line: continue
                 name, origin = line.split('\t')
                 # Check if it's a clone and follows the naming convention
                 if origin != '-' and name.endswith('-disk'):
                     # Extract client name from clone name (pool/client-disk)
                     match = re.match(rf"^{ZFS_POOL}/([\w-]+)-disk$", name)
                     if match:
                         client_name = match.group(1)
                         zfs_clones[client_name] = {"clone": name, "origin": origin}

        # 2. Get iSCSI target info (Placeholder - needs real implementation)
        # This part is complex and requires parsing targetcli config or output reliably.
        # For now, we construct a placeholder target IQN based on client name.
        iscsi_targets = {} # Placeholder
        # Example of how you might populate this if parsing worked:
        # for name in zfs_clones.keys():
        #    iscsi_targets[name] = f"iqn.2025-04.mydomain.server:{name}" # Adjust domain/date

        # 3. Get DHCP info
        dhcp_info = get_client_dhcp_info()

        # 4. Combine information
        all_client_names = set(zfs_clones.keys()) | set(dhcp_info.keys()) # Get unique names from both sources

        for name in all_client_names:
            clone_info = zfs_clones.get(name, {})
            dhcp_client_info = dhcp_info.get(name, {})

            # Placeholder for super client status - needs actual logic
            # e.g., check a database flag or ZFS properties
            is_super = False

            client = {
                "id": name, # Use name as ID
                "name": name,
                "clone": clone_info.get("clone", "N/A"),
                "mac": dhcp_client_info.get("mac", "N/A"),
                "ip": dhcp_client_info.get("ip", "N/A"),
                # Construct placeholder target name if available
                "target": f"iqn.2025-04.mydomain.server:{name}" if name else "N/A", # Adjust domain/date
                "status": "Unknown", # Will be updated below
                "isSuperClient": is_super
            }
            client["status"] = get_client_status(client["ip"]) # Update status based on IP check
            clients_data.append(client)

    except Exception as e:
        print(f"Error getting clients: {e}")
        # Avoid crashing the backend, return empty list or error
        return jsonify({"error": f"Failed to retrieve client list: {e}"}), 500

    # Sort clients alphabetically by name
    clients_data.sort(key=lambda c: c['name'])
    return jsonify(clients_data)

@app.route('/api/clients', methods=['POST'])
def add_client():
    data = request.get_json()
    if not data or 'name' not in data or 'mac' not in data or 'snapshot' not in data:
        abort(400, description="Missing required fields: name, mac, snapshot")

    client_name = data['name']
    mac_address = data['mac'].upper()
    snapshot_name = data['snapshot']
    clone_name = f"{ZFS_POOL}/{client_name}-disk"

    # --- Input Validation ---
    if not re.match(r'^[\w-]+$', client_name):
         abort(400, description="Invalid client name format (use alphanumeric, _, -).")
    if re.search(r'\s', client_name): # Disallow spaces
         abort(400, description="Client name cannot contain spaces.")
    if not re.match(r'^([0-9A-F]{2}[:-]){5}([0-9A-F]{2})$', mac_address):
         abort(400, description="Invalid MAC address format.")
    # Normalize MAC address format (e.g., to use colons)
    mac_address = re.sub(r'[-:]', ':', mac_address)

    # Validate snapshot existence (optional but recommended)
    try:
        run_command(['zfs', 'list', '-H', '-t', 'snapshot', snapshot_name], use_sudo=True)
    except subprocess.CalledProcessError:
         abort(404, description=f"Snapshot '{snapshot_name}' not found.")
    except Exception as e:
         print(f"Error validating snapshot {snapshot_name}: {e}")
         abort(500, description="Error validating snapshot existence.")

    # Check if client name (clone or DHCP entry) already exists
    if os.path.exists(os.path.join(DHCP_INCLUDE_DIR, f"{client_name}.conf")):
         abort(409, description=f"DHCP configuration for client '{client_name}' already exists.")
    try:
        run_command(['zfs', 'list', '-H', clone_name], use_sudo=True, check=False)
        # If previous command didn't raise error and returncode is 0, it exists
        if run_command.returncode == 0:
             abort(409, description=f"ZFS volume '{clone_name}' already exists.")
    except Exception: # Catch potential errors from run_command itself if check=False
        pass # Okay if it doesn't exist


    print(f"Received request to add client: {client_name}, MAC: {mac_address}, Snapshot: {snapshot_name}")

    # --- Define resources to be created for potential rollback ---
    created_zfs_clone = False
    created_dhcp_config = False
    # iscsi_configured = False # Add flag if iSCSI implemented

    try:
        # --- Step 1: Create ZFS Clone ---
        print(f"Creating ZFS clone: {clone_name} from {snapshot_name}")
        run_command(['zfs', 'clone', snapshot_name, clone_name], use_sudo=True)
        created_zfs_clone = True

        # --- Step 2: Create iSCSI Target ---
        # !! Placeholder - Requires robust implementation !!
        target_iqn = f"iqn.2025-04.mydomain.server:{client_name}" # Adjust domain/date
        # block_backstore_name = f"{client_name}_disk"
        # dev_path = f"/dev/zvol/{clone_name}" # Path to the ZVOL device
        print(f"!!! Placeholder: iSCSI target configuration for {target_iqn} skipped !!!")
        # Add actual iSCSI configuration logic here (JSON or targetcli scripting)
        # If successful, set iscsi_configured = True


        # --- Step 3: Create DHCP Reservation ---
        print(f"Creating DHCP reservation for {client_name} ({mac_address})")
        if DHCP_CONFIG_METHOD == "include_files":
            dhcp_client_conf_path = os.path.join(DHCP_INCLUDE_DIR, f"{client_name}.conf")
            # Determine next available IP (improved logic needed for production)
            existing_ips = get_client_dhcp_info().values()
            ip_prefix = "192.168.1." # !!! Adjust to your network !!!
            start_ip = 100
            end_ip = 200
            assigned_ip = None
            used_ips_suffix = {int(info['ip'].split('.')[-1]) for info in existing_ips if info['ip'].startswith(ip_prefix)}
            for i in range(start_ip, end_ip + 1):
                 if i not in used_ips_suffix:
                     assigned_ip = f"{ip_prefix}{i}"
                     break
            if not assigned_ip:
                 raise Exception("Could not find an available IP address in the range.") # Rollback triggered below

            # Ensure YOUR_ISCSI_SERVER_IP is defined or retrieved dynamically
            your_iscsi_server_ip = "192.168.1.10" # !!! Replace with actual server IP !!!
            dhcp_entry = f"""
# Configuration for {client_name}
host {client_name} {{
    hardware ethernet {mac_address};
    fixed-address {assigned_ip};
    option host-name "{client_name}"; # Optional: Set hostname via DHCP
    # Example root-path for iPXE (adjust server IP and IQN format if needed)
    # option root-path "iscsi:{your_iscsi_server_ip}::::{target_iqn}";
}}
"""
            # Write DHCP config file
            try:
                # Requires write permissions to DHCP_INCLUDE_DIR for the user running Flask
                with open(dhcp_client_conf_path, 'w') as f:
                    f.write(dhcp_entry)
                print(f"Wrote DHCP config to {dhcp_client_conf_path}")
                created_dhcp_config = True
            except Exception as e:
                 raise Exception(f"Failed to write DHCP config file: {e}") # Rollback triggered

            # Reload/Restart DHCP server
            try:
                # Check syntax before restarting (optional but safer)
                # run_command(['dhcpd', '-t', '-cf', DHCP_CONFIG_PATH], use_sudo=True)
                run_command(['systemctl', 'restart', 'isc-dhcp-server.service'], use_sudo=True) # Adjust service name if needed
            except Exception as e:
                 raise Exception(f"Failed to restart DHCP service: {e}") # Rollback triggered

        else: # main_config method
            print("!!! Placeholder: Adding DHCP entry to main config skipped !!!")
            raise NotImplementedError("DHCP main_config method not implemented.")


        # --- Success ---
        return jsonify({"message": f"Client {client_name} added successfully", "assigned_ip": assigned_ip}), 201

    except Exception as e:
        # --- Rollback Logic ---
        print(f"Error adding client {client_name}: {e}. Initiating rollback.")
        error_message = f"Failed to add client: {e}"
        try:
            if created_dhcp_config:
                print(f"Rolling back DHCP config: Removing {dhcp_client_conf_path}")
                try:
                    os.remove(dhcp_client_conf_path)
                    # Attempt to restart DHCP again after removal
                    run_command(['systemctl', 'restart', 'isc-dhcp-server.service'], use_sudo=True, check=False)
                except Exception as dhcp_rollback_e:
                    print(f"Error during DHCP rollback: {dhcp_rollback_e}")
                    error_message += f" (DHCP rollback failed: {dhcp_rollback_e})"

            # Add iSCSI rollback here if implemented
            # if iscsi_configured:
            #    print(f"Rolling back iSCSI config for {client_name}")
            #    # Add iSCSI deletion logic

            if created_zfs_clone:
                print(f"Rolling back ZFS clone: Destroying {clone_name}")
                try:
                    run_command(['zfs', 'destroy', clone_name], check=False, use_sudo=True)
                except Exception as zfs_rollback_e:
                    print(f"Error during ZFS rollback: {zfs_rollback_e}")
                    error_message += f" (ZFS rollback failed: {zfs_rollback_e})"

        except Exception as rollback_e:
            print(f"Critical error during rollback: {rollback_e}")
            error_message += f" (Critical rollback error: {rollback_e})"

        # Return error after attempting rollback
        return jsonify({"error": error_message}), 500


@app.route('/api/clients/<client_id>', methods=['DELETE'])
def delete_client(client_id):
    # Basic validation
    if not re.match(r'^[\w-]+$', client_id):
         abort(400, description="Invalid client ID format.")

    clone_name = f"{ZFS_POOL}/{client_id}-disk"
    target_iqn = f"iqn.2025-04.mydomain.server:{client_id}" # Adjust domain/date
    dhcp_client_conf_path = os.path.join(DHCP_INCLUDE_DIR, f"{client_id}.conf")

    print(f"Received request to delete client: {client_id}")

    try:
        errors = []
        # --- Step 1: Delete DHCP Reservation ---
        print(f"Deleting DHCP config for {client_id}")
        dhcp_restarted = False
        if DHCP_CONFIG_METHOD == "include_files":
            if os.path.exists(dhcp_client_conf_path):
                try:
                    # Requires write permissions
                    os.remove(dhcp_client_conf_path)
                    print(f"Removed {dhcp_client_conf_path}")
                    # Reload/Restart DHCP server (only once if multiple files deleted)
                    try:
                        run_command(['systemctl', 'restart', 'isc-dhcp-server.service'], use_sudo=True)
                        dhcp_restarted = True
                    except Exception as e:
                         errors.append(f"Failed to restart DHCP service after removing config: {e}")
                         print(errors[-1])
                except Exception as e:
                    errors.append(f"Failed to remove DHCP config file {dhcp_client_conf_path}: {e}")
                    print(errors[-1])
            else:
                 print("DHCP config file not found, skipping.")
        else:
            print("!!! Placeholder: Removing DHCP entry from main config skipped !!!")
            # Add logic to remove from main config and restart DHCP if needed

        # --- Step 2: Delete iSCSI Target ---
        # !! Placeholder - Requires robust implementation !!
        print(f"Deleting iSCSI target for {client_id}")
        # Add actual iSCSI deletion logic here (JSON or targetcli scripting)
        # Example conceptual commands:
        # try:
        #     run_command(['targetcli', f'/iscsi delete {target_iqn}'], check=False, use_sudo=True)
        #     run_command(['targetcli', f'/backstores/block delete {client_id}_disk'], check=False, use_sudo=True)
        #     run_command(['targetcli', 'saveconfig'], check=False, use_sudo=True) # Save changes
        # except Exception as e:
        #     errors.append(f"Failed to delete iSCSI target {target_iqn}: {e}")
        #     print(errors[-1])
        print("!!! Placeholder: iSCSI target deletion skipped !!!")


        # --- Step 3: Destroy ZFS Clone ---
        print(f"Destroying ZFS clone: {clone_name}")
        try:
            # Check if clone exists before trying to destroy
            check_result = run_command(['zfs', 'list', '-H', clone_name], check=False, use_sudo=True)
            if check_result.returncode == 0:
                 run_command(['zfs', 'destroy', clone_name], use_sudo=True)
                 print(f"Destroyed ZFS clone {clone_name}")
            else:
                 print(f"ZFS clone {clone_name} not found, skipping destroy.")
        except subprocess.CalledProcessError as e:
             # Don't stop the whole process if destroy fails, just log it
             errors.append(f"Failed to destroy ZFS clone {clone_name}: {e.stderr or e}")
             print(errors[-1])
        except Exception as e:
             errors.append(f"Error destroying ZFS clone {clone_name}: {e}")
             print(errors[-1])


        # --- Report Results ---
        if errors:
            # Use 500 if critical parts failed, 207 if mostly successful with minor issues
            status_code = 500 if "Failed to destroy ZFS clone" in str(errors) else 207
            return jsonify({"message": f"Client {client_id} deletion attempted with errors", "errors": errors}), status_code
        else:
            return jsonify({"message": f"Client {client_id} deleted successfully"}), 200

    except Exception as e:
        print(f"An unexpected error occurred during deletion: {e}")
        return jsonify({"error": f"An unexpected error occurred: {e}"}), 500

# --- Snapshot Actions ---
@app.route('/api/snapshots', methods=['POST'])
def create_snapshot():
    data = request.get_json()
    if not data or 'name' not in data:
         abort(400, description="Missing required field: name (snapshot name)")
    snapshot_name = data['name']

    # Basic validation - Check format pool/master@snapname
    # Ensure it contains '@' and starts with the pool name
    if '@' not in snapshot_name or not snapshot_name.startswith(ZFS_POOL + '/'):
         abort(400, description=f"Invalid snapshot name format. Expected {ZFS_POOL}/master@snapname")
    # Further validation: check if master dataset exists
    master_name = snapshot_name.split('@')[0]
    try:
        run_command(['zfs', 'list', '-H', master_name], use_sudo=True)
    except subprocess.CalledProcessError:
         abort(404, description=f"Master dataset '{master_name}' not found.")
    except Exception as e:
         print(f"Error validating master dataset {master_name}: {e}")
         abort(500, description="Error validating master dataset.")

    try:
        run_command(['zfs', 'snapshot', snapshot_name], use_sudo=True)
        return jsonify({"message": f"Snapshot {snapshot_name} created successfully"}), 201
    except subprocess.CalledProcessError as e:
        # Check if snapshot already exists
        if 'dataset already exists' in (e.stderr or ''):
             return jsonify({"error": f"Snapshot '{snapshot_name}' already exists."}), 409 # Conflict
        else:
             print(f"Error creating snapshot {snapshot_name}: {e}")
             return jsonify({"error": f"Failed to create snapshot: {e.stderr or e}"}), 500
    except Exception as e:
        print(f"Error creating snapshot {snapshot_name}: {e}")
        return jsonify({"error": f"An unexpected error occurred: {e}"}), 500

@app.route('/api/snapshots/<path:snapshot_name_encoded>', methods=['DELETE'])
def delete_snapshot(snapshot_name_encoded):
     # Decode the snapshot name from the URL path component
     # Example: pool/fs@snap -> pool%2Ffs%40snap -> pool/fs@snap
     try:
         snapshot_name = snapshot_name_encoded # Flask does URL decoding automatically for path converters
     except Exception as e:
         abort(400, description=f"Invalid snapshot name encoding in URL: {e}")

     # Basic validation
     if '@' not in snapshot_name or not snapshot_name.startswith(ZFS_POOL + '/'):
         abort(400, description="Invalid snapshot name format.")

     print(f"Received request to delete snapshot: {snapshot_name}")

     try:
        # Attempt to destroy the snapshot.
        # ZFS will prevent deletion if there are dependent clones unless -R or -f is used.
        # We avoid -R/-f for safety here. Frontend should handle the 409 error.
        run_command(['zfs', 'destroy', snapshot_name], use_sudo=True)
        return jsonify({"message": f"Snapshot {snapshot_name} deleted successfully"}), 200
     except subprocess.CalledProcessError as e:
         # Check if error is due to clones existing
         if 'has dependent clones' in (e.stderr or ''):
              print(f"Snapshot {snapshot_name} has dependent clones.")
              return jsonify({"error": f"Snapshot '{snapshot_name}' has dependent clones. Promote or destroy clones first."}), 409 # Conflict
         else:
              print(f"Error deleting snapshot {snapshot_name}: {e}")
              return jsonify({"error": f"Failed to delete snapshot: {e.stderr or e}"}), 500
     except Exception as e:
        print(f"Error deleting snapshot {snapshot_name}: {e}")
        return jsonify({"error": f"An unexpected error occurred: {e}"}), 500


# --- Client Control Actions (Placeholders / Wake-on-LAN) ---

@app.route('/api/clients/<client_id>/control', methods=['POST'])
def control_client(client_id):
    data = request.get_json()
    if not data or 'action' not in data:
        abort(400, description="Missing required field: action")
    action = data['action']

    # Basic validation for client_id
    if not re.match(r'^[\w-]+$', client_id):
         abort(400, description="Invalid client ID format.")

    # Get client MAC address (needed for wake) - Improve efficiency if needed
    dhcp_info = get_client_dhcp_info()
    mac_address = dhcp_info.get(client_id, {}).get("mac", None)

    print(f"Received control action '{action}' for client: {client_id}")

    if action == 'wake':
        if not mac_address or mac_address == "N/A":
            return jsonify({"error": f"MAC address not found or invalid for client '{client_id}', cannot wake."}), 404
        try:
            # Ensure 'wakeonlan' utility is installed: sudo apt install wakeonlan
            run_command(['wakeonlan', mac_address], use_sudo=False) # wakeonlan usually doesn't need sudo
            return jsonify({"message": f"Wake-on-LAN packet sent to {mac_address} for client {client_id}"}), 200
        except FileNotFoundError:
             print("Error: 'wakeonlan' command not found. Please install it.")
             return jsonify({"error": "'wakeonlan' command not found. Install it on the server."}), 501 # Not Implemented
        except subprocess.CalledProcessError as e:
             print(f"Error running wakeonlan for {client_id} ({mac_address}): {e}")
             return jsonify({"error": f"Failed to send Wake-on-LAN: {e.stderr or e}"}), 500
        except Exception as e:
            print(f"Error sending Wake-on-LAN for {client_id}: {e}")
            return jsonify({"error": f"Failed to send Wake-on-LAN: {e}"}), 500

    elif action == 'reboot':
        # Requires remote execution mechanism (SSH, agent, IPMI) - Placeholder
        print(f"!!! Placeholder: Reboot action for {client_id} requires external setup !!!")
        # Example using SSH (requires key setup, known IP, user permissions):
        # client_ip = dhcp_info.get(client_id, {}).get("ip", None)
        # if client_ip and client_ip != "N/A":
        #     try:
        #         run_command(['ssh', f'user@{client_ip}', 'sudo reboot'], use_sudo=False, check=True)
        #         return jsonify({"message": f"Reboot command sent via SSH to {client_id} ({client_ip})"}), 200
        #     except Exception as e:
        #         return jsonify({"error": f"Failed to send SSH reboot command: {e}"}), 500
        # else:
        #     return jsonify({"error": "Client IP not found for SSH reboot."}), 404
        return jsonify({"message": f"Placeholder: Reboot action for {client_id} not implemented in backend."}), 501 # Not Implemented

    elif action == 'shutdown':
        # Requires remote execution mechanism - Placeholder
        print(f"!!! Placeholder: Shutdown action for {client_id} requires external setup !!!")
        return jsonify({"message": f"Placeholder: Shutdown action for {client_id} not implemented in backend."}), 501 # Not Implemented

    elif action == 'toggleSuper':
         is_super = data.get('makeSuper', False) # Frontend needs to send desired state
         clone_name = f"{ZFS_POOL}/{client_id}-disk"
         print(f"Attempting to toggle Super Client for {client_id} ({clone_name}) to {is_super}")
         # Placeholder: Implement actual logic.
         # Option 1 (Flag): Update a database or file flag (simple, doesn't affect ZFS behavior)
         # Option 2 (ZFS Promote): If disabling super -> snapshot current state? If enabling -> zfs promote? Needs careful thought.
         #   - Promoting makes the clone independent of its original snapshot.
         #   - This is complex: What happens when you disable it again? Snapshot and re-clone from master?
         if is_super:
             # Example: Try to promote the clone (makes changes persistent directly)
             try:
                 run_command(['zfs', 'promote', clone_name], use_sudo=True)
                 print(f"Promoted ZFS clone {clone_name} for Super Client mode.")
                 # You might want to store this state persistently (e.g., ZFS property or DB)
                 # run_command(['zfs', 'set', 'your_namespace:superclient=on', clone_name], use_sudo=True)
                 return jsonify({"message": f"Super Client mode enabled for {client_id} by promoting clone."}), 200
             except Exception as e:
                 print(f"Error promoting clone {clone_name}: {e}")
                 return jsonify({"error": f"Failed to enable Super Client mode (promote failed): {e}"}), 500
         else:
             # Disabling Super Client mode is harder if promotion was used.
             # Simplest approach: Re-clone from a master snapshot (loses current changes).
             # More complex: Snapshot current state, destroy, re-clone, potentially copy data back?
             print(f"!!! Placeholder: Disabling Super Client mode for {client_id} requires defined logic (e.g., re-clone) !!!")
             # You would need the frontend to ask which snapshot to re-clone from.
             return jsonify({"message": f"Placeholder: Disabling Super Client mode for {client_id} not fully implemented."}), 501

    elif action == 'edit':
         # Placeholder: Requires modal in frontend and backend logic
         # Could update DHCP name, potentially rename ZFS/iSCSI (complex)
         print(f"!!! Placeholder: Edit Client action for {client_id} requires implementation !!!")
         return jsonify({"message": f"Placeholder: Edit Client for {client_id} not implemented."}), 501 # Not Implemented

    else:
        abort(400, description=f"Invalid action: {action}")


# --- CORS Handling (Optional, adjust origin for security) ---
# You might need this if frontend and backend are on different origins (ports/domains)
# from flask_cors import CORS
# CORS(app, resources={r"/api/*": {"origins": ["http://localhost:3000", "http://your-frontend-domain.com"]}})

# --- Main Execution ---
if __name__ == '__main__':
    # Host '0.0.0.0' makes it accessible on the network
    # Debug=True is NOT for production! Use False for production.
    # Use a proper WSGI server like Gunicorn or Waitress for production.
    print("--- Starting Diskless Boot Manager Backend ---")
    print(f"Flask Debug Mode: {app.debug}")
    print("!!! SECURITY WARNING: Ensure proper sudoers configuration and run with a dedicated, non-root user !!!")
    print(f"!!! Using ZFS Pool: {ZFS_POOL} !!!")
    print(f"!!! Using DHCP Config Method: {DHCP_CONFIG_METHOD} ({DHCP_INCLUDE_DIR if DHCP_CONFIG_METHOD == 'include_files' else DHCP_CONFIG_PATH}) !!!")
    # Example for production using Waitress (pip install waitress):
    # from waitress import serve
    # serve(app, host='0.0.0.0', port=5000)
    app.run(host='0.0.0.0', port=5000) # Keep debug=True for development ONLY
