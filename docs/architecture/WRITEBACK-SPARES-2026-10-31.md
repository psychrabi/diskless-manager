# Writeback spares: Phase 0 baseline and Phase 1 report

Reference: `ggnet_diskless_manager_implementation_handoff.md`, phases 0 and 1.
Design decision: `docs/adr/0006-prewarmed-writeback-spares.md`.

## Phase 0 baseline

Checked out at `feat/verified-image-factory`, commit
`d81c776f1c5657cea846d414d42aa04f6a8e466a`, with two untracked build
artifacts (`diskless-manager`, `diskless-manager.service`) left untouched.

Commands and results before any change:

```sh
cargo test --manifest-path src-tauri/Cargo.toml   # 649 unit, 13 integration, 9 doc tests passed
```

Frontend baseline was established from the same working tree
(`bun run lint`, `bun run test --run`, `bun run build`).

Existing pieces reused instead of reimplemented:

| Need | Existing component |
| --- | --- |
| Durable switch with rollback | `StorageService::replace_offline` / `recover_offline` (`OfflineReplacement`) |
| Offline detection and journal recovery | `client_lifecycle` sweep and `PendingJournal::Replacement` |
| Boot storage readiness | `client_boot::ensure_client_boot_ready` + `ensure_client_storage_for_boot` |
| Per-client serialization | `ClientMutationLocks::{lock, try_lock_client}` |
| Storage identity | `core::reconciliation::storage_spec_for_client`, `lifecycle_fingerprint` |
| CHAP | `StorageService::set_target_chap`, `ensure_chap_credentials` |
| Session truth | `StorageService::target_connected_if_present` (fails closed) |

## Phase 1 changes

- `migrations/0029_client_writeback_spares.sql`: spare records (client,
  dataset, source snapshot, fingerprint, status, timestamps, backoff, error).
- `application/writeback_spare_service.rs`: fingerprinting, eligibility,
  sweep (build / expire / discard), journaled boot adoption, and the
  `CapacityProbe` / `SessionProbe` seams used for testing.
- `StorageService::prepare_spare_clone`, `discard_spare_clone`,
  `spare_is_consumable`, `consume_spare_offline`, `volume_exists`,
  `check_volume_presence`.
- `spare_prewarm` settings section (disabled by default) with server-side
  validation and a settings card.
- `GET /api/system/storage/spares`: admin-only spare status (ready count, age,
  status, last error) for dashboards.
- Boot hook: `client_boot::ensure_client_boot_ready` tries adoption before the
  additive readiness call, which still validates and exposes the result.

## Tests

Automated, all with fake ZFS/iSCSI backends and in-memory SQLite:

- Spare store round trip, unknown-status rejection, `ON DELETE CASCADE`.
- Fingerprint stability and invalidation by source revision, persistence
  choice and game selection.
- Eligibility rules (persistent, disabled, Super, no snapshot, unprovisioned).
- Dataset naming, namespace and master guards.
- Journaled adoption: success, existing boot storage refusal, connected
  client refusal, wrong origin, unreadable query failure, cleanup of
  unverifiable clones.
- Crash boundaries: after the rename, before the rename, and target-creation
  failure after the rename.
- Sweep: one spare per eligible client, capacity floor, TTL expiry, active or
  unknown session, persistent client, live writeback refusal.
- Admin-only status endpoint.
- Settings form defaults, limit editing and GiB-to-bytes conversion.

Full-suite results are recorded in the pull request description.

## Measurements

Not performed. No baseline-vs-spare boot timing was taken, so this change
makes **no performance claim**. The added cost per sweep is one batched
`zfs list` presence query plus one `zpool list` free-space read; adoption
replaces clone construction with a rename plus target reconciliation.

## Safety and rollback

- Disable `storage.spare_prewarm.enabled` to stop new builds; existing spares
  expire at `ttl_hours` (or are removed immediately by the next sweep once
  superseded or stale). Deleting a client cascades its spare row.
- Adoption never runs over an existing boot dataset, a connected target, an
  export, a pending journal, an installer session or an unresolvable session
  state.
- Any adoption failure restores a consistent volume through the journal and
  falls back to the normal boot path; only an unrecoverable journal blocks
  boot, with a recorded boot failure and the standard recovery retry.

## Remaining work

1. Measure cold-boot stages with and without a ready spare on lab hardware
   (Windows 10/11, SATA/NVMe initiator), including a repeat boot.
2. Apply offline Windows servicing to spares (handoff Phase 3) and include the
   serviced revision in the fingerprint.
3. Surface spare status in the dashboard UI and add an alarm for repeated
   build failures or a full ready-spare limit that never drains.
