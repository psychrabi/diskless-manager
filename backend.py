#!/usr/bin/env python3

import subprocess
import json
from flask import Flask, request, jsonify, abort, Response # Add Response for config view
from flask_cors import CORS # Import Flask-Cors
import shlex # Used for safer command splitting
import re
import os
import math # For size parsing

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
DHCP_CONFIG_METHOD = "main_config"
DHCP_CONFIG_PATH = "/etc/dhcp/dhcpd.conf"
DHCP_INCLUDE_DIR = "/etc/dhcp/clients.d"  # Used if method is "include_files"

# Ensure DHCP_INCLUDE_DIR exists if using include_files method
if DHCP_CONFIG_METHOD == "include_files":
    os.makedirs(DHCP_INCLUDE_DIR, exist_ok=True)

# TFTP Configuration Path (usually defaults file)
TFTP_CONFIG_PATH = "/etc/default/tftpd-hpa"

# iSCSI Target Configuration Path (LIO JSON config)
ISCSI_CONFIG_PATH = "/etc/rtslib-fb-target/saveconfig.json"

# Service name mapping for systemctl
SERVICE_MAP = {
    'iscsi': 'target.service',
    'dhcp': 'isc-dhcp-server.service', # Adjust if needed
    'tftp': 'tftpd-hpa.service',
}

# Config file mapping for viewing
CONFIG_FILE_MAP = {
    'dhcp': DHCP_CONFIG_PATH,
    'tftp': TFTP_CONFIG_PATH,
    'iscsi': ISCSI_CONFIG_PATH,
    # Add other relevant configs if needed
}

app = Flask(__name__)

# --- CORS Configuration ---
# Configure CORS to allow requests from your frontend's origin.
# Replace "http://localhost:5173" with the actual origin of your React app
# (e.g., "http://<your-frontend-ip>:<port>" or your domain).
# Using "*" allows all origins, which is less secure for production.
# Allow specific methods and headers if needed
CORS(app, resources={r"/api/*": {"origins": ["http://localhost:5173", "http://192.168.1.206:5173"]}}) # Example


# --- Helper Functions ---

def run_command(command_list, check=True, capture_output=True, text=True, use_sudo=True, timeout=60):
    """
    Runs a system command securely using subprocess.run.
    Handles sudo execution, basic error checking, and timeout.

    Args:
        command_list (list): The command and its arguments as a list.
        check (bool): If True, raise CalledProcessError on non-zero exit code.
        capture_output (bool): If True, capture stdout and stderr.
        text (bool): If True, decode stdout/stderr as text.
        use_sudo (bool): If True, prepend sudo command from config.
        timeout (int): Command timeout in seconds.

    Returns:
        subprocess.CompletedProcess: The result object.

    Raises:
        subprocess.CalledProcessError: If check is True and command fails.
        subprocess.TimeoutExpired: If the command times out.
        FileNotFoundError: If the command (or sudo) is not found.
        Exception: For other potential errors.
    """
    if not isinstance(command_list, list):
        raise ValueError("command_list must be a list")

    cmd = [SUDO_CMD, *command_list] if use_sudo else command_list
    print(f"Running command: {' '.join(cmd)}")

    try:
        result = subprocess.run(
            cmd,
            check=check,
            capture_output=capture_output,
            text=text,
            timeout=timeout
        )
        return result
    except subprocess.CalledProcessError as e:
        error_msg = f"Command failed with exit code {e.returncode}: {' '.join(cmd)}\n{e.stderr if e.stderr else e.stdout if e.stdout else ''}"
        print(f"Error running command: {error_msg}")
        raise Exception(error_msg) from e
    except subprocess.TimeoutExpired as e:
        error_msg = f"Command timed out after {timeout} seconds: {' '.join(cmd)}"
        print(f"Timeout running command: {error_msg}")
        raise Exception(error_msg) from e
        # print(f"Stdout: {e.stdout}")
        raise # Re-raise the exception after logging
    except FileNotFoundError as e:
        print(f"Error: Command or sudo not found: {e}")
        raise
    except Exception as e:
        print(f"An unexpected error occurred running command: {e}")
        raise

def parse_zfs_list(output):
    """ Parses output of 'zfs list -H -o name,creation,used' """
    datasets = []
    for line in output.strip().split('\n'):
        try:
            # Split by tab and trim whitespace
            parts = [p.strip() for p in line.split('\t')]
            if len(parts) != 3:
                print(f"Warning: Skipping malformed zfs list line (wrong number of fields): {line}")
                continue
            
            name, creation, used = parts
            
            # Convert creation time to a more standardized format
            try:
                # Try to parse the date string
                from datetime import datetime
                creation_dt = datetime.strptime(creation, '%a %b %d %H:%M %Y')
                creation = creation_dt.strftime('%Y-%m-%d %H:%M:%S')
            except ValueError:
                print(f"Warning: Could not parse creation time: {creation}")
                pass
            
            datasets.append({
                'name': name,
                'created': creation,
                'used': used
            })
        except Exception as e:
            print(f"Warning: Skipping malformed zfs list line: {line} (Error: {e})")
    return datasets

def parse_size_to_bytes(size_str):
    """ Parses human-readable size string (e.g., 50G, 1T) to bytes. """
    size_str = size_str.strip().upper()
    units = {"B": 1, "K": 1024, "M": 1024**2, "G": 1024**3, "T": 1024**4, "P": 1024**5}
    match = re.match(r'^(\d+(\.\d+)?)\s*([KMGTPE]?)B?$', size_str)
    if not match:
        raise ValueError(f"Invalid size format: {size_str}")

    value = float(match.group(1))
    unit = match.group(3) if match.group(3) else 'B' # Default to Bytes if no unit

    if unit not in units:
         raise ValueError(f"Invalid size unit: {unit}")

    return int(value * units[unit])


def get_client_status(client_ip):
    """ Checks client status using ping. """
    if not client_ip or client_ip == "N/A":
        return "Unknown"
    try:
        # Run ping command (1 packet, 1 second timeout)
        result = run_command(
            ['ping', '-c', '1', '-W', '1', client_ip],
            check=False, use_sudo=False
        )
        return "Online" if result.returncode == 0 else "Offline"
    except Exception as e:
        print(f"Error pinging client {client_ip}: {e}")
        return "Error"


