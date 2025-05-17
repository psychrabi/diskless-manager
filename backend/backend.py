#!/usr/bin/env python3

import subprocess
import json
from flask import Flask, request, jsonify, abort, Response # Add Response for config view
from flask_cors import CORS # Import Flask-Cors
import shlex # Used for safer command splitting
import re
import os
import math # For size parsing
import time

# --- Configuration ---
# !! SECURITY WARNING !!: Running Flask as root is highly discouraged.
# Configure sudoers to allow the user running Flask to execute *specific*
# required commands without a password. Example sudoers entry:
# flaskuser ALL=(ALL) NOPASSWD: /usr/sbin/zfs, /usr/bin/targetcli, /bin/systemctl, /usr/sbin/dhcpd, /usr/bin/wakeonlan
# Adapt the paths and commands based on your system.
SUDO_CMD = "/usr/bin/sudo" # Adjust path if needed

# Base path for ZFS operations (e.g., your main pool)
ZFS_POOL = "nsboot0" # !!! IMPORTANT: Change this to your actual ZFS pool name !!!

# Client configuration file path
CONFIG_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "config.json")

# Ensure clients config directory exists
os.makedirs(os.path.dirname(CONFIG_PATH), exist_ok=True)

# DHCP Configuration - Choose ONE method:
DHCP_CONFIG_PATH = "/etc/dhcp/dhcpd.conf"

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
CORS(app, resources={r"/api/*": {"origins": "*"}}) # Allow all origins for testing


# --- Helper Functions ---

def check_duplicate_client(name, mac, ip):
    """Check if a client with the same name, IP, or MAC already exists."""
    try:
        if not os.path.exists(CONFIG_PATH):
            return None
            
        with open(CONFIG_PATH, 'r') as f:
            config = json.load(f)
        
        clients = config.get('clients', [])
        if not isinstance(clients, list):
            return None
            
        # Check for duplicates
        for client in clients:
            if name.lower() == client.get('name', '').lower():
                return f"A client with name '{name}' already exists"
            if ip == client.get('ip', ''):
                return f"IP address {ip} is already in use by client '{client.get('name')}'"
            if mac.upper() == client.get('mac', '').upper():
                return f"MAC address {mac} is already in use by client '{client.get('name')}'"
        return None
    except Exception as e:
        print(f"Error checking for duplicate clients: {e}")
        return "Error checking for existing clients"

def validate_client_inputs(name, mac, ip):
    """Validate client input parameters."""
    if not all([name, mac, ip]):
        raise ValueError("Missing required fields: name, mac, ip")
    
    if not re.match(r'^[\w-]+$', name):
        raise ValueError("Invalid client name format (use alphanumeric, _, -)")
    
    if not re.match(r'^([0-9A-Fa-f]{2}[:-]){5}([0-9A-Fa-f]{2})$', mac):
        raise ValueError("Invalid MAC address format")
    
    if not re.match(r'^([\d]{1,3}\.){3}\d{1,3}$', ip):
        raise ValueError("Invalid IP address format")

def format_client_name(name):
    """Format client name to PCXXX format."""
    if '_' in name:
        return f"PC{int(name.split('_')[1]):03d}"
    return name.upper()

def get_client_paths(client_id):
    """Get all relevant paths for a client."""
    client_id = client_id.lower()
    return {
        'clone': f"{ZFS_POOL}/{client_id}-disk",        
        'target_iqn': f"iqn.2025-04.com.nsboot:{client_id.replace('_', '')}",
        'block_store': f"block_{client_id}"
    }

def create_dhcp_entry(name, mac, ip, target_iqn):
    """Create DHCP host entry configuration."""
    formatted_name = format_client_name(name)
    return f"""host {formatted_name} {{
    hardware ethernet {mac};
    fixed-address {ip};
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
    option root-path "iscsi:{SERVER_IP}::::{target_iqn}";
}}"""

