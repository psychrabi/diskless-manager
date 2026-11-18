# ADR 0011: Managed independent image copies

## Status

Accepted (Phase 4C of the ggNet improvement handoff).

## Context

Operators need a second, independent master image for testing or for a
separate client group without touching the image that clients boot from.
Cloning a snapshot (`zfs clone`) produces a dependent dataset that cannot be
destroyed while its origin exists, and promoting it would alter the source's
snapshot chain. Renaming or swapping masters in place would break assignments
and the non-destructive restore-point rule in ADR 0004.

`zfs send | zfs recv` produces an independent dataset, but a multi-hour send
must not hold the client mutation lock, and an interrupted transfer must never
leave a half-written dataset that looks assignable.

## Decision

- `ImageCopyService` streams `zfs send -c` into `zfs recv -u` for a new dataset
  inside the managed image parent (`<pool>/image-disk/<name>`). The source
  image, its snapshots and its clones are read-only inputs and are never
  promoted, renamed or rolled back.
- Copying is asynchronous. The request reserves the destination name, refuses
  an existing dataset, and returns a job from an in-process registry. The
  registry reports approximate progress from the destination's `used` property
  and accepts a cancellation request.
- The destination becomes a managed image only after the transfer exits
  cleanly and its virtual size matches the source. A failed, cancelled or
  mis-sized transfer destroys the partial dataset and records the error on the
  job.
- Copies are refused for snapshots, non-ready images and images owned by an
  unfinished installer session, and while another copy of the same source is
  running.
- The registry is runtime state. It is not persisted because a copy cannot be
  resumed; after a restart the operator sees a leftover dataset instead of a
  job that claims to still be running. A stale partial destination is removed
  through the normal image delete path once its transfer is known to be gone.
- Copy endpoints are administrator-only and use the shared error contract.

## Consequences

- Copies are consistent with the image model: one independent master, no
  shared origin, no client migration as a side effect.
- Journaled deletion and replacement records never reference a copy in flight,
  so a partial dataset cannot be mistaken for client storage.
- Progress is approximate (destination allocation, not stream position) and a
  cancelled transfer can take one poll interval to stop.
- Multi-hour transfers still consume pool bandwidth; scheduling them during
  maintenance windows remains an operational decision, not automatic
  behaviour.