def get_client_dhcp_info():
    """Parses DHCP host entries from the main dhcpd.conf file."""
    clients = {}
    if DHCP_CONFIG_METHOD != "main_config":
        print("Warning: get_client_dhcp_info only implemented for 'main_config' method.")
        return clients

    try:
        with open(DHCP_CONFIG_PATH, 'r') as f:
            content = f.read()
        
        # Parse host entries using regex
        host_blocks = re.finditer(
            r'^\s*host\s+([\w-]+)\s*{([^}]*)}',
            content,
            re.MULTILINE | re.DOTALL
        )
        
        for host_block in host_blocks:
            hostname = host_block.group(1)
            block_content = host_block.group(2)
            
            # Extract MAC address
            mac_match = re.search(r'^\s*hardware\s+ethernet\s+([\w:]+)\s*;', block_content, re.MULTILINE | re.IGNORECASE)
            ip_match = re.search(r'^\s*fixed-address\s+([\d.]+)\s*;', block_content, re.MULTILINE | re.IGNORECASE)
            
            if mac_match and ip_match:
                mac = mac_match.group(1).upper()
                mac = re.sub(r'[-:]', ':', mac)  # Normalize MAC
                ip = ip_match.group(1)
                clients[hostname] = {"mac": mac, "ip": ip}
            else:
                print(f"Warning: Could not parse host/mac/ip for host {hostname}")
                
    except Exception as e:
        print(f"Error reading or parsing DHCP config {DHCP_CONFIG_PATH}: {e}")
    
    return clients


# --- API Endpoints ---

@app.route('/api/status', methods=['GET'])
def get_api_status():
    return jsonify({"status": "ok", "message": "Backend is running"})

@app.route('/api/services', methods=['GET'])
def get_services_status():
    statuses = {}
    # Check systemd services
    for key, service_name in SERVICE_MAP.items():
        try:
            result = run_command(['systemctl', 'is-active', service_name], check=False, use_sudo=False)
            status = result.stdout.strip() if result.returncode == 0 else 'inactive'
            statuses[key] = {"name": service_name.replace('.service', ''), "status": status}
        except Exception as e:
            print(f"Error checking service {service_name}: {e}")
            statuses[key] = {"name": service_name.replace('.service', ''), "status": "error"}

    # ZFS pool health check
    zfs_status = 'error'
    try:
         result = run_command(['zpool', 'status', ZFS_POOL], use_sudo=True, check=False)
         if result.returncode == 0:
             pool_state = 'unknown'
             for line in result.stdout.strip().split('\n'):
                 if line.strip().startswith('state:'):
                     pool_state = line.split(':')[1].strip()
                     break
             zfs_status = 'active' if pool_state == 'ONLINE' else 'degraded'
             print(f"ZFS pool '{ZFS_POOL}' state: {pool_state} -> Status: {zfs_status}")
         else:
             print(f"zpool status command failed for '{ZFS_POOL}' with code {result.returncode}")
             zfs_status = 'error'
    except FileNotFoundError:
        print(f"Error: 'zpool' command not found.")
        zfs_status = 'error'
    except Exception as e:
         print(f"Error checking ZFS pool status for {ZFS_POOL}: {e}")
         zfs_status = 'error'
    statuses['zfs'] = {"name": f"ZFS Pool ({ZFS_POOL})", "status": zfs_status}

    return jsonify(statuses)

# --- Service Control & Config View ---

@app.route('/api/services/<service_key>/control', methods=['POST'])
def control_service(service_key):
    """ Handles actions like 'restart' for services """
    if service_key not in SERVICE_MAP:
        abort(404, description=f"Unknown service key: {service_key}")

    data = request.get_json()
    if not data or 'action' not in data:
        abort(400, description="Missing required field: action")
    action = data['action']
    service_name = SERVICE_MAP[service_key]

    print(f"Received control action '{action}' for service: {service_key} ({service_name})")

    if action == 'restart':
        try:
            # Use check=True to ensure restart command doesn't fail silently
            run_command(['systemctl', 'restart', service_name], use_sudo=True, check=True)
            return jsonify({"message": f"Service '{service_name}' restart command issued successfully."}), 200
        except subprocess.CalledProcessError as e:
            print(f"Error restarting service {service_name}: {e}")
            return jsonify({"error": f"Failed to restart service '{service_name}': {e.stderr or e}"}), 500
        except Exception as e:
            print(f"Unexpected error restarting service {service_name}: {e}")
            return jsonify({"error": f"An unexpected error occurred: {e}"}), 500
    else:
        abort(400, description=f"Unsupported action '{action}' for service '{service_key}'")


@app.route('/api/services/<service_key>/config', methods=['GET'])
def get_service_config(service_key):
    """ Retrieves the content of a service's configuration file. """
    if service_key not in CONFIG_FILE_MAP:
        abort(404, description=f"Configuration file mapping not found for service key: {service_key}")

    config_path = CONFIG_FILE_MAP[service_key]
    print(f"Attempting to read config file for {service_key}: {config_path}")

    # SECURITY NOTE: Reading arbitrary files based on user input is dangerous.
    # This implementation relies on a predefined map (CONFIG_FILE_MAP) for safety.
    # Ensure the user running Flask has read permissions for these files.

    if not os.path.exists(config_path):
         abort(404, description=f"Configuration file not found: {config_path}")
    if not os.path.isfile(config_path):
         abort(400, description=f"Configuration path is not a file: {config_path}")

    try:
        with open(config_path, 'r') as f:
            content = f.read()
        # Return as plain text
        return Response(content, mimetype='text/plain')
    except PermissionError:
         print(f"Permission denied reading config file: {config_path}")
         abort(403, description=f"Permission denied reading configuration file for {service_key}.")
    except Exception as e:
        print(f"Error reading config file {config_path}: {e}")
        abort(500, description=f"Error reading configuration file for {service_key}.")


# --- Master Image Management ---

