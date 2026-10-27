# Non-destructive restore-point apply

## Status

Accepted

## Context

Administrators need to return an image to an earlier restore point without
discarding newer restore points. The existing rollback action rewrites the
master in place and requires explicit approval to delete newer snapshots. It is
bound to an exact snapshot list and is intentionally destructive.

Clients boot from a master plus an optional snapshot. A newly applied image is
only useful if it is an assignable master, and client reassignment already has
safety checks for active sessions, storage teardown, and pending recovery.

## Decision

Apply a restore point by creating a new master image from the selected snapshot.
The source image, its newer snapshots, and every client assignment remain
unchanged. Administrators reassign clients through the existing edit-client
flow.

The new image is a ZFS clone of the source snapshot and is recorded as
`kind = master`, `parent_id = NULL`, `source_snapshot = NULL`. Do not promote the
clone. This matches the existing serviced-clone publish behavior and keeps the
source snapshot available as the clone's origin.

If metadata insertion fails, destroy the new dataset. Never leave a ZFS clone
without an image record.

## Consequences

- Applying is fast and copy-on-write, but the new master depends on the source
  snapshot at the ZFS level. The source snapshot cannot be destroyed while the
  applied master exists. The deletion path keeps metadata when ZFS deletion
  fails, so the failure is recoverable.
- The applied master has no snapshots of its own. It can be assigned directly,
  or an administrator can create a snapshot first for snapshot-backed client
  storage.
- Apply does not take the client mutation lock because it never changes client
  assignments. Client switching remains serialized by the existing client
  update path.
