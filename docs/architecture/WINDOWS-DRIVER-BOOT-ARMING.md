# Windows Network Driver Harvesting and Boot Arming

This branch separates Windows driver package handling from boot-time registry arming.

## Goals

- Parse complete Windows INF packages instead of guessing driver names by vendor.
- Match client hardware deterministically using INF hardware IDs and architecture.
- Install drivers through DISM as complete packages.
- Arm only Windows services that already exist in the target SYSTEM hive.
- Keep invasive compatibility tweaks opt-in.
- Never create broad vendor NIC service keys, never copy standalone SYS binaries, and never inject CriticalDeviceDatabase entries.

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
- verifies service keys already exist before changing them
- sets native NIC driver `Start=0`
- sets `BootFlags=1` only when the service is a driver service
- arms existing `iScsiPrt` when present
- sets existing `Tcpip` and `ndis` to boot-start
- leaves `MSiSCSI` unchanged
- raises `disk\TimeOutValue` to 60 seconds by default

The armer does not:

- fabricate Realtek or Intel services
- choose a driver by vendor name
- replace driver binaries
- write CriticalDeviceDatabase entries
- rewrite GroupOrderList
- apply NIC-vendor PHY settings
- delete SYSTEM.LOG1 / SYSTEM.LOG2

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
