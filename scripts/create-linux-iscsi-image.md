# Build a Linux iSCSI boot image

Use **Images → Linux Image Builder** in Diskless Manager. Upload a structurally
valid Linux ISO through Installer Media, run **Check ISO**, then enter a new
image name and size and confirm the build. The app creates the ZFS image volume,
checks it, builds the boot files, and adds it to the image catalog. It supports
Ubuntu/Kubuntu `casper/filesystem.squashfs`, Ubuntu Desktop
`casper/minimal.squashfs`, Pop!_OS `casper_pop-os_*/filesystem.squashfs`, Fedora
or Nobara `LiveOS/squashfs.img`, and CachyOS `arch/x86_64/airootfs.sfs`.
SteamOS recovery media is not supported. The OS is also checked against the
extracted root filesystem before any target is written.

The builder copies the live root, installs the distro's iSCSI and UEFI GRUB
tools, and creates a generic iBFT-aware initramfs that includes Ethernet
drivers. The boot entry uses the client's firmware iBFT data, so the master
does not contain a client address, NIC identity, or iSCSI target. It enables
NetworkManager and masks systemd-networkd to avoid competing configuration of
the root-disk NIC. SSH host keys are generated before the SSH service starts, so
clients do not inherit the same server identity. Secure Boot must be disabled on
clients.

Choose the first-boot username and password in the builder. The account gets
sudo access by default, and that can be disabled before building. Account
credentials are applied inside the new image and are not saved in the image
catalog. The image is not published in the catalog until the build succeeds.
If a build fails after writing begins, the partial ZFS image remains visible
with a failed status so it can be deleted before retrying. The builder refuses
existing signatures, snapshots, clone origins, mounts, block-device holders,
and iSCSI/NVMe-oF exports.

The CLI entry point remains as an optional thin wrapper around the same worker
for administrators who need to build into an existing empty ZFS volume. It asks
for the first-boot username, password, and whether to grant sudo access. The
password is hidden while typing and is sent to the worker through stdin.

```bash
sudo bash scripts/create-linux-iscsi-image.sh \
  --iso /path/to/kubuntu-live.iso \
  --volume diskless/image-disk/kubuntu
```

Add `--check` to validate the ISO and target without partitioning or formatting
the target. The CLI path does not create an image catalog record.
