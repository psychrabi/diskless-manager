# Windows client reporting test procedure (opt-in, Windows 10/11 only)

This is a manual hardware procedure. It runs only on a designated disposable
managed test client that boots its image over iSCSI. Never run it against a
production client.

## Prerequisites

- A managed Windows 10/11 client booted from its assigned iSCSI image, with
  an active iSCSI session visible on the server.
- `diskless-windows-servicer.exe` installed on the client
  (`tools/windows-servicer/install.ps1`).
- The client's Boot NIC uses a signed third-party driver.

## Steps

1. On the client, in an elevated PowerShell session, register the reporter:

   ```powershell
   .\register-client-report.ps1 -ServerUrl http://<server>:8080 -ClientId <client-id>
   ```

2. Run one report and confirm success output (`{"ok": true}`):

   ```powershell
   & "C:\Program Files\Diskless Manager\diskless-windows-servicer.exe" report-client
   ```

3. On the server, confirm the report row exists and carries the Boot NIC
   hardware ID:

   ```sql
   SELECT client_id, windows_version, architecture, boot_nic_ids, package_ids
   FROM windows_driver_reports;
   ```

4. Confirm the exported package entered the catalog:

   ```sh
   curl -H "Authorization: Bearer <token>" \
     http://127.0.0.1:8080/api/pxe/network-drivers | jq '.[].name'
   ```

5. Negative checks (each must fail without changing the catalog):
   - Stop the client's iSCSI session and re-run: exit code 1, server has no
     new report row.
   - Run with an unknown `-ClientId`: exit code 1.
   - `pnputil /export-driver` failure (e.g. remove the driver package first):
     exit code 1 with no successful report.

## Notes

- The per-client config holds only the server URL and client ID. No shared
  credential is embedded in the image; the server binds the report to the
  live iSCSI session peer address.
- Exported archives travel in the request body only and are never logged.
