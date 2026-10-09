#!/usr/bin/env bash
set -Eeuo pipefail

usage() {
  cat <<'USAGE'
Create a bootable Linux iSCSI master image from a Fedora KDE/Workstation,
Kubuntu, or CachyOS live ISO and an empty ZFS volume.

Usage:
  sudo bash scripts/create-linux-iscsi-image.sh --iso FILE.iso --volume POOL/VOLUME [--check]

The volume is destructively repartitioned and formatted. It must be an empty,
standalone ZFS volume (no origin, snapshots, mounts, holders, or iSCSI export).
SteamOS recovery media is unsupported. Secure Boot must be disabled on clients.
Default login: diskless / diskless123 (change the password after first login).
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
[[ $EUID == 0 ]] || { echo 'Run with sudo.' >&2; exit 1; }
[[ -f $ISO && -n $VOLUME ]] || { usage >&2; exit 2; }
command -v unshare >/dev/null || { echo 'Install util-linux (unshare).'; exit 1; }

# Isolate all temporary mounts from the host mount namespace (notably /run).
if [[ ${DISKLESS_IMAGE_WORKER:-0} != 1 ]]; then
  exec unshare --mount --propagation private env DISKLESS_IMAGE_WORKER=1 \
    bash "$0" --iso "$ISO" --volume "$VOLUME" $([[ $CHECK == 1 ]] && echo --check)
fi
mount --make-rprivate /

for cmd in zfs xorriso mount umount rsync parted partprobe udevadm wipefs \
           lsblk blockdev mkfs.fat mkfs.ext4 blkid chroot; do
  command -v "$cmd" >/dev/null || { echo "Missing required command: $cmd" >&2; exit 1; }
done

DEV=$(zfs get -H -o value volmode "$VOLUME" 2>/dev/null) || { echo "Cannot read ZFS volume $VOLUME." >&2; exit 1; }
[[ $DEV == dev || $DEV == default ]] || { echo "$VOLUME is not an exposed ZFS volume (volmode=$DEV)." >&2; exit 1; }
DEV=$(readlink -f "/dev/zvol/$VOLUME")
[[ -b $DEV ]] || { echo "No block device for $VOLUME." >&2; exit 1; }
TYPE=$(zfs get -H -o value type "$VOLUME") || { echo 'Cannot read target type.' >&2; exit 1; }
[[ $TYPE == volume ]] || { echo 'Target must be a ZFS volume.' >&2; exit 1; }
ORIGIN=$(zfs get -H -o value origin "$VOLUME") || { echo 'Cannot read target origin.' >&2; exit 1; }
[[ $ORIGIN == - ]] || { echo 'Target is a clone; refusing.' >&2; exit 1; }
SNAPSHOTS=$(zfs list -H -t snapshot -o name -r "$VOLUME") || { echo 'Cannot inspect target snapshots.' >&2; exit 1; }
[[ -z $SNAPSHOTS ]] || { echo 'Target has snapshots; refusing.' >&2; exit 1; }
SIGNATURES=$(wipefs -n --noheadings --output TYPE "$DEV") || { echo 'Could not inspect target signatures; refusing.' >&2; exit 1; }
[[ -z $SIGNATURES ]] || { echo 'Target has existing signatures; refusing.' >&2; exit 1; }
MOUNTPOINTS=$(lsblk -nr -o MOUNTPOINTS "$DEV") || { echo 'Could not inspect target mounts; refusing.' >&2; exit 1; }
[[ -z $(tr -d '[:space:]' <<<"$MOUNTPOINTS") ]] || { echo 'Target or a child is mounted; refusing.' >&2; exit 1; }
HOLDERS=$(ls -A "/sys/class/block/$(basename "$DEV")/holders") || { echo 'Could not inspect target holders; refusing.' >&2; exit 1; }
[[ -z $HOLDERS ]] || { echo 'Target has active holders; refusing.' >&2; exit 1; }
for path in /sys/kernel/config/target/core/*/*/udev_path \
            /sys/kernel/config/nvmet/subsystems/*/namespaces/*/device_path; do
  [[ -r $path ]] && [[ $(<"$path") == "$DEV" ]] && {
    echo 'Target is exported through iSCSI/NVMe-oF; refusing.' >&2; exit 1;
  }
done

TMP=$(mktemp -d /tmp/diskless-linux-image.XXXXXX)
ISO_MNT=$TMP/iso
PAYLOAD_MNT=$TMP/payload
ROOT_MNT=$TMP/root
mkdir -p "$ISO_MNT" "$PAYLOAD_MNT" "$ROOT_MNT"
MOUNTS=()
cleanup() {
  local rc=$? p failed=0
  trap - EXIT
  for ((i=${#MOUNTS[@]}-1; i>=0; i--)); do
    p=${MOUNTS[i]}
    if ! umount -R "$p"; then echo "Could not unmount $p; retaining $TMP" >&2; failed=1; fi
  done
  if ((failed == 0)); then
    rm -rf -- "$TMP"
    if ((rc == 0 && CHECK == 0)); then echo "Image ready: $VOLUME (UEFI fallback: EFI/BOOT/BOOTX64.EFI)."; fi
  else rc=1
  fi
  exit "$rc"
}
trap cleanup EXIT

mount -o loop,ro "$ISO" "$ISO_MNT"; MOUNTS+=("$ISO_MNT")
DISTRO= PAYLOAD=
if [[ -f $ISO_MNT/casper/filesystem.squashfs ]]; then
  DISTRO=ubuntu; PAYLOAD=$ISO_MNT/casper/filesystem.squashfs
elif [[ -f $ISO_MNT/LiveOS/squashfs.img ]]; then
  DISTRO=fedora; PAYLOAD=$ISO_MNT/LiveOS/squashfs.img
elif [[ -f $ISO_MNT/arch/x86_64/airootfs.sfs ]]; then
  DISTRO=cachyos; PAYLOAD=$ISO_MNT/arch/x86_64/airootfs.sfs
else
  echo 'Unsupported ISO layout. Expected Kubuntu casper, Fedora LiveOS, or CachyOS live ISO.' >&2
  exit 1
fi
mount -o loop,ro "$PAYLOAD" "$PAYLOAD_MNT"; MOUNTS+=("$PAYLOAD_MNT")
# Fedora's squashfs.img is often EROFS containing rootfs.img; loop autodetection handles it.
SRC=$PAYLOAD_MNT
if [[ -f $SRC/LiveOS/rootfs.img ]]; then
  mkdir "$TMP/fedora-root"
  mount -o loop,ro "$SRC/LiveOS/rootfs.img" "$TMP/fedora-root"; MOUNTS+=("$TMP/fedora-root")
  SRC=$TMP/fedora-root
fi
[[ -f $SRC/etc/os-release ]] || { echo 'Could not find an installed root filesystem in the ISO payload.' >&2; exit 1; }
case $DISTRO in
  ubuntu) grep -qiE 'Ubuntu|Kubuntu' "$SRC/etc/os-release" || { echo 'Unexpected OS in casper payload.' >&2; exit 1; } ;;
  fedora) grep -qi Fedora "$SRC/etc/os-release" || { echo 'Unexpected OS in LiveOS payload.' >&2; exit 1; } ;;
  cachyos) grep -qiE 'CachyOS|Arch Linux' "$SRC/etc/os-release" || { echo 'Unexpected OS in airootfs payload.' >&2; exit 1; } ;;
esac

if ((CHECK)); then
  printf 'Preflight OK: %s payload, target %s (%s bytes).\n' "$DISTRO" "$DEV" "$(blockdev --getsize64 "$DEV")"
  exit 0
fi

SIZE=$(blockdev --getsize64 "$DEV")
[[ $SIZE -ge 21474836480 ]] || { echo 'Target must be at least 20 GiB.' >&2; exit 1; }
parted -s -a optimal "$DEV" mklabel gpt
parted -s -a optimal "$DEV" mkpart ESP fat32 32MiB 1056MiB
parted -s "$DEV" set 1 esp on
parted -s -a optimal "$DEV" mkpart root ext4 1056MiB 100%
partprobe "$DEV"; udevadm settle
EFI=${DEV}p1 ROOT=${DEV}p2
[[ -b $EFI && -b $ROOT ]] || { echo 'Kernel did not expose expected zvol partition devices.' >&2; exit 1; }
mkfs.fat -F 32 -n DISK_EFI "$EFI"
mkfs.ext4 -F -L DISK_ROOT "$ROOT"
mount "$ROOT" "$ROOT_MNT"; MOUNTS+=("$ROOT_MNT")
mkdir -p "$ROOT_MNT/boot/efi"
mount "$EFI" "$ROOT_MNT/boot/efi"; MOUNTS+=("$ROOT_MNT/boot/efi")
rsync -aHAXx --numeric-ids --info=progress2 \
  --exclude='/dev/*' --exclude='/proc/*' --exclude='/sys/*' \
  --exclude='/run/*' --exclude='/tmp/*' --exclude='/boot/efi/***' \
  --exclude='/etc/machine-id' --exclude='/var/lib/dbus/machine-id' \
  --exclude='/etc/ssh/ssh_host_*' "$SRC/" "$ROOT_MNT/"

# Give the image a generic network/iSCSI initramfs and standalone UEFI GRUB.
mkdir -p "$ROOT_MNT/run" "$ROOT_MNT/dev" "$ROOT_MNT/proc" "$ROOT_MNT/sys"
mount -t tmpfs tmpfs "$ROOT_MNT/run"; MOUNTS+=("$ROOT_MNT/run")
mount --rbind /dev "$ROOT_MNT/dev"; mount --make-rslave "$ROOT_MNT/dev"; MOUNTS+=("$ROOT_MNT/dev")
mount -t proc proc "$ROOT_MNT/proc"; MOUNTS+=("$ROOT_MNT/proc")
mount -t sysfs -o ro sysfs "$ROOT_MNT/sys"; MOUNTS+=("$ROOT_MNT/sys")
# Avoid following a live-image resolv.conf symlink into the host's /run.
if [[ -e /etc/resolv.conf ]]; then
  cp -L /etc/resolv.conf "$TMP/resolv.conf"
  rm -f "$ROOT_MNT/etc/resolv.conf"
  install -m 644 "$TMP/resolv.conf" "$ROOT_MNT/etc/resolv.conf"
fi

case $DISTRO in
  fedora)
    chroot "$ROOT_MNT" dnf -y install dracut dracut-network iscsi-initiator-utils grub2-tools-extra grub2-efi-x64-modules NetworkManager openssh-server sudo kernel-core
    ;;
  ubuntu)
    # The copied live root retains media repositories that are unavailable in the chroot.
    for source in "$ROOT_MNT/etc/apt/sources.list" "$ROOT_MNT"/etc/apt/sources.list.d/*.list; do
      [[ -f $source ]] || continue
      sed -i -E '/^[[:space:]]*deb(-src)?[[:space:]].*(file:\/+cdrom|cdrom:)/s/^/# /' "$source"
    done
    for source in "$ROOT_MNT"/etc/apt/sources.list.d/*.sources; do
      [[ -f $source ]] || continue
      awk 'BEGIN { RS=""; ORS="\n\n" }
        /(^|\n)URIs:[^\n]*(file:\/+cdrom|cdrom:)/ {
          gsub(/(^|\n)Enabled:[^\n]*/, "")
          $0 = $0 "\nEnabled: no"
        }
        { print }' "$source" > "$source.tmp"
      cat "$source.tmp" > "$source"
      rm "$source.tmp"
    done
    chroot "$ROOT_MNT" apt-get update
    chroot "$ROOT_MNT" env DEBIAN_FRONTEND=noninteractive apt-get install -y dracut-core dracut-network open-iscsi grub-efi-amd64-bin grub-common network-manager openssh-server sudo linux-image-generic
    ;;
  cachyos)
    chroot "$ROOT_MNT" pacman -Syu --noconfirm dracut dracut-network open-iscsi grub networkmanager openssh sudo linux-cachyos-lts
    ;;
