#!/usr/bin/env bash
set -Eeuo pipefail
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
ISO_MNT=$work/casper-iso
mkdir -p "$ISO_MNT/casper"
touch "$ISO_MNT/casper/minimal.squashfs"
source_code=$(sed -n '/^DISTRO= PAYLOAD=/,/^mount -o loop,ro "\$PAYLOAD"/{ /^mount -o loop,ro "\$PAYLOAD"/d; p; }' "$(dirname "$0")/create-linux-iscsi-image.sh")
eval "$source_code"
[[ $DISTRO == ubuntu && $PAYLOAD == "$ISO_MNT/casper/minimal.squashfs" ]]
touch "$ISO_MNT/casper/filesystem.squashfs"
eval "$source_code"
[[ $DISTRO == ubuntu && $PAYLOAD == "$ISO_MNT/casper/filesystem.squashfs" ]]
ISO_MNT=$work/pop-iso
mkdir -p "$ISO_MNT/casper_pop-os_24.04_amd64_generic_debug_681"
touch "$ISO_MNT/casper_pop-os_24.04_amd64_generic_debug_681/filesystem.squashfs"
eval "$source_code"
[[ $DISTRO == ubuntu && $PAYLOAD == "$ISO_MNT/casper_pop-os_24.04_amd64_generic_debug_681/filesystem.squashfs" ]]
ISO_MNT=$work/nobara-iso
mkdir -p "$ISO_MNT/LiveOS"
touch "$ISO_MNT/LiveOS/squashfs.img"
eval "$source_code"
[[ $DISTRO == fedora && $PAYLOAD == "$ISO_MNT/LiveOS/squashfs.img" ]]
SRC=$work/nobara-root
mkdir -p "$SRC/etc"
printf 'NAME="Nobara Linux"\nID=nobara\n' > "$SRC/etc/os-release"
validation_code=$(sed -n '/^case \$DISTRO in/,/^esac/{ p; /^esac/q; }' "$(dirname "$0")/create-linux-iscsi-image.sh")
eval "$validation_code"
printf 'NAME="Unknown Linux"\nID=unknown\n' > "$SRC/etc/os-release"
if (eval "$validation_code") 2>/dev/null; then
  echo 'Unknown LiveOS identity was accepted.' >&2
  exit 1
fi
echo 'Ubuntu, Pop!_OS, and Nobara live-root checks passed.'