def update_dhcp_config(client_id, dhcp_entry, is_new=True):
    """Update DHCP configuration file."""
   
    # Read existing config
    with open(DHCP_CONFIG_PATH, 'r') as f:
        content = f.read()
    
    # Backup current config
    dhcp_backup_path = f"{DHCP_CONFIG_PATH}.bak"
    with open(dhcp_backup_path, 'w') as bf:
        bf.write(content)
    
    try:
        if not is_new:
            # Remove existing entry
            formatted_name = format_client_name(client_id)
            host_pattern = rf'host\s+{formatted_name}\s*\{{(?:[^{{}}]|(?:\{{[^{{}}]*\}}))*\}}\s*'
            content = re.sub(host_pattern, '', content, count=1, flags=re.DOTALL)
            content = re.sub(r'\n\s*\n{2,}', '\n\n', content)
        
        # Add new entry
        content = content.rstrip() + '\n\n' + dhcp_entry
        
        # Write updated config
        with open(DHCP_CONFIG_PATH, 'w') as f:
            f.write(content)
        
        return True
    except Exception as e:
        # Restore backup on error
        with open(dhcp_backup_path, 'r') as bf:
            with open(DHCP_CONFIG_PATH, 'w') as f:
                f.write(bf.read())
        raise e
    finally:
        # Clean up backup
        if os.path.exists(dhcp_backup_path):
            os.remove(dhcp_backup_path)

def setup_iscsi_target(target_iqn, block_store, volume_path):
    """Set up iSCSI target with block store and LUN."""
    # Create target if it doesn't exist
    result = run_command(['targetcli', 'iscsi/ ls'], use_sudo=True, check=False)
    if target_iqn not in result.stdout:
        run_command(['targetcli', 'iscsi/ create', target_iqn], use_sudo=True)
        
        # Set TPG1 attributes
        run_command(['targetcli', f'iscsi/{target_iqn}/tpg1 set attribute generate_node_acls=1'], use_sudo=True)
        run_command(['targetcli', f'iscsi/{target_iqn}/tpg1 set attribute cache_dynamic_acls=1'], use_sudo=True)
        run_command(['targetcli', f'iscsi/{target_iqn}/tpg1 set attribute demo_mode_write_protect=0'], use_sudo=True)
        run_command(['targetcli', f'iscsi/{target_iqn}/tpg1 set attribute authentication=0'], use_sudo=True)
    
    # Create or update block store
    result = run_command(['targetcli', 'backstores/block/ ls'], use_sudo=True, check=False)
    if block_store in result.stdout:
        run_command(['targetcli', 'backstores/block/ delete', block_store], use_sudo=True)
    run_command(['targetcli', 'backstores/block create', block_store, volume_path], use_sudo=True)
    
    # Create LUN if it doesn't exist
    result = run_command(['targetcli', f'iscsi/{target_iqn}/tpg1/luns ls'], use_sudo=True, check=False)
    if block_store not in result.stdout:
        run_command(['targetcli', f'iscsi/{target_iqn}/tpg1/luns create', f'/backstores/block/{block_store}'], use_sudo=True)
    
    # Ensure portal exists
    result = run_command(['targetcli', f'iscsi/{target_iqn}/tpg1/portals/ ls'], use_sudo=True, check=False)
    if '0.0.0.0' not in result.stdout:
        run_command(['targetcli', f'iscsi/{target_iqn}/tpg1/portals/ create 0.0.0.0 3260'], use_sudo=True)
    run_command(['targetcli', 'saveconfig'], use_sudo=True)


def cleanup_iscsi_target(target_iqn, block_store):
    """Clean up iSCSI target and associated resources."""
    try:
        print(f"Cleaning up iSCSI target {target_iqn} and block store {block_store}")
        
        # First try to delete the target using the full path
        try:
            print(f"Deleting iSCSI target {target_iqn}")
            run_command(['targetcli', f'iscsi/ delete {target_iqn}'], use_sudo=True, check=True)
        except subprocess.CalledProcessError as e:
            print(f"Warning: Could not delete target {target_iqn} directly: {e}")
        
        # Delete block store if it exists
        if block_store:
            try:
                result = run_command(['targetcli', 'backstores/block ls'], use_sudo=True, check=False)
                if block_store in result.stdout:
                    print(f"Deleting block store {block_store}")
                    run_command(['targetcli', f'backstores/block/ delete {block_store}'], use_sudo=True)
            except Exception as e:
                print(f"Warning: Could not delete block store {block_store}: {e}")
        
        # Save the configuration
        run_command(['targetcli', 'saveconfig'], use_sudo=True, check=False)
        return True
        
    except Exception as e:
        print(f"Warning: Failed to clean up iSCSI target {target_iqn}: {e}")
        return False


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
    
    
    
def get_server_ip():
    """Detects the server's IP address."""
    try:
        # Get the IP address of the network interface
        result = run_command(['ip', 'route', 'get', '1'], check=False)
        if result.returncode != 0:
            raise Exception(f"Failed to get server IP: {result.stderr or result.stdout}")
            
        # Extract the IP address from the output
        for line in result.stdout.split('\n'):
            if 'src' in line:
                ip = line.split('src')[1].strip().split()[0]
                if ip.startswith('192.168.') or ip.startswith('10.'):
                    return ip
    
        raise Exception("Could not find valid server IP address")
    
    except Exception as e:
        print(f"Warning: Failed to detect server IP: {e}")
        # Fallback to hardcoded IP if detection fails
        return "192.168.1.200"  

# Get server IP at startup
SERVER_IP = get_server_ip()
print(f"Using server IP: {SERVER_IP}")

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

def launch_remote_desktop(client_ip, username="diskless"):
    """Launch remote desktop connection to client with improved settings."""
    try:
        # Enhanced FreeRDP command with better resolution and control options
        rdp_command = [
                    'xfreerdp',
                    '/v:' + client_ip,
                    '/u:' + username,
                    '/p:1',
                    '/cert-ignore',
                    '/w:1920',                    # Width
                    '/h:1080',                    # Height
                    '/dynamic-resolution',
                    '+clipboard',
                    '/gdi:sw',                    # Hardware GDI rendering
                    '/network:auto',              # Auto-detect network
                    '/bpp:32',                    # 32-bit color depth
                    '/sec:nla',           # Try NLA security
                    '/timeout:20000'
                ]
        
        print(f"Launching enhanced RDP connection to {client_ip}")
        
        # Launch FreeRDP
        process = subprocess.Popen(
            rdp_command,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            start_new_session=True
        )
        
        # Wait briefly to check for immediate failures
        try:
            process.wait(timeout=5)
            if process.returncode != 0:
                stdout = process.stdout.read().decode()
                stderr = process.stderr.read().decode()
                
                # Try fallback options if first attempt fails
                fallback_command = [
                    'xfreerdp',
                    '/v:' + client_ip,
                    '/u:' + username,
                    '/p:1',
                    '/cert-ignore',
                    '/w:1366',            # Lower resolution
                    '/h:768',
                    '/dynamic-resolution',
                    '+clipboard',
                    '/gdi:sw',            # Software GDI
                    '/network:auto',
                    '/bpp:24',            # 24-bit color
                    '/sec:nla',           # Try NLA security
                    '/timeout:20000'
                ]
                
                print("First attempt failed, trying fallback options...")
                process = subprocess.Popen(
                    fallback_command,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    start_new_session=True
                )
                
                process.wait(timeout=5)
                if process.returncode != 0:
                    stdout2 = process.stdout.read().decode()
                    stderr2 = process.stderr.read().decode()
                    raise Exception(f"Both RDP attempts failed.\nFirst attempt: {stderr}\nFallback attempt: {stderr2}")
                    
        except subprocess.TimeoutExpired:
            # Process didn't exit immediately - good
            print(f"RDP connection initiated successfully to {client_ip}")
            return True
            
        return True
    except Exception as e:
        print(f"Error launching remote desktop: {e}")
        raise
   
    
def get_client_info():
    """Parses DHCP host entries from the main dhcpd.conf file."""
    clients = {}
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

def save_client_config(client_data):
    """Save client configuration to JSON file (clients as a list)."""
    try:
        # Read existing config if it exists
        if os.path.exists(CONFIG_PATH):
            with open(CONFIG_PATH, 'r') as f:
                config = json.load(f)
        else:
            config = {
                'clients': [],
                'masters': {},              
            }
        
        # Ensure clients is a list
        if not isinstance(config.get('clients'), list):
            config['clients'] = []
        
        # Update or add client data
        updated = False
        for idx, c in enumerate(config['clients']):
            if c.get('id') == client_data['id']:
                config['clients'][idx] = client_data
                updated = True
                break
        if not updated:
            config['clients'].append(client_data)
        
        # Write updated config
        with open(CONFIG_PATH, 'w') as f:
            json.dump(config, f, indent=2)
        
        return True
    except Exception as e:
        print(f"Error saving client config: {e}")
        return False

def delete_client_config(client_id):
    """Delete client configuration from JSON file (list version)."""
    print(f"Deleting client config: {client_id}")
    try:
        if not os.path.exists(CONFIG_PATH):
            return True
        
        with open(CONFIG_PATH, 'r') as f:
            config = json.load(f)
        
        clients = config.get('clients', [])
        if not isinstance(clients, list):
            return True
        
        # Remove client by id (case-insensitive)
        new_clients = [c for c in clients if c.get('id', '').lower() != client_id.lower()]
        config['clients'] = new_clients
        
        with open(CONFIG_PATH, 'w') as f:
            json.dump(config, f, indent=2)
        
        return True
    except Exception as e:
        print(f"Error deleting client config: {e}")
        return False

