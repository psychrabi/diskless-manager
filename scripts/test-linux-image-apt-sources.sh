#!/usr/bin/env bash
set -Eeuo pipefail

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
ROOT_MNT=$TMP
mkdir -p "$TMP/etc/apt/sources.list.d"
cat > "$TMP/etc/apt/sources.list" <<'LIST'
deb file:/cdrom resolute main restricted
deb cdrom:[Kubuntu]/ resolute main
deb https://archive.ubuntu.com/ubuntu resolute main universe
# deb file:/cdrom resolute main
LIST
cat > "$TMP/etc/apt/sources.list.d/ubuntu.sources" <<'SOURCES'
Types: deb
URIs: file:/cdrom
Suites: resolute
Components: main
Enabled: yes

Types: deb
URIs: https://archive.ubuntu.com/ubuntu
Suites: resolute
Components: main universe
Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg
SOURCES
cp "$TMP/etc/apt/sources.list.d/ubuntu.sources" "$TMP/etc/apt/sources.list.d/ubuntu.sources.before"
# Exercise the builder's Ubuntu preparation without mounts or package installation.
eval "$(sed -n '/^  ubuntu)$/,/^    chroot .* apt-get update/{ /^  ubuntu)$/d; /chroot .* apt-get update/d; p; }' "$(dirname "$0")/create-linux-iscsi-image.sh")"
! grep -Eq '^deb (file:/cdrom|cdrom:)' "$TMP/etc/apt/sources.list"
grep -Fxq 'deb https://archive.ubuntu.com/ubuntu resolute main universe' "$TMP/etc/apt/sources.list"
awk 'BEGIN { RS="" } /URIs: file:\/cdrom/ { if ($0 !~ /Enabled: no/ || $0 ~ /Enabled: yes/) exit 1; found=1 } END { if (!found) exit 1 }' "$TMP/etc/apt/sources.list.d/ubuntu.sources"
diff <(awk 'BEGIN { RS="" } /URIs: https:/ { print }' "$TMP/etc/apt/sources.list.d/ubuntu.sources.before") <(awk 'BEGIN { RS="" } /URIs: https:/ { print }' "$TMP/etc/apt/sources.list.d/ubuntu.sources")
echo 'APT source regression checks passed.'