@app.route('/api/masters', methods=['GET'])
def get_masters():
    # ... (keep existing implementation) ...
    masters_data = []
    try:
        # print(f"ZFS Pool: {ZFS_POOL}")
        # print("Raw ZFS list output:")
        result = run_command(
            ['zfs', 'list', '-H', '-t', 'filesystem,volume', '-o', 'name,creation,used', '-r', ZFS_POOL],
            use_sudo=True
        )
        # print("=== Raw ZFS List Output ===")
        # print(result.stdout)
        # print("=== End Raw Output ===")

        all_datasets = parse_zfs_list(result.stdout)
        # print(f"Parsed {len(all_datasets)} datasets:")
        # for ds in all_datasets:
        #     print(f"Dataset: {ds['name']}")

        master_names = []
        # Look for master images in the root and any subdirectories
        for ds in all_datasets:
            # Check if this dataset or any of its snapshots end with '-master'
            if ds['name'].lower().endswith('-master'):
                print(f"Found master dataset: {ds['name']}")
                master_names.append(ds['name'])
                continue
            
            # Check snapshots of this dataset
            try:
                snap_result = run_command(
                    ['zfs', 'list', '-H', '-t', 'snapshot', '-o', 'name', '-r', ds['name']],
                    use_sudo=True, check=False
                )
                if snap_result.returncode == 0:
                    print(f"Checking snapshots for {ds['name']}")
                    for snap in snap_result.stdout.splitlines():
                        # print(f"Checking snapshot: {snap}")
                        if snap.lower().endswith('-master'):
                            print(f"Found master snapshot: {snap}")
                            master_names.append(snap)
            except Exception as e:
                print(f"Error checking snapshots for {ds['name']}: {e}")
        
        # Remove duplicates and sort
        master_names = sorted(set(master_names))
        print(f"Found {len(master_names)} master images:")
        for master in master_names:
            print(f"Master: {master}")

        for master_name in master_names:
            snapshots = []
            try:
                snap_result = run_command(
                    ['zfs', 'list', '-H', '-t', 'snapshot', '-o', 'name,creation,used', '-r', master_name],
                    use_sudo=True, check=False
                )
                if snap_result.returncode == 0:
                    snapshots = parse_zfs_list(snap_result.stdout)
            except Exception as snap_e:
                print(f"Error listing snapshots for {master_name}: {snap_e}")

            masters_data.append({
                 "id": master_name,
                 "name": master_name,
                 "snapshots": sorted(snapshots, key=lambda s: s['created'])
            })
    except Exception as e:
        print(f"Error getting masters: {e}")
        return jsonify({"error": f"Failed to retrieve master images: {e}"}), 500
    return jsonify(masters_data)


@app.route('/api/masters', methods=['POST'])
def create_master():
    """ Creates a new ZFS volume (ZVOL) intended as a master image base. """
    data = request.get_json()
    if not data or 'name' not in data or 'size' not in data:
        abort(400, description="Missing required fields: name, size (e.g., '50G')")

    master_base_name = data['name'] # e.g., "win11-base"
    size_str = data['size'] # e.g., "50G"
    # Construct full ZVOL name, assuming convention pool/basename-master
    master_zvol_name = f"{ZFS_POOL}/{master_base_name}-master"

    # --- Input Validation ---
    if not re.match(r'^[\w-]+$', master_base_name):
         abort(400, description="Invalid master base name format (use alphanumeric, _, -).")
    if re.search(r'\s', master_base_name):
         abort(400, description="Master base name cannot contain spaces.")
    try:
        # Validate size format (optional but good practice)
        parse_size_to_bytes(size_str)
    except ValueError as e:
        abort(400, description=f"Invalid size format: {e}")

    # Check if ZVOL already exists
    try:
        result = run_command(['zfs', 'list', '-H', master_zvol_name], use_sudo=True, check=False)
        if result.returncode == 0:
             abort(409, description=f"ZFS volume '{master_zvol_name}' already exists.")
    except Exception as e:
        print(f"Error checking ZFS volume existence for {master_zvol_name}: {e}")
        abort(500, description="Error checking if ZFS volume exists.")

    print(f"Received request to create master ZVOL: {master_zvol_name} with size {size_str}")

    try:
        # Create the ZFS volume (ZVOL)
        # Add -o volblocksize=8k or 16k if desired for Windows performance
        run_command(['zfs', 'create', '-V', size_str, '-o', 'volblocksize=8k', master_zvol_name], use_sudo=True)
        print(f"Successfully created ZVOL {master_zvol_name}")
        # Return the created master info (optional)
        return jsonify({
            "message": f"Master ZVOL '{master_zvol_name}' created successfully.",
            "master": {
                 "id": master_zvol_name,
                 "name": master_zvol_name,
                 "snapshots": [] # Newly created master has no snapshots
            }
        }), 201

    except subprocess.CalledProcessError as e:
        print(f"Error creating ZVOL {master_zvol_name}: {e}")
        return jsonify({"error": f"Failed to create ZFS volume: {e.stderr or e}"}), 500
    except Exception as e:
        print(f"Unexpected error creating ZVOL {master_zvol_name}: {e}")
        return jsonify({"error": f"An unexpected error occurred: {e}"}), 500


# --- Client Management ---

@app.route('/api/clients', methods=['GET'])
def get_clients():
    # ... (keep existing implementation) ...
    clients_data = []
    try:
        zfs_clones = {}
        try:
            zfs_result = run_command(
                ['zfs', 'list', '-H', '-t', 'volume', '-o', 'name,origin', '-r', ZFS_POOL],
                use_sudo=True
            )
            if zfs_result.returncode == 0:
                for line in zfs_result.stdout.strip().split('\n'):
                    if not line: continue
                    name, origin = line.split('\t')
                    if origin != '-' and name.endswith('-disk'):
                        match = re.match(rf"^{ZFS_POOL}/([\w-]+)-disk$", name)
                        if match:
                            client_name = match.group(1)
                            zfs_clones[client_name] = {"clone": name, "origin": origin}
        except Exception as zfs_e:
            print(f"Error listing ZFS clones: {zfs_e}")

        iscsi_targets = {} # Placeholder
        dhcp_info = get_client_dhcp_info()
        all_client_names = set(zfs_clones.keys()) | set(dhcp_info.keys())

        for name in sorted(list(all_client_names)):
            clone_info = zfs_clones.get(name, {})
            dhcp_client_info = dhcp_info.get(name, {})
            is_super = False # Placeholder
            client_ip = dhcp_client_info.get("ip", "N/A")
            client = {
                "id": name, "name": name,
                "clone": clone_info.get("clone", "N/A"),
                "mac": dhcp_client_info.get("mac", "N/A"),
                "ip": client_ip,
                "target": f"iqn.2025-04.com.nsboot:{name}" if name else "N/A", # Adjust domain/date
                "status": get_client_status(client_ip),
                "isSuperClient": is_super
            }
            clients_data.append(client)
    except Exception as e:
        print(f"Error getting clients: {e}")
        return jsonify({"error": f"Failed to retrieve client list: {e}"}), 500
    return jsonify(clients_data)