def save_master_config(master_data):
    """Save master image configuration to JSON file."""
    try:
        # Read existing config if it exists
        if os.path.exists(CONFIG_PATH):
            with open(CONFIG_PATH, 'r') as f:
                config = json.load(f)
        else:
            config = {
                'clients': {},
                'masters': {},              
            }
        
        # Update or add master data
        config['masters'][master_data['name']] = {
            'name': master_data['name'],
            'size': master_data.get('size', ''),
            'snapshots': master_data.get('snapshots', []),
            'created_at': master_data.get('created_at'),
            'last_modified': master_data.get('last_modified', '')
        }
        
        # Write updated config
        with open(CONFIG_PATH, 'w') as f:
            json.dump(config, f, indent=2)
        
        return True
    except Exception as e:
        print(f"Error saving master config: {e}")
        return False

def delete_master_config(master_name):
    """Delete master image configuration from JSON file."""
    try:
        if not os.path.exists(CONFIG_PATH):
            return True
            
        with open(CONFIG_PATH, 'r') as f:
            config = json.load(f)
        
        if 'masters' in config and master_name in config['masters']:
            del config['masters'][master_name]
            
            with open(CONFIG_PATH, 'w') as f:
                json.dump(config, f, indent=2)
        
        return True
    except Exception as e:
        print(f"Error deleting master config: {e}")
        return False


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

@app.route('/api/system/ram', methods=['GET'])
def get_ram_usage():
    """Get current RAM usage statistics."""
    try:
        # Get memory usage using free command
        result = run_command(['free', '-h'], use_sudo=True)
        lines = result.stdout.splitlines()
        
        # Parse the output
        mem_line = lines[1].split()
        swap_line = lines[2].split()
        
        return jsonify({
            'memory': {
                'total': mem_line[1],
                'used': mem_line[2],
                'free': mem_line[3],
                'shared': mem_line[4],
                'buff/cache': mem_line[5],
                'available': mem_line[6]
            },
            'swap': {
                'total': swap_line[1],
                'used': swap_line[2],
                'free': swap_line[3]
            }
        }), 200
    except Exception as e:
        return jsonify({"error": f"Failed to get RAM usage: {str(e)}"}), 500

@app.route('/api/system/ram/clear', methods=['POST'])
def clear_ram_cache():
    """Clear RAM cache (sync and drop caches)."""
    try:
        # First sync to ensure all data is written to disk
        run_command(['sync'], use_sudo=True)
        
        # Drop caches (1=pagecache, 2=inodes/dentries, 3=all)
        run_command(['echo', '3', '>', '/proc/sys/vm/drop_caches'], use_sudo=True)
        
        return jsonify({"message": "RAM cache cleared successfully"}), 200
    except Exception as e:
        return jsonify({"error": f"Failed to clear RAM cache: {str(e)}"}), 500
    
    

@app.route('/api/services/<service_key>/config', methods=['GET'])
def get_service_config(service_key):
    """ Retrieves the content of a service's configuration file. """
    if service_key == 'zfs':
        try:
            # Get ZFS pool information
            result = run_command(['zpool', 'status'], use_sudo=True)
            zpool_status = result.stdout
            
            # Get ZFS dataset information
            result = run_command(['zfs', 'list', '-H', '-t', 'all', '-o', 'name,type,used,avail,refer,mountpoint'], use_sudo=True)
            zfs_list = result.stdout
            
            content = f"=== ZFS Pool Status ===\n{zpool_status}\n\n=== ZFS Datasets ===\n{zfs_list}"
            return Response(content, mimetype='text/plain')
        except Exception as e:
            print(f"Error getting ZFS information: {e}")
            abort(500, description=f"Error getting ZFS information: {str(e)}")
    else:
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

@app.route('/api/masters/default', methods=['POST'])
def set_default_master():
    """Set the default master image."""
    data = request.get_json()
    if not data or 'name' not in data:
        abort(400, description="Missing required field: name")
    
    master_name = data['name']
    
    try:
        # Load existing config
        config = {}
        if os.path.exists(CONFIG_PATH):
            with open(CONFIG_PATH, 'r') as f:
                config = json.load(f)
        
        # Update default master
        if 'settings' not in config:
            config['settings'] = {}
        config['settings']['default_master'] = master_name
        
        # Save config
        with open(CONFIG_PATH, 'w') as f:
            json.dump(config, f, indent=2)
            
        return jsonify({"message": f"Default master set to {master_name}"})
        
    except Exception as e:
        print(f"Error setting default master: {e}")
        abort(500, description=f"Failed to set default master: {e}")


