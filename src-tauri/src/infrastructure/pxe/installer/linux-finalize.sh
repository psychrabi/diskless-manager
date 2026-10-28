#!/bin/sh
# Configure the installed OS to use the firmware-provided iBFT at every boot.
set -eu
TARGET=$1
FAMILY=$2
REPORT='@@REPORT@@'
VERIFY_URL='@@VERIFY_URL@@'
umask 077
curl --fail --silent --show-error --max-time 15 -H 'Content-Type: application/json' -d '{"state":"adapting","progress":78,"message":"Preparing firmware iBFT boot"}' "$REPORT"
mkdir -p "$TARGET/etc/iscsi"
rm -f "$TARGET/etc/iscsi/initiatorname.iscsi"
rm -rf "$TARGET/etc/iscsi/nodes" "$TARGET/etc/iscsi/send_targets"
if [ "$FAMILY" = ubuntu ]; then
    cat > "$TARGET/etc/iscsi/iscsi.initramfs" <<'CONFIG'
ISCSI_AUTO=true
CONFIG
    chmod 600 "$TARGET/etc/iscsi/iscsi.initramfs"
    chroot "$TARGET" update-initramfs -u -k all
else
    mkdir -p "$TARGET/etc/dracut.conf.d"
    printf 'add_dracutmodules+=" iscsi "\n' > "$TARGET/etc/dracut.conf.d/diskless.conf"
    # iPXE supplies a client-bound iBFT, including CHAP, on normal boot.
    chroot "$TARGET" grubby --update-kernel=ALL --remove-args='rd.iscsi.initiator rd.iscsi.target.name rd.iscsi.target.ip rd.iscsi.target.port netroot ifname BOOTIF' --args='rd.iscsi.firmware=1 rd.iscsi.ibft=1 rd.neednet=1 ip=ibft'
    chroot "$TARGET" dracut -f --regenerate-all
fi
# The normal boot reporter uses only the iBFT target and public verification endpoint.
# POSIX shell and curl keep it portable across Ubuntu and minimal RHEL installs.
cat > "$TARGET/usr/local/sbin/diskless-verify-boot" <<'REPORTER'
#!/bin/sh
set -u
VERIFY_URL='@@VERIFY_URL@@'
LIMIT=12
IBFT=/sys/firmware/ibft/target0/target-name
[ -r "$IBFT" ] || exit 0
TARGET=$(tr -cd 'A-Za-z0-9_.:-' < "$IBFT")
[ -n "$TARGET" ] || exit 0
ATTEMPT=${TARGET##*.}
case "$ATTEMPT" in ''|*[!0-9]*) exit 0;; esac
IDS=
for M in /sys/class/net/*/device/modalias; do
  [ -r "$M" ] || continue
  ID=$(tr -cd 'A-Za-z0-9_.:+&-' < "$M")
  [ -n "$ID" ] || continue
  if [ -z "$IDS" ]; then IDS="\"$ID\""; else IDS="$IDS,\"$ID\""; fi
done
[ -n "$IDS" ] || IDS='"ibft"'
OSV=$(uname -r | tr -cd 'A-Za-z0-9_.-+')
ARCH=$(uname -m | tr -cd 'A-Za-z0-9_')
BODY=$(printf '{"target_iqn":"%s","attempt":%s,"os_family":"linux","os_version":"%s","architecture":"%s","boot_nic_ids":[%s],"boot_nic_mac":null}' "$TARGET" "$ATTEMPT" "$OSV" "$ARCH" "$IDS")
I=0
CODE=000
while [ "$I" -lt "$LIMIT" ]; do
  CODE=$(curl -sS --max-time 10 -o /run/diskless-verify.body -w '%{http_code}' -H 'Content-Type: application/json' -d "$BODY" "$VERIFY_URL" 2>/dev/null) || CODE=000
  [ "$CODE" = 200 ] && break
  [ "$CODE" = 204 ] && break
  I=$((I + 1))
  [ "$I" -lt "$LIMIT" ] && sleep 10
done
rm -f /run/diskless-verify.body
case "$CODE" in
  200) /usr/bin/systemctl poweroff --no-block 2>/dev/null || /sbin/poweroff ;;
  204) /usr/bin/systemctl disable diskless-verify-boot.service 2>/dev/null || true ;;
esac
exit 0
REPORTER
chmod 755 "$TARGET/usr/local/sbin/diskless-verify-boot"
cat > "$TARGET/etc/systemd/system/diskless-verify-boot.service" <<'UNIT'
[Unit]
Description=Report verified diskless normal boot
After=network-online.target iscsid.service
Wants=network-online.target

[Service]
Type=oneshot
ExecStart=/usr/local/sbin/diskless-verify-boot

[Install]
WantedBy=multi-user.target
UNIT
chroot "$TARGET" systemctl enable diskless-verify-boot.service
curl --fail --silent --show-error --max-time 15 -H 'Content-Type: application/json' -d '{"state":"installed","progress":88,"message":"Installation and adaptation complete"}' "$REPORT"
