# Installer, hardware identity, and Windows volume safety

Authorized by the user on 2026-10-25: include the larger NexBoot-inspired installation, hardware-profile/identity cache, and hive recovery features. Implement independently in the existing Rust server/React UI and Windows worker; do not transplant AGPL source.

## Contracts

1. Installer sessions are durable, client-bound application operations using cataloged media and existing per-client storage. Provide explicit supported unattended Windows/Ubuntu/RHEL recipes plus interactive ISO boot. A session records progress/errors/events and binds boot payloads/reporting to its client, media checksum, target IQN and expected disk size. Only validated job-owned installation storage may be partitioned. Detect prerequisites and firmware mismatches before exporting destructive instructions. Failed/cancelled sessions retain recovery information; retries cannot overwrite a different client or assigned image. Existing BIOS/normal boot remain supported; unsupported unattended combinations are explicitly rejected.
2. Hardware observations produce stable profiles from normalized boot NIC hardware IDs, architecture and firmware, keeping MAC/client identity separate. Per-client Windows identity caches are immutable snapshots/revisions keyed by original image revision and observed profile. Capture/apply require offline, unmounted, exclusively owned storage and no active iSCSI sessions. A cache cannot be applied to another client, source revision or changed profile. Use existing image/storage services and mutation locks; retain records and resources when cleanup fails. Expose capture/apply/status through authenticated admin API and UI.
3. Before publishing a saved Windows Super revision, inspect the offline candidate through the existing trusted Windows worker. Refuse hibernated Windows. Use native Windows registry loading to replay supported SYSTEM logs on the candidate, with private backups and clean-header verification; do not implement an unverified Linux binary log replayer. The original working image remains intact. Worker targets are temporary job-owned servicing targets with CHAP and verified release; never mount/export a live client disk. Interrupted safety checks retain the Super recovery record and retry deterministically. Lack of worker/prerequisites produces actionable errors before publication.

## Integration and safety

Handlers remain thin and use the shared API error envelope. Management APIs require admin authentication. Boot-report endpoints are separately scoped using client address and bounded session tokens, never management JWTs in boot payloads. Shared application state injects configuration/backends; no mutable globals. Long commands run asynchronously or inside existing blocking workers while holding mutation locks. Use fake infrastructure and isolated SQLite/files for tests; do not modify live ZFS/iSCSI or client disks.

Existing uncommitted work is preserved. No commit/push/deployment is included beyond the separately requested checkpoint commit. Publish only results verified by the available platform; Windows and firmware/hardware behavior require explicit test machines.

## Supported workflows

- **Interactive ISO boot** (`interactive`): any firmware, any cataloged Windows/Linux/other ISO. The server attaches the ISO to the client target as read-only LUN 2 and boots the media's own menu; no client test session or observed boot result is required, and no unattended answer file is used.
- **Windows unattended** (`windows`): UEFI amd64 only, cataloged Windows ISO containing `sources/install.wim` (install.esd is rejected), and a deployed dedicated WinPE installer (`boot/winpe/` with `wimboot`, `BCD`, `boot.sdi`, `boot-installer.wim`, `installer-v1.json` attesting amd64/PowerShell/StorageWMI). WinPE boots over the network, re-verifies target IQN, iSCSI bus type, exact disk size and UEFI, then partitions the job target and streams `install.wim` over HTTP.
- **Ubuntu unattended** (`ubuntu`): Ubuntu Server 24.04 amd64 live-server ISO. Requires `casper/vmlinuz`, `casper/initrd` and a `.disk/info` matching that release. Direct-installs onto the iSCSI by-path device with GPT/ESP and reports progress; the media ISO is served over the scoped session URL.
- **RHEL unattended** (`rhel`): RHEL 9 x86_64 full DVD (`.treeinfo` version 9, `images/install.img`, `repodata/repomd.xml`). Uses `inst.ks` from the session and restricts partitioning to the single iSCSI by-path disk.
- **Hardware identity**: reboot an assigned client into WinPE inventory (`/api/clients/{id}/inventory-boot`) to record the profile; capture then snapshots the client's current master as an immutable baseline, and apply restores that baseline into a per-client replacement disk while retaining the original for recovery.
- **Windows save safety**: saving a Super session whose revision is Windows provisions a temporary servicing target on the worker, refuses hibernated or dirty-hive candidates unless native log replay succeeds, and only then publishes the snapshot.

A prepared installation session owns the client's boot payload: while a session is `ready`/`booting`/`installing`, PXE boot serves the session script with its scoped token URLs instead of the generic installer menu, and a session that fails validation denies boot rather than falling back. Sessions are revoked on server restart and require an administrator to power the client off and close recovery.

## Remaining limits

- No Windows or firmware behavior is verified here: WinPE media must be built and the elevated Windows worker deployed with the `volume_safety` capability; native SYSTEM replay was exercised only by its header/format unit tests on this platform.
- Unattended installation is UEFI/GPT only. BIOS clients keep normal and interactive boot; an unattended request on BIOS is rejected with an explicit error.
- ISO preparation requires `7z` on the server host for bounded member inspection; no UDF directory/WIM inspection or install.esd support exists.
- Hardware identity covers boot NIC hardware IDs, architecture and firmware. It does not synthesize NIC model configuration from MAC/device-instance data, and a changed profile or source revision invalidates a cache instead of adapting it.
- Verification used fake backends, in-memory SQLite and temporary files. No live ZFS pool, iSCSI target, client disk or Windows registry was touched.
