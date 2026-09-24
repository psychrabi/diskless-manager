#!/bin/sh
# Universal installer for diskless-manager (x86_64).
# Layout expected next to this script: diskless-manager, dist/, diskless-manager.service
# Usage: sudo ./install.sh
set -e

if [ "$(id -u)" -ne 0 ]; then
  echo "Run as root: sudo ./install.sh" >&2
  exit 1
fi

detect_distro() {
  if [ -f /etc/debian_version ]; then echo debian;
  elif [ -f /etc/redhat-release ]; then echo redhat;
  elif [ -f /etc/arch-release ]; then echo arch;
  else echo unknown; fi
}

DISTRO=$(detect_distro)
echo "Detected distribution family: $DISTRO"

install_deps() {
  case "$DISTRO" in
    debian) apt-get update && apt-get install -y isc-dhcp-server tftpd-hpa samba apache2 wakeonlan targetcli-fb zfsutils-linux ;;
    redhat) dnf install -y dhcp-server tftp-server samba httpd wol targetcli zfs ;;
    arch) pacman -Sy --noconfirm dhcp tftp-hpa samba apache wakeonlan targetcli-fb zfs-dkms zfs-utils ;;
    *) echo "Unknown distribution; install DHCP/TFTP/Samba/HTTPD/WOL/targetcli/ZFS equivalents manually." ;;
  esac
}

if [ "${SKIP_DEPS:-0}" != "1" ]; then
  install_deps
fi

install -m 0755 diskless-manager /usr/bin/diskless-manager
mkdir -p /opt/diskless-manager
cp -r dist /opt/diskless-manager/dist
if [ -d /usr/lib/systemd/system ]; then UNIT_DIR=/usr/lib/systemd/system; else UNIT_DIR=/lib/systemd/system; fi
install -m 0644 diskless-manager.service "$UNIT_DIR/diskless-manager.service"
systemctl daemon-reload 2>/dev/null || true

echo "Installed. Next steps:"
echo "  1. Run Privileged Access setup from the app wizard (writes /etc/sudoers.d/diskless-manager)."
echo "  2. Set a JWT secret for the service user."
echo "  3. systemctl enable --now diskless-manager"
echo "  4. Open http://localhost:8080 (LAN: DISKLESS_API_ADDR=0.0.0.0:8080 via systemctl edit)."
