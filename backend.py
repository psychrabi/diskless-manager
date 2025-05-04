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
DHCP_CONFIG_METHOD = "include_files" # or "main_config"
DHCP_CONFIG_PATH = "/etc/dhcp/dhcpd.conf"
DHCP_INCLUDE_DIR = "/etc/dhcp/clients.d" # Used if method is "include_files"

# TFTP Configuration Path (usually defaults file)
TFTP_CONFIG_PATH = "/etc/default/tftpd-hpa"

# iSCSI Target Configuration Path (LIO JSON config)
ISCSI_CONFIG_PATH = "/etc/target/saveconfig.json"

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
    """ Parses DHCP include files to get client IPs/MACs """
    clients = {}
    if DHCP_CONFIG_METHOD != "include_files":
        print("Warning: DHCP parsing only implemented for 'include_files' method.")
        return clients

    if not os.path.isdir(DHCP_INCLUDE_DIR):
         print(f"DHCP include directory not found: {DHCP_INCLUDE_DIR}")
         return clients

    try:
        for filename in os.listdir(DHCP_INCLUDE_DIR):
            if filename.endswith(".conf"):
                filepath = os.path.join(DHCP_INCLUDE_DIR, filename)
                try:
                    with open(filepath, 'r') as f:
                        content = f.read()
                        # Improved parsing to handle comments and varying whitespace
                        host_match = re.search(r'^\s*host\s+([\w-]+)\s*{', content, re.MULTILINE | re.IGNORECASE)
                        mac_match = re.search(r'^\s*hardware\s+ethernet\s+([\w:]+)\s*;', content, re.MULTILINE | re.IGNORECASE)
                        ip_match = re.search(r'^\s*fixed-address\s+([\d.]+)\s*;', content, re.MULTILINE | re.IGNORECASE)

                        if host_match and mac_match and ip_match:
                            hostname = host_match.group(1)
                            mac = mac_match.group(1).upper()
                            mac = re.sub(r'[-:]', ':', mac) # Normalize MAC
                            ip = ip_match.group(1)
                            clients[hostname] = {"mac": mac, "ip": ip}
                        else:
                            print(f"Warning: Could not parse host/mac/ip from {filepath}")
                except Exception as e:
                     print(f"Error reading or parsing DHCP file {filepath}: {e}")
    except Exception as e:
        print(f"Error listing DHCP include directory {DHCP_INCLUDE_DIR}: {e}")

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
        print(f"ZFS Pool: {ZFS_POOL}")
        print("Raw ZFS list output:")
        result = run_command(
            ['zfs', 'list', '-H', '-t', 'filesystem,volume', '-o', 'name,creation,used', '-r', ZFS_POOL],
            use_sudo=True
        )
        print("=== Raw ZFS List Output ===")
        print(result.stdout)
        print("=== End Raw Output ===")

        all_datasets = parse_zfs_list(result.stdout)
        print(f"Parsed {len(all_datasets)} datasets:")
        for ds in all_datasets:
            print(f"Dataset: {ds['name']}")

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
                        print(f"Checking snapshot: {snap}")
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
        print(f"Looking for snapshot: {ZFS_POOL}/{master}@base")
        base_snapshot = f"{ZFS_POOL}/{master}@base"
        result = run_command(['zfs', 'list', '-H', '-t', 'snapshot', base_snapshot], use_sudo=True, check=False)
        
        if result.returncode == 0:
            # Base snapshot exists, create clone as before            
            snapshot_name = f"{ZFS_POOL}/{master}@{name}_base"
            run_command(['zfs', 'snapshot', snapshot_name], use_sudo=True)
            
            # Create ZFS clone from the snapshot
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
            
            print(f"=== Setting up block store ===")
            print(f"Block name: block_{name}")
            print(f"Target path: {target_path}")
            
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
        if DHCP_CONFIG_METHOD == "include_files":
            existing_ips = get_client_dhcp_info().values()
            ip_prefix = "192.168.1." # !!! Adjust network !!!
            start_ip, end_ip = 100, 200
            used_ips_suffix = {int(info['ip'].split('.')[-1]) for info in existing_ips if info['ip'].startswith(ip_prefix)}
            for i in range(start_ip, end_ip + 1):
                 if i not in used_ips_suffix: assigned_ip = f"{ip_prefix}{i}"; break
            if not assigned_ip: raise Exception("No available IP address in range.")

            # Format name as PCXXX
            formatted_name = f"PC{int(name.split('_')[1]):03d}" if '_' in name else name.upper()
            
            # Get iSCSI target name and details
            iscsi_target = f"iqn.2025-04.com.nsboot:{name}"
            # Assuming server IP is 192.168.1.206 (from the logs)
            server_ip = "192.168.1.206"           
            
            dhcp_entry = f"host {formatted_name} {{\n    hardware ethernet {mac};\n    fixed-address {assigned_ip};\n    option host-name \"{formatted_name}\";\n    if substring (option vendor-class-identifier, 15, 5) = \"00000\" {{\n        filename \"ipxe.kpxe\";\n    }}\n    elsif substring (option vendor-class-identifier, 15, 5) = \"00006\" {{\n        filename \"ipxe32.efi\";\n    }}\n    else {{\n        filename \"ipxe.efi\";\n    }}\n    option root-path \"iscsi:{server_ip}::::{iscsi_target}\";\n}}"
            try:
                dhcp_client_conf_path = os.path.join(DHCP_INCLUDE_DIR, f"{name}.conf")
                with open(dhcp_client_conf_path, 'w') as f:
                    f.write(dhcp_entry)
                print(f"Wrote DHCP config: {dhcp_client_conf_path}")
                created_dhcp_config = True
            except Exception as e: 
                raise Exception(f"Failed writing DHCP config: {e}")
            try: 
                run_command(['systemctl', 'restart', 'isc-dhcp-server.service'], use_sudo=True)
            except Exception as e: 
                raise Exception(f"Failed restarting DHCP: {e}")
            
            # Verify DHCP configuration
            try:
                with open(dhcp_client_conf_path, 'r') as f:
                    content = f.read()
                if assigned_ip not in content or mac not in content:
                    raise Exception("DHCP configuration format validation failed")
            except Exception as e:
                raise Exception(f"DHCP configuration validation failed: {e}")
        else: raise NotImplementedError("DHCP main_config method not implemented.")

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

        print(f"Deleting iSCSI target for {client_id} (Placeholder)...") # Placeholder
        # Add iSCSI deletion logic here

        print(f"Destroying ZFS clone: {clone_name}")
        try:
            check_result = run_command(['zfs', 'list', '-H', clone_name], check=False, use_sudo=True)
            if check_result.returncode == 0: run_command(['zfs', 'destroy', clone_name], use_sudo=True); print(f"Destroyed {clone_name}")
            else: print(f"ZFS clone {clone_name} not found.")
        except subprocess.CalledProcessError as e: return jsonify({"error": f"Failed destroying ZFS clone: {e.stderr or e}", "details": errors}), 500
        except Exception as e: return jsonify({"error": f"Error destroying ZFS clone: {e}", "details": errors}), 500

        if errors: return jsonify({"message": f"Client {client_id} deleted with issues.", "errors": errors}), 207
        else: return jsonify({"message": f"Client {client_id} deleted successfully"}), 200
    except Exception as e: return jsonify({"error": f"Unexpected error during deletion: {e}"}), 500


