# Preview-first image-snapshot retention

## Status

Accepted (disabled by default; dry run is the default once enabled)

## Context

Recovery points accumulate: every serviced clone, restore point and manual
snapshot is kept, and nothing removes them. Image-snapshot history is the only
part of the storage model that grows without a bound, while every other
lifecycle path (client writebacks, prewarmed spares, factory drafts) already
has a staged cleanup with ownership checks.

A cleanup that guesses is worse than no cleanup. A snapshot can be referenced by
a client's saved assignment, a managed clone's origin, a prewarmed spare's
source, an unfinished factory attempt, or a Super session recovery record, and a
ZFS snapshot with a dependent clone cannot be destroyed at all.

## Decision

Add an optional retention pass over image snapshots.

- `storage.retention.enabled` defaults to `false`; when enabled,
  `storage.retention.dry_run` defaults to `true`.
- `min_keep_snapshots` (default 3) newest snapshots per image are always kept,
  and `min_age_hours` (default 168) is a floor on top of that. Both are bounded
  by `Settings::validate`.
- Planning is read-only and returns candidates *and* refusals with reasons, so
  an administrator sees why a snapshot was left alone.
- A candidate is refused while a client assignment, a managed clone origin, a
  prewarmed spare, an unfinished factory session, or a Super session recovery
  record still names it. The check runs twice: while planning and again
  immediately before each deletion.
- Deletion goes through the existing `ImageService::delete`, which stages
  metadata removal in a transaction, keeps it open until ZFS succeeds, and
  refuses images with dependants. This change adds no new destructive path.
- A destructive run requires `confirm = true`; a dry run records the plan and
  deletes nothing.
- Every planned, skipped, deleted and failed snapshot is written to
  `storage_retention_events` with the acting administrator.

## Non-acceptable alternatives

- Do not delete snapshots by age without the keep count, the ownership checks
  and the confirmation gate.
- Do not treat a failed reference query as "unreferenced".
- Do not delete a persistent client's writeback or its source snapshot as part
  of retention.
- Do not block active clients or hold the global client mutation lock for a
  retention pass.

## Consequences

- Snapshot growth becomes bounded, and every removal is explainable after the
  fact from the audit table.
- Retention never reclaims space when a snapshot is still referenced: the
  operator sees the blocking owner instead of a failed deletion.
- Free-space headroom for imports and clones is still governed by each job's
  own checks; this ADR adds a read-only planner, not a pool reservation.
- Prewarming and boot paths are untouched: retention only removes snapshots
  that no client, clone, spare or session references.
