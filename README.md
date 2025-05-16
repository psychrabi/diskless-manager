# Diskless Boot Manager

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
- Python 3.7+
- Node.js 16+ (for frontend)
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
    wakeonlan \
    samba \
    samba-common-bin \
    openssh-server \
    net-tools \
    dnsutils \
    iputils-ping
```

### Required Services
```bash
# Check service status
sudo systemctl status \
    iscsitarget \
    tftpd-hpa \
    isc-dhcp-server \
    smbd \
    nmbd \
    ssh

# Enable services to start on boot
sudo systemctl enable \
    iscsitarget \
    tftpd-hpa \
    isc-dhcp-server \
    smbd \
    nmbd \
    ssh

# Start services
sudo systemctl start \
    iscsitarget \
    tftpd-hpa \
    isc-dhcp-server \
    smbd \
    nmbd \
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

### Python Dependencies
```bash
flask
flask-cors
```

### System Packages
```bash
sudo apt install \
  zfsutils-linux \
  targetcli-fb \
  isc-dhcp-server \
  tftpd-hpa \
  wakeonlan
```

## 🛠️ Installation

1. **Clone the Repository**
   ```bash
   git clone https://github.com/yourusername/diskless-manager.git
   cd diskless-manager
   ```

2. **Setup Backend**
   ```bash
   python3 -m venv venv
   source venv/bin/activate
   pip install -r requirements.txt
   ```

3. **Setup Frontend**
   ```bash
   cd frontend
   npm install
   ```

4. **Configure Services**
   ```bash
   sudo mkdir -p /etc/diskless-manager
   sudo cp config/config.json /etc/diskless-manager/
   sudo cp config/dhcpd.conf /etc/dhcp/
   sudo cp config/tftpd-hpa /etc/default/
   ```

## ⚙️ Configuration

1. **Backend Settings** (`/etc/diskless-manager/config.json`):
   ```json
   {
     "zfs_pool": "nsboot0",
     "master_dataset": "nsboot0/Windows11-master",
     "clients_dataset": "nsboot0",
     "iscsi_target_prefix": "iqn.2025-05.com.nsboot",
     "tftp_dir": "/srv/tftp",
     "network_subnet": "192.168.1.0/24"
   }
   ```

2. **Configure Sudo Access**
   ```bash
   # Add to /etc/sudoers.d/diskless-manager
   flaskuser ALL=(ALL) NOPASSWD: /usr/sbin/zfs,/usr/bin/targetcli,/bin/systemctl,/usr/sbin/dhcpd,/usr/bin/wakeonlan
   ```

## 🚀 Usage

1. **Start Backend API**
   ```bash
   cd /srv/tftp/diskless-manager
   source venv/bin/activate
   python3 backend.py
   ```

2. **Start Frontend Development Server**
   ```bash
   cd frontend
   npm run dev
   ```

3. **Access Web Interface**
   - Open browser to `http://localhost:5173`

## 📁 Project Structure

```
diskless-manager/
├── backend/
│   ├── backend.py          # Flask API server
│   ├── config.json         # Default configuration
│   └── requirements.txt    # Python dependencies
├── frontend/              # React frontend
│   ├── src/
│   │   ├── components/
│   │   ├── services/
│   │   └── App.jsx
│   └── package.json
└── README.md
```

## 🔒 Security Considerations

- Run Flask behind a reverse proxy in production
- Enable HTTPS/SSL
- Implement proper authentication
- Configure firewall rules
- Regular ZFS snapshots for backup
- Audit system logs

## 🤝 Contributing

1. Fork the repository
2. Create feature branch
3. Commit changes
4. Push to branch
5. Create Pull Request

## 📝 License

This project is licensed under the MIT License - see the LICENSE file for details.