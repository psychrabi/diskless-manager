#!/usr/bin/env bash
set -euo pipefail

# Build the experimental NVMe/TCP-capable iPXE firmware used by diskless-manager.
# The normal snponly.efi remains untouched; this produces snponly-nvmeof.efi.
#
# Usage:
#   ./scripts/build-nvmeof-ipxe.sh
#   ./scripts/build-nvmeof-ipxe.sh --install
#
# --install copies the resulting firmware to /srv/tftp/snponly-nvmeof.efi.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_ROOT="${ROOT}/.cache/diskless-nvmeof-firmware"
KURRENT_DIR="${WORK_ROOT}/kurrent-firmware"
OUT_DIR="${ROOT}/dist/nvmeof-firmware"
EMBED_SCRIPT="${ROOT}/src-tauri/script/nvmeof-auto.ipxe"

KURRENT_URL="https://github.com/dutyc/kurrent-firmware.git"
KURRENT_COMMIT="9811a27c213a1e2602eff9c7383a3baf0508ddd6"
INSTALL=0

if [[ "${1:-}" == "--install" ]]; then
  INSTALL=1
elif [[ $# -gt 0 ]]; then
  echo "Usage: $0 [--install]" >&2
  exit 2
fi

for cmd in git make gcc sha256sum; do
  command -v "$cmd" >/dev/null || {
    echo "Missing required command: $cmd" >&2
    exit 1
  }
done

mkdir -p "${WORK_ROOT}" "${OUT_DIR}"

if [[ ! -d "${KURRENT_DIR}/.git" ]]; then
  git clone "${KURRENT_URL}" "${KURRENT_DIR}"
fi

git -C "${KURRENT_DIR}" fetch --depth 1 origin "${KURRENT_COMMIT}"
git -C "${KURRENT_DIR}" checkout --force "${KURRENT_COMMIT}"
git -C "${KURRENT_DIR}" clean -fdxq

# Replace Kurrent's embedded control-plane script with the diskless-manager
# handoff script. The patch chain and upstream iPXE baseline stay pinned by
# Kurrent's build/build.sh at this commit.
cp "${EMBED_SCRIPT}" "${KURRENT_DIR}/embed/auto.ipxe"

(
  cd "${KURRENT_DIR}"
  ./build/build.sh
)

SOURCE="${KURRENT_DIR}/dist/direct-uefi/snponly.efi"
DEST="${OUT_DIR}/snponly-nvmeof.efi"

[[ -f "${SOURCE}" ]] || {
  echo "Expected firmware artifact not found: ${SOURCE}" >&2
  exit 1
}

cp "${SOURCE}" "${DEST}"
sha256sum "${DEST}" > "${DEST}.sha256"

echo "Built: ${DEST}"
cat "${DEST}.sha256"

if [[ ${INSTALL} -eq 1 ]]; then
  echo "Installing to /srv/tftp/snponly-nvmeof.efi"
  sudo install -m 0644 "${DEST}" /srv/tftp/snponly-nvmeof.efi
  echo "Installed /srv/tftp/snponly-nvmeof.efi"
fi