@app.route('/api/masters', methods=['GET'])
def get_masters():
    masters_data = []
    
    # Get default master from config
    default_master = None
    try:
        if os.path.exists(CONFIG_PATH):
            with open(CONFIG_PATH, 'r') as f:
                config = json.load(f)
                default_master = config.get('settings', {}).get('default_master')
    except Exception as e:
        print(f"Error reading default master: {e}")
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
                 "is_default": master_name == default_master,
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
def get_clients(client_id=None):
    """Retrieve client configuration from JSON file (list version), including status."""
    try:
        if not os.path.exists(CONFIG_PATH):
            return None
        
        with open(CONFIG_PATH, 'r') as f:
            config = json.load(f)
        
        clients = config.get('clients', [])
        if not isinstance(clients, list):
            return None
        
        # Add status to each client
        for client in clients:
            client['status'] = get_client_status(client.get('ip', ''))
            print(f"Client {client['id']} status: {client['status']}")
        
        if client_id is None:
            return clients
        # Find client by id (case-insensitive)
        for c in clients:
            if c.get('id', '').lower() == client_id.lower():
                return c
        return None
    except Exception as e:
        print(f"Error retrieving client config: {e}")
        return None

@app.route('/api/clients', methods=['POST'])
def add_client():
    """Add a new client with ZFS clone, iSCSI target, and DHCP configuration."""
    data = request.get_json()
    if not data:
        abort(400, description="No data provided")
    
    try:
        # Extract and validate client details
        name = data.get('name', '').strip().lower()
        mac = data.get('mac', '').strip().upper()
        ip = data.get('ip', '').strip()
        master = data.get('master', '').strip()
        snapshot = data.get('snapshot', '').strip() if data.get('snapshot') is not None else ''
        
        
        # Validate inputs
        validate_client_inputs(name, mac, ip)
        if not master:
            raise ValueError("Master image is required")
            
        # Check for duplicate name, IP, or MAC
        duplicate_error = check_duplicate_client(name, mac, ip)
        if duplicate_error:
            raise ValueError(duplicate_error)
    
        # Get client paths
        paths = get_client_paths(name)
        
        # Create ZFS clone
        if snapshot:
            # Use provided snapshot
            clone_name = paths['clone']
            run_command(['zfs', 'clone', snapshot, clone_name], use_sudo=True)
        else:
            # Check if base snapshot exists
            base_snapshot = f"{ZFS_POOL}/{master}@base"
            result = run_command(['zfs', 'list', '-H', '-t', 'snapshot', base_snapshot], use_sudo=True, check=False)
            
            if result.returncode == 0:
                # Create new snapshot for this client
                snapshot_name = f"{ZFS_POOL}/{master}@{name}_base"
                run_command(['zfs', 'snapshot', snapshot_name], use_sudo=True)
                run_command(['zfs', 'clone', snapshot_name, paths['clone']], use_sudo=True)
            else:
                # Use master volume directly
                paths['clone'] = master
        
        # Set up iSCSI target
        setup_iscsi_target(paths['target_iqn'], paths['block_store'], f"/dev/zvol/{paths['clone']}")
        
        # Create DHCP entry
        dhcp_entry = create_dhcp_entry(name, mac, ip, paths['target_iqn'])
            
        # Update DHCP configuration
        update_dhcp_config(name, dhcp_entry, is_new=True)
            
        # Save client configuration to JSON file
        client_data = {
            'id': name,
            'name': name.upper(),
            'mac': mac,
            'ip': ip,
            'master': master,
            'snapshot': snapshot,
            'block_store': paths['block_store'],
            'target_iqn': paths['target_iqn'],
            'writeback': paths['clone'],
            'created_at': time.strftime('%Y-%m-%d %H:%M:%S'),
            'last_modified': time.strftime('%Y-%m-%d %H:%M:%S')
        }
        if not save_client_config(client_data):
            print(f"Warning: Failed to save client configuration for {name}")
            
            # Restart DHCP service
        run_command(['systemctl', 'restart', 'isc-dhcp-server.service'], use_sudo=True)
            
        return jsonify({"message": f"Client {name} added successfully", "assigned_ip": ip}), 201
        
    except ValueError as e:
        return jsonify({"error": str(e)}), 400
    except Exception as e:
        return jsonify({"error": f"An unexpected error occurred: {e}"}), 500


