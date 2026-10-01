# Diskless Manager

Diskless Manager is a Linux server with a browser UI for managing PXE/iPXE boot,
ZFS-backed client disks, and iSCSI targets. The React frontend and Rust/Axum
backend run without a desktop shell. The backend serves both the API and the
built frontend; `src-tauri/` retains its historical directory name.

## Features

- **Storage and images:** ZFS pool creation, master images, snapshots, client
  clones, and game disk assignment.
- **Clients:** image and snapshot selection, persistent or resettable writeback,
  BIOS/UEFI PXE configuration, per-client CHAP credentials, Wake-on-LAN, and
  remote power actions.
- **Enrollment:** a time-limited registration window for unknown PXE clients.
  Newly enrolled clients remain disabled until configured and enabled.
- **Monitoring:** live client and server metrics, service status, ZFS statistics,
  and application logs.
- **Server setup:** privileged access, dependency installation, storage, network,
  DHCP, TFTP, HTTP, Samba, and boot script configuration with readiness checks.
- **Administration:** first-administrator setup, role-based access, user and
  license management, SSH testing, and application backup/restore.
- **Experimental NVMe/TCP:** expose an existing client ZVOL through Linux
  NVMe/TCP. iSCSI is the default boot path; firmware tooling is provided in
  `scripts/build-nvmeof-ipxe.sh`.

## Requirements

The backend integrates with systemd and Linux storage/network tools. Distribution
handling covers Debian/Ubuntu, Fedora/RHEL derivatives, and Arch Linux.

For a source build, install Bun, a current stable Rust toolchain, and native build
tools including `pkg-config` and OpenSSL development headers. React 19 and Vite 8
are installed through `bun install`; they are not separate server prerequisites.

The setup wizard checks distribution-specific packages and can install them
through the authorized backend account. Core tools include OpenZFS, `targetcli`,
`qemu-img`, ISC DHCP, TFTP, Apache, and Samba. NFS tools are required when NFS is
enabled. Wake-on-LAN, FreeRDP, and `iftop` are optional for setup readiness.

| Service/tool | Debian/Ubuntu package | Fedora/RHEL package | Arch package |
| --- | --- | --- | --- |
| ZFS | `zfsutils-linux` | `zfs` | `zfs-utils` |
| iSCSI targets | `targetcli-fb` | `targetcli` | `targetcli-fb` |
| Image conversion | `qemu-utils` | `qemu-img` | `qemu` |
| DHCP | `isc-dhcp-server` | `dhcp-server` | `dhcp` |
| TFTP | `tftpd-hpa` | `tftp-server` | `tftp-hpa` |
| HTTP boot files | `apache2` | `httpd` | `apache` |
| Samba | `samba` | `samba` | `samba` |
| NFS | `nfs-kernel-server` | `nfs-utils` | `nfs-utils` |

ZFS also needs a working kernel module for the running kernel. The Fedora/RHEL
installation path attempts to enable the OpenZFS repository and install matching
kernel development packages. On Arch, provision a compatible ZFS module and any
packages unavailable through your configured repositories before retrying setup.

Remote client actions need SSH access and credentials for the client computer.
Bootloader binaries and bootable OS images must be supplied separately.

## Build and run

```bash
git clone https://github.com/psychrabi/diskless-manager.git
cd diskless-manager
bun install --frozen-lockfile
bun run build
cargo build --release --locked --manifest-path src-tauri/Cargo.toml
FRONTEND_DIR="$PWD/dist" ./src-tauri/target/release/diskless-manager
```

Open `http://localhost:8080`. Run the backend as the account that will own its
configuration and application database.

For development, use two terminals from the repository root:

```bash
# Terminal 1: backend
bun run dev:backend

# Terminal 2: frontend
bun dev
```

Open the Vite URL, normally `http://localhost:5173`. Vite proxies `/api` and `/ws`
to the backend at `127.0.0.1:8080`.

## First-run setup

1. Create the first administrator and sign in. No default administrator password
   is shipped.
2. Complete **Authorize** to grant the backend account the required privileged
   commands.
