# Windows Network Driver Harvesting and Boot Arming

This branch separates Windows driver package handling from boot-time registry arming.

## Goals

- Parse complete Windows INF packages instead of guessing driver names by vendor.
- Match client hardware deterministically using INF hardware IDs and architecture.
- Install drivers through DISM as complete packages.
- Arm registered services and reflect missing selected kernel services from verified DISM-staged INF packages.
- Keep invasive compatibility tweaks opt-in.
- Never guess vendor services or use loose uploaded SYS files; never inject CriticalDeviceDatabase entries.

## Driver harvesting

`windows_inf.rs` parses:

- Class / ClassGuid
- Provider
- DriverVer
- CatalogFile entries
- architecture decorations such as NTamd64, NTx86 and NTarm64
- hardware IDs
- compatible IDs
- AddService declarations
- associated function-driver services (`SPSVCINST_ASSOCSERVICE`, flag `0x2`), distinct from filters
- ServiceBinary declarations

It handles UTF-8, UTF-8 BOM, UTF-16LE and UTF-16BE INF files. Legacy non-UTF-8 files are decoded lossily so structural ASCII tokens remain usable.

Imported network-driver packages persist the harvested metadata in both `index.json` and schema-version-2 `manifest.json`.

## Driver selection

Selection order is deterministic:

1. explicit package selection
2. exact PNP hardware-ID match
3. compatible PNP-ID match
4. MAC match
5. existing service-name match

When the caller supplies an architecture and the package declares architecture support, architecture mismatch rejects the package rather than merely lowering its score.

## Offline boot arming

`windows_boot_arming.rs` uses the native Windows registry loader through `reg.exe load` / `reg.exe unload`.

The armer:

- resolves `Select\Current`
- inspects the Net class GUID `{4d36e972-e325-11ce-bfc1-08002be10318}`
- discovers the NIC services Windows already bound
- also arms function-driver services from the selected network INFs, even without a device binding
- verifies registered services are kernel/filesystem drivers
- reflects absent selected kernel services only from explicit, unambiguous INF service declarations and a byte-matched DISM-staged package
- preserves declared service parameters; refuses unsupported directives, unresolved Include/Needs and conflicting variants
- sets native NIC driver `Start=0`
- adds the network-boot bit to existing `BootFlags` only when the service is a driver service
- arms existing `iScsiPrt` when present
- sets existing `Tcpip` and `ndis` to boot-start
- leaves `MSiSCSI` unchanged
- raises `disk\TimeOutValue` to 60 seconds by default

The armer does not:

- fabricate Realtek or Intel services
- choose a driver by vendor name
- replace differing existing driver binaries
- write CriticalDeviceDatabase entries
- rewrite GroupOrderList
- apply NIC-vendor PHY settings
- delete SYSTEM.LOG1 / SYSTEM.LOG2

Selection uses complete packages and Windows/DISM still installs them. The INF parser
does not execute `Include/Needs`; network packages whose associated service cannot be
resolved are rejected before mounting rather than reported ready. Missing services with unsupported service directives or differing architecture/OS profiles are rejected. Dirid 13 services load from the staged DriverStore package; dirid 12 services copy only the verified staged binary and refuse a differing destination file. A registered,
armed service is not a substitute for a boot test on the new hardware.
The result warns when a selected service has no native Net-class adapter binding
in the offline inventory. An existing binding also does not prove that a new PCI
location or MAC address can initialize before Windows needs the boot disk.

## Dirty hives

The REGF primary and secondary sequence numbers are checked before mutation.

Dirty hives are rejected by default. This prevents the tool from deleting or bypassing Windows registry transaction logs merely to force an offline modification through.

`allow_dirty_hive` exists as an explicit override for controlled recovery/testing workflows.

## Optional compatibility profile

These changes are disabled by default and must be explicitly enabled:

- `DisableTaskOffload=1`
- disabling crash dumps
- clearing `PagingFiles` / `ExistingPageFiles`

They are modeled as compatibility settings rather than universal diskless-boot requirements.

## Servicing environment

Driver installation and SYSTEM hive mutation are currently Windows-servicing operations:

- DISM installs the complete driver package.
- `reg.exe` loads and updates the offline SYSTEM hive.

The Linux server can own orchestration, package storage and client selection, while a Windows/WinPE servicing stage performs authoritative Windows image modification.


## Remote Windows servicing worker

The normal Diskless Manager server runs on Linux, so DISM and the Windows
registry loader are not locally available. The implementation therefore also
builds a dedicated console helper:

`diskless-windows-servicer.exe`

The helper has no HTTP server and opens no listening port. It is invoked locally
or through Windows OpenSSH.

Supported protocol commands:

```text
diskless-windows-servicer.exe capabilities
diskless-windows-servicer.exe prepare
```

`prepare` reads a `WindowsImagePreparationRequest` JSON document from stdin
and writes a versioned JSON response to stdout. Protocol version 2 also reports
whether the SSH-launched worker process has an elevated administrator token.
The UI will not enable image preparation unless the worker tools are available
and the remote session is elevated.

The Linux server invokes the helper through the existing SSH transport. Request
JSON is written to the SSH channel stdin rather than embedded in the command
line. This avoids shell quoting problems and keeps servicing parameters out of
remote process listings.

The helper path defaults to:

```text
C:\Program Files\Diskless Manager\diskless-windows-servicer.exe
```

and can be overridden per request.

SSH host-key verification remains enabled. The servicing host must therefore be
present in the server account's OpenSSH `known_hosts` file before remote
servicing is allowed.

### Remote preparation sequence

```text
Linux Diskless Manager
        |
        | authenticated SSH + verified host key
        v
diskless-windows-servicer.exe
        |
        +-- DISM mount selected WIM index
        |
        +-- install complete INF driver package(s)
        |
        +-- locate Windows/System32/config/SYSTEM
        |
        +-- inspect active ControlSet and Windows NIC bindings
        |
        +-- produce conservative boot-arm plan
        |
        +-- apply plan only when commit=true
        |
        +-- DISM commit
        |
        '-- on any failure: DISM discard
```

With `commit=false`, the WIM is mounted, driver installation is exercised and
the boot-arm plan is generated, but registry changes are not applied and the
entire DISM mount is discarded. This is the recommended validation mode before
modifying a master image.

### Management API

Admin-authenticated endpoints:

```text
GET  /api/pxe/windows/servicing
POST /api/pxe/windows/prepare-image

POST /api/pxe/windows/servicing/remote
POST /api/pxe/windows/prepare-image/remote
```

The local endpoints are useful when Diskless Manager itself is running in a
Windows servicing environment. The remote endpoints are the normal path for a
Linux diskless server controlling a Windows servicing worker.


## Managed driver catalog staging

The preferred remote workflow does not require manually copying a driver folder
to the Windows worker.

The Image Management UI can select one or more packages already imported into
the Diskless Manager network-driver catalog. The server resolves those package
IDs to managed directories under `pxe/network-drivers/drivers`; arbitrary
local filesystem paths are not accepted by this endpoint.

For each operation the server:

1. creates a unique staging root under
   `C:\ProgramData\Diskless Manager\staging\<uuid>\drivers`;
2. uploads only the selected imported package directories through SFTP;
3. rejects symbolic links and non-regular files while staging;
4. runs the normal transactional Windows preparation flow using the staged
   directory as the DISM driver root;
5. removes the unique staging root after success or failure.

The API response reports the number of staged packages, uploaded bytes and
whether staging cleanup completed.

Admin endpoint:

```text
POST /api/pxe/windows/prepare-image/remote/catalog
```

The older `/remote` endpoint remains available for advanced cases where the
driver package directory already exists on the Windows worker.

## Standalone Windows helper build

The Windows utility is intentionally a separate Rust crate so it does not link
the Linux-only ZFS/LIO/configfs portions of the main application:

```text
tools/windows-servicer/
```

Build it on Windows with:

```powershell
cargo build --release --manifest-path tools/windows-servicer/Cargo.toml
```

Output:

```text
tools/windows-servicer/target/release/diskless-windows-servicer.exe
```

Tagged releases build this helper on a native Windows GitHub Actions runner and
attach `diskless-windows-servicer.exe` to the same GitHub release as the Linux
Diskless Manager package.


## Installing a Windows servicing worker

Tagged releases include both:

```text
diskless-windows-servicer.exe
install.ps1
```

Place both files in the same directory on the Windows servicing computer and run
an elevated PowerShell session:

```powershell
Set-ExecutionPolicy -Scope Process Bypass
.\install.ps1
```

This installs the helper to:

```text
C:\Program Files\Diskless Manager\diskless-windows-servicer.exe
```

and creates the managed staging root under:

```text
C:\ProgramData\Diskless Manager\staging
```

If Windows OpenSSH Server is not installed/configured yet, the installer can do
that explicitly:

```powershell
.\install.ps1 -ConfigureOpenSsh
```

That switch installs the Windows OpenSSH Server optional capability when needed,
sets `sshd` to start automatically, starts it, and enables the standard inbound
TCP/22 firewall rule. Without the switch, SSH configuration is left untouched.

The installer must be run as an administrator and finishes by executing the
helper's `capabilities` command. Key-based SSH access from the Linux Diskless
Manager host is preferred for routine servicing; avoid passing an administrator
password through a plain-HTTP management session.


### SSH authentication from a headless server

When the servicing request does not include a password, the backend now tries
authentication in this order:

1. the process SSH agent;
2. `~/.ssh/id_ed25519`;
3. `~/.ssh/id_ecdsa`;
4. `~/.ssh/id_rsa`.

This is designed for the headless/systemd deployment where an interactive SSH
agent is often unavailable. The private key must be readable by the Linux
account running Diskless Manager, and the corresponding public key must be
authorized for the Windows servicing account.

Host-key verification is still mandatory. Add the Windows worker's host key to
the service account's `~/.ssh/known_hosts` before using the UI. Password
authentication remains supported, but key-based authentication is preferred,
especially when the browser management interface is served over plain HTTP.

## Managed image snapshots

Windows Servicing defaults to a managed Windows image and snapshot. Start with a snapshot already known to boot from SAN: NIC servicing prepares drivers and registry services; it does not repair an unrelated EFI/BCD configuration. Supply the SSH worker, a new output image name, the server iSCSI address reachable by the worker, and imported packages or an existing worker driver folder. WIM-file servicing remains available.

The server clones the immutable snapshot, marks the clone preparing, and creates a unique CHAP-protected servicing target. The Windows helper identifies its disk through that exact iSCSI session, refuses boot/system disks, captures both GPT headers and partition tables, temporarily changes a colliding disk ID, mounts only that disk's partitions, and runs offline DISM and registry preparation. Windows may also regenerate partition GUIDs when bringing a colliding clone online. After removing mount paths and offlining the disk, the helper restores the original disk and partition GUIDs, verifies both GPT copies, and disconnects before reporting success. Restoration refuses partition layout changes or invalid checksums. Supported GPT geometry is 512/4096-byte sectors with 128 entries of 128 bytes.

Only after the safe iSCSI provisioner confirms no active sessions and removes the owned target/backstore does the server create the output snapshot and publish the clone as a selectable master image. Logical publication does not ZFS-promote the clone or move source snapshots. Dry run removes the disposable clone after release; source images and client targets remain unchanged.

Failures retain the clone and any remaining servicing target with an explicit error naming them. If worker release fails, its staging lease records disk number, saved GPT identity and target. Inspect that worker and confirm release before deleting retained resources. An HTTP disconnect does not cancel the server's cleanup task.

Offline reflection supports explicit kernel service declarations, repeated service-relative AddReg entries, BootFlags, and the default System event source. EventMessageFile remains REG_EXPAND_SZ and must point inside Windows System32. It refuses a dirty SYSTEM hive before registry loading unless the override is enabled. Filters, unsupported directives/flags and ambiguous variants require installing the NIC in the image first. Every new hardware family still needs a real SAN boot test; successful servicing is not proof of universal hardware compatibility.

### Windows 11 VM validation (2026-10-10)

The serviced clone preserved the source GPT metadata byte-for-byte and booted
through VirtIO. A newly introduced Intel e1000e adapter did not reconnect to
iSCSI during the kernel handoff. Adding Intel as a secondary adapter to the
VirtIO-booted clone let Windows install it natively; that same Intel instance
then reached the Windows sign-in screen with no VirtIO adapter attached.
Moving Intel to a different PCI location and MAC did not reconnect. Reapplying
boot-start settings to the native-installed snapshot and disabling automatic
restart confirmed `INACCESSIBLE_BOOT_DEVICE (0x7B)`, with parameter 2
`0xC0000034` (`STATUS_OBJECT_NAME_NOT_FOUND`). Native installation on one VM is
therefore insufficient evidence of portability to another client. These tests left MSiSCSI unchanged
and did not inject CriticalDeviceDatabase entries.

The resulting `windows11-virtio-intel-verified@native-nic-verified-20261010`
snapshot is a diagnostic baseline for those tested adapter instances. A
target-specific Windows/WinPE adapter preparation stage remains necessary to
investigate first-boot automation; offline package/service preparation alone
has not met the universal-boot goal.
