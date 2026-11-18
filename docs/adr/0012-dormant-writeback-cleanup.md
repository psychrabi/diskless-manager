# ADR 0012: Dormant writeback cleanup

## Status

Accepted (Phase 4A capacity upkeep of the ggNet improvement handoff).

## Context

A client's writable dataset (`<writeback parent>/<CLIENT>-disk`) is normal
storage for as long as the client exists, but nothing else ever removes it. An
offline deletion that fails after the client row is dropped, a manual
`zfs create` under the writeback parent, or a restored database leaves
datasets that no client, assignment or journal owns. They keep consuming pool
space with no UI surface and no explanation.

Image retention (ADR 0010) covers snapshots and cannot reclaim these: they are
not image rows, and deleting them through the image path is rejected because
no image record exists.

## Decision

- A dedicated, administrator-only cleanup previews and reclaims only datasets
  this server can prove are unowned, and reports every refusal with a reason.
- Ownership evidence is required from four independent sources: the dataset
  must be a direct child of a dataset tagged `org.diskless:type=writeback`, its
  name must be exactly `<CLIENT>-disk`, no client row may carry that name
  case-insensitively, and no spare row may reference the dataset.
- The iSCSI target the name maps to (`<prefix>:client.<lowercase name>`) must
  not exist. Sessions cannot outlive their target, and a lingering target means
  the storage path was not torn down, so the dataset is refused rather than
  guessed at.
- The dataset must be older than `storage.dormant_cleanup.min_age_hours`
  (default 24 h), which keeps a just-created or just-restored dataset out of
  the plan while its owner is still being written.
- Mount state is checked through the image backend's fail-closed
  `validate_unmounted`: an unreadable mount table refuses the dataset instead of
  assuming it is free.
- Spares, game clones (`-games`) and unrelated volumes under the writeback
  parent are never candidates; only the exact `-disk` suffix qualifies.
- The feature is disabled by default with `dry_run: true`. A destructive run
  needs `enabled`, `dry_run: false` and `confirm=true`, and re-verifies every
  candidate immediately before `zfs destroy`, so a client created after the
  plan keeps its storage.
- Every planned, skipped, deleted and failed dataset is written to
  `storage_dormant_cleanup_events` with the actor.

## Consequences

- Stale storage from a failed deletion can be reclaimed without touching any
  dataset that still has an owner, a spare, a target, or a live mount.
- A dataset whose iSCSI target still exists stays reported as skipped until the
  target is reconciled, which is deliberate: the operator fixes the target, not
  the cleanup.
- ZFS itself is the last guard: `zfs destroy` fails on a busy volume, and that
  failure is recorded as a failed candidate rather than retried silently.
- The preview costs one recursive `zfs list` and a few point queries, so it is
  cheap enough to run before every destructive attempt.