# --- Client Management ---
@app.route('/api/clients/<client_id>/edit', methods=['POST'])
def edit_client(client_id):
    """
    Edit client details (name, MAC address, IP address)
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

        # Update MAC address if provided
        if 'mac' in data:
            new_mac = data['mac'].strip()
            if not re.match(r'^([0-9A-Fa-f]{2}[:-]){5}([0-9A-Fa-f]{2})$', new_mac):
                return jsonify({"error": "Invalid MAC address format"}), 400
            
            # Update DHCP configuration
            dhcp_config_path = f"/etc/dhcp/dhcpd.conf"
            with open(dhcp_config_path, 'r') as f:
                dhcp_config = f.read()
            
            # Replace MAC address in configuration
            old_mac = dhcp_info[client_id]['mac']
            dhcp_config = dhcp_config.replace(old_mac, new_mac)
            
            with open(dhcp_config_path, 'w') as f:
                f.write(dhcp_config)
            
            # Restart DHCP service
            run_command(['systemctl', 'restart', 'isc-dhcp-server'], use_sudo=True)

        # Update IP address if provided
        if 'ip' in data:
            new_ip = data['ip'].strip()
            if not re.match(r'^([\d]{1,3}\.){3}\d{1,3}$', new_ip):
                return jsonify({"error": "Invalid IP address format"}), 400
            
            # Update DHCP configuration
            dhcp_config_path = f"/etc/dhcp/dhcpd.conf"
            with open(dhcp_config_path, 'r') as f:
                dhcp_config = f.read()
            
            # Replace IP address in configuration
            old_ip = dhcp_info[client_id]['ip']
            dhcp_config = dhcp_config.replace(old_ip, new_ip)
            
            with open(dhcp_config_path, 'w') as f:
                f.write(dhcp_config)
            
            # Restart DHCP service
            run_command(['systemctl', 'restart', 'isc-dhcp-server'], use_sudo=True)

        # Update client name if provided
        if 'name' in data:
            new_name = data['name'].strip()
            if not re.match(r'^[\w-]+$', new_name):
                return jsonify({"error": "Invalid client name format"}), 400
            
            # Rename ZFS volume
            old_volume = f"{ZFS_POOL}/{client_id}-disk"
            new_volume = f"{ZFS_POOL}/{new_name}-disk"
            run_command(['zfs', 'rename', old_volume, new_volume], use_sudo=True)
            
            # Update DHCP configuration
            dhcp_config_path = f"/etc/dhcp/dhcpd.conf"
            with open(dhcp_config_path, 'r') as f:
                dhcp_config = f.read()
            
            dhcp_config = dhcp_config.replace(client_id, new_name)
            with open(dhcp_config_path, 'w') as f:
                f.write(dhcp_config)
            
            # Restart DHCP service
            run_command(['systemctl', 'restart', 'isc-dhcp-server'], use_sudo=True)

        return jsonify({"message": f"Client {client_id} updated successfully"}), 200

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
        try: run_command(['wakeonlan', mac_address], use_sudo=False); return jsonify({"message": f"Wake-on-LAN sent to {mac_address}"}), 200
        except FileNotFoundError: return jsonify({"error": "'wakeonlan' not found. Install it."}), 501
        except Exception as e: return jsonify({"error": f"Wake-on-LAN failed: {e}"}), 500
    elif action == 'reboot': return jsonify({"message": f"Placeholder: Reboot {client_id} not implemented."}), 501
    elif action == 'shutdown': return jsonify({"message": f"Placeholder: Shutdown {client_id} not implemented."}), 501
    elif action == 'toggleSuper':
         is_super = data.get('makeSuper', False); clone_name = f"{ZFS_POOL}/{client_id}-disk"
         print(f"Toggle Super Client for {client_id} ({clone_name}) to {is_super}")
         if is_super:
             try: run_command(['zfs', 'promote', clone_name], use_sudo=True); return jsonify({"message": f"Super Client enabled for {client_id} (promoted)."}), 200
             except Exception as e: return jsonify({"error": f"Failed enabling Super Client (promote failed): {e}"}), 500
         else: return jsonify({"message": f"Placeholder: Disabling Super Client {client_id} not implemented."}), 501
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