@app.route('/api/clients', methods=['POST'])
def add_client():
    print("=== Starting client creation ===")
    data = request.get_json()
    if not data:
        print("Error: No data received in request")
        abort(400, description="No data provided")
    
    name = data.get('name')
    mac = data.get('mac')
    ip = data.get('ip')
    master = data.get('master')
    snapshot = data.get('snapshot')
    
    print(f"=== Client details ===")
    print(f"Name: {name}")
    print(f"MAC: {mac}")
    print(f"IP: {ip}")
    print(f"Master: {master}")
    print(f"Snapshot: {snapshot}")
    
    if not all([name, mac, ip, master]):
        print("Error: Missing required fields")
        abort(400, description="Missing required fields: name, mac, ip, master")
    
    if not re.match(r'^([0-9A-Fa-f]{2}[:-]){5}([0-9A-Fa-f]{2})$', mac):
        print(f"Error: Invalid MAC address format: {mac}")
        abort(400, description="Invalid MAC address format")
    
    if not re.match(r'^([\d]{1,3}\.){3}\d{1,3}$', ip):
        print(f"Error: Invalid IP address format: {ip}")
        abort(400, description="Invalid IP address format")
    
    # Create ZFS clone from snapshot or master
    try:
        print(f"=== Checking base snapshot ===")
        print(f"Looking for snapshot: {snapshot}")
        
        # If a specific snapshot is provided, use that instead of base
        if snapshot:
            print(f"=== Using provided snapshot: {snapshot}")
            snapshot_name = snapshot
            clone_name = f"{ZFS_POOL}/{name}-disk"
            run_command(['zfs', 'clone', snapshot_name, clone_name], use_sudo=True)
            created_zfs_clone = True
            
            target_iqn = f"iqn.2025-04.com.nsboot:{name.lower().replace('_', '')}" # Adjust to lowercase and remove underscores
            target_path = f"/dev/zvol/{clone_name}"  # Remove ZFS_POOL prefix if present
        else:
            # Check if base snapshot exists
            base_snapshot = f"{ZFS_POOL}/{master}@base"
            result = run_command(['zfs', 'list', '-H', '-t', 'snapshot', base_snapshot], use_sudo=True, check=False)
            
            if result.returncode == 0:
                # Base snapshot exists, create clone from it
                snapshot_name = f"{ZFS_POOL}/{master}@{name}_base"
                run_command(['zfs', 'snapshot', snapshot_name], use_sudo=True)
                
                clone_name = f"{ZFS_POOL}/{name}-disk"
                run_command(['zfs', 'clone', snapshot_name, clone_name], use_sudo=True)
                created_zfs_clone = True
                
                target_iqn = f"iqn.2025-04.com.nsboot:{name.lower().replace('_', '')}" # Adjust to lowercase and remove underscores
                target_path = f"/dev/zvol/{clone_name}"  # Remove ZFS_POOL prefix if present
            else:
                # No base snapshot exists, use master volume directly
                target_iqn = f"iqn.2025-04.com.nsboot:{name.lower().replace('_', '')}" # Adjust to lowercase and remove underscores
                target_path = f"/dev/zvol/{master}"  # Remove ZFS_POOL prefix if present
                created_zfs_clone = False
                
                # Check if master volume is already in use by any block store
                print(f"=== Checking if master volume {master} is in use ===")
                result = run_command(['targetcli', 'backstores/block/ ls'], use_sudo=True, check=False)
                
                # Find all block stores using this master volume
                block_stores_to_delete = []
                for line in result.stdout.split('\n'):
                    if f"/dev/zvol/{master}" in line:
                        # Extract block store name from line (format: o- block_name ...)
                        if line.startswith('o- '):
                            block_store_name = line.split(' ')[1]
                            block_stores_to_delete.append(block_store_name)
                
                # Delete all block stores using this master volume
                for block_store in block_stores_to_delete:
                    print(f"Deleting block store {block_store} that uses master volume {master}")
                    run_command(['targetcli', 'backstores/block/ delete', block_store], use_sudo=True)
        
        try:
            # Check if target exists by listing all iSCSI targets
            result = run_command(['targetcli', 'iscsi/ ls'], use_sudo=True, check=False)
            print(f"Existing iSCSI targets: {result.stdout}")
            target_exists = target_iqn in result.stdout
            
            # Create iSCSI target if it doesn't exist
            print(f"=== Checking target existence ===")
            print(f"Target IQN: {target_iqn}")
            print(f"Target exists: {target_exists}")
            
            if not target_exists:
                print("Creating new target...")
                run_command(['targetcli', 'iscsi/ create', target_iqn], use_sudo=True)
            else:
                print("Target already exists, skipping creation")
            
            # print(f"=== Setting up block store ===")
            # print(f"Block name: block_{name}")
            # print(f"Target path: {target_path}")
            
            # First check if block store already exists and delete it
            block_store_name = f"block_{name}"
            result = run_command(['targetcli', 'backstores/block/ ls'], use_sudo=True, check=False)
            if block_store_name in result.stdout:
                print(f"Deleting existing block store {block_store_name}...")
                run_command(['targetcli', 'backstores/block/ delete', block_store_name], use_sudo=True)
            
            # Create new block store
            print(f"Creating new block store: {block_store_name}")
            run_command(['targetcli', 'backstores/block create', block_store_name, target_path], use_sudo=True)
            
            # Check if block store exists
            result = run_command(['targetcli', 'backstores/block/ ls'], use_sudo=True, check=False)
            print(f"Existing block stores: {result.stdout}")
            
            if f'block_{name}' in result.stdout:
                print(f"Block store block_{name} already exists, skipping creation")
            else:
                print(f"Creating new block store: block_{name}")
                run_command(['targetcli', 'backstores/block create', f'block_{name}', target_path], use_sudo=True)
            
            # Create LUN if it doesn't exist
            print(f"=== Setting up LUN ===")
            result = run_command(['targetcli', f'iscsi/{target_iqn}/tpg1/luns ls'], use_sudo=True, check=False)
            print(f"Existing LUNs: {result.stdout}")
            if f'block_{name}' not in result.stdout:
                print(f"Creating LUN for block_{name}")
                run_command(['targetcli', f'iscsi/{target_iqn}/tpg1/luns create', f'/backstores/block/block_{name}'], use_sudo=True)
            else:
                print("LUN already exists, skipping creation")
            
            # Check for existing portals
            print(f"=== Checking for existing portals ===")
            result = run_command(['targetcli', f'iscsi/{target_iqn}/tpg1/portals/ ls'], use_sudo=True, check=False)
            print(f"Existing portals: {result.stdout}")
            
            # Skip portal creation - we'll just check if they exist
            if '0.0.0.0' not in result.stdout:
                print("No portal with 0.0.0.0 found")
            else:
                print("Portal with 0.0.0.0 already exists")
            
            # # Create LUN and portal
            # run_command(['targetcli', f'iscsi/{target_iqn}/tpg1/luns create', f'/backstores/block/block_{name}'], use_sudo=True)
            
            # # Check if portal already exists
            # result = run_command(['targetcli', f'iscsi/{target_iqn}/tpg1/portals/ ls'], use_sudo=True, check=False)
            # if '0.0.0.0' not in result.stdout:targetcli
            # Enable and start target service
            print("=== Enabling and starting target service ===")
            run_command(['systemctl', 'enable', 'target'], use_sudo=True)
            run_command(['systemctl', 'start', 'target'], use_sudo=True)
            
            print(f"=== Client creation completed ===")
            print(f"Successfully created iSCSI target {target_iqn}")
            print(f"Target path: {target_path}")
            print(f"Block store: block_{name}")
        except Exception as e:
            raise Exception(f"Failed to create iSCSI target: {e}")

        print(f"Creating DHCP reservation for {name} ({mac})")
        assigned_ip = None
        
        # Get existing DHCP configurations
        existing_ips = get_client_dhcp_info().values()
        ip_prefix = "192.168.1." # !!! Adjust network !!!
        start_ip, end_ip = 100, 200
        used_ips_suffix = {int(info['ip'].split('.')[-1]) for info in existing_ips if info['ip'].startswith(ip_prefix)}
        
        # Find available IP
        for i in range(start_ip, end_ip + 1):
            if i not in used_ips_suffix: 
                assigned_ip = f"{ip_prefix}{i}"; 
                break
        
        if not assigned_ip: 
            raise Exception("No available IP address in range.")

        # Format name as PCXXX
        formatted_name = f"PC{int(name.split('_')[1]):03d}" if '_' in name else name.upper()
        
        # Get iSCSI target name and details
        iscsi_target = f"iqn.2025-04.com.nsboot:{name}"
        # Assuming server IP is 192.168.1.206 (from the logs)
        server_ip = "192.168.1.206"            
        
        dhcp_entry = f"host {formatted_name} {{\n    hardware ethernet {mac};\n    fixed-address {assigned_ip};\n    option host-name \"{formatted_name}\";\n    if substring (option vendor-class-identifier, 15, 5) = \"00000\" {{\n        filename \"ipxe.kpxe\";\n    }}\n    elsif substring (option vendor-class-identifier, 15, 5) = \"00006\" {{\n        filename \"ipxe32.efi\";\n    }}\n    else {{\n        filename \"ipxe.efi\";\n    }}\n    option root-path \"iscsi:{server_ip}::::{iscsi_target}\";\n}}"
        
        try:
            if DHCP_CONFIG_METHOD == "include_files":
                # For include_files method, write to individual config file
                dhcp_client_conf_path = os.path.join(DHCP_INCLUDE_DIR, f"{name}.conf")
                with open(dhcp_client_conf_path, 'w') as f:
                    f.write(dhcp_entry)
                print(f"Wrote DHCP config: {dhcp_client_conf_path}")
                created_dhcp_config = True
            else:  # main_config method
                # Read existing config
                with open(DHCP_CONFIG_PATH, 'r') as f:
                    content = f.read()
                
                # Find existing host entries
                host_entries = re.findall(r'host\s+([\w-]+)\s*{([^}]*)}', content, re.MULTILINE | re.DOTALL)
                
                # Check if host already exists
                host_exists = any(host[0] == name for host in host_entries)
                
                # If host exists, remove it first
                if host_exists:
                    new_content = re.sub(
                        rf'host\s+{re.escape(name)}\s*{{[^}}]*}}\s*\n*',
                        '',
                        content,
                        flags=re.MULTILINE | re.DOTALL
                    )
                else:
                    new_content = content
                
                # Add new host entry
                new_content += f"\n{dhcp_entry}\n"
                
                # Write updated config
                with open(DHCP_CONFIG_PATH, 'w') as f:
                    f.write(new_content)
                
                print(f"Updated DHCP config: {DHCP_CONFIG_PATH}")
                created_dhcp_config = True
            
            # Restart DHCP service
            run_command(['systemctl', 'restart', 'isc-dhcp-server.service'], use_sudo=True)
            
            # Verify DHCP configuration
            if DHCP_CONFIG_METHOD == "include_files":
                with open(dhcp_client_conf_path, 'r') as f:
                    content = f.read()
            else:
                with open(DHCP_CONFIG_PATH, 'r') as f:
                    content = f.read()
            
            if assigned_ip not in content or mac not in content:
                raise Exception("DHCP configuration format validation failed")
            
        except Exception as e:
            raise Exception(f"Failed updating DHCP config: {e}")

        return jsonify({"message": f"Client {name} added", "assigned_ip": assigned_ip}), 201

    except Exception as e:
        error_message = str(e)
        print(f"Error adding client {name}: {error_message}. Rolling back.")
        try:
            if 'created_dhcp_config' in locals() and created_dhcp_config:
                print(f"Rolling back DHCP: Removing {dhcp_client_conf_path}")
                try:
                    dhcp_client_conf_path = os.path.join(DHCP_INCLUDE_DIR, f"{name}.conf")
                    if os.path.exists(dhcp_client_conf_path): os.remove(dhcp_client_conf_path)
                    run_command(['systemctl', 'restart', 'isc-dhcp-server.service'], use_sudo=True, check=False)
                except Exception as dhcp_rb_e: 
                    error_message += f" (DHCP rollback failed: {str(dhcp_rb_e)})"
            
            if 'created_zfs_clone' in locals() and created_zfs_clone:
                print(f"Rolling back ZFS: Destroying {clone_name}")
                try:
                    check_res = run_command(['zfs', 'list', '-H', clone_name], check=False, use_sudo=True)
                    if check_res.returncode == 0:
                        run_command(['zfs', 'destroy', clone_name], use_sudo=True, check=False)
                except Exception as zfs_rb_e: 
                    error_message += f" (ZFS rollback failed: {str(zfs_rb_e)})"
            
            # Return proper JSON error response
            return jsonify({"error": error_message, "success": False}), 500
        except Exception as rb_e:
            return jsonify({"error": f"Failed to rollback after error: {str(rb_e)}", "success": False}), 500


