# Windows Network Driver Harvesting and Boot Arming

This branch separates Windows driver package handling from boot-time registry arming.

## Goals

- Parse complete Windows INF packages instead of guessing driver names by vendor.
- Match client hardware deterministically using INF hardware IDs and architecture.
- Install drivers through DISM as complete packages.
- Arm registered services and reflect missing selected kernel services from verified DISM-staged INF packages.
- Keep invasive compatibility tweaks opt-in.
- Never guess vendor services or use loose uploaded SYS files.
- Write CriticalDeviceDatabase entries only for explicit, inventoried Boot NICs.

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

Imported network-driver packages persist the harvested metadata in both `index.json` and schema-version-2 `manifest.json`. The catalog also keeps the INF-associated function-driver services separately from filter services.

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
- leaves `MSiSCSI` unchanged apart from normalizing a malformed `ImagePath`
- writes opt-in CriticalDeviceDatabase entries for each configured Boot NIC
- raises `disk\TimeOutValue` to 60 seconds by default

The armer does not:

- fabricate Realtek or Intel services
- choose a driver by vendor name
- replace differing existing driver binaries
- write CriticalDeviceDatabase entries unless explicit `critical_device_boot_nic(s)` entries are supplied
- rewrite GroupOrderList
- apply NIC-vendor PHY settings
- delete SYSTEM.LOG1 / SYSTEM.LOG2

Selection uses complete packages and Windows/DISM still installs them. The INF parser
does not execute `Include/Needs`; network packages whose associated service cannot be
resolved are rejected before mounting rather than reported ready. Missing services with unsupported service directives or differing architecture/OS profiles are rejected. Dirid 13 services load from the staged DriverStore package; dirid 12 services copy only the verified staged binary and refuse a differing destination file. A registered,
armed service is not a substitute for a boot test on the new hardware.
CDDB entries are planned only when the selected package resolves to one associated NIC
service. If a catalog package flattens multiple associated services without a
device-specific mapping, the armer does not guess; strict binding validation still
requires an existing `Enum`/CDDB mapping or fails the image preparation.
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

Assignments enable `DisableTaskOffload`. The NIC stability profile is applied
only to a Realtek 8125 service explicitly mapped from its PCI hardware ID. A
CDDB-started NIC has no adapter instance, so Realtek 8125 parameters fall back to the NIC service's
`Parameters` key during early boot; the stability profile mirrors a
known-good locally-bound instance and disables the power-saving and
offload defaults that drop iSCSI boot sessions mid-boot:

- `*EEE`, `AdvancedEEE`, `EnableGreenEthernet`, `PowerSavingMode`
- `*InterruptModeration`, `*SelectiveSuspend`
- `*LsoV2IPv4`, `*LsoV2IPv6`, `*TCPChecksumOffloadIPv4`, `*TCPChecksumOffloadIPv6`

Revision identity includes an arm-policy digest
(`boot_arm_policy_digest`) in addition to the driver packages and Boot NIC
scope. Assignment preparation includes all inventoried Windows clients in one
shared revision, so clients with different adapters can reuse the same prepared
snapshot.

For NexBoot WinPE adaptation, the planned NIC groups and package IDs are stored
with the installation session and reused by the adaptation manifest. WinPE
reports the connection belonging to the assigned iSCSI session; the server
checks that the actual adapter is covered before returning the full shared boot
contract. The server records a `preparing` revision before starting WinPE.
After the resulting image passes normal-boot verification, cleanup writes a
durable `serviced-<revision>` snapshot and advances that revision to
`awaiting-test`; later clients with the same source, package, NIC and arm-policy
hash reuse that published snapshot instead of starting WinPE again. Interrupted
sessions that can still resume retain their revision; closed failed sessions
release it for retry.

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
and writes a versioned JSON response to stdout. The current protocol version is
5; it also reports whether the SSH-launched worker process has an elevated
administrator token. The server requires an exact protocol-version match, so a
stale helper is rejected instead of silently ignoring new boot-arm fields such
as multi-adapter Boot NIC bindings. The UI will not enable image preparation unless
the worker tools are available and the remote session is elevated.

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

The optional in-Windows startup adaptation stages drivers with `pnputil`; it
does not create CriticalDeviceDatabase entries, change MSiSCSI startup, or
require a service for hardware that is not present. This is separate from
assignment servicing, which prepares and arms the image offline before the
client boots it. Boot-critical NICs must be prepared offline and verified
separately.

The startup-registration cleanup and client workspace ACL have isolated
Windows PowerShell regression tests. Run these on an elevated Windows test
machine; they use temporary directories and do not attach storage:

```powershell
cargo test --manifest-path tools/windows-servicer/Cargo.toml startup_cleanup_preserves_other_policy_entries_and_retries_on_failure
cargo test --manifest-path tools/windows-servicer/Cargo.toml client_workspace_removes_existing_explicit_grants -- --ignored
```

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

