# Promote Persistent Writeback to a New Master Image

## Goal

Let an administrator turn a client's persistent boot-disk writeback into a
separate, reusable master image without modifying the client's disk or the
currently selected master. The promoted image must appear in Image Management
and have an initial snapshot that clients can select.

## User flow

1. An administrator opens the context menu for an offline client and selects
   **Promote Writeback**.
2. The action is available only for a normal client with a selected source
   snapshot and persistent writeback. Super clients and non-persistent clients
   cannot be promoted through this action.
3. The administrator enters a valid, unique ZFS image name and confirms the
   promotion.
4. The server makes a point-in-time snapshot of the client writeback, copies
   that snapshot into a new image ZVOL, registers the new volume as a master,
   and creates an initial snapshot on the new master.
5. The UI reports the new master and initial snapshot. The administrator can
   assign and test it using the existing client image/snapshot controls.

The client must be offline for the entire operation. The form explains that
the new master captures the complete state of that client's Windows boot disk,
including machine-specific files and settings.

## Storage and image lifecycle

Promotion uses an independent ZFS send/receive copy, not a ZFS clone. The
source snapshot is the consistent copy point; the destination is created under
the configured image parent with the source volume's size and block size. The
destination is registered as an `ImageKind::Master` image with matching OS and
format metadata, and receives a generated initial snapshot with a name safe
for existing snapshot-selection controls.

The existing client writeback ZVOL and selected source master remain intact.
The source snapshot is retained as provenance for the promoted image. No
client assignment changes automatically. Promotion requires enough free pool
space for a full independent copy and fails clearly if the destination name
already exists.

## API and validation

Add an authenticated administrator-only endpoint scoped to a client, accepting
the requested new image name. Server-side validation must load the client and
reject the operation unless the client is offline, uses persistent writeback,
is not in Super mode, and has a configured source image/snapshot and existing
client writeback volume. Validate the name with the existing ZFS name rules
and verify destination absence before creating any persistent objects.

The response identifies the registered master and its initial snapshot. The
operation must not change the client's configured master/snapshot, iSCSI
target, or boot-disk attachment.

## Failure handling

Perform the operation in ordered stages: create source snapshot, stream it to
the destination ZVOL, register image metadata, and create the destination's
initial snapshot. If copying or metadata/snapshot registration fails, remove
only the incomplete destination and any metadata inserted for it. Preserve the
client volume, source snapshot, and all pre-existing images. Return an error
that identifies the failed stage; if cleanup also fails, report both errors
and leave enough information for reconciliation.

The send/receive implementation must pass arguments directly to processes and
stream data without invoking a shell. It must detect failures from either side
of the pipeline and avoid treating a partial receive as a usable image.

## UI and integration

Add the client action to the existing offline-client context menu. The modal
collects the new image name, explains that the operation copies the full disk
and leaves existing images unchanged, and disables submission while running.
On success, refresh client/image data and show the new master and snapshot.
The promoted image then uses the existing Image Management list, snapshot
management, and client assignment flows; no separate promotion-only image
type or assignment behavior is introduced.

## Testing

- Service tests validate eligibility, destination collision handling, correct
  source snapshot and destination selection, and registration as a master.
- Failure tests verify that a failed stream or registration leaves the source
  volume and existing master untouched and cleans up a partial destination.
- API tests verify administrator authorization and rejection for online,
  Super, non-persistent, or incompletely configured clients.
- UI tests verify action visibility, submission payload, progress/error state,
  and success refresh behavior.
- ZFS send/receive behavior should be exercised in an integration test when a
  disposable OpenZFS environment is available; unit tests cover command and
  pipeline failure handling without touching the user's pool.

## Out of scope

- Overwriting or replacing the current master.
- Automatically switching any clients to the promoted image.
- Selectively merging files or registry settings.
- Promoting game-disk writeback or Super-client changes through this action.
