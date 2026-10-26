#!/usr/bin/env bash
# Run inside a gold Linux master, not on the Diskless Manager server.
# sudo bash prepare-iscsi-root.sh --dry-run
# sudo bash prepare-iscsi-root.sh --apply
set -euo pipefail
MODE=""
for arg in "$@"; do
  case "$arg" in
    --dry-run|--apply) [[ -z "$MODE" ]] || exit 2; MODE="$arg" ;;
    *) echo "Usage: $0 --dry-run|--apply" >&2; exit 2 ;;
  esac
done
[[ -n "$MODE" ]] || { echo "Choose --dry-run or --apply" >&2; exit 2; }
[[ $EUID -eq 0 ]] || { echo "Run as root inside the Linux master" >&2; exit 1; }
[[ -f /etc/os-release ]] || { echo "Missing /etc/os-release" >&2; exit 1; }
source /etc/os-release
echo "Linux: $ID / $MODE"
if command -v update-initramfs >/dev/null 2>&1; then
  command -v iscsiadm >/dev/null 2>&1 || { echo "Install open-iscsi first" >&2; exit 1; }
  command -v python3 >/dev/null 2>&1 || { echo "Python 3 required" >&2; exit 1; }
  echo "Plan: ISCSI_AUTO=true, GRUB iscsi_auto rd.iscsi.ibft=1 ip=dhcp, rebuild initramfs"
  [[ "$MODE" == --dry-run ]] && exit 0
  install -d -m 0755 /etc/iscsi
  printf 'ISCSI_AUTO=true\n' > /etc/iscsi/iscsi.initramfs
  touch /etc/default/grub
  cp -a /etc/default/grub "/etc/default/grub.diskless-backup.$(date +%Y%m%d%H%M%S)"
  python3 - <<'PY'
from pathlib import Path
import re
p = Path('/etc/default/grub')
s = p.read_text()
m = re.search(r'(?m)^GRUB_CMDLINE_LINUX=(["\x27])(.*?)\1', s)
required = ['iscsi_auto', 'rd.iscsi.ibft=1', 'ip=dhcp']
if m:
    tokens = m.group(2).split()
    tokens.extend(token for token in required if token not in tokens)
    s = s[:m.start()] + 'GRUB_CMDLINE_LINUX="' + ' '.join(tokens) + '"' + s[m.end():]
else:
    s += '\nGRUB_CMDLINE_LINUX="iscsi_auto rd.iscsi.ibft=1 ip=dhcp"\n'
p.write_text(s)
PY
  update-initramfs -u -k all
  if command -v update-grub >/dev/null 2>&1; then update-grub; fi
elif command -v dracut >/dev/null 2>&1; then
  command -v iscsiadm >/dev/null 2>&1 || { echo "Install iscsi-initiator-utils first" >&2; exit 1; }
  echo "Plan: dracut network + iscsi modules, kernel rd.iscsi.ibft=1 ip=dhcp, rebuild initramfs"
  [[ "$MODE" == --dry-run ]] && exit 0
  install -d -m 0755 /etc/dracut.conf.d
  printf 'add_dracutmodules+=" network iscsi "\n' > /etc/dracut.conf.d/91-diskless-iscsi.conf
  dracut --regenerate-all --force
  if command -v grubby >/dev/null 2>&1; then
    grubby --update-kernel=ALL --args="rd.iscsi.ibft=1 ip=dhcp"
  else
    echo "grubby absent; set kernel rd.iscsi.ibft=1 ip=dhcp manually" >&2
  fi
else
  echo "Unsupported: neither update-initramfs nor dracut installed" >&2; exit 1
fi
echo "Reboot-test the Linux master and then capture a clean image snapshot."
