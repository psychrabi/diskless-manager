# Diskless Manager Production Architecture Migration Design

## Status

Approved on 2026-09-02. This design uses `fix/dhcp-production-config` as its baseline. The separate `feature/network-driver-injection` branch remains out of scope for later integration.

## Goal

Make Diskless Manager safe for production-oriented testing while completing its staged migration to one control-plane architecture. Preserve existing SQLite data, configuration, ZFS datasets, iSCSI targets, DHCP reservations, PXE scripts, and persisted resource identities throughout the transition.

## Scope

This migration covers authentication, startup, persistence, configuration, provisioning, infrastructure ownership, image lifecycle, error contracts, compatibility code, deletion proof, and verification. Network-driver upload, selection, validation, and Windows image injection remain out of scope. The resulting interfaces must accept that feature later without another architecture rewrite.

## Established requirements

The full Project Analysis Diskless Manager discussion established these requirements:

1. A master image is not a shared writable client disk. Normal clients receive client-specific writable ZFS storage.
2. ZFS clones originate from snapshots. `ImageKind` and `source_snapshot` remain authoritative image metadata.
3. Existing clients retain their persisted IQNs, dataset names, backstores, block devices, and LUN ownership.
4. `StorageSource::ExistingClientVolume` represents client-owned storage prepared before iSCSI exposure. `StorageSource::ExistingVolume` represents explicitly existing or shared storage. Both remain until replaced deliberately.
5. Rollback removes only resources created by its transaction. It never destroys a master, shared volume, or pre-existing target resource.
6. Destructive repair and deletion stop while an iSCSI session is active.
7. Targetcli's automatic wildcard portal, such as `[::0]:3260`, is valid. Provisioning discovers and reuses a suitable portal instead of creating a conflicting portal.
8. DHCP changes follow render, syntax validation, install, and reload. Failed validation never reloads the service.
9. Generic `autoexec.ipxe` and client-specific scripts have separate roles. Unprovisioned clients can enter WinPE without an early SAN attachment; provisioned clients boot their persisted target.
10. The current DHCP/iPXE production fixes, configured server address, configured HTTP port, reconciliation, metrics, and production verification scripts remain intact.
11. The fresh WinPE `boot.wim` that resolved the read-only symptom remains the testing baseline.
12. Windows iSCSI boot requires boot-capable NDIS NIC drivers. NetAdapterCx selection and multi-NIC driver injection remain later work.

## Target architecture

```text
HTTP handlers / CLI
        |
        v
Application modules
        |
        v
Domain models and policies
        |
        v
Repository and infrastructure interfaces
        |
        +--> SQLite
        +--> ZFS
        +--> LIO / targetcli
        +--> ISC DHCP
        +--> HTTP / TFTP / iPXE files
        +--> systemd and host commands
```

HTTP handlers and the CLI validate transport input, call an application interface, and translate its result. Application modules own complete operations, ordering, idempotency, and rollback. Domain modules own validated models, identity, ownership, and safety policy. Infrastructure adapters exclusively execute host commands or mutate host configuration. Repositories exclusively persist application state.

The design favors deep modules. Callers learn one operation interface instead of the ordering requirements of ZFS, targetcli, DHCP, PXE, and SQLite.

## Migration strategy

Each slice follows `expand -> migrate -> verify -> contract`:

1. Add the new interface beside the existing path.
2. Preserve compatibility readers and existing resource identities.
3. Run old and new representations together where comparison is safe.
4. Verify application state against host state.
5. Switch callers to the new interface.
6. Remove compatibility code only after its deletion gate passes.

Each slice remains independently reversible. No stage implicitly authorizes destructive contraction from a later stage.

## File classification

### Authoritative modules

Retain and build upon:

- `src-tauri/src/application/storage_service.rs`
- `src-tauri/src/application/image_service.rs`
- `src-tauri/src/application/services.rs`
- `src-tauri/src/domain/storage.rs`
- `src-tauri/src/domain/provisioning.rs`
- `src-tauri/src/infrastructure/iscsi/`
- `src-tauri/src/infrastructure/image/`
- implemented modules under `src-tauri/src/infrastructure/zfs/`
- `src-tauri/src/core/provisioning_transaction.rs`
- `src-tauri/src/core/reconciliation.rs`
- `src-tauri/src/core/dhcp_reconciliation.rs`
- `src-tauri/src/core/system_reconciliation.rs`
- `src-tauri/src/ipxe.rs`
- `src-tauri/src/metrics.rs`
- `scripts/migrate-legacy-pxe-autoexec.sh`
- `scripts/verify-production-diskless.sh`
- `src-tauri/script/autoexec.ipxe`

