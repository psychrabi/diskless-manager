# Windows client reporting test procedure (opt-in, Windows 10/11 only)

For local NIC installation and boot preparation before capturing a reference
image, follow [Windows Client preparation](../README.md). This procedure below
tests reporting after an iSCSI boot.

This is a manual hardware procedure. It runs only on a designated disposable
managed test client that boots its image over iSCSI. Never run it against a
production client.

## Prerequisites

- A managed Windows 10/11 client booted from its assigned iSCSI image, with
an active iSCSI session visible on the server.

- `diskless-windows-servicer.exe` installed on the client
(`tools/windows-servicer/install-client.ps1`).

- The client's Boot NIC uses a signed third-party driver.

## Graphical worker application

The worker is also a Windows desktop application. Build it on Windows with:

```
cargo build --release --manifest-path tools\windows-servicer\Cargo.toml
```

Install the resulting `diskless-windows-servicer.exe` with
`install-client.ps1`, then launch the executable without arguments. The GUI
provides fields for the server URL, client ID, report configuration path, and
driver directory, plus the reporter toggles the configuration contract supports
(export driver packages, installation verification, and the disposable
test-client image revision). Buttons cover saving configuration, checking
capabilities, testing server reachability (any HTTP status counts as
reachable), reporting the client, collecting/uploading NIC packages, and
installing local NIC drivers.

The header shows whether the application is running elevated. Installing local
NIC drivers is enabled only when elevated, and "Restart elevated" relaunches
the application through UAC for that purpose. When a driver installation
reports that a reboot is pending, a "Restart now" button reboots the client;
the status area lists each INF package with its outcome (installed, installed
with a reboot required, or no matching device).

The existing command-line protocol is unchanged for server-side SSH servicing:

```
diskless-windows-servicer.exe capabilities
diskless-windows-servicer.exe version
diskless-windows-servicer.exe prepare
diskless-windows-servicer.exe report-client [config] [--retry]
diskless-windows-servicer.exe collect-nic <report-config>
diskless-windows-servicer.exe collect-drivers <dest>
diskless-windows-servicer.exe install-nic-drivers <driver-root>
```

The GUI runs long-running operations on a background thread so the window stays
responsive. Run it elevated when installing drivers or servicing volumes.

## Steps

1. On the client, in an elevated PowerShell session, register the reporter:

   ```
   .\register-client-report.ps1 -ServerUrl http://<server>:8080 -ClientId <client-id>
   ```

1. Run one report and confirm success output (`{"ok": true}`):

   ```
   & "C:\Program Files\Diskless Manager\diskless-windows-servicer.exe" report-client
   ```

1. On the server, confirm the report row exists and carries the Boot NIC
hardware ID:

   ```sql
   SELECT client_id, windows_version, architecture, boot_nic_ids, package_ids
   FROM windows_driver_reports;
   ```

1. Confirm the exported package entered the catalog:

   ```
   curl -H "Authorization: Bearer <token>" \
     http://127.0.0.1:8080/api/pxe/network-drivers | jq '.[].name'
   ```

1. Negative checks (each must fail without changing the catalog):
  - Stop the client's iSCSI session and re-run: exit code 1, server has no
  new report row.
  - Run with an unknown `-ClientId`: exit code 1.
  - `pnputil /export-driver` failure (e.g. remove the driver package first):
  exit code 1 with no successful report.

## CCBoot-style local NIC collection

The client installer creates `C:\ProgramData\Diskless Manager\Drivers`. The
explicit collection command discovers all physical adapters, resolves their
third-party published `oem*.inf` packages, exports each package with
`pnputil /export-driver`, and uploads the resulting archive through the same
session-bound report endpoint:

```
& "C:\Program Files\Diskless Manager\diskless-windows-servicer.exe" `
  collect-nic `
  "C:\ProgramData\Diskless Manager\client-report.json"
```

This is the equivalent of CCBoot NIC PNP's **Collect Local NIC** operation.
The server validates the archive, extracts and inspects the INF metadata, and
places the package in the managed NIC catalog. The package can then be selected
by Windows image preparation and injected into a disposable image revision.

## Install collected/known NIC drivers into a Windows OS

To install packages that have already been copied into the driver's workspace,
run the helper elevated:

```
& "C:\Program Files\Diskless Manager\diskless-windows-servicer.exe" `
  install-nic-drivers `
  "C:\ProgramData\Diskless Manager\Drivers"
```

The command recursively finds INF files and calls `pnputil /add-driver <inf> /install` for each package. An exit code of 0 means the package installed
for a present device, 3010 means it installed and a reboot is required to use
it, and 259 means the package joined the driver store but no present device
matched it (or a better driver is already bound) — reported as
`no_matching_device` without failing the run, matching
`assets/auto-adapt.ps1`. The JSON result summarizes the per-package outcomes:

```json
{"ok": true, "operation": "install-nic-drivers", "inf_count": 2,
 "installed": 1, "no_matching_device": 1, "reboot_required": true,
 "packages": [{"inf": "oem10.inf", "outcome": "installed_reboot_required"}]}
```

This is the local equivalent of applying known
PNP NIC drivers to the running OS. For image-wide deployment, use the server's
managed Windows preparation flow instead; it performs offline injection and
boot arming before publication.

## Capture every third-party driver package

`collect-nic` uploads only NIC packages. To stage a complete driver store,
run the helper elevated:

```
& "C:\Program Files\Diskless Manager\diskless-windows-servicer.exe" `
  collect-drivers `
  "C:\ProgramData\Diskless Manager\Drivers"
```

`pnputil /export-driver *` writes one subdirectory per package under the
destination, and the helper reports the exported package count. The worker GUI
offers the same operation as "Export all driver packages" into its configured
driver directory. The server's NIC catalog only accepts NIC packages, so a
full capture stays on disk for an administrator to stage or archive; it is
never uploaded through the session-bound report endpoint.

## Diagnostics

Every client-side operation appends a timestamped line to `logs\client.log`
next to the reporter configuration (by default
`C:\ProgramData\Diskless Manager\logs\client.log`); local driver installs log
next to the driver directory they were given. The file rotates to
`client.log.1` once it exceeds 1 MiB, and a failed log write never blocks the
operation itself. Failed GUI operations print the log file that carries the
details.

The boot-time `report-service` also records start, success, and failure events
in the Windows Application event log under the `DisklessManagerReporter`
source. `install-client.ps1` registers that source so the events render their
message text; without registration the events still appear with the raw text
in the event detail view.

## Notes

- The per-client config holds only the server URL and client ID. No shared
credential is embedded in the image; the server binds the report to the
live iSCSI session peer address.

- Exported archives travel in the request body only and are never logged.

- Driver installation is deliberately explicit and elevated. The client does
not download arbitrary packages from the server without an administrator
first staging them in the local driver directory or selecting them through
managed image preparation. For the same reason the helper has no self-update:
the deployed version is visible in the `capabilities` JSON (`version` field)
and via `diskless-windows-servicer.exe version`, and refreshing a deployment
means re-running `install-client.ps1` with a new build.

- The helper and installer target x64 Windows clients; ARM64 clients are not
covered by this procedure. The driver catalog itself stores packages per
architecture (the reporter normalizes `ARM64` to `arm64`), but no ARM64
helper build is produced, so do not expect this manual flow to work on an
ARM64 reference machine.
