#!/usr/bin/env bash
# Exercise the repair's actual loader writes and rollback on a disposable FAT image.
set -euo pipefail
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
repair=$(dirname "$0")/repair-pc001-grub.sh
for invalid in 'diskless/image-disk/fedora@installed' 'diskless/image-disk/../other' 'diskless/writeback/other'; do
  if bash "$repair" --volume "$invalid" > "$work/args.log" 2>&1; then
    echo 'Expected invalid master argument to fail.' >&2; exit 1
  fi
  [[ $(cat "$work/args.log") == Usage:* ]]
done
sed -n '/^backup=/,$p' "$repair" | sed '1c backup=$TEST_BACKUP' > "$work/writes.sh"
export efi="$work/efi.img" root_uuid=11111111-2222-3333-4444-555555555555 volume=test
truncate -s 64M "$efi"
mkfs.fat "$efi" >/dev/null
mmd -i "$efi" ::/EFI ::/EFI/BOOT ::/EFI/fedora
printf original > "$work/original"
mcopy -i "$efi" "$work/original" ::/EFI/BOOT/BOOTX64.EFI
for state in absent present; do
  export TEST_BACKUP="$work/$state"
  mkdir "$TEST_BACKUP"
  bash -e "$work/writes.sh" >/dev/null
  mcopy -o -i "$efi" ::/EFI/BOOT/BOOTX64.EFI "$work/fallback"
  mcopy -o -i "$efi" ::/EFI/fedora/grubx64.efi "$work/vendor"
  cmp "$work/fallback" "$work/vendor"
  grub2-file --is-x86_64-efi "$work/vendor"
done
real_mcopy=$(command -v mcopy)
mkdir "$work/bin"
cat > "$work/bin/mcopy" <<'WRAPPER'
#!/usr/bin/env bash
if [[ $* == *BOOTX64.EFI.new* && $* == *::/EFI/fedora/grubx64.efi* ]]; then exit 1; fi
exec "$REAL_MCOPY" "$@"
WRAPPER
chmod +x "$work/bin/mcopy"
export REAL_MCOPY="$real_mcopy"
for state in present absent; do
  if [[ $state == absent ]]; then mdel -i "$efi" ::/EFI/fedora/grubx64.efi; fi
  export TEST_BACKUP="$work/failure-$state"
  mkdir "$TEST_BACKUP"
  if PATH="$work/bin:$PATH" bash -e "$work/writes.sh" >/dev/null 2>&1; then
    echo 'Expected vendor write failure.' >&2; exit 1
  fi
  mcopy -o -i "$efi" ::/EFI/BOOT/BOOTX64.EFI "$work/restored"
  cmp "$TEST_BACKUP/BOOTX64.EFI.original" "$work/restored"
  if [[ $state == present ]]; then
    mcopy -o -i "$efi" ::/EFI/fedora/grubx64.efi "$work/restored-vendor"
    cmp "$TEST_BACKUP/grubx64.efi.original" "$work/restored-vendor"
  elif mdir -i "$efi" ::/EFI/fedora/grubx64.efi >/dev/null 2>&1; then
    echo 'Unexpected vendor loader after rollback.' >&2; exit 1
  fi
done
echo 'PASS: both EFI paths match; failed vendor writes restore both original states.'
