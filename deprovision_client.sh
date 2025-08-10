#!/bin/bash
# Deprovision PXE/iSCSI client completely
# Usage: deprovision_client.sh <MAC> [--force] [--keep-zfs]

set -euo pipefail

# Ensure system paths for daemons
export PATH=/usr/sbin:/usr/bin:/sbin:/bin

# --- config ---
ZPOOL="diskless"
IQN_BASE="iqn.2025-04.com.nsboot"
TARGETCLI_BIN="/usr/bin/targetcli"
PXE_DIR="/srv/tftp/pxelinux.cfg"
LOGFILE="/var/log/provision.log"
LOCKDIR="/var/lock/diskless"
# Binaries (use absolute paths for daemon environments)
ZFS_BIN="/usr/sbin/zfs"
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
    logger -t deprovision_client -- "$*" || true
  fi
}

die() { log "[ERROR] $*"; exit 1; }

require_root() { [[ $EUID -eq 0 ]] || die "Run as root."; }
have() { command -v "$1" >/dev/null 2>&1; }

usage() {
  echo "Usage: $0 <MAC> [--force] [--keep-zfs] [--dry-run]"
  echo ""
  echo "Options:"
  echo "  --force     Force deprovisioning even if client is online"
  echo "  --keep-zfs  Keep the ZFS clone (don't destroy it)"
  echo "  --dry-run   Show what would be done without actually doing it"
  echo ""
  echo "Examples:"
  echo "  $0 70:5a:0f:07:6f:75"
  echo "  $0 70:5a:0f:07:6f:75 --force"
  echo "  $0 70:5a:0f:07:6f:75 --keep-zfs --dry-run"
}