@app.route('/api/clients/<client_id>', methods=['DELETE'])
def delete_client(client_id):
    # ... (keep existing implementation) ...
    clone_name = f"{ZFS_POOL}/{client_id}-disk"
    dhcp_client_conf_path = os.path.join(DHCP_INCLUDE_DIR, f"{client_id}.conf")
    print(f"Deleting client: {client_id}")
    errors = []
    try:
        dhcp_restarted_needed = False
        if DHCP_CONFIG_METHOD == "include_files" and os.path.exists(dhcp_client_conf_path):
            try: os.remove(dhcp_client_conf_path); print(f"Removed {dhcp_client_conf_path}"); dhcp_restarted_needed = True
            except Exception as e: errors.append(f"Failed removing DHCP config: {e}"); print(errors[-1])
        if dhcp_restarted_needed:
            try: print("Restarting DHCP..."); run_command(['systemctl', 'restart', 'isc-dhcp-server.service'], use_sudo=True)
            except Exception as e: errors.append(f"Failed restarting DHCP: {e}"); print(errors[-1])

        print(f"Deleting iSCSI target for {client_id}")
        try:
            # Get iSCSI target name
            target_iqn = f"iqn.2025-04.com.nsboot:{client_id.lower().replace('_', '')}"
            
            # Check if target exists
            result = run_command(['targetcli', 'iscsi/ ls'], use_sudo=True, check=False)
            if target_iqn in result.stdout:
                print(f"Found iSCSI target: {target_iqn}")
                
                # Delete LUNs
                result = run_command(['targetcli', f'iscsi/{target_iqn}/tpg1/luns ls'], use_sudo=True, check=False)
                for line in result.stdout.split('\n'):
                    if 'block_' in line:
                        lun_name = line.split()[1]
                        print(f"Deleting LUN: {lun_name}")
                        run_command(['targetcli', f'iscsi/{target_iqn}/tpg1/luns delete', lun_name], use_sudo=True)
                
                # Delete block store
                block_store_name = f"block_{client_id}"
                result = run_command(['targetcli', 'backstores/block/ ls'], use_sudo=True, check=False)
                if block_store_name in result.stdout:
                    print(f"Deleting block store: {block_store_name}")
                    run_command(['targetcli', 'backstores/block/ delete', block_store_name], use_sudo=True)
                
                # Delete target
                print(f"Deleting iSCSI target: {target_iqn}")
                run_command(['targetcli', 'iscsi/ delete', target_iqn], use_sudo=True)
            else:
                print(f"iSCSI target {target_iqn} not found")
        except Exception as e:
            errors.append(f"Failed cleaning up iSCSI resources: {e}")
            print(errors[-1])

        print(f"Destroying ZFS clone: {clone_name}")
        try:
            # Check if clone exists
            check_result = run_command(['zfs', 'list', '-H', clone_name], check=False, use_sudo=True)
            if check_result.returncode == 0:
                # First try to force unmount any mounted filesystems
                run_command(['zfs', 'umount', '-f', clone_name], use_sudo=True, check=False)
                
                # Then destroy the clone
                run_command(['zfs', 'destroy', '-f', clone_name], use_sudo=True)
                print(f"Destroyed {clone_name}")
            else:
                print(f"ZFS clone {clone_name} not found.")
        except subprocess.CalledProcessError as e: return jsonify({"error": f"Failed destroying ZFS clone: {e.stderr or e}", "details": errors}), 500
        except Exception as e: return jsonify({"error": f"Error destroying ZFS clone: {e}", "details": errors}), 500

        if errors: return jsonify({"message": f"Client {client_id} deleted with issues.", "errors": errors}), 207
        else: return jsonify({"message": f"Client {client_id} deleted successfully"}), 200
    except Exception as e: return jsonify({"error": f"Unexpected error during deletion: {e}"}), 500


