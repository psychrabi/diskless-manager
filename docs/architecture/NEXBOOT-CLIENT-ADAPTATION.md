# NexBoot-inspired client boot adaptations

This feature branch extends the existing Rust Diskless Manager. It does **not**
replace the existing transactional LIO/configfs implementation, the targetcli
fallback, or the Windows DISM + offline SYSTEM hive workflow.

## 5. iSCSI targets — existing implementation

The production provider is `SafeIscsiProvisioner`; it selects native LIO
configfs when available, otherwise targetcli. Configfs operations use two locks
and persist through targetcli after a transaction. Existing tests check
idempotency, rollback on failed save, multiple LUNs, wildcard portals and
refusal to remove a target while sessions are active.

Verify on a **throwaway** server/ZVOL:

```bash
sudo modprobe target_core_mod
sudo modprobe iscsi_target_mod
sudo systemctl status target || true
sudo targetcli ls
sudo zfs list -t volume
sudo ss -lnt | grep ':3260'
cargo test --locked --manifest-path src-tauri/Cargo.toml infrastructure::iscsi::
```

Native configfs does not eliminate the need for targetcli to persist state.
This branch deliberately leaves the backend unchanged until LIO integration
tests run against the target Linux kernel and a disposable ZVOL.

## 6. Windows NIC driver / PnP

The existing workflow is the primary boot-critical path:

1. Harvest the complete matching INF/SYS/CAT package and review exact PCI
   hardware IDs and architecture. A CAT file's presence is not itself proof
   the signature is trusted; DISM on Windows is authoritative.
2. Stage that complete package to an elevated Windows servicing worker.
3. Prepare a Windows master WIM offline via DISM, initially `commit=false`.
4. Inspect the `SYSTEM` hive, only arm native Windows NIC services already
   present and bound. Do not invent service keys or add blanket
   `CriticalDeviceDatabase` mappings.
5. After validating the dry run and signed driver, prepare with
   `commit=true` and capture the installed image into the ZFS master.
6. Test the actual NIC with an isolated PXE workstation.

**Important:** this integration does not make a non-bootable Windows volume
bootable merely by installing a driver after Windows reaches the desktop. A
boot-critical NIC driver must be in the image and initialized early enough
for iSCSI root to remain connected when Windows takes over from iPXE.

## 7. Windows network recovery (opt-in)

`scripts/windows/restore-network.ps1` is installed on an already bootable
Windows master. It does not reset/disable the NIC, renew DHCP, flush routes, or
run until Windows has booted. It is therefore **not** a workaround for
`INACCESSIBLE_BOOT_DEVICE`.

The PXE registration listener now also accepts:

```text
GET http://<server>:4237/boot/net-config/<client-mac-without-separators>
```

The request is answered only when the source IP is the enabled client's
recorded reservation, its MAC is registered, and an iSCSI target is configured.
The result contains MAC, reserved IPv4 address, DHCP default gateway and DNS.
Keep TCP 4237 limited to the boot VLAN. This endpoint is not an authenticated
management endpoint and its source-IP binding is only a LAN-level measure; do
not forward it to the public internet.

Copy the script into the master image and install its startup task as admin:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\restore-network.ps1 -Server 192.168.1.250 -Install
Get-ScheduledTask -TaskName DisklessManager-NetworkRestore
```

Test after a clean boot:

```powershell
Get-IscsiSession
Get-IscsiConnection
Get-NetIPConfiguration
Get-NetRoute -DestinationPrefix '0.0.0.0/0'
Get-Content 'C:\ProgramData\Diskless Manager\network-restore.log' -Tail 60
```

A different PXE server address or port can be supplied as `-Server`/`-Port`.
The script requires its current NIC MAC and IP to match the returned record.
On any failure it leaves the boot adapter unchanged and logs no completed
restoration.

Remove the opt-in task if it causes a problem:

```powershell
Unregister-ScheduledTask -TaskName DisklessManager-NetworkRestore -Confirm:$false
```

## 8. Linux image preparation (opt-in)

Use `scripts/linux/prepare-iscsi-root.sh` **inside the gold Linux client**,
not on the diskless storage server. It checks for `open-iscsi` (Debian/Ubuntu)
or `iscsi-initiator-utils` (Fedora/RHEL) and chooses between
`update-initramfs` and `dracut`:

```bash
sudo bash scripts/linux/prepare-iscsi-root.sh --dry-run
sudo bash scripts/linux/prepare-iscsi-root.sh --apply
```

The Debian/Ubuntu path configures `ISCSI_AUTO=true`, updates GRUB kernel
arguments, and regenerates initramfs. Fedora configures dracut's network and
iSCSI modules and kernel arguments. Verify the image's existing root
filesystem identifiers (UUID/PARTUUID), kernel/initramfs paths, NIC modules,
and bootloader. Those distribution-specific details cannot be inferred from
the image's ZFS volume name. Test **one** sacrificial PXE client first.

## Validation and rollout

```bash
bun install --frozen-lockfile
bun run test --run
bun run build
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path tools/windows-servicer/Cargo.toml
```

1. Take database backup and preserve ZFS master snapshots before preparing a
   Windows WIM or Linux root filesystem.
2. Validate Windows 10 and Windows 11 independently, using a known-good
   network driver/firmware pair and cold boot.
3. Confirm Windows retains established iSCSI sessions after the network task.
4. Test Linux at the firmware/iPXE, initramfs, root mount and post-boot stages.
5. Keep application changes on this feature branch pending real hardware tests.

### Not yet implemented

* Automatic Linux ZVOL offline mounts and rebuilds from the server UI.
* Automated Windows startup-task installation through the Windows servicing
  worker. The script is installed explicitly on the master.
* Real physical-client confirmation and timing of NIC handoff on Windows 11.
* An end-to-end boot test for both initramfs families and direct LIO.

These operations require explicit per-image controls, backup/rollback, and
hardware test evidence before they can be enabled for production.
