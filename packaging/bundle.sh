#!/bin/sh
# Package the already-built server and browser UI. Run from the repository root.
set -eu

BINARY=${1:-src-tauri/target/release/diskless-manager}
OUTPUT=${2:-dist/diskless-manager-linux-x86_64.tar.gz}
[ -f "$BINARY" ] || { echo "Missing server binary: $BINARY" >&2; exit 1; }
[ -f dist/index.html ] || { echo "Build the frontend first: bun run build" >&2; exit 1; }
STAGING=$(mktemp -d)
trap 'rm -rf "$STAGING"' EXIT HUP INT TERM
mkdir -p "$STAGING/diskless-manager/dist" "$(dirname "$OUTPUT")"
install -m 0755 "$BINARY" "$STAGING/diskless-manager/diskless-manager"
# Copy only frontend assets, excluding a previous package produced in dist/.
cp dist/index.html "$STAGING/diskless-manager/dist/"
for entry in dist/*; do
    case "$entry" in *.tar.gz|dist/index.html) continue ;; esac
    cp -R "$entry" "$STAGING/diskless-manager/dist/"
done
install -m 0755 packaging/install.sh "$STAGING/diskless-manager/install.sh"
install -m 0644 systemd/diskless-manager.service "$STAGING/diskless-manager/diskless-manager.service"
tar -czf "$OUTPUT" -C "$STAGING" diskless-manager
printf 'Created %s\n' "$OUTPUT"
