# Managed Windows boot network drivers

## Goal

Prepare Windows 10/11 images with the network drivers needed by every managed client, so new diskless clients can boot from their assigned iSCSI image without `INACCESSIBLE_BOOT_DEVICE` failures.

## Agreed behavior

1. Manage network adapter drivers only. The Boot NIC is the adapter Windows uses to reach its iSCSI boot disk; its matching driver is required. Other adapters may be included when available but do not block boot.
2. Populate the driver catalog from administrator-imported vendor packages and packages collected from working managed clients.
3. Collect preboot hardware IDs and match packages by device ID, Windows version, and architecture. Do not rely on MAC address alone.
4. Require valid Windows package signatures and compatible INF metadata before a package is eligible for a test revision. Never use DISM's unsigned-driver override.
5. Include the reporting component and all compatible catalog drivers in automatically prepared Windows image revisions. New image revisions are separate from existing assignments; do not rewrite the image a client is already using.
6. A designated test client with matching NIC hardware must boot the exact candidate revision. The test passes only when the client component reports that revision and Boot NIC after Windows starts and the server confirms the client's iSCSI session is active.
7. New clients use the newest tested driver revision. Preserve the last known-good revision for rollback. If no tested driver matches a client's Boot NIC, hold it before Windows boot and show the missing hardware ID.

## Client reporting trust

Reports and harvested package data must be tied to a registered client and its active iSCSI session. Do not accept reports based only on a caller-supplied client ID or MAC address, and do not place a shared credential in a shared image. Keep credentials and package contents out of logs and public client responses.

LIO's generated ACL session list exposes initiator IQNs but not peer addresses. Client targets therefore need an explicit ACL for the deterministic per-client initiator IQN already emitted by iPXE. The server may use that ACL's session information to bind reports to the active peer address. Keep generated ACL support enabled for compatibility with other initiators; do not use an unrelated ACL's address to authenticate a client report.

## Existing code and constraints

- Driver ZIP import and INF metadata live in `src-tauri/src/infrastructure/pxe/driver_injection.rs`; selection lives in `driver_selection.rs` and currently serves PXE/WinPE.
- Managed Windows servicing in `src-tauri/src/application/windows_servicing.rs` clones an image, attaches it to the Windows servicing worker, adds selected drivers, arms boot services, and publishes a clone only on commit.
- Client creation in `src-tauri/src/api/handlers/clients.rs` creates storage and an iSCSI target but does not select Windows drivers or create image revisions.
- `tools/windows-servicer` is a one-shot servicing helper today. No WinPE source image or Windows client reporting service is checked into the repository; add source scripts/helpers and document the Windows ADK build prerequisite without committing a generated WinPE image.
- Preserve both iSCSI target backends, shared safety checks, mutation locks, journaled cleanup, and the HTTP API error contract. Use fake infrastructure for tests. Hardware boot tests are explicit opt-in only.

## Acceptance criteria

- Unsigned, damaged, or OS/architecture-incompatible packages never reach a published revision.
- A report from an unknown client, inactive iSCSI session, or mismatched session peer is rejected without catalog or test-state mutation.
- A package harvested from a working client can enter the same catalog and selection flow as an imported package.
- A new image revision contains the compatible catalog driver set and reporting component; the source image and existing clients remain unchanged.
- A candidate revision cannot become the default for new clients until the matching-hardware boot result is recorded and iSCSI connectivity is confirmed.
- Client provisioning refuses an untested or missing Boot NIC match with an actionable hardware-ID message.
- Unit/integration tests cover catalog validation, report authorization, revision publication/rollback, and boot-test gating. Real hardware tests run only against a designated disposable test client.

## References

- [Microsoft: Add and remove drivers to an offline Windows image](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/add-and-remove-drivers-to-an-offline-windows-image?view=windows-11)
- [Microsoft: PnPUtil command syntax](https://learn.microsoft.com/en-us/windows-hardware/drivers/devtest/pnputil-command-syntax)
- [Microsoft: Catalog files and digital signatures](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/catalog-files)
- [Microsoft: SignTool verification](https://learn.microsoft.com/en-us/windows-hardware/drivers/devtest/signtool)
