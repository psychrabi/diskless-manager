#!/bin/sh
# Persist the same assigned target into the installed OS before declaring success.
set -eu
TARGET=$1
FAMILY=$2
umask 077
mkdir -p "$TARGET/etc/iscsi"
cp -a /etc/iscsi/initiatorname.iscsi "$TARGET/etc/iscsi/initiatorname.iscsi"
if [ -d /etc/iscsi/nodes ]; then cp -a /etc/iscsi/nodes "$TARGET/etc/iscsi/"; fi
if [ "$FAMILY" = ubuntu ]; then
    cat > "$TARGET/etc/iscsi/iscsi.initramfs" <<'CONFIG'
ISCSI_AUTO=true
ISCSI_INITIATOR='@@INITIATOR@@'
ISCSI_TARGET_NAME='@@IQN@@'
ISCSI_TARGET_IP='@@PORTAL@@'
ISCSI_TARGET_PORT=3260
ISCSI_USERNAME='@@CHAP_USER@@'
ISCSI_PASSWORD='@@CHAP_SECRET@@'
CONFIG
    chmod 600 "$TARGET/etc/iscsi/iscsi.initramfs"
    chroot "$TARGET" update-initramfs -u -k all
else
    mkdir -p "$TARGET/etc/dracut.conf.d"
    printf 'add_dracutmodules+=" iscsi "\ninstall_items+=" /etc/iscsi/initiatorname.iscsi "\n' > "$TARGET/etc/dracut.conf.d/diskless.conf"
    # iPXE supplies a client-bound iBFT, including CHAP, on normal boot.
    chroot "$TARGET" grubby --update-kernel=ALL --args='rd.neednet=1 ip=dhcp rd.iscsi.firmware=1'
    chroot "$TARGET" dracut -f --regenerate-all
fi
