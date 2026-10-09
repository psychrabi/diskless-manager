# Build a Linux iSCSI boot image

`create-linux-iscsi-image.sh` copies a supported live ISO into an empty ZFS
zvol, installs the distro's iSCSI and UEFI GRUB tools, and creates a generic
iBFT-aware initramfs. Run this on the ZFS server:

```bash
sudo bash scripts/create-linux-iscsi-image.sh \
  --iso /path/to/kubuntu-26.04-desktop-amd64.iso \
  --volume diskless/image-disk/kubuntu
```

Supported layouts are Kubuntu/Ubuntu `casper/filesystem.squashfs`, Fedora KDE
or Workstation Live `LiveOS/squashfs.img`, and CachyOS `arch/x86_64/airootfs.sfs`.
The script identifies the OS from the extracted root filesystem. SteamOS recovery
media is not a supported live-root layout and will be rejected.
For Ubuntu/Kubuntu, installation-media APT sources are disabled in the copied
image before package installation; online repositories remain available.

The builder creates user `diskless` with password `diskless123`, a home directory,
and sudo access requiring that password. Change it after first login with `passwd`.

To inspect the ISO and target without writing to the zvol, add `--check`:

```bash
sudo bash scripts/create-linux-iscsi-image.sh --iso /path/to/live.iso \
  --volume diskless/image-disk/test --check
```

The target must be an empty, standalone ZFS volume of at least 20 GiB. The script
refuses volumes with existing signatures, snapshots, clone origins, mounts, or
block-device holders. Once it passes preflight, it repartitions and formats the
volume; a failed build can leave a partially written volume, which must be
recreated before retrying. It uses an isolated mount namespace and does not
modify the host's mount tree or host bootloader.

After success, take the master snapshot and assign the zvol to a test client. Boot
in UEFI mode with Secure Boot disabled and select the SAN boot entry. The generated
boot entry uses iPXE firmware iBFT data (`ip=ibft`) for the client-specific iSCSI
connection, so no client address or IQN is embedded in the master. Kernel updates
inside the image require rebuilding `/boot/diskless-initramfs.img` for the new
kernel before clients can boot that kernel.
