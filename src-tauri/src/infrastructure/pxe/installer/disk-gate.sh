#!/bin/sh
# This gate runs in the installer, never on the management server.
set -eu
IQN='@@IQN@@'
PORTAL='@@PORTAL@@:3260'
EXPECTED='@@BYTES@@'
REPORT='@@REPORT@@'
[ "$(uname -m)" = x86_64 ] || { echo 'Unattended Linux requires x86_64' >&2; exit 1; }
[ -d /sys/firmware/efi ] || { echo 'Unattended Linux requires UEFI' >&2; exit 1; }
report() {
    payload='{"state":"installing","progress":10,"message":"Verified exclusive target IQN, LUN and disk size before partition"}'
    if command -v curl >/dev/null; then curl -fsS -H Content-Type:application/json -d "$payload" "$REPORT" >/dev/null
    else wget -q -O /dev/null --header=Content-Type:application/json --post-data="$payload" "$REPORT"; fi
}
command -v iscsiadm >/dev/null || { echo 'Installer must include open-iscsi/iscsi-initiator-utils' >&2; exit 1; }
command -v blockdev >/dev/null
command -v udevadm >/dev/null
# Use the existing client's ACL identity, never a discovery-generated identity.
mkdir -p /etc/iscsi
printf 'InitiatorName=%s\n' '@@INITIATOR@@' > /etc/iscsi/initiatorname.iscsi
if command -v systemctl >/dev/null; then systemctl restart iscsid; fi
if ! iscsiadm -m node -T "$IQN" -p "$PORTAL" >/dev/null 2>&1; then
    iscsiadm -m node -T "$IQN" -p "$PORTAL" --op new
fi
if [ -n '@@CHAP_USER@@' ]; then
    iscsiadm -m node -T "$IQN" -p "$PORTAL" --op update -n node.session.auth.authmethod -v CHAP
    iscsiadm -m node -T "$IQN" -p "$PORTAL" --op update -n node.session.auth.username -v '@@CHAP_USER@@'
    iscsiadm -m node -T "$IQN" -p "$PORTAL" --op update -n node.session.auth.password -v '@@CHAP_SECRET@@'
fi
CONNECTED=0
for SESSION in /sys/class/iscsi_session/session*; do
    [ -f "$SESSION/targetname" ] || continue
    if [ "$(cat "$SESSION/targetname")" = "$IQN" ]; then CONNECTED=1; fi
done
if [ "$CONNECTED" = 0 ]; then iscsiadm -m node -T "$IQN" -p "$PORTAL" --login; fi
udevadm settle --timeout=30
DISK="/dev/disk/by-path/ip-${PORTAL}-iscsi-${IQN}-lun-0"
[ -b "$DISK" ] || { echo 'Expected target LUN 0 is missing' >&2; exit 1; }
[ "$(blockdev --getsize64 "$DISK")" = "$EXPECTED" ] || { echo 'Target disk size changed; refusing partition' >&2; exit 1; }
# Resolve sysfs identity independently from the persistent symlink.
DEVICE=$(basename "$(readlink -f "$DISK")")
FOUND=0
for SESSION in /sys/class/iscsi_session/session*; do
    [ -f "$SESSION/targetname" ] || continue
    [ "$(cat "$SESSION/targetname")" = "$IQN" ] || continue
    if find -L "$SESSION/device" -path "*/block/$DEVICE" -print -quit | grep -q .; then FOUND=$((FOUND + 1)); fi
done
[ "$FOUND" = 1 ] || { echo 'Disk does not resolve to exactly one expected iSCSI session' >&2; exit 1; }
[ -z "$(lsblk -nro MOUNTPOINTS "$DISK" | tr -d '[:space:]')" ] || { echo 'Target is mounted; refusing partition' >&2; exit 1; }
report
