#!/bin/bash
# Provision PXE/iSCSI client from ZFS golden image
# Usage: provision_client.sh <MAC> [--size 60G] [--initiator-iqn iqn....] [--ipxe-path ipxe.lkrn]
#
# This script checks if the client already exists in config.json before provisioning.
# If the client's MAC address is found in the config, provisioning is skipped.

set -euo pipefail

# Ensure system paths for daemons (dhcpd often has a minimal PATH)
export PATH=/usr/sbin:/usr/bin:/sbin:/bin

# --- config ---
ZPOOL="diskless"
MASTER_VOL="win11-master"            # zvol with preinstalled OS
GOLDEN_SNAP="golden-boot"            # shared snapshot for clones
IQN_BASE="iqn.2025-04.local.diskless"
TARGETCLI_BIN="/usr/bin/targetcli"
TARGET_PORTAL="192.168.1.250:3260"   # ip:port of this server (iSCSI)
PXE_DIR="/srv/tftp/pxelinux.cfg"
DEFAULT_VOLSIZE="60G"
IPXE_KERNEL="ipxe.lkrn"              # path relative to TFTP root
LOGFILE="/var/log/provision.log"
LOCKDIR="/var/lock/diskless"
# Binaries (use absolute paths for daemon environments)
ZFS_BIN="/usr/sbin/zfs"
UDEVADM_BIN="/usr/bin/udevadm"
FLOCK_BIN="/usr/bin/flock"
# -------------

log() {
  local ts msg
  ts=$(date '+%F %T')
  msg="[${ts}] $*"
  # Append to logfile if possible
  if { [ -f "$LOGFILE" ] && [ -w "$LOGFILE" ]; } || [ -w "$(dirname "$LOGFILE")" ]; then
    printf '%s\n' "$msg" >> "$LOGFILE" || true
  fi
  # Also send to syslog if available
  if command -v logger >/dev/null 2>&1; then
    logger -t provision_client -- "$*" || true
  fi
}
die() { log "[ERROR] $*"; exit 1; }

#require_root() { [[ $EUID -eq 0 ]] || die "Run as root."; }
have() { command -v "$1" >/dev/null 2>&1; }

usage() {
  echo "Usage: $0 <MAC> [--size 60G] [--initiator-iqn IQN] [--ipxe-path ipxe.lkrn]"
}