@app.route('/api/clients/edit/<client_id>', methods=['POST'])
def edit_client(client_id):
    
    """Edit client details and update associated resources."""
    print(f"\n=== Starting edit_client for {client_id} ===")
    
    if not re.match(r'^[\w-]+$', client_id):
        print(f"Error: Invalid client ID format: {client_id}")
        abort(400, description="Invalid client ID")
    
    data = request.get_json()
    if not data:
        print("Error: No data provided in request")
        abort(400, description="No update data provided")

    try:
        # Get current client info
        print("Getting current client info...")
        client_info = get_clients(client_id)
        if client_info is None:
            print(f"Error: Client {client_id} not found in config")
            return jsonify({"error": f"Client {client_id} not found"}), 404

        # Get current paths
        print("Getting current paths...")
        current_paths = get_client_paths(client_id)
        print(f"Current paths: {current_paths}")

        # Get new client details
        print("Extracting new client details...")
        new_name = data.get('name', client_info.get('name', client_id)).strip()
        new_mac = data.get('mac', client_info.get('mac', '')).strip().upper()
        new_ip = data.get('ip', client_info.get('ip', '')).strip()
        new_master = data.get('master', client_info.get('master', '')).strip()
        new_snapshot = data.get('snapshot', client_info.get('snapshot', '')).strip() if data.get('snapshot') else ''
        
        print(f"New details:")
        print(f"  Name: {new_name}")
        print(f"  MAC: {new_mac}")
        print(f"  IP: {new_ip}")
        print(f"  Master: {new_master}")
        print(f"  Snapshot: {new_snapshot}")

        # Validate inputs
        print("Validating inputs...")
        validate_client_inputs(new_name, new_mac, new_ip)
        
        # Check what has changed
        name_changed = new_name != client_info.get('name', client_id)
        mac_changed = new_mac != client_info.get('mac')
        ip_changed = new_ip != client_info.get('ip')
        master_changed = new_master != client_info.get('master')
        snapshot_changed = new_snapshot != client_info.get('snapshot')
        
        print("\n=== Changes Detected ===")
        changes = []
        if name_changed: changes.append(f"Name: {client_info.get('name', client_id)} -> {new_name}")
        if mac_changed: changes.append(f"MAC: {client_info.get('mac')} -> {new_mac}")
        if ip_changed: changes.append(f"IP: {client_info.get('ip')} -> {new_ip}")
        if master_changed: changes.append(f"Master: {client_info.get('master')} -> {new_master}")
        if snapshot_changed: changes.append(f"Snapshot: {client_info.get('snapshot')} -> {new_snapshot}")
        
        if not changes:
            print("No changes detected in the submitted data.")
        else:
            print("Changes to be applied:")
            for change in changes:
                print(f"  - {change}")
        print("======================\n")
        
        # Case 1: Only MAC or IP changed
        if (mac_changed or ip_changed) and not (name_changed or master_changed or snapshot_changed):
            print("Only MAC or IP changed, updating config and DHCP...")
            
            # Update client info in config
            client_info.update({
                'mac': new_mac,
                'ip': new_ip,
                'last_modified': time.strftime("%Y-%m-%d %H:%M:%S")
            })
            
            # Update DHCP config
            dhcp_entry = create_dhcp_entry(
                client_info.get('name', client_id),
                new_mac,
                new_ip,
                client_info.get('target_iqn')
            )
            update_dhcp_config(client_id, dhcp_entry, is_new=False)
            
            # Save the updated config
            save_client_config(client_info)
            return jsonify({"message": f"Successfully updated client {client_id}"})
        
        # Case 2: Name, master, or snapshot changed
        if name_changed or master_changed or snapshot_changed:
            print("Name, master, or snapshot changed, updating resources...")
            
            # Generate new target IQN and block store names
            new_target_iqn = f"iqn.2025-04.com.nsboot:{new_name.lower()}"
            new_block_store = f"block_{new_name.lower()}"
            
            # Get the current master and snapshot if not changed
            current_master = new_master if master_changed else client_info.get('master')
            current_snapshot = new_snapshot if snapshot_changed else client_info.get('snapshot')
            
            # Determine block device based on master and snapshot
            block_device = None
            
            if current_master:
                if current_snapshot:
                    # Create new clone from snapshot
                    try:
                        # Clean up old clone if it exists
                        old_clone = current_paths.get('clone')
                        # Check if old clone exists
                        result = run_command(['zfs', 'list', '-H', old_clone], use_sudo=True, check=False)
                        if result.returncode == 0:
                            print(f"Old ZFS clone exists: {old_clone}")
                            # Destroy old clone
                            print(f"Destroying old ZFS clone: {old_clone}")
                            run_command(['zfs', 'destroy', old_clone], use_sudo=True)
                        
                        # Create new clone
                        new_clone = f"{ZFS_POOL}/{new_name}-disk"
                        
                        print(f"Creating new ZFS clone from {current_snapshot} to {new_clone}")
                        run_command(['zfs', 'clone', current_snapshot, new_clone], use_sudo=True)
                        block_device = f"/dev/zvol/{new_clone}"
                    except Exception as e:
                        print(f"Error creating ZFS clone: {e}")
                        return jsonify({"error": f"Failed to create ZFS clone: {str(e)}"}), 500
                else:
                    # Use master directly
                    print(f"Using master directly: {current_master}")
                    block_device = f"/dev/zvol/{current_master}"
            
            # Update client info with new values
            client_info.update({
                'id': new_name.lower(),
                'name': new_name,
                'mac': new_mac,
                'ip': new_ip,
                'master': current_master,
                'snapshot': current_snapshot,
                'target_iqn': new_target_iqn,
                'block_store': new_block_store,
                'block_device': block_device,
                'last_modified': time.strftime("%Y-%m-%d %H:%M:%S")
            })
            
            # Update DHCP config
            dhcp_entry = create_dhcp_entry(
                new_name,
                new_mac,
                new_ip,
                new_target_iqn
            )
            update_dhcp_config(client_id, dhcp_entry, is_new=False)
            
            # Update iSCSI target
            try:
                # Clean up old target and block store if they exist
                old_target_iqn = current_paths.get('target_iqn')
                old_block_store = current_paths.get('block_store')
                
                if old_target_iqn or old_block_store:
                    print(f"Cleaning up old iSCSI target and block store: {old_target_iqn}, {old_block_store}")
                    try:
                        cleanup_iscsi_target(old_target_iqn, old_block_store)
                    except Exception as cleanup_error:
                        print(f"Warning: Error during cleanup of old target/block store: {cleanup_error}")
                
                # Now create the new target and block store
                print(f"Creating new iSCSI target: {new_target_iqn} with block store: {new_block_store}")
                setup_iscsi_target(new_target_iqn, new_block_store, block_device)
                    
            except Exception as e:
                print(f"Error updating iSCSI target: {e}")
                return jsonify({"error": f"Failed to update iSCSI target: {str(e)}"}), 500
            
            # Save the updated config
            save_client_config(client_info)
            
            # If name changed, update the client ID in the config
            if name_changed:
                delete_client_config(client_id)
                save_client_config(client_info)
            
            return jsonify({"message": f"Successfully updated client {client_id} and associated resources"})
        
        # If we get here, no changes were made
        return jsonify({"message": "No changes detected or no action required"})

    except Exception as e:
        print(f"Unexpected error: {e}")
        return jsonify({"error": f"Failed to update client: {str(e)}"}), 500

