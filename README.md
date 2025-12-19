# Diskless Manager

A web-based toolkit for managing diskless PXE/iSCSI boot environments using ZFS, iSCSI, DHCP, and TFTP.

## 🚀 Features

- **ZFS Management**
  - Create and manage master images
  - Snapshot management for quick rollback
  - Automated clone management for clients

- **Network Boot Configuration**
  - iSCSI target setup and management
  - DHCP/PXE boot configuration
  - TFTP file management

- **Client Management**
  - Add/Edit/Remove diskless clients
  - Real-time status monitoring
  - Wake-on-LAN support
  - Client power management

## 📋 Requirements

### System Requirements

- Linux with ZFS support
- React 19.2 (for frontend)
- Tauri v2 and Rust (for backend)
- ISC DHCP Server
- TFTP Server
- iSCSI Target Support
- Samba (for Windows client management)
- OpenSSH Server (for remote management)

### System Packages

```bash
sudo apt update
sudo apt install \
    zfsutils-linux \
    targetcli-fb \
    isc-dhcp-server \
    tftpd-hpa \
    apache2 \
    wakeonlan \
    samba \
    samba-common-bin \
    openssh-server \
    net-tools
```

### Required Services

```bash
# Check service status
sudo systemctl status \
    target \
    tftpd-hpa \
    isc-dhcp-server \
    smbd \
    apache2 \
    ssh

# Enable services to start on boot
sudo systemctl enable \
    target \
    tftpd-hpa \
    isc-dhcp-server \
    smbd \
    apache2 \
    ssh

# Start services
sudo systemctl start \
    target \
    tftpd-hpa \
    isc-dhcp-server \
    smbd \
    apache2 \
    ssh
```

### Samba Configuration

```bash
# Create diskless user for Samba
sudo smbpasswd -a diskless

# Add to /etc/samba/smb.conf
[global]
   workgroup = WORKGROUP
   security = user
   map to guest = never

[diskless]
   path = /srv/tftp
   browseable = yes
   read only = no
   guest ok = no
   valid users = diskless
```

### System Packages

```bash
sudo apt install \
    zfsutils-linux \
    targetcli-fb \
    isc-dhcp-server \
    tftpd-hpa \
    apache2 \
    wakeonlan \
    samba \
    samba-common-bin \
    openssh-server \
    net-tools

```

## 🛠️ Installation

1. **Clone the Repository**
   ```bash
   git clone https://github.com/yourusername/diskless-manager.git
   cd diskless-manager
   ```
2. **Setup the App**

   ```bash
   bun install
   ```

3. **Configure Services**
   ```bash
   sudo mkdir -p /srv/tftp
   sudo mkdir -p /srv/shared
   sudo mkdir -p /srv/iscsi
   sudo mkdir -p ~/.config/com.diskless.local
   sudo cp config/config.json ~/.config/com.diskless.local
   ```

## ⚙️ Configuration

1. **Backend Settings** (`~/.config/com.diskless.local/config.json`):

   ```json
   {
     "zfs_pool": "diskless",
     "master_dataset": "diskless/Windows11-master",
     "clients_dataset": "diskless",
     "iscsi_target_prefix": "iqn.2025-05.local.diskless",
     "tftp_dir": "/srv/tftp",
     "network_subnet": "192.168.1.0/24"
   }
   ```

2. **Configure Sudo Access**
   ```bash
   # Add to /etc/sudoers.d/diskless-manager
   %USER% ALL=(ALL) NOPASSWD: /usr/sbin/zfs,/usr/bin/targetcli,/bin/systemctl,/usr/sbin/dhcpd,/usr/bin/wakeonlan
   ```

### 4. Configure Environment Variables

**IMPORTANT - Security Configuration:**

The application requires a JWT secret for authentication. Generate a secure random secret:

```bash
# Generate a secure random secret
openssl rand -base64 32

# Set the JWT_SECRET environment variable
export JWT_SECRET="your-generated-secret-here"

# Or add to your shell profile (~/.bashrc, ~/.zshrc, etc.)
echo 'export JWT_SECRET="your-generated-secret-here"' >> ~/.bashrc
source ~/.bashrc
```

⚠️ **Security Notes:**

- Never commit the actual JWT_SECRET to version control
- Use a different secret for development and production
- Keep your secret secure and rotate it periodically
- See `src-tauri/.env.example` for reference

## 🚀 Usage

1. **Start App**

   ```bash
   bun tauri dev
   ```

2. **Access Web Interface**
   - Open browser to `http://localhost:5173`

## 📁 Project Structure

```
diskless-manager/

│   ├── src-tauri/
│   │   ├── src/
│   │   ├── icons/
│   │   ├── Cargo.toml
│   │   ├── Cargo.lock
│   │   ├── tauri.conf.json
│   │   └── build.rs
│   ├── package.json
│   ├── src/
│   │   ├── components/
│   │   ├── assets/
│   │   ├── contexts/
│   │   ├── hooks/
│   │   ├── lib/
│   │   ├── router/
│   │   ├── store/
│   │   ├── utils/
│   │   ├── index.css
│   │   └── main.jsx
│   └── package.json
└── README.md
```
