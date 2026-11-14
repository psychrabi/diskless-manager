# Diskless Manager Windows Client

A Windows 10/11 client for installing signed Ethernet driver packages and
preparing a reference Windows installation for PXE/iPXE boot over iSCSI.
The same executable still provides the server's SSH servicing protocol.

## Prepare a reference PC

1. Run `diskless-windows-servicer.exe` on the Windows reference PC. Use
   **Restart elevated**, or right-click the executable and choose **Run as
   administrator**. A server connection is not required for local preparation.
2. Extract the NIC vendor's signed driver package into a directory, enter it
   in **Driver directory**, and click **Install local NIC drivers**. Only
   network-class INFs are installed; Windows PnP performs signature checks.
   Reboot first if installation requests it.
3. Click **Refresh boot adapters** and select the wired PCI Ethernet adapter
   that firmware/iPXE will use. USB, Wi-Fi, and virtual adapters are not offered.
4. Click **Preview preparation** to see the intended registry changes.
   Confirm the checkbox and click **Prepare for diskless boot** to apply them.
5. Keep the recovery-journal location shown in Status. Restart from the local
   disk, then shut Windows down fully. Capture/import that disk image using the
   server's existing image workflow. The client does not upload a live system disk.
6. Assign the image to a disposable server client and boot through PXE/iPXE
   with an iBFT iSCSI handoff. Check Windows boots, networking works, and the
   server sees the intended iSCSI session before wider deployment.

For clients with different NICs, collect their complete driver packages and
use the server's managed Windows image preparation to service the shared
image. Staging a package for an absent device with PnP does not establish an
early-boot binding for that hardware. Local preparation covers the selected
installed adapter, and does not claim universal hardware compatibility.

## Changes and recovery

Preparation reuses the server's boot policy planner. It enables early loading
for the selected NIC, iScsiPrt, NDIS, and TCP/IP; preserves existing NIC BootFlags;
adds the selected NIC's critical-device mapping; enables automatic MSiSCSI
service startup; allows a longer disk timeout; and disables Fast Startup.
It keeps Windows firewall/filter settings and driver signature enforcement.
It does not restart or disconnect the network adapter during preparation.

Before writing, the client saves each original registry value and its type to
`%ProgramData%\DisklessManager-BootBackup-<id>\registry-before.clixml`.
Only administrators and SYSTEM have access. An apply failure attempts to restore
all original values; rollback failures include the recovery-journal path.
Driver installation is separate and is not rolled back by this journal.

If the process or PC is interrupted during preparation, preserve that journal
and restore the affected values using their recorded `path`, `name`, `kind`,
`value`, and `existed` fields before retrying. Do not import the whole SYSTEM
hive of another PC. A journal is local recovery data, not an image to deploy.

## Command line

List adapters:

```powershell
.\diskless-windows-servicer.exe list-boot-nics
```

Preview using an `id` returned by that command (JSON escapes backslashes):

```powershell
@{ adapter_id = 'PCI\VEN_8086&DEV_100E\3&example'; apply = $false } |
    ConvertTo-Json -Compress | .\diskless-windows-servicer.exe prepare-client
```

Set `apply = $true` to perform the previewed operation. The client re-discovers
the adapter and rebuilds the plan before applying; it never accepts arbitrary
registry changes from the CLI. Omitting `apply` means preview.

## Build and checks

```powershell
cargo build --release --locked --manifest-path tools\windows-servicer\Cargo.toml
cargo test --locked --manifest-path tools\windows-servicer\Cargo.toml
cargo clippy --locked --manifest-path tools\windows-servicer\Cargo.toml --all-targets --all-features -- -D warnings
```

The release executable is `tools\windows-servicer\target\release\diskless-windows-servicer.exe`.
It can run directly. `install-client.ps1` also installs it and configures
optional reporting to Diskless Manager; see [reporter procedures](client-report/TESTING.md).
The registry planning tests use fake inventories. They do not modify the host.
Real driver installation, registry rollback, GUI behavior, and LAN boot require
the disposable Windows hardware procedure.

## Reference investigation

The workflow was informed by static REA inspection of CCBoot's shipped client
and its published documentation. See [the evidence record](REA-NOTES.md).
The implementation uses this project's own boot policy and Windows tools;
no CCBoot executable, driver, or UI resource is included.