@app.route('/api/clients/reset/<client_id>', methods=['POST'])
def reset_client(client_id):
    """Reset a client by deleting its ZFS clone and iSCSI target and create it again."""
    if not re.match(r'^[\w-]+$', client_id):
        abort(400, description="Invalid client ID")

    print("Getting current client info...")
    client_info = get_clients(client_id)
    if client_info is None:
        print(f"Error: Client {client_id} not found in config")
        return jsonify({"error": f"Client {client_id} not found"}), 404

    current_paths = get_client_paths(client_id)
    # Cleanup iSCSI Target and delete block store
    try:
        # Clean up old target and block store if they exist
        target_iqn = current_paths.get('target_iqn')
        block_store = current_paths.get('block_store')
        
        if target_iqn or block_store:
            print(f"Cleaning up old iSCSI target and block store: {target_iqn}, {block_store}")
            try:
                cleanup_iscsi_target(target_iqn, block_store)
            except Exception as cleanup_error:
                print(f"Warning: Error during cleanup of old target/block store: {cleanup_error}")
        
        # Now create the new target and block store
        print(f"Creating new iSCSI target: {target_iqn} with block store: {block_store}")
             
    
        # Clone name
        clone = f"{ZFS_POOL}/{client_id.upper()}-disk"
        
        # Clean up old clone if it exists
        # Check if old clone exists
        result = run_command(['zfs', 'list', '-H', clone], use_sudo=True, check=False)
        if result.returncode == 0:
            print(f"Old ZFS clone exists: {clone}")
            # Destroy old clone
            print(f"Destroying old ZFS clone: {clone}")
            run_command(['zfs', 'destroy', clone], use_sudo=True)
                                
        print(f"Creating new ZFS clone from {client_info['snapshot']} to {clone}")
        run_command(['zfs', 'clone', client_info['snapshot'], clone], use_sudo=True)
        block_device = f"/dev/zvol/{clone}"
    
        # Setup updated iSCSI Target
        setup_iscsi_target(target_iqn, block_store, block_device)

    except Exception as e:
        print(f"Unexpected error: {e}")
        return jsonify({"error": f"Failed to reset client: {str(e)}"}), 500