esac

# Create the default login account.
chroot "$ROOT_MNT" useradd --create-home --shell /bin/bash diskless
printf '%s\n' 'diskless:diskless123' | chroot "$ROOT_MNT" chpasswd
printf '%s\n' 'diskless ALL=(ALL:ALL) ALL' > "$ROOT_MNT/etc/sudoers.d/diskless"
chmod 440 "$ROOT_MNT/etc/sudoers.d/diskless"
chroot "$ROOT_MNT" visudo --check

# Find a matching kernel and build an iPXE-iBFT aware initramfs.
MODULES="$ROOT_MNT/usr/lib/modules"
[[ -d $MODULES ]] || MODULES="$ROOT_MNT/lib/modules"
if [[ $DISTRO == cachyos ]]; then
  KVER=$(find "$MODULES" -mindepth 1 -maxdepth 1 -type d -name '*cachyos-lts' -printf '%f\n' | sort -V | tail -n1)
else
  KVER=$(find "$MODULES" -mindepth 1 -maxdepth 1 -type d -printf '%f\n' | sort -V | tail -n1)
fi
[[ -n $KVER ]] || { echo 'No installed kernel modules found.' >&2; exit 1; }
case $DISTRO in
  cachyos) KERNEL=$(find "$ROOT_MNT/boot" -maxdepth 1 -type f -name 'vmlinuz-linux-cachyos-lts' -print -quit) ;;
  *) KERNEL="$ROOT_MNT/boot/vmlinuz-$KVER" ;;