# --- Client Management ---
@app.route('/api/clients/edit/<client_id>', methods=['POST'])
def edit_client(client_id):
    """
    Edit client details (name, MAC address, IP address) in the main DHCP config file.
    """
    if not re.match(r'^[\w-]+$', client_id):
        abort(400, description="Invalid client ID")
    
    data = request.get_json()
    if not data:
        abort(400, description="No update data provided")

    try:
        # Get current client info
        dhcp_info = get_client_dhcp_info()
        if client_id not in dhcp_info:
            return jsonify({"error": f"Client {client_id} not found"}), 404

        new_name = data.get('name', client_id).strip()
        new_mac = data.get('mac', dhcp_info[client_id]['mac']).strip().upper()
        new_ip = data.get('ip', dhcp_info[client_id]['ip']).strip()

        # Validate inputs
        if not re.match(r'^[\w-]+$', new_name):
            return jsonify({"error": "Invalid client name format"}), 400
        if not re.match(r'^([0-9A-Fa-f]{2}[:-]){5}([0-9A-Fa-f]{2})$', new_mac):
            return jsonify({"error": "Invalid MAC address format"}), 400
        if not re.match(r'^([\d]{1,3}\.){3}\d{1,3}$', new_ip):
            return jsonify({"error": "Invalid IP address format"}), 400

        # Prepare new DHCP host entry
        formatted_name = f"PC{int(new_name.split('_')[1]):03d}" if '_' in new_name else new_name.upper()
        iscsi_target = f"iqn.2025-04.com.nsboot:{new_name.lower().replace('_', '')}"
        server_ip = "192.168.1.209"  # Adjust to your server's IP
        dhcp_entry = f"""host {formatted_name} {{
    hardware ethernet {new_mac};
    fixed-address {new_ip};
    option host-name "{formatted_name}";
    if substring (option vendor-class-identifier, 15, 5) = "00000" {{
        filename "ipxe.kpxe";
    }}
    elsif substring (option vendor-class-identifier, 15, 5) = "00006" {{
        filename "ipxe32.efi";
    }}
    else {{
        filename "ipxe.efi";
    }}
    option root-path "iscsi:{server_ip}::::{iscsi_target}";
}}
"""

        # Read and update the main DHCP config
        dhcp_config_path = DHCP_CONFIG_PATH
        try:
            with open(dhcp_config_path, 'r') as f:
                dhcp_config = f.read()
        except Exception as e:
            return jsonify({"error": f"Failed to read DHCP config: {e}"}), 500

        # Backup the current DHCP config
        dhcp_backup_path = f"{dhcp_config_path}.bak"
        try:
            with open(dhcp_backup_path, 'w') as bf:
                bf.write(dhcp_config)
        except Exception as e:
            return jsonify({"error": f"Failed to create DHCP config backup: {e}"}), 500

        # Replace or update the host entry
        host_pattern = rf'host\s+{client_id}\s*{{[^}}]*}}'
        if re.search(host_pattern, dhcp_config, re.MULTILINE):
            dhcp_config = re.sub(host_pattern, dhcp_entry, dhcp_config, count=1, flags=re.MULTILINE)
        else:
            # If the host entry doesn't exist, append it
            dhcp_config = dhcp_config.rstrip() + '\n\n' + dhcp_entry

        # Write the updated DHCP config
        try:
            with open(dhcp_config_path, 'w') as f:
                f.write(dhcp_config)
        except Exception as e:
            return jsonify({"error": f"Failed to write DHCP config: {e}"}), 500

        # Update ZFS volume and iSCSI target if name changed
        if new_name != client_id:
            old_volume = f"{ZFS_POOL}/{client_id}-disk"
            new_volume = f"{ZFS_POOL}/{new_name}-disk"
            old_target_iqn = f"iqn.2025-04.com.nsboot:{client_id.lower().replace('_', '')}"
            new_target_iqn = iscsi_target
            block_store_name = f"block_{new_name}"

            try:
                # Rename ZFS volume
                run_command(['zfs', 'rename', old_volume, new_volume], use_sudo=True)

                # Update iSCSI target
                run_command(['targetcli', f'iscsi/ delete {old_target_iqn}'], use_sudo=True, check=False)
                run_command(['targetcli', f'iscsi/ create {new_target_iqn}'], use_sudo=True)
                run_command(['targetcli', f'backstores/block create {block_store_name} /dev/zvol/{new_volume}'], use_sudo=True)
                run_command(['targetcli', f'iscsi/{new_target_iqn}/tpg1/luns create /backstores/block/{block_store_name}'], use_sudo=True)
                
                # Ensure portal exists
                result = run_command(['targetcli', f'iscsi/{new_target_iqn}/tpg1/portals/ ls'], use_sudo=True, check=False)
                if '0.0.0.0' not in result.stdout:
                    run_command(['targetcli', f'iscsi/{new_target_iqn}/tpg1/portals/ create 0.0.0.0 3260'], use_sudo=True)
            except Exception as e:
                # Rollback DHCP config
                with open(dhcp_backup_path, 'r') as bf:
                    with open(dhcp_config_path, 'w') as f:
                        f.write(bf.read())
                return jsonify({"error": f"Failed to update ZFS or iSCSI: {e}"}), 500

        # Restart DHCP service
        try:
            run_command(['systemctl', 'restart', 'isc-dhcp-server.service'], use_sudo=True)
        except Exception as e:
            # Rollback DHCP config
            with open(dhcp_backup_path, 'r') as bf:
                with open(dhcp_config_path, 'w') as f:
                    f.write(bf.read())
            return jsonify({"error": f"Failed to restart DHCP service: {e}"}), 500

        # Clean up backup
        if os.path.exists(dhcp_backup_path):
            os.remove(dhcp_backup_path)

        return jsonify({"message": f"Client {client_id} updated successfully to {new_name}"}), 200

    except Exception as e:
        return jsonify({"error": f"Failed to update client: {str(e)}"}), 500