3. Install missing dependencies and create or select the configured ZFS pool.
4. Save the server network settings, then configure DHCP with matching server
   address, subnet mask, and gateway. Saving network settings in the wizard does
   not apply them to the host interface.
5. Apply TFTP, HTTP, and Samba configuration.
6. Review and save the iPXE boot script. Place the configured bootloader binaries
   in the TFTP root (default `/srv/tftp`). The defaults are `undionly.kpxe`,
   `ipxe.efi`, and `snponly.efi`; saving the script does not install these files.
7. Refresh server readiness and confirm setup to open the dashboard.

Management pages remain blocked until setup is confirmed and the readiness checks
pass. Checks cover authorization, required dependencies, saved network settings,
the selected pool, applied service configurations, and nonempty boot files.
Existing installations also need this review and confirmation. Changed settings
or service files can invalidate readiness and require setup again.

### Privileged access on a headless server

The browser authorization button requests approval through Polkit on the **server
computer**, even when the browser runs on another computer. Without a desktop
Polkit authentication agent, run this in a terminal as the backend account:

```bash
diskless-manager authorize
# From a development checkout after building:
./src-tauri/target/debug/diskless-manager authorize
```

Sudo prompts in that terminal. The command generates a distribution-specific
sudoers rule and validates it before replacing the existing rule. Retry
**Authorize** in the browser afterwards; an existing grant returns
**Already Authorized**.

## Configuration and application data

On Linux, application data lives in the backend account's
`$XDG_CONFIG_HOME/com.diskless.local/`, normally
`~/.config/com.diskless.local/`. Startup creates the directory, default settings,
and SQLite database automatically. Configure the server through the wizard and
**System Settings**; no manual copy of a sample configuration is required.

| File/directory | Purpose |
| --- | --- |
| `diskless.db` | Users, clients, image metadata, settings, and other application records |
| `config.toml` | Local server settings; saved database settings are merged at startup |
| `config.json` | Compatibility configuration file, included in backups when present |
| `jwt-secret` | Automatically generated signing secret when `JWT_SECRET` is unset |
| `diskless-manager.log` | Backend log |
| `backups/` | Safety backups created during restore |

The default network settings target server `192.168.1.250/24`, gateway
`192.168.1.254`, and DHCP range `192.168.1.100–192.168.1.200`. Review them for your
LAN before applying service configuration. HTTP boot files and TFTP default to
`/srv/tftp`; the Samba share defaults to `/srv/shared`.

### Environment variables

| Variable | Default | Purpose |
| --- | --- | --- |
| `FRONTEND_DIR` | `../dist` relative to the backend working directory | Built frontend directory; set an absolute path for deployment |
| `DISKLESS_API_ADDR` | `127.0.0.1:8080` | Management API, WebSocket metrics, and browser UI listener |
| `DISKLESS_ENROLL_ADDR` | `0.0.0.0:4237` | Dedicated PXE enrollment listener |
| `JWT_SECRET` | Generated and persisted locally | Optional signing secret override, at least 32 bytes |

An explicit signing secret can be generated with `openssl rand -base64 32` and
set in the backend environment. Keep it stable across restarts and out of version
control. A shell export does not configure a systemd service.

## Deployment

### LAN access

To serve the management UI to other computers:

```bash
DISKLESS_API_ADDR=0.0.0.0:8080 \
  FRONTEND_DIR="$PWD/dist" \
  ./src-tauri/target/release/diskless-manager
```

Open `http://<server-ip>:8080` and allow the configured management port through
your server firewall. PXE clients use the separate enrollment listener on TCP
4237 by default, alongside DHCP, TFTP, HTTP boot files, and iSCSI services.
Enrollment of unknown clients is closed until an administrator opens its
registration window.

### Systemd

