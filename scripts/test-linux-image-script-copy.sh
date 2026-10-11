#!/usr/bin/env bash
set -euo pipefail
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
export ORIGINAL_SCRIPT="$work/builder.sh" ISO=test.iso VOLUME=test CHECK=0
sed -n '/^if \[\[ ${DISKLESS_IMAGE_WORKER/,/^fi/p' \
  "$(dirname "$0")/create-linux-iscsi-image.sh" > "$ORIGINAL_SCRIPT"
printf "printf 'frozen\\n'\n" >> "$ORIGINAL_SCRIPT"
unshare() {
  printf "printf 'edited\\n'\n" > "$ORIGINAL_SCRIPT"
  shift 3
  "$@"
}
export -f unshare
[[ $(bash "$ORIGINAL_SCRIPT") == frozen ]]
echo 'Running image build survives source edits.'
