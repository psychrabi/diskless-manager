#!/usr/bin/env bash
set -euo pipefail
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
MODULES="$work/modules" KVER=test ROOT_MNT="$work/root"
mkdir -p "$MODULES/$KVER/kernel/drivers/net/ethernet/vendor"
touch "$MODULES/$KVER/kernel/drivers/net/ethernet/vendor/"{r8169.ko.zst,e1000e.ko.xz,igc.ko,README}
chroot() {
  local drivers=
  while (($#)); do
    if [[ $1 == --add-drivers ]]; then drivers=$2; break; fi
    shift
  done
  for driver in iscsi_tcp iscsi_ibft r8169 e1000e igc; do
    [[ " $drivers " == *" $driver "* ]] || { echo "Missing driver: $driver" >&2; return 1; }
  done
  [[ $drivers != *'.ko'* && $drivers != *README* ]]
}
eval "$(sed -n '/^NET_DRIVERS=/,/^ROOT_UUID=/{ /^ROOT_UUID=/d; p; }' "$(dirname "$0")/create-linux-iscsi-image.sh")"
echo 'Linux image Ethernet driver checks passed.'