Deploy the binary to `/usr/bin/diskless-manager`, the built `dist/` directory to
`/opt/diskless-manager/dist`, and the supplied unit to
`/etc/systemd/system/diskless-manager.service`. The supplied unit runs as `root`
with `HOME=/root`; its application data therefore defaults to
`/root/.config/com.diskless.local/`. If changing the service account, update
`User` and `HOME` together and authorize that account.

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now diskless-manager
```

For LAN access, add an override with `sudo systemctl edit diskless-manager`:

```ini
[Service]
Environment=DISKLESS_API_ADDR=0.0.0.0:8080
```

Restart the service after changing its environment:

```bash
sudo systemctl restart diskless-manager
```

The service manager uses distribution-specific units:

| Service | Debian/Ubuntu | Fedora/RHEL | Arch |
| --- | --- | --- | --- |
| DHCP | `isc-dhcp-server` | `dhcpd` | `dhcpd4` |
| TFTP | `tftpd-hpa` | `tftp` (`tftp.socket` for boot activation) | `tftpd` |
| iSCSI | `rtslib-fb-targetctl` | `target` | `target` |
| HTTP | `apache2` | `httpd` | `httpd` |
| Samba | `smbd`, `nmbd` | `smb`, `nmb` | `smb`, `nmb` |
| NFS | `nfs-kernel-server` | `nfs-server` | `nfs-server` |

### Packaging

A deployment bundle contains the release binary, `dist/`,
`systemd/diskless-manager.service`, and `packaging/install.sh` copied as
`install.sh`. Run `sudo ./install.sh` from the bundle directory. The installer
copies files and reloads systemd; enable the application service afterwards.
Use `sudo SKIP_DEPS=1 ./install.sh` when dependencies are already provisioned.
The installer assumes its package names are available in configured repositories;
it does not perform the wizard's OpenZFS repository setup.

`packaging/diskless-manager.spec` provides RPM packaging for a prebuilt x86_64
binary and frontend. There is no desktop bundler.

## Application backup and restore

Administrators can download or restore a JSON backup in **Application Settings**
or the setup wizard's recovery section. Backups include the application database,
local configuration files, and signing secret. This covers users, clients, image
metadata, settings, and licenses. Store backups securely: they contain
authentication secrets.

ZFS datasets, OS image contents, and operating system service files are excluded.
An application backup alone cannot recreate the client disks on another server.

1. Choose an application backup JSON file (maximum 64 MiB).
2. Confirm **Stage restore**. Live application data remains in place until restart.
3. Restart the backend, then sign in with an administrator from the backup and
   review server setup.

Private safety backups are saved before staging and again before applying.
Restore revokes existing sessions and clears setup completion. If `JWT_SECRET`
is configured, it must match the backup's signing secret. Failed application
recovers the original installation and retains the rejected bundle as
`failed-restore-<id>.json`; check the backend log. Recovery errors stop startup
to protect the original data.

## Development checks

```bash
bun run test --run
bunx eslint src
bun run build
cargo test --locked --manifest-path src-tauri/Cargo.toml
```

Rust WebSocket tests bind local sockets. The DHCP syntax integration test is
ignored by default and requires the ISC `dhcpd` binary.

`bun run lint` scans the whole checkout; untracked release bundles or local skill
scripts can introduce unrelated errors. The source-scoped command above checks
the frontend code.

`scripts/verify-production-diskless.sh` checks an installed server against the
repository's default network and boot layout. Review its fixed IP addresses and
interface expectations before using it on a customized installation.

## Project structure

```text
diskless-manager/
├── src/                     # React UI, API client, hooks, stores, and tests
├── src-tauri/
│   ├── src/
│   │   ├── api/             # Axum routes, handlers, authentication middleware
│   │   ├── application/     # Client and storage workflows
│   │   ├── domain/          # Domain types
│   │   ├── infrastructure/  # ZFS, iSCSI, DHCP, and PXE integrations
│   │   ├── persistence/     # SQLite repositories
│   │   └── services/        # System service configuration and control
│   ├── migrations/          # Database migrations
│   ├── script/              # iPXE scripts
│   ├── tests/               # Backend integration tests
│   └── Cargo.toml
├── scripts/                 # Deployment verification and PXE tooling
├── systemd/                 # Application service unit
├── packaging/               # Bundle installer and RPM spec
├── screenshots/
├── package.json
└── vite.config.js
```