# --- Snapshot Actions ---
@app.route('/api/snapshots', methods=['POST'])
def create_snapshot():
    # ... (keep existing implementation) ...
    data = request.get_json()
    if not data or 'name' not in data: abort(400, description="Missing field: name")
    snapshot_name = data['name']
    if '@' not in snapshot_name or not snapshot_name.startswith(ZFS_POOL + '/'): abort(400, description=f"Invalid snapshot name. Expected {ZFS_POOL}/master@snapname")
    master_name = snapshot_name.split('@')[0]
    try: run_command(['zfs', 'list', '-H', master_name], use_sudo=True)
    except subprocess.CalledProcessError: abort(404, description=f"Master '{master_name}' not found.")
    except Exception as e: abort(500, description=f"Error validating master: {e}")
    try: print(f"Creating snapshot: {snapshot_name}"); run_command(['zfs', 'snapshot', snapshot_name], use_sudo=True); return jsonify({"message": f"Snapshot {snapshot_name} created"}), 201
    except subprocess.CalledProcessError as e:
        if 'dataset already exists' in (e.stderr or ''): return jsonify({"error": f"Snapshot '{snapshot_name}' already exists."}), 409
        else: return jsonify({"error": f"Failed creating snapshot: {e.stderr or e}"}), 500
    except Exception as e: return jsonify({"error": f"Unexpected error: {e}"}), 500

@app.route('/api/snapshots/<path:snapshot_name_encoded>', methods=['DELETE'])
def delete_snapshot(snapshot_name_encoded):
     # ... (keep existing implementation) ...
     try: snapshot_name = snapshot_name_encoded
     except Exception as e: abort(400, description=f"Invalid snapshot name encoding: {e}")
     if '@' not in snapshot_name or not snapshot_name.startswith(ZFS_POOL + '/'): abort(400, description="Invalid snapshot name format.")
     print(f"Deleting snapshot: {snapshot_name}")
     try: run_command(['zfs', 'destroy', snapshot_name], use_sudo=True); return jsonify({"message": f"Snapshot {snapshot_name} deleted"}), 200
     except subprocess.CalledProcessError as e:
         if 'has dependent clones' in (e.stderr or ''): return jsonify({"error": f"Snapshot '{snapshot_name}' has dependent clones."}), 409
         else: return jsonify({"error": f"Failed deleting snapshot: {e.stderr or e}"}), 500
     except Exception as e: return jsonify({"error": f"Unexpected error: {e}"}), 500