esac
[[ -f $KERNEL ]] || { echo "No kernel image matching $KVER in /boot." >&2; exit 1; }
cp "$KERNEL" "$ROOT_MNT/boot/diskless-vmlinuz"
chroot "$ROOT_MNT" dracut --force --no-hostonly --no-hostonly-cmdline \
  --add 'network iscsi' --add-drivers 'iscsi_tcp iscsi_ibft' \
  "/boot/diskless-initramfs.img" "$KVER"

ROOT_UUID=$(blkid -s UUID -o value "$ROOT")
EFI_UUID=$(blkid -s UUID -o value "$EFI")
printf 'UUID=%s / ext4 defaults 0 1\nUUID=%s /boot/efi vfat defaults,umask=0077 0 2\n' \
  "$ROOT_UUID" "$EFI_UUID" > "$ROOT_MNT/etc/fstab"
mkdir -p "$ROOT_MNT/boot/grub"
cat > "$ROOT_MNT/boot/grub/grub.cfg" <<GRUB
search --no-floppy --fs-uuid --set=root $EFI_UUID
set timeout=3
set default=0
menuentry 'Diskless Linux (iSCSI)' {
  search --no-floppy --fs-uuid --set=root $ROOT_UUID
  linux /boot/diskless-vmlinuz root=UUID=$ROOT_UUID ip=ibft rd.neednet=1 netroot=iscsi rd.iscsi.firmware=1 rw
  initrd /boot/diskless-initramfs.img
}
GRUB
if [[ -x $ROOT_MNT/usr/bin/grub2-mkstandalone ]]; then GRUB_BIN=/usr/bin/grub2-mkstandalone
elif [[ -x $ROOT_MNT/usr/bin/grub-mkstandalone ]]; then GRUB_BIN=/usr/bin/grub-mkstandalone
else echo 'grub-mkstandalone is missing.' >&2; exit 1; fi
mkdir -p "$ROOT_MNT/boot/efi/EFI/BOOT"
chroot "$ROOT_MNT" "$GRUB_BIN" -O x86_64-efi -o /boot/efi/EFI/BOOT/BOOTX64.EFI \
  --modules='part_gpt fat ext2 search search_fs_uuid normal linux' \
  "boot/grub/grub.cfg=/boot/grub/grub.cfg"
if [[ $DISTRO == fedora ]]; then
  mkdir -p "$ROOT_MNT/boot/efi/EFI/fedora"
  cp "$ROOT_MNT/boot/efi/EFI/BOOT/BOOTX64.EFI" "$ROOT_MNT/boot/efi/EFI/fedora/grubx64.efi"
fi

# Clear live-session identity; first boot should create a fresh machine identity.
if [[ $DISTRO == ubuntu ]]; then
  chroot "$ROOT_MNT" systemctl enable ssh.service
else
  chroot "$ROOT_MNT" systemctl enable sshd.service
fi
zfs set org.diskless:os=linux "$VOLUME"
rm -f "$ROOT_MNT/etc/machine-id" "$ROOT_MNT/var/lib/dbus/machine-id" "$ROOT_MNT"/etc/ssh/ssh_host_*
: > "$ROOT_MNT/etc/machine-id"
sync
printf '\nBuild finished for %s; unmounting the temporary filesystems now.\n' "$VOLUME"