main() {
  #require_root
  # If not root (e.g., called by dhcpd), we'll use sudo for privileged ops
  if [[ $EUID -ne 0 ]]; then
    SUDO="/usr/bin/sudo -n"
  else
    SUDO=""
  fi  
  
  [[ -x $ZFS_BIN ]] || die "zfs not found at $ZFS_BIN"
  [[ -x $FLOCK_BIN ]] || die "flock not found at $FLOCK_BIN"
  [[ -x $TARGETCLI_BIN ]] || die "targetcli not found at $TARGETCLI_BIN"
  have jq || die "jq not found - required for config.json parsing"

  [[ $# -ge 1 ]] || { usage; exit 2; }

  # Accept either: <MAC> ... or <IP> <MAC> ...
  local mac_pat='^([0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}$'
  local mac_raw=""
  if [[ ${1:-} =~ $mac_pat ]]; then
    mac_raw="$1"; shift
  elif [[ $# -ge 2 && ${2:-} =~ $mac_pat ]]; then
    # discard IP ($1), take MAC from $2
    shift
    mac_raw="$1"; shift
  else
    die "First or second argument must be a MAC address"
  fi
  local volsize="$DEFAULT_VOLSIZE"
  local initiator_iqn=""
  local ipxe_kernel="$IPXE_KERNEL"

  while [[ $# -gt 0 ]]; do
    case "$1" in
      --size) volsize="$2"; shift 2;;
      --initiator-iqn) initiator_iqn="$2"; shift 2;;
      --ipxe-path) ipxe_kernel="$2"; shift 2;;
      -h|--help) usage; exit 0;;
      *) die "Unknown arg: $1";;
    esac
  done

  # Normalize MAC for file names and IQNs
  local mac_lc mac_hy mac_iqn
  mac_lc="$(echo "$mac_raw" | tr '[:upper:]' '[:lower:]')"
  mac_hy="${mac_lc//:/-}"               # 01-xx-xx-...
  mac_iqn="${mac_lc//:/-}"              # same hyphenated form

  # Check if client already exists in config.json
  # Try to find config file in standard locations
  local config_file=""
  
  # When running with sudo, try to get the actual user's home directory
  local user_home=""
  if [[ -n "${SUDO_USER:-}" ]]; then
    user_home=$(eval echo "~$SUDO_USER")
  elif [[ -n "${HOME:-}" ]]; then
    user_home="$HOME"
  else
    # Fallback to /home/rabistha if HOME is not set
    user_home="/home/rabistha"
  fi
  
  local possible_paths=(
    "$user_home/.config/com.diskless.local/config.json"
    "/home/rabistha/.config/com.diskless.local/config.json"
    "/etc/diskless-manager/config.json"
  )
  
  for path in "${possible_paths[@]}"; do
    if [[ -f "$path" ]]; then
      config_file="$path"
      break
    fi
  done
  
  if [[ -n "$config_file" ]]; then
    # Try exact match first
    if jq -e --arg mac "$mac_raw" '.clients[] | select(.mac == $mac)' "$config_file" >/dev/null 2>&1; then
      log "[INFO] Client with MAC $mac_raw already exists in config.json ($config_file), skipping provisioning"
      exit 0
    else
      # Try case-insensitive match
      local mac_upper=$(echo "$mac_raw" | tr '[:lower:]' '[:upper:]')
      if jq -e --arg mac "$mac_upper" '.clients[] | select(.mac == $mac)' "$config_file" >/dev/null 2>&1; then
        log "[INFO] Client with MAC $mac_raw already exists in config.json ($config_file), skipping provisioning"
        exit 0
      fi
    fi
  else
    log "[WARN] Config file not found in any standard location, proceeding with provisioning"
  fi

  local client_vol="client-${mac_iqn}"
  local target_iqn="${IQN_BASE}:${mac_iqn}"
  local zvol_path="/dev/zvol/${ZPOOL}/${client_vol}"

  umask 022
  mkdir -p "$(dirname "$LOGFILE")" "$PXE_DIR" "$LOCKDIR"

  # Per-client lock so different clients can provision concurrently
  local client_lock_file="${LOCKDIR}/provision_${mac_hy}.lock"
  # Ensure lock directory exists and is writable
  if ! mkdir -p "$(dirname "$client_lock_file")" 2>/dev/null; then
    # Fallback to /tmp if /var/lock is not writable
    LOCKDIR="/tmp/diskless-locks"
    client_lock_file="${LOCKDIR}/provision_${mac_hy}.lock"
    mkdir -p "$LOCKDIR" || die "Cannot create lock directory"
  fi
  
  exec 9>"$client_lock_file"
  $FLOCK_BIN -n 9 || die "Another provisioning run for ${mac_lc} is in progress"

  log "[INFO] Provisioning $mac_lc -> ${ZPOOL}/${client_vol} (${target_iqn})"

  # 1) Ensure golden snapshot exists once (narrow critical section)
  exec 8>"${LOCKDIR}/golden_snapshot.lock"
  $FLOCK_BIN 8
  if ! $SUDO $ZFS_BIN list -t snapshot "${ZPOOL}/${MASTER_VOL}@${GOLDEN_SNAP}" >/dev/null 2>&1; then
    log "[INFO] Creating golden snapshot ${ZPOOL}/${MASTER_VOL}@${GOLDEN_SNAP}"
    $SUDO $ZFS_BIN snapshot "${ZPOOL}/${MASTER_VOL}@${GOLDEN_SNAP}" || true
  else
    log "[INFO] Using existing golden snapshot ${GOLDEN_SNAP}"
  fi
  $FLOCK_BIN -u 8

  # 2) Create clone if needed
  if ! $SUDO $ZFS_BIN list "${ZPOOL}/${client_vol}" >/dev/null 2>&1; then
    log "[INFO] Creating clone ${ZPOOL}/${client_vol}"
    $SUDO $ZFS_BIN clone "${ZPOOL}/${MASTER_VOL}@${GOLDEN_SNAP}" "${ZPOOL}/${client_vol}"
  else
    log "[INFO] Clone ${ZPOOL}/${client_vol} already exists"
  fi

  # Ensure volsize (only grow or match)
  local cur_size
  cur_size="$($SUDO $ZFS_BIN get -H -o value volsize "${ZPOOL}/${client_vol}")"
  if [[ "$cur_size" != "$volsize" ]]; then
    # Attempt to grow; shrinking might fail if usage > new size
    log "[INFO] Setting volsize=${volsize} (was ${cur_size})"
    $SUDO $ZFS_BIN set volsize="${volsize}" "${ZPOOL}/${client_vol}" || log "[WARN] volsize adjustment failed; continuing"
  fi

  # Wait for /dev node
  $SUDO $UDEVADM_BIN settle || true
  [[ -b "$zvol_path" ]] || sleep 1

  # 3) targetcli: backstore, target, lun, attrs, saveconfig (idempotent)
  # Serialize targetcli mutations to avoid concurrent saveconfig/sysfs races
  exec 7>"${LOCKDIR}/targetcli.lock"
  $FLOCK_BIN 7
  create_backstore_if_missing "$client_vol" "$zvol_path"
  create_target_if_missing "$target_iqn"
  set_tpg_attrs "$target_iqn"
  # ensure_portal "$target_iqn" "${TARGET_PORTAL}"
  ensure_lun "$target_iqn" "$client_vol"
  ensure_acl_if_requested "$target_iqn" "$initiator_iqn"
  save_target_config
  $FLOCK_BIN -u 7

  # 4) PXE config (lowercase 01-mac)
  local pxe_file="${PXE_DIR}/01-${mac_hy}"
  log "[INFO] Writing PXE config ${pxe_file}"
  cat > "${pxe_file}" <<EOF
DEFAULT linux
LABEL linux
  KERNEL ${ipxe_kernel}
  APPEND dhcp && sanboot iscsi:${TARGET_PORTAL}::::${target_iqn}
EOF

  log "[OK] Provisioned ${mac_lc} -> ${client_vol} (${target_iqn})"
}

create_backstore_if_missing() {
  local name="$1" devpath="$2"
  # Check if backstore exists
  if $SUDO $TARGETCLI_BIN /backstores/block ls | grep -q " ${name} "; then
    log "[INFO] targetcli: removing existing backstore ${name}"
    $SUDO $TARGETCLI_BIN /backstores/block delete "${name}" >/dev/null || true
  fi
  log "[INFO] targetcli: creating backstore ${name} -> ${devpath}"
  $SUDO $TARGETCLI_BIN /backstores/block create "${name}" "${devpath}" >/dev/null
}

create_target_if_missing() {
  local iqn="$1"
  # Check if target exists
  if $SUDO $TARGETCLI_BIN /iscsi ls | grep -q " ${iqn} "; then
    log "[INFO] targetcli: removing existing target ${iqn}"
    $SUDO $TARGETCLI_BIN /iscsi delete "${iqn}" >/dev/null || true
  fi
  log "[INFO] targetcli: creating target ${iqn}"
  $SUDO $TARGETCLI_BIN /iscsi create "${iqn}" >/dev/null
}

set_tpg_attrs() {
  local iqn="$1"
  log "[INFO] targetcli: setting TPG attributes"
  $SUDO $TARGETCLI_BIN /iscsi/"${iqn}"/tpg1 set attribute generate_node_acls=1 cache_dynamic_acls=1 demo_mode_write_protect=0 authentication=0 >/dev/null
}

ensure_portal() {
  local iqn="$1" portal="$2"
  # Default portal 0.0.0.0:3260 exists; ensure specific portal too
  if ! $TARGETCLI_BIN /iscsi/"${iqn}"/tpg1/portals ls | grep -q " ${portal} "; then
    log "[INFO] targetcli: adding portal ${portal}"
    $TARGETCLI_BIN /iscsi/"${iqn}"/tpg1/portals create "${portal}" >/dev/null || true
  fi
}

ensure_lun() {
  local iqn="$1" backstore="$2"
  # Check if LUN exists and remove it
  if $SUDO $TARGETCLI_BIN /iscsi/"${iqn}"/tpg1/luns ls | grep -q "/backstores/block/${backstore}"; then
    log "[INFO] targetcli: removing existing LUN for ${backstore}"
    $SUDO $TARGETCLI_BIN /iscsi/"${iqn}"/tpg1/luns delete 0 >/dev/null || true
  fi
  log "[INFO] targetcli: creating LUN 0 -> ${backstore}"
  $SUDO $TARGETCLI_BIN /iscsi/"${iqn}"/tpg1/luns create /backstores/block/"${backstore}" >/dev/null
}

ensure_acl_if_requested() {
  local iqn="$1" init_iqn="$2"
  [[ -z "$init_iqn" ]] && { log "[INFO] targetcli: dynamic ACLs enabled, no explicit ACL"; return; }
  # Remove existing ACL if it exists
  if $SUDO $TARGETCLI_BIN /iscsi/"${iqn}"/tpg1/acls ls | grep -q " ${init_iqn} "; then
    log "[INFO] targetcli: removing existing ACL for ${init_iqn}"
    $SUDO $TARGETCLI_BIN /iscsi/"${iqn}"/tpg1/acls delete "${init_iqn}" >/dev/null || true
  fi
  log "[INFO] targetcli: creating ACL for initiator ${init_iqn}"
  $SUDO $TARGETCLI_BIN /iscsi/"${iqn}"/tpg1/acls create "${init_iqn}" >/dev/null
}

save_target_config() {
  log "[INFO] targetcli: saving config"
  $SUDO $TARGETCLI_BIN saveconfig >/dev/null || true
}

trap 'die "Unhandled error (see log)."' ERR
main "$@"