# --- Client Control Actions ---
@app.route('/api/clients/<client_id>/control', methods=['POST'])
def control_client(client_id):
    # ... (keep existing implementation) ...
    data = request.get_json(); action = data.get('action')
    if not action: abort(400, description="Missing action")
    if not re.match(r'^[\w-]+$', client_id): abort(400, description="Invalid client ID")
    dhcp_info = get_client_dhcp_info(); mac_address = dhcp_info.get(client_id, {}).get("mac")
    print(f"Control action '{action}' for client: {client_id}")

    if action == 'wake':
        if not mac_address or mac_address == "N/A": return jsonify({"error": f"MAC address not found for '{client_id}'"}), 404
        try: 
            result = run_command(['wakeonlan', mac_address], use_sudo=False, check=False)
            if result.returncode != 0:
                return jsonify({"error": f"Wake-on-LAN failed: {result.stderr or result.stdout}"}), 500
            return jsonify({"message": f"Wake-on-LAN sent to {mac_address}"}), 200
        except FileNotFoundError:
            return jsonify({"error": "'wakeonlan' not found. Install it."}), 501
        except subprocess.CalledProcessError as e:
            return jsonify({"error": f"Wake-on-LAN failed: {e.stderr or e}"}), 500
        except Exception as e:
            return jsonify({"error": f"Wake-on-LAN failed: {str(e)}"}), 500

    elif action == 'reboot':
        if not mac_address or mac_address == "N/A": return jsonify({"error": f"MAC address not found for '{client_id}'"}), 404
        try:
            # Get client IP from DHCP info
            client_ip = dhcp_info.get(client_id, {}).get("ip")
            if not client_ip:
                return jsonify({"error": f"IP address not found for '{client_id}'"}), 404
            
            # Use samba net rpc to reboot Windows client
            # Requires Samba tools and proper Windows credentials
            # Format: net rpc shutdown -r -I <IP> -U <username%password> -f -t 0
            net_command = ['net', 'rpc', 'shutdown', '-r', '-I', client_ip, '-U', 'diskless%1', '-f', '-t', '0']
            result = run_command(net_command, use_sudo=False, check=False)
            if result.returncode != 0:
                return jsonify({"error": f"Failed to reboot client: {result.stderr or result.stdout}"}), 500
            return jsonify({"message": f"Reboot command sent to {client_id} ({client_ip})"}), 200
        except subprocess.CalledProcessError as e:
            return jsonify({"error": f"Failed to reboot client: {e.stderr or e}"}), 500
        except Exception as e:
            return jsonify({"error": f"Failed to reboot client: {str(e)}"}), 500

    elif action == 'shutdown':
        if not mac_address or mac_address == "N/A": return jsonify({"error": f"MAC address not found for '{client_id}'"}), 404
        try:
            # Get client IP from DHCP info
            client_ip = dhcp_info.get(client_id, {}).get("ip")
            if not client_ip:
                return jsonify({"error": f"IP address not found for '{client_id}'"}), 404
            
            # Use samba net rpc to shutdown Windows client
            # Requires Samba tools and proper Windows credentials
            # Format: net rpc shutdown -S -I <IP> -U <username%password> -f
            net_command = ['net', 'rpc', 'shutdown', '-S', client_ip, '-U', 'diskless%1']
            result = run_command(net_command, use_sudo=False, check=False)
            if result.returncode != 0:
                return jsonify({"error": f"Failed to shutdown client: {result.stderr or result.stdout}"}), 500
            return jsonify({"message": f"Shutdown command sent to {client_id} ({client_ip})"}), 200
        except subprocess.CalledProcessError as e:
            return jsonify({"error": f"Failed to shutdown client: {e.stderr or e}"}), 500
        except Exception as e:
            return jsonify({"error": f"Failed to shutdown client: {str(e)}"}), 500

    elif action == 'toggleSuper':
         is_super = data.get('makeSuper', False)
         clone_name = f"{ZFS_POOL}/{client_id}-disk"
         print(f"Toggle Super Client for {client_id} ({clone_name}) to {is_super}")
         
         # Check if clone exists
         try:
             result = run_command(['zfs', 'list', '-H', clone_name], check=False, use_sudo=True)
             if result.returncode != 0:
                 return jsonify({"error": f"ZFS clone {clone_name} not found"}), 404
         except Exception as e:
             return jsonify({"error": f"Failed to check ZFS clone: {e}"}), 500
         
         if is_super:
             try:
                 # First check if clone is already independent
                 result = run_command(['zfs', 'get', '-H', 'origin', clone_name], use_sudo=True)
                 origin = result.stdout.strip().split('\t')[2]
                 if origin == '-':
                     return jsonify({"message": f"Client {client_id} is already a Super Client"}), 200
                 
                 # Promote the clone
                 result = run_command(['zfs', 'promote', clone_name], use_sudo=True, check=False)
                 if result.returncode != 0:
                     return jsonify({"error": f"Failed enabling Super Client (promote failed): {result.stderr or result.stdout}"}), 500
                 return jsonify({"message": f"Super Client enabled for {client_id} (promoted)."}), 200
             except subprocess.CalledProcessError as e:
                 return jsonify({"error": f"Failed enabling Super Client (promote failed): {e.stderr or e}"}), 500
             except Exception as e:
                 return jsonify({"error": f"Failed enabling Super Client: {str(e)}"}), 500
         else:
             # For disabling Super Client, we need to find the original master and create a new clone
             try:
                 # Get the original master from the origin property
                 result = run_command(['zfs', 'get', '-H', 'origin', clone_name], use_sudo=True)
                 origin = result.stdout.strip().split('\t')[2]
                 
                 if origin == '-':
                     return jsonify({"error": f"Client {client_id} is not a Super Client"}), 400
                 
                 # Create a new clone from the original master
                 new_clone_name = f"{ZFS_POOL}/{client_id}-disk-temp"
                 result = run_command(['zfs', 'clone', origin, new_clone_name], use_sudo=True, check=False)
                 if result.returncode != 0:
                     return jsonify({"error": f"Failed disabling Super Client (clone failed): {result.stderr or result.stdout}"}), 500
                 
                 # Rename the temporary clone to the original name
                 result = run_command(['zfs', 'rename', new_clone_name, clone_name], use_sudo=True, check=False)
                 if result.returncode != 0:
                     return jsonify({"error": f"Failed disabling Super Client (rename failed): {result.stderr or result.stdout}"}), 500
                 
                 return jsonify({"message": f"Super Client disabled for {client_id} (reverted to clone)."}), 200
             except subprocess.CalledProcessError as e:
                 return jsonify({"error": f"Failed disabling Super Client: {e.stderr or e}"}), 500
             except Exception as e:
                 return jsonify({"error": f"Failed disabling Super Client: {str(e)}"}), 500
    elif action == 'edit': return jsonify({"message": f"Placeholder: Edit Client {client_id} not implemented."}), 501
    else: abort(400, description=f"Invalid action: {action}")


# --- Main Execution ---
if __name__ == '__main__':
    print("--- Starting Diskless Boot Manager Backend ---")
    print(f"Flask Debug Mode: {app.debug}")
    print("!!! SECURITY WARNING: Ensure proper sudoers configuration and run with a dedicated, non-root user !!!")
    print(f"!!! Using ZFS Pool: {ZFS_POOL} !!!")
    print(f"!!! Using DHCP Config Method: {DHCP_CONFIG_METHOD} ({DHCP_INCLUDE_DIR if DHCP_CONFIG_METHOD == 'include_files' else DHCP_CONFIG_PATH}) !!!")
    # Use debug=False for production
    app.run(host='0.0.0.0', port=5000, debug=False)