### Windows 11 iSCSI boot validation (2026-11-04)

A disposable clone of `windows11-netkvm` was booted locally once on a SATA
virtio VM so Windows created the PCI NIC instance, then serviced from its own
SYSTEM shell:

- installed the complete NetKVM package (`netkvm.inf`, `.cat`, `.sys` and
  `netkvmp.exe`) with `pnputil /add-driver ... /install`; the resulting class
  instance was `{4d36e972-e325-11ce-bfc1-08002be10318}\0014` with service
  `netkvm`;
- set `netkvm` and `iScsiPrt` to `Start=0` with `BootFlags=1`;
- snapshotted the clone, imported it, assigned it to the client, and recreated
  its writeback and iSCSI target.

The clone then booted Windows through iPXE `sanboot` to the sign-in screen and
desktop; the target showed a `LOGGED_IN` session and the guest answered on
`192.168.123.10`. The local boot plus `pnputil` mattered because an earlier
hand-copied DriverStore package omitted `netkvmp.exe`; a driver package is only
complete when every file referenced by its INF is present.

That local-boot sequence is a recovery/diagnostic path, not the supported
automation. The supported path performs the same install and arm operations
through the Windows servicer:

1. Import the complete network driver package and record the client's Boot NIC
   inventory.
2. Open Verified Image Factory, choose **Adapt existing Windows image**, and
   select a ready Windows master, snapshot and dedicated WinPE media.
3. The server creates a private clone; WinPE mounts it, selects the catalog
   driver bundle, and runs `diskless-windows-servicer.exe adapt-volume`.
4. The servicer runs DISM `Add-Driver` and arms the selected NIC services and
   `iScsiPrt`; the server saves, verifies a normal SAN boot, then publishes and
   optionally assigns the image.
5. The installer downloads the current servicer from
   `tools/windows-servicer/diskless-windows-servicer.exe` and verifies its
   size and SHA-256, so a WinPE image with a stale baked helper is not used.

No manual `pnputil`, registry edit or local boot is part of the supported
path. A real SAN boot test remains the acceptance gate.

## Assignment-driven Boot NIC servicing

Assigning a Windows image to a client is a boot-preparation operation, not a
driver-file coverage check. The server installs the selected driver packages
and arms all known Boot NICs unless a revision built for the same source
snapshot, package set, installation strategy, and Boot NIC inventory already
exists.

The assignment flow:

1. Loads all known Windows Boot NICs, including pending WinPE inventory, with
   verified Windows reports taking precedence over stale pre-boot rows.
2. Selects complete catalog packages for every adapter by exact then compatible
   PNP ID. Preparation fails when any inventoried adapter lacks a matching
   package or declared boot-start service.
3. Builds one CriticalDeviceDatabase mapping per adapter, preferring subsystem
   IDs and using vendor/device IDs only when no subsystem was reported.
   Conflicting mappings for the same hardware ID fail safely.
4. Enables `require_boot_nic_binding` for every adapter group. Servicing fails
   instead of publishing a candidate with one NIC mapped and another omitted.
5. Hashes the sorted package IDs, installation strategy, normalized NIC set and
   arm policy. Clients with the same inventory reuse one prepared revision; a
   newly inventoried NIC expands the scope and creates a new revision.
6. `offline_dism` installs the union of drivers and bindings into one candidate
   image. `nexboot_boot_time` adapts a client-bound copy through WinPE; that
   path now sends the iSCSI adapter's hardware IDs, installs its matching
   package and applies the required boot binding before Windows starts.
7. Existing CriticalDeviceDatabase coverage counts only when its Service value
   names a registered kernel NIC driver and its ClassGUID is the network class.

Published driver files alone do not make a NIC boot-ready. The shared revision
must bind every inventoried device ID to its installed boot-start service.

### Revision reuse

Revisions are reusable per `(source image, source snapshot, package set,
strategy, Boot NIC ID set)`. Normalization uppercases and sorts the union of
known IDs, so inventory ordering does not create duplicate revisions. A newly
inventoried NIC expands the shared scope; already assigned clients remain on an
older revision until explicitly reassigned.

### Remaining limits

- The `offline_dism` path requires a configured Windows servicing worker.
  Without one, assignment fails before touching the image.
- CriticalDeviceDatabase mappings start the selected NIC service before
  disk-based PnP. They do not clone NexBoot's full per-adapter `Enum`, network
  class, GUID, MAC and TCP/IP registry identity; Windows still creates and
  personalizes those records through PnP after the iSCSI root is available.
- The read-only `windows_driver_coverage` probe remains available for
  diagnostics but is not consulted during assignment and does not influence
  revision reuse.
- A successful assignment only proves that packages installed and planned
  registry values were written. Every new hardware family still needs a real
  SAN boot test on the target machine; the Windows 11 VM results above still
  apply and a real SAN boot test remains the acceptance gate.

