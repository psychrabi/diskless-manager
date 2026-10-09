#!/usr/bin/env bash
# Offline Fedora repair for PC001's clone or an explicitly selected master.
set -Eeuo pipefail

volume=diskless/writeback/PC001-disk
if (($#)); then
  if [[ $# != 2 || $1 != --volume || ! $2 =~ ^diskless/image-disk/[A-Za-z0-9_-]+$ ]]; then
    echo 'Usage: repair-pc001-grub.sh [--volume diskless/image-disk/IMAGE]' >&2
    exit 1
  fi
  volume=$2
fi

[[ $EUID == 0 ]] || { echo 'Run with sudo.' >&2; exit 1; }
for cmd in zfs readlink lsblk blkid ss debugfs mcopy mdir mdel grub2-mkstandalone grub2-script-check grub2-file cmp sync; do
  command -v "$cmd" >/dev/null || { echo "Missing command: $cmd" >&2; exit 1; }
done

[[ $(zfs get -H -o value type "$volume") == volume ]] || exit 1
device=$(readlink -f "/dev/zvol/$volume")
efi=${device}p1
root=${device}p2
[[ -b $efi && -b $root ]] || { echo 'Expected EFI and root partitions are missing.' >&2; exit 1; }
mounts=$(lsblk -nr -o MOUNTPOINTS "$device")
[[ -z ${mounts//[[:space:]]/} ]] || { echo 'Volume is mounted; refusing to edit it.' >&2; exit 1; }
sessions=$(ss -Htn state established '( sport = :3260 )')
[[ -z $sessions ]] || { echo 'Stop iSCSI clients before running this offline repair.' >&2; exit 1; }
[[ $(blkid -s TYPE -o value "$efi") == vfat && $(blkid -s TYPE -o value "$root") == ext4 ]] || {
  echo 'Expected FAT EFI and ext4 root filesystems.' >&2; exit 1;
}
release_path=/etc/os-release
release_info=$(debugfs -R 'stat /etc/os-release' "$root" 2>/dev/null)
# debugfs does not follow Fedora's /etc/os-release symlink.
[[ $release_info != *'Type: symlink'* ]] || release_path=/usr/lib/os-release
os_release=$(debugfs -R "cat $release_path" "$root" 2>/dev/null)
[[ -n $os_release ]] || { echo "Cannot read $release_path from $volume; refusing." >&2; exit 1; }
[[ $'\n'$os_release$'\n' == *$'\nID=fedora\n'* || $'\n'$os_release$'\n' == *$'\nID="fedora"\n'* ]] || {
  echo "$volume is not a Fedora image; refusing." >&2; exit 1;
}
for file in /boot/diskless-vmlinuz /boot/diskless-initramfs.img; do
  info=$(debugfs -R "stat $file" "$root" 2>/dev/null)
  [[ $info == *'Type: regular'* ]] || { echo "Missing boot file: $file" >&2; exit 1; }
done
root_uuid=$(blkid -s UUID -o value "$root")
[[ $root_uuid =~ ^[[:xdigit:]]{8}-[[:xdigit:]]{4}-[[:xdigit:]]{4}-[[:xdigit:]]{4}-[[:xdigit:]]{12}$ ]] || exit 1

backup=$(mktemp -d /var/tmp/diskless-pc001-grub.XXXXXX)
echo "Backup and repair files: $backup"
cat > "$backup/grub.cfg" <<GRUB
set timeout=10
set default=0
menuentry 'Fedora (iSCSI)' {
  search --no-floppy --fs-uuid --set=root $root_uuid
  linux /boot/diskless-vmlinuz root=UUID=$root_uuid ip=ibft rd.neednet=1 netroot=iscsi rd.iscsi.firmware=1 rw
  initrd /boot/diskless-initramfs.img
}
menuentry 'Fedora (iSCSI diagnostic console - Ctrl+Alt+F9)' {
  search --no-floppy --fs-uuid --set=root $root_uuid
  linux /boot/diskless-vmlinuz root=UUID=$root_uuid ip=ibft rd.neednet=1 netroot=iscsi rd.iscsi.firmware=1 rw systemd.debug_shell=1
  initrd /boot/diskless-initramfs.img
}
GRUB
grub2-script-check "$backup/grub.cfg"
grub2-mkstandalone -O x86_64-efi -o "$backup/BOOTX64.EFI.new" \
  --modules='part_gpt fat ext2 search search_fs_uuid normal linux' \
  "boot/grub/grub.cfg=$backup/grub.cfg"
grub2-file --is-x86_64-efi "$backup/BOOTX64.EFI.new"
mcopy -i "$efi" ::/EFI/BOOT/BOOTX64.EFI "$backup/BOOTX64.EFI.original"
# Fedora shim and cached firmware entries use the vendor path.
mdir -i "$efi" ::/EFI/fedora/ >/dev/null
vendor_present=false
if mdir -i "$efi" ::/EFI/fedora/grubx64.efi >/dev/null 2>&1; then
  vendor_present=true
  mcopy -i "$efi" ::/EFI/fedora/grubx64.efi "$backup/grubx64.efi.original"
fi

# Keep the original loader recoverable if writing or read-back verification fails.
restore_loader() {
  local rc=$?
  trap - EXIT
  if ((rc != 0)); then
    echo 'Repair failed; restoring the original loader.' >&2
    if ! mcopy -o -i "$efi" "$backup/BOOTX64.EFI.original" ::/EFI/BOOT/BOOTX64.EFI; then
      echo "RESTORE FAILED: original loader remains at $backup/BOOTX64.EFI.original" >&2
    fi
    if $vendor_present; then
      mcopy -o -i "$efi" "$backup/grubx64.efi.original" ::/EFI/fedora/grubx64.efi || echo "Vendor loader restore failed; backup: $backup" >&2
    else
      mdel -i "$efi" ::/EFI/fedora/grubx64.efi >/dev/null 2>&1 || true
    fi
    sync
  fi
  exit "$rc"
}
trap restore_loader EXIT
mcopy -o -i "$efi" "$backup/BOOTX64.EFI.new" ::/EFI/BOOT/BOOTX64.EFI
sync
mcopy -i "$efi" ::/EFI/BOOT/BOOTX64.EFI "$backup/BOOTX64.EFI.readback"
cmp "$backup/BOOTX64.EFI.new" "$backup/BOOTX64.EFI.readback"
mcopy -o -i "$efi" "$backup/BOOTX64.EFI.new" ::/EFI/fedora/grubx64.efi
sync
mcopy -i "$efi" ::/EFI/fedora/grubx64.efi "$backup/grubx64.efi.readback"
cmp "$backup/BOOTX64.EFI.new" "$backup/grubx64.efi.readback"
echo "GRUB updated on $volume. Select the diagnostic console entry, then press Ctrl+Alt+F9."
