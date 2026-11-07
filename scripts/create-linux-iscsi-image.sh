#!/usr/bin/env bash
set -Eeuo pipefail

usage() {
  cat <<'USAGE'
Create a bootable Linux iSCSI image using the installed Diskless Manager worker.

Usage:
  sudo bash scripts/create-linux-iscsi-image.sh --iso FILE.iso --volume POOL/VOLUME [--check]

The target must be an empty, standalone ZFS volume of at least 20 GiB. A build
repartitions and formats the target. The app's Images > Linux Image Builder
workflow is preferred for creating and cataloging a new image.
USAGE
}

ISO= VOLUME= CHECK=0
while (($#)); do
  case "$1" in
    --iso) ISO=${2:?--iso requires a path}; shift 2 ;;
    --volume) VOLUME=${2:?--volume requires POOL/VOLUME}; shift 2 ;;
    --check) CHECK=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

[[ $EUID == 0 ]] || { echo 'Run with sudo bash.' >&2; exit 1; }
[[ -f $ISO && -n $VOLUME ]] || { usage >&2; exit 2; }
ISO=$(realpath -- "$ISO")

if ((CHECK)); then
  exec /usr/bin/diskless-manager internal-linux-image --check --iso "$ISO" --volume "$VOLUME"
fi

[[ -r /dev/tty ]] || { echo 'Use the app or run this command from a terminal to enter account settings.' >&2; exit 1; }
read -r -p 'First-boot username [diskless]: ' USERNAME </dev/tty
USERNAME=${USERNAME:-diskless}
read -r -s -p "Password for ${USERNAME}: " PASSWORD </dev/tty
printf '\n' >/dev/tty
read -r -p 'Grant this account sudo access? [Y/n]: ' SUDO_ANSWER </dev/tty
if [[ ${SUDO_ANSWER,,} == n || ${SUDO_ANSWER,,} == no ]]; then
  SUDO_USER=false
else
  SUDO_USER=true
fi

USERNAME_JSON=${USERNAME//\\/\\\\}
USERNAME_JSON=${USERNAME_JSON//\"/\\\"}
PASSWORD_JSON=${PASSWORD//\\/\\\\}
PASSWORD_JSON=${PASSWORD_JSON//\"/\\\"}
printf '{"username":"%s","password":"%s","sudo_user":%s}\n' "$USERNAME_JSON" "$PASSWORD_JSON" "$SUDO_USER" \
  | /usr/bin/diskless-manager internal-linux-image --build --iso "$ISO" --volume "$VOLUME"
unset PASSWORD PASSWORD_JSON USERNAME_JSON
