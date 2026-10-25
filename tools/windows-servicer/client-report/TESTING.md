# Windows client reporting test procedure (opt-in, Windows 10/11 only)

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
driver directory, plus buttons for saving configuration, checking capabilities,
reporting the client, collecting/uploading NIC packages, and installing local
NIC drivers.

The existing command-line protocol is unchanged for server-side SSH servicing:

```
diskless-windows-servicer.exe capabilities
diskless-windows-servicer.exe prepare
diskless-windows-servicer.exe collect-nic <report-config>
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

The command recursively finds INF files and calls `pnputil /add-driver <inf> /install` for each package. This is the local equivalent of applying known
PNP NIC drivers to the running OS. For image-wide deployment, use the server's
managed Windows preparation flow instead; it performs offline injection and
boot arming before publication.

## Notes

- The per-client config holds only the server URL and client ID. No shared
credential is embedded in the image; the server binds the report to the
live iSCSI session peer address.

- Exported archives travel in the request body only and are never logged.

- Driver installation is deliberately explicit and elevated. The client does
not download arbitrary packages from the server without an administrator
first staging them in the local driver directory or selecting them through
managed image preparation.