### Behavior to migrate

- Move orchestration from `core/provisioning.rs` into an application provisioning module.
- Finish the client migration from `core/client.rs` into `domain/client.rs` and `ClientRepository`.
- Merge `api/handlers/clients.rs` and `clients_v2.rs` after every operation uses the application interface.
- Move DHCP rendering and host-file access from top-level `dhcp.rs` and `services/dhcp.rs` behind a DHCP infrastructure interface.
- Move iPXE rendering and publication behind a PXE infrastructure interface while preserving current output and tests.
- Consolidate top-level `zfs.rs` into the existing ZFS and image infrastructure modules.
- Move reusable host operations from `commands/system.rs` into application and infrastructure modules.
- Replace `config.rs` plus `core/config.rs` with a typed configuration repository owned by `AppState`.
- Move image ownership out of `core/image.rs` after the repository and application service use one image domain model.
- Replace startup SQL in `state/app_state.rs` with versioned migrations.

### Compatibility modules and data

Retain until their callers and upgrade paths are eliminated:

- top-level `client.rs`, because the CLI calls `add_client_impl`;
- Tauri command wrappers that still satisfy a compiled compatibility interface;
- `settings_legacy` and `zfsPool` readers;
- persisted `block_store`, `block_device`, and `target_iqn` fields;
- `StorageSource::ExistingVolume` and `StorageSource::ExistingClientVolume`.

Compatibility modules receive tests and deprecation notes. Broad `dead_code` allowances do not prove that migration is complete.

### Intentionally staged destinations

Implement each file as the selected destination or consolidate it into a stronger existing module:

- `src-tauri/src/infrastructure/dhcp/mod.rs`
- `src-tauri/src/infrastructure/dhcp/isc_dhcp.rs`
- `src-tauri/src/infrastructure/pxe/mod.rs`
- `src-tauri/src/infrastructure/pxe/ipxe.rs`
- `src-tauri/src/api/handlers/provisioning.rs`
- `src-tauri/src/infrastructure/zfs/executor.rs`
- `src-tauri/src/infrastructure/iscsi/targetcli_transaction_tests.rs`

The targetcli transaction test becomes a real host-gated or ignored integration test. Empty placeholders do not survive final contraction.

### Deletion candidates

Strong candidates:

- unrelated Nepal billing and IRD documentation;
- `src/assets/react.svg`;
- `src/components/RAMUsage.jsx`;
- `package-lock.json` after Bun-only verification.

Conditional candidates:

- abandoned FileIO image modules;
- old Tauri-only log and service wrappers;
- compatibility commands after all callers move;
- empty staged files after a destination decision.

No candidate is deleted until it passes the deletion gate.

## Stage 0: Baseline and safety inventory

Record HTTP routes, authentication policy, CLI commands, direct Rust callers, database schema, configuration fallback order, managed host files, system commands, privilege rules, public Rust interfaces, and current ZFS/iSCSI ownership rules.

Add characterization tests for client lifecycle, image lifecycle, DHCP/PXE output, iSCSI ownership, configuration normalization, and reconciliation. Add representative legacy database fixtures. Extend the production verifier only with read-only checks.

Stage 0 makes no runtime deletion and performs no host mutation.

## Stage 1: Production security and startup

Replace the known `admin/admin123` account with a one-time initialization flow. A fresh installation starts without a usable default credential and permits first-admin creation only while no user exists. Later password changes require authentication, authorization, and current-password validation.

Production startup requires a persistent JWT secret. Development may use an ephemeral secret only in explicit development mode. Protect privileged setup, package installation, service configuration, and host-network endpoints. Restrict production CORS to the desktop origin. Remove unnecessary CSP allowances. Avoid reusable credentials in WebSocket URLs.

Manage Axum as an application task with startup acknowledgement and graceful shutdown. Initialization errors reach the user and logs without panicking the desktop process.

