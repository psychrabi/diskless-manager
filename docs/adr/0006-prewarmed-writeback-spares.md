# Prewarmed writeback spares

## Status

Accepted (implementation behind a disabled-by-default flag)

## Context

ADR 0002 makes client storage lazy: after the configured offline delay the
non-persistent writeback clone is removed and rebuilt from its assigned
snapshot at the next PXE request. That keeps the pool small, but puts a ZFS
clone (and any later Windows personalization) on the boot critical path.

The existing `OfflineReplacement` journal already switches a client between
clones without destroying the previous one before the new one is verified.
Automatic reset currently deletes the writeback and lets boot recreate it.

## Decision

Add prewarmed replacement clones ("spares") that are job-owned ZFS clones of a
client's assigned snapshot.

- A spare is recorded in `client_writeback_spares` with the client, source
  snapshot, a configuration fingerprint, status (`building`, `ready`,
  `invalid`, `failed`, `consuming`) and timestamps.
- The fingerprint covers the client's assigned source revision, dataset,
  backstore, target IQN, client name, persistence choice and game selection, so
  any of those changes invalidates the spare. MAC alone is not a driver
  compatibility proof and is not used.
- A spare is only built when the client is enabled, non-persistent, offline
  with a *confirmed* closed session, has no pending storage journal, no
  installer session and no hardware-identity reservation, and its writeback
  clone is already absent. Unknown session state blocks the build.
- Adoption at boot requires the same fingerprint, the recorded source
  snapshot, the spare's verified origin, an absent boot dataset, and no local
  mounts. The adoption is written to `client_offline_resets` as an
  `OfflineReplacement` *before* the rename, then consumed with the existing
  `OfflineReplacement::recover_offline` recovery contract. The journaled spec
  carries no CHAP secret; authentication is re-applied from live state after
  the target is exposed.
- A missing, stale, wrong-client, wrong-snapshot or unreadable spare falls
  back to the existing additive boot path. Only a failure that leaves the
  journal unrecoverable blocks boot, and it does so with a recorded boot
  failure.
- Cleanup runs every sweep under the same per-client mutation lock: expired
  (TTL), superseded (boot storage exists again), stale (fingerprint changed),
  ineligible, or interrupted spares are destroyed. Journals win over cleanup.
- Limits: `storage.spare_prewarm.enabled` (default false),
  `max_ready_spares` (default 2, max 64), `ttl_hours` (default 24, 1-168) and
  `min_free_bytes` (default 50 GiB). New builds stop when pool free space is
  below the floor or cannot be read.

## Consequences

- At most `max_ready_spares` extra clones occupy the pool; the free-space
  floor and the count limit, not the client count, bound the cost.
- Adoption is a rename plus target reconciliation, so the PXE critical path
  skips clone construction. It is not a performance claim until measured.
- No change to per-client CHAP: the target is rebuilt with the same
  authentication immediately after adoption, and an adopted clone keeps its
  per-client target.
- Hierarchy stays: a spare is `pool/.../<writeback>-spare-<token>` and is
  never discovered as a master, game clone or client dataset.
- Windows personalization of spares (handoff Phase 3) is not implemented;
  `prepare_spare_clone` is the extension point, and the fingerprint already
  covers the assigned revision.