main() {
  require_root
  [[ -x $ZFS_BIN ]] || die "zfs not found at $ZFS_BIN"
  [[ -x $FLOCK_BIN ]] || die "flock not found at $FLOCK_BIN"
  [[ -x $TARGETCLI_BIN ]] || die "targetcli not found at $TARGETCLI_BIN"

  [[ $# -ge 1 ]] || { usage; exit 2; }

  # Parse arguments
  local mac_raw="$1"; shift
  local force=false
  local keep_zfs=false
  local dry_run=false

  while [[ $# -gt 0 ]]; do
    case "$1" in
      --force) force=true; shift;;
      --keep-zfs) keep_zfs=true; shift;;
      --dry-run) dry_run=true; shift;;
      -h|--help) usage; exit 0;;
      *) die "Unknown arg: $1";;
    esac
  done

  # Normalize MAC for file names and IQNs
  local mac_lc mac_hy mac_iqn
  mac_lc="$(echo "$mac_raw" | tr '[:upper:]' '[:lower:]')"
  mac_hy="${mac_lc//:/-}"               # 70-5a-0f-07-6f-75
  mac_iqn="${mac_lc//:/-}"              # same hyphenated form

  local client_vol="client-${mac_iqn}"
  local target_iqn="${IQN_BASE}:${mac_iqn}"
  local zvol_path="/dev/zvol/${ZPOOL}/${client_vol}"
  local pxe_file="${PXE_DIR}/01-${mac_hy}"

  umask 022
  mkdir -p "$(dirname "$LOGFILE")" "$LOCKDIR"

  # Per-client lock for deprovisioning
  local client_lock_file="${LOCKDIR}/deprovision_${mac_hy}.lock"
  # Ensure lock directory exists and is writable
  if ! mkdir -p "$(dirname "$client_lock_file")" 2>/dev/null; then
    # Fallback to /tmp if /var/lock is not writable
    LOCKDIR="/tmp/diskless-locks"
    client_lock_file="${LOCKDIR}/deprovision_${mac_hy}.lock"
    mkdir -p "$LOCKDIR" || die "Cannot create lock directory"
  fi
  
  exec 9>"$client_lock_file"
  $FLOCK_BIN -n 9 || die "Another deprovisioning run for ${mac_lc} is in progress"

  log "[INFO] Deprovisioning $mac_lc -> ${ZPOOL}/${client_vol} (${target_iqn})"

  local errors=()
  local warnings=()

  # 1. Check if client is online (unless --force)
  if [[ "$force" != "true" ]]; then
    if check_client_online "$mac_lc"; then
      die "Client $mac_lc appears to be online. Use --force to deprovision anyway."
    fi
  fi

  # 2. Disconnect iSCSI sessions (if any)
  if [[ "$dry_run" != "true" ]]; then
    log "[INFO] Checking for active iSCSI sessions"
    disconnect_iscsi_sessions "$target_iqn" || warnings+=("Failed to disconnect iSCSI sessions")
  else
    log "[DRY-RUN] Would check for active iSCSI sessions"
  fi

  # 3. Remove iSCSI target and backstore
  if [[ "$dry_run" != "true" ]]; then
    log "[INFO] Removing iSCSI target and backstore"
    remove_iscsi_target "$target_iqn" "$client_vol" || errors+=("Failed to remove iSCSI target")
  else
    log "[DRY-RUN] Would remove iSCSI target ${target_iqn} and backstore ${client_vol}"
  fi

  # 4. Remove PXE configuration
  if [[ "$dry_run" != "true" ]]; then
    log "[INFO] Removing PXE configuration"
    remove_pxe_config "$pxe_file" || warnings+=("Failed to remove PXE configuration")
  else
    log "[DRY-RUN] Would remove PXE configuration ${pxe_file}"
  fi

  # 5. Remove DHCP reservation (if exists)
  if [[ "$dry_run" != "true" ]]; then
    log "[INFO] Removing DHCP reservation"
    remove_dhcp_reservation "$mac_lc" || warnings+=("Failed to remove DHCP reservation")
  else
    log "[DRY-RUN] Would remove DHCP reservation for ${mac_lc}"
  fi

  # 6. Destroy ZFS clone (unless --keep-zfs)
  if [[ "$keep_zfs" != "true" ]]; then
    if [[ "$dry_run" != "true" ]]; then
      log "[INFO] Destroying ZFS clone"
      destroy_zfs_clone "$client_vol" || errors+=("Failed to destroy ZFS clone")
    else
      log "[DRY-RUN] Would destroy ZFS clone ${ZPOOL}/${client_vol}"
    fi
  else
    log "[INFO] Keeping ZFS clone ${ZPOOL}/${client_vol} (--keep-zfs specified)"
  fi

  # 7. Clean up lock files
  if [[ "$dry_run" != "true" ]]; then
    cleanup_lock_files "$mac_hy" || warnings+=("Failed to clean up lock files")
  else
    log "[DRY-RUN] Would clean up lock files for ${mac_hy}"
  fi

  # Report results
  if [[ "$dry_run" == "true" ]]; then
    log "[DRY-RUN] Deprovisioning simulation completed for ${mac_lc}"
    exit 0
  fi

  if [[ ${#errors[@]} -gt 0 ]]; then
    log "[ERROR] Deprovisioning failed with errors: ${errors[*]}"
    if [[ ${#warnings[@]} -gt 0 ]]; then
      log "[WARN] Warnings: ${warnings[*]}"
    fi
    exit 1
  fi

  if [[ ${#warnings[@]} -gt 0 ]]; then
    log "[WARN] Deprovisioning completed with warnings: ${warnings[*]}"
  fi

  log "[OK] Successfully deprovisioned ${mac_lc} -> ${client_vol} (${target_iqn})"
}

check_client_online() {
  local mac="$1"
  # Check if client has an active DHCP lease
  if command -v dhcp-lease-list >/dev/null 2>&1; then
    dhcp-lease-list | grep -q "$mac" && return 0
  fi
  
  # Check if client is responding to ping (if we can determine IP)
  local client_ip=""
  if [[ -f "/var/lib/dhcp/dhcpd.leases" ]]; then
    client_ip=$(grep -A 10 "$mac" /var/lib/dhcp/dhcpd.leases | grep "lease" | tail -1 | awk '{print $2}' | tr -d ';')
  fi
  
  if [[ -n "$client_ip" ]]; then
    ping -c 1 -W 2 "$client_ip" >/dev/null 2>&1 && return 0
  fi
  
  return 1
}

disconnect_iscsi_sessions() {
  local target_iqn="$1"
  
  # Get active sessions for this target
  local sessions
  sessions=$($TARGETCLI_BIN /iscsi/"${target_iqn}"/tpg1/acls ls 2>/dev/null | grep -v "No ACLs" | awk '{print $1}' || true)
  
  if [[ -n "$sessions" ]]; then
    log "[INFO] Found active sessions: $sessions"
    # Note: In a real implementation, you might want to gracefully disconnect
    # For now, we just log the sessions
  fi
  
  return 0
}

remove_iscsi_target() {
  local target_iqn="$1"
  local backstore="$2"
  
  # Serialize targetcli mutations
  exec 7>"${LOCKDIR}/targetcli.lock"
  $FLOCK_BIN 7
  
  # Remove LUN first
  if $TARGETCLI_BIN /iscsi/"${target_iqn}"/tpg1/luns ls 2>/dev/null | grep -q "/backstores/block/${backstore}"; then
    log "[INFO] Removing LUN for ${backstore}"
    $TARGETCLI_BIN /iscsi/"${target_iqn}"/tpg1/luns delete 0 >/dev/null 2>&1 || true
  fi
  
  # Remove ACLs
  local acls
  acls=$($TARGETCLI_BIN /iscsi/"${target_iqn}"/tpg1/acls ls 2>/dev/null | grep -v "No ACLs" | awk '{print $1}' || true)
  for acl in $acls; do
    log "[INFO] Removing ACL ${acl}"
    $TARGETCLI_BIN /iscsi/"${target_iqn}"/tpg1/acls delete "${acl}" >/dev/null 2>&1 || true
  done
  
  # Remove target
  if $TARGETCLI_BIN /iscsi ls 2>/dev/null | grep -q " ${target_iqn} "; then
    log "[INFO] Removing iSCSI target ${target_iqn}"
    $TARGETCLI_BIN /iscsi delete "${target_iqn}" >/dev/null 2>&1 || return 1
  fi
  
  # Remove backstore
  if $TARGETCLI_BIN /backstores/block ls 2>/dev/null | grep -q " ${backstore} "; then
    log "[INFO] Removing backstore ${backstore}"
    $TARGETCLI_BIN /backstores/block delete "${backstore}" >/dev/null 2>&1 || return 1
  fi
  
  # Save configuration
  $TARGETCLI_BIN saveconfig >/dev/null 2>&1 || true
  
  $FLOCK_BIN -u 7
  return 0
}

remove_pxe_config() {
  local pxe_file="$1"
  
  if [[ -f "$pxe_file" ]]; then
    log "[INFO] Removing PXE configuration ${pxe_file}"
    rm -f "$pxe_file" || return 1
  else
    log "[INFO] PXE configuration ${pxe_file} does not exist"
  fi
  
  return 0
}

remove_dhcp_reservation() {
  local mac="$1"
  
  # This is a simplified version - in a real implementation, you'd need to
  # parse and modify the DHCP configuration file
  log "[INFO] DHCP reservation removal not implemented (manual cleanup required)"
  log "[INFO] Check /etc/dhcp/dhcpd.conf for host entries with MAC ${mac}"
  
  return 0
}

destroy_zfs_clone() {
  local client_vol="$1"
  local full_path="${ZPOOL}/${client_vol}"
  
  # Check if clone exists
  if $ZFS_BIN list "${full_path}" >/dev/null 2>&1; then
    log "[INFO] Destroying ZFS clone ${full_path}"
    $ZFS_BIN destroy "${full_path}" || return 1
  else
    log "[INFO] ZFS clone ${full_path} does not exist"
  fi
  
  return 0
}

cleanup_lock_files() {
  local mac_hy="$1"
  
  # Remove any leftover lock files for this client
  local lock_pattern="${LOCKDIR}/*${mac_hy}*.lock"
  for lock_file in $lock_pattern; do
    if [[ -f "$lock_file" ]]; then
      log "[INFO] Removing lock file ${lock_file}"
      rm -f "$lock_file" || return 1
    fi
  done
  
  return 0
}

trap 'die "Unhandled error (see log)."' ERR
main "$@"