@app.route('/api/clients/<client_id>', methods=['DELETE'])
def delete_client(client_id):
    """Delete a client and all associated resources."""
    if not re.match(r'^[\w-]+$', client_id):
        abort(400, description="Invalid client ID")
    
    errors = []
    paths = get_client_paths(client_id)
    
    try:
        # Clean up DHCP configuration
        try:            
            update_dhcp_config(client_id, "", is_new=False)
            
            # Restart DHCP service
            run_command(['systemctl', 'restart', 'isc-dhcp-server.service'], use_sudo=True)
        except Exception as e:
            errors.append(f"Failed to clean up DHCP config: {e}")
                
        # Clean up ZFS clone
        try: 
            result = run_command(['zfs', 'list', '-H', paths['clone']], check=False, use_sudo=True)
            if result.returncode == 0:
                run_command(['zfs', 'destroy', paths['clone']], use_sudo=True)
        except Exception as e: 
            errors.append(f"Failed to destroy ZFS clone: {e}")
        
        # Clean up iSCSI target
        if not cleanup_iscsi_target(paths['target_iqn'], paths['block_store']):
            errors.append(f"Failed to clean up iSCSI target")
            
        # Delete client configuration from JSON file
        if not delete_client_config(client_id):
            errors.append("Failed to delete client configuration file")
        
        if errors:
            return jsonify({
                "message": f"Client {client_id} deleted with issues",
                "errors": errors
            }), 207
        return jsonify({"message": f"Client {client_id} deleted successfully"}), 200
            
    except Exception as e:
        return jsonify({"error": f"Unexpected error during deletion: {str(e)}"}), 500

@app.route('/api/clients/<client_id>/remote', methods=['POST'])
def remote_client(client_id):
    """Connect to client via Remote Desktop."""
    try:
        # Get client info
        client_info = get_clients(client_id)
        if not client_info:
            return jsonify({"error": "Client not found"}), 404

        # Get client IP
        client_ip = client_info.get('ip')
        if not client_ip:
            return jsonify({"error": "Client IP not found"}), 404

        # Check if client is online
        status = get_client_status(client_ip)
        if status != "Online":
            return jsonify({"error": "Client is not online"}), 400

        # Launch remote desktop
        launch_remote_desktop(client_ip)
        
        return jsonify({
            "message": f"Remote desktop connection initiated to {client_id}",
            "ip": client_ip
        })

    except Exception as e:
        return jsonify({
            "error": f"Failed to launch remote desktop: {str(e)}"
        }), 500

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
    client_info = get_clients(client_id)
    
    # Check if client was found
    if client_info is None:
        print(f"Error: Client {client_id} not found for control action '{action}'")
        return jsonify({"error": f"Client {client_id} not found"}), 404
    
    mac_address = client_info.get("mac")
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
            client_ip = client_info.get(client_id, {}).get("ip")
            if not client_ip:
                return jsonify({"error": f"IP address not found for '{client_id}'"}), 404
            
            # Use samba net rpc to reboot Windows client
            # Requires Samba tools and proper Windows credentials
            # Format: net rpc shutdown -r -I <IP> -U <username%password> -f -t 0
            net_command = ['net', 'rpc', 'shutdown', '-r', '-I', client_ip, '-U', 'diskless%1', '-f', '-t', '0']
            print(f"Using server IP: {SERVER_IP}")
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
            client_ip = client_info.get(client_id, {}).get("ip")
            if not client_ip:
                return jsonify({"error": f"IP address not found for '{client_id}'"}), 404
            
            # Use samba net rpc to shutdown Windows client
            # Requires Samba tools and proper Windows credentials
            # Format: net rpc shutdown -S -I <IP> -U <username%password> -f
            net_command = ['net', 'rpc', 'shutdown', '-S', client_ip, '-U', 'diskless%1']
            print(f"Using server IP: {SERVER_IP}")
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
         clone_name = f"{ZFS_POOL}/{client_id.lower()}-disk"
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
                 new_clone_name = f"{ZFS_POOL}/{client_id.lower()}-disk-temp"
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
    # Use debug=False for production
    app.run(host='0.0.0.0', port=5000, debug=False)