### Linux SYSTEM hive adapter (excluded)

`windows_linux_registry.rs` is a blocking adapter over the external
`hivexregedit` tool. It exports a hive into a typed snapshot, stages requested
values into a private copy, verifies every value after the merge, and only then
atomically replaces the hive while preserving permission bits. It refuses a
non-REGF, dirty, symlinked or oversized hive, and a failed merge or verification
leaves the original bytes untouched.

The handoff prohibits Linux `hivexregedit` registry changes. The unused adapter
is excluded from the build and must not be wired into servicing. Boot arming
continues through the trusted Windows worker or WinPE workflow.

| Requirement or changed public output | Check and observed result | Limit |
| --- | --- | --- |
| Dashboard counts only public masters | `dashboard_http_count_matches_public_image_library` calls the authenticated `/api/system/status` route with public, private and servicing records; the count is two. | Real migrated SQLite with isolated test records. `/api/masters` needs ZFS and is not exercised by this test. |
| Existing-driver assignments must not be gated on worker settings | `checks existing image drivers in NexBoot mode without a Windows worker` edits a MAC and submits the real React form with only the NexBoot strategy configured. It observes preparation requested, original assignment preserved, no fallback confirmation and the successful update. It failed before the UI gate change and passed after. | API responses are mocked at the frontend boundary. End-to-end worker avoidance on the actual Windows image is not established. |
| Reusable revisions avoid a new worker capability check | Existing SQLite revision tests prove package-set coverage and frontend tests prove prepared revision reuse preserves the returned image. The worker-profile gate was moved after these reuse returns. | A complete assignment request traversing this backend reuse path has not been run. The gate ordering is inspection evidence only. |
| Missing-driver error offers WinPE or worker configuration | The handler now explains that no confirmed coverage or reusable revision exists and offers a new WinPE-prepared snapshot or worker configuration. | Reaching this output requires the native probe/catalog path, which was not exercised end to end. Message delivery is unverified, and missing drivers do not yet get automatic worker-free adaptation. |
| Coverage is not a boot-success claim | The same form test checks the success toast explicitly requires a real SAN boot test. `explains coverage-first preparation when NexBoot mode has no worker` checks the rendered guidance and missing-driver requirement. | No Windows boot was performed. |
| Unpublished sources cannot bypass preparation checks | `assignment_rejects_unpublished_sources_before_driver_inspection` calls the real authenticated assignment route. Ready clones, draft/verifying masters and a draft snapshot all return HTTP 400 with the expected shared error message before storage inspection. | Does not exercise a successful privileged probe. |
| Probe must fail conservatively | Filesystem regressions accept published matching packages/inbox drivers and reject missing publication, missing binaries, wrong IDs, wrong or empty architecture. | Isolated synthetic files, not a Windows DriverStore captured from a running client. |
| Temporary storage cleanup must preserve safety | Fake-backend regressions observe cleanup after settle/probe failure, preservation of both primary and cleanup errors, and no destruction when mount inspection refuses release. Failed unmount code retains its mount directory rather than recursively deleting it. | No real mounts or ZFS mutations in tests. Failed-unmount retention still needs privileged integration validation. |
| No boot-critical install deferred until ordinary startup | This fast path performs no installation. Missing-driver fallback still uses existing offline servicing. | Full worker-free WinPE preparation is not implemented. |
| Clean-room boundary | Changes use independent Rust filesystem/storage code and existing project helpers. No upstream source was copied into these edits. The pinned upstream behavioral review remains the reference. | This records implementation provenance, not a legal compatibility determination. |
| Leave changes uncommitted and undeployed | HEAD remains `c620a4d`. Installed `/usr/bin/diskless-manager` still has its November 3, 2026 15:30:11 +05:45 timestamp. | Built `dist/` is not evidence of deployment. |

The real-runtime acceptance attempt was read-only: the live
`GET /api/system/status` returned HTTP 401 with the shared unauthorized JSON
contract, and no authenticated Diskless Manager browser tab was available.
The process is UID 1000 without effective `CAP_SYS_ADMIN`, the live database
is not readable, and `virsh --connect qemu:///system domstate windows-worker`
reported `shut off`. Starting that VM, modifying live storage or deploying a
new binary was not attempted. Thus actual Windows assignment, native NTFS
inspection and SAN boot acceptance remain blocked/unverified, rather than
being inferred from the passing software tests.

After adding the HTTP comparison and UI-output assertions, final checks passed:
Rust formatting, all-target/all-feature clippy with warnings denied, 740 unit
tests, 14 integration tests and 9 doctests; frontend lint, 203 tests and the
production build. Three Rust tests remain environment-gated: real LIO,
Windows PowerShell and ISC dhcpd validation. React test `act` warnings remain.
The Windows helper was not changed or separately rebuilt. These results
support the specific rows above, not completion of the missing native/WinPE
workflow or a claim that the user's Windows client now boots successfully.
