#!/usr/bin/env bash
set -euo pipefail
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
ROOT_MNT="$work/root" MODULES="$work/modules" KVER=test
mkdir -p "$MODULES/$KVER/kernel/drivers/net/ethernet" "$ROOT_MNT/usr/lib/open-iscsi"
chroot() {
  local args=" $* "
  if [[ -f $ROOT_MNT/usr/lib/open-iscsi/startup-checks.sh ]]; then
    for file in /usr/lib/open-iscsi/startup-checks.sh /etc/iscsi/iscsid.conf /etc/iscsi/initiatorname.iscsi /usr/sbin/iscsi-iname; do
      [[ $args == *"$file"* ]] || { echo "Missing initramfs dependency: $file" >&2; return 1; }
    done
  else
    [[ $args != *startup-checks.sh* ]]
  fi
}
section=$(sed -n '/^NET_DRIVERS=/,/^ROOT_UUID=/{ /^ROOT_UUID=/d; p; }' "$(dirname "$0")/create-linux-iscsi-image.sh")
touch "$ROOT_MNT/usr/lib/open-iscsi/startup-checks.sh"
eval "$section"
rm "$ROOT_MNT/usr/lib/open-iscsi/startup-checks.sh"
eval "$section"
echo 'iSCSI startup dependency checks passed.'