## Stage 2: Versioned persistence and configuration

Move schema evolution into ordered SQLite migration files with recorded history. Enable foreign keys on every connection and test them. Back up the database before an irreversible transition.

Migration fixtures cover legacy image inference, explicit `ImageKind`, `source_snapshot`, legacy datetimes, `settings_legacy`, per-key settings, `zfsPool`, `zpool_name`, and persisted client storage identities.

During expansion, the typed repository and legacy reader normalize to the same value. Once every caller uses `AppState` and comparison tests pass, remove `CONFIG_CACHE` and the legacy writer. Preserve compatibility readers through the upgrade window.

## Stage 3: Authoritative provisioning slice

Expose one application interface for:

```text
create client
update client
reset client
delete client
inspect client
reconcile client
```

The application module coordinates SQLite, ZFS, iSCSI, DHCP, client-specific iPXE, validation, and service reload. HTTP and CLI callers use the same interface.

Normal client creation produces client-specific writable storage. A snapshot source creates a client clone. Existing client-owned storage remains representable during migration. Shared or maintenance storage requires an explicit mode and cannot be mistaken for client-owned storage.

The transaction records each resource it creates. A retry is idempotent. Rollback runs in dependency order, reports cleanup failures separately, and removes only transaction-owned resources. Persisted IQNs remain unchanged. Deletion and destructive reconciliation reject active sessions.

## Stage 4: Infrastructure consolidation

Create infrastructure interfaces for DHCP, PXE, ZFS, iSCSI, and host operations. Run blocking processes on bounded blocking workers or asynchronous process adapters. Keep command construction, validation, timeouts, output limits, logging, and privilege policy inside infrastructure implementations.

Consolidation removes duplicate command execution only after behavior tests prove equivalence.

## Stage 5: Contract and cleanup

Use one structured HTTP error body:

```json
{
  "code": "stable_machine_code",
  "message": "safe user-facing explanation",
  "operation_id": "correlation identifier",
  "details": {}
}
```

Map expected failures consistently:

- `400` invalid transport input;
- `401` or `403` authentication or authorization;
- `404` missing resource;
- `409` ownership, dependency, active-session, or state conflict;
- `422` valid input that violates infrastructure policy;
- `503` unavailable host dependency;
- `500` unexpected internal failure.

Logs retain technical context and the operation identifier without exposing secrets in responses.

### Deletion gate

A file or compatibility path may be removed only when:

1. It has no runtime, test, CLI, build, packaging, migration, or operational references.
2. Its behavior exists behind the authoritative interface.
3. Upgrade fixtures no longer depend on it.
4. Old/new comparison tests pass where both paths can run safely.
5. Rust, frontend, packaging, migration, and production checks pass.
6. A clean build from an empty build cache passes after deletion.

Removal is a separate contraction change, never an incidental migration edit.

## Verification

Each stage runs its relevant checks:

- Rust formatting, compilation, Clippy with warnings denied, and tests;
- frontend lint, unit/component tests, and production build;
- API contract tests;
- legacy database migration tests;
- failure injection after provisioning boundaries;
- reconciliation tests for missing, partial, stale, shared, and connected resources;
- host-gated ZFS and targetcli integration tests;
- clean Debian packaging;
- non-destructive production verification.

Controlled WinPE and Windows iSCSI hardware tests begin only after the software gates pass.

## Rollback

Each stage documents its rollback before implementation. Schema changes retain a pre-migration backup and tested restore procedure. Application rollout keeps compatibility readers through verification. Infrastructure migration preserves managed-file backups and existing resource names. Contraction begins only when forward and rollback verification pass.

## Completion criteria

The migration is complete when:

- HTTP and CLI client operations use one application interface;
- application modules own orchestration and rollback;
- domain models own identity and safety policy;
- infrastructure adapters exclusively mutate the host;
- repositories exclusively persist state;
- configuration has one runtime source of truth;
- SQLite migrations are versioned and tested;
- security and startup blockers are closed;
- frontend tests exist and pass;
- compatibility code has a documented upgrade purpose or has passed the deletion gate;
- the Debian package and non-destructive verifier pass;
- network-driver injection can integrate through the established seams.
