# Super-client ISO installation workflow

## Status

Approved by the user on 2026-10-14. No application-code changes are included in this documentation task.

## Goal

Allow an administrator to place a client in persistent Super-client mode so it boots an OS-appropriate installer from managed ISO media while installing to an isolated, writable copy of its assigned image. The client remains in Super mode across reboots until **Save super client** or **Discard super client** is explicitly selected.

Windows assignments use configured WinPE deployment media. Linux assignments use configured live-install media. The client's writable installation volume is iSCSI LUN 0; the selected read-only installer ISO is exposed on the same client target as LUN 1. iPXE attaches LUN 0 and boots from LUN 1.

## Existing context

- The project already has persistent `BootMode::Super`, a client action for toggling it, an iPXE-generated client menu, and storage abstractions that describe multiple LUNs.
- Current Super-mode activation points the client target directly at the shared master volume. That behavior is unsafe for OS installation and must not be used by the installer flow.
- Normal client boot storage uses LUN 0. Optional game-disk LUN numbering begins at LUN 1 only when the client's **Use game disk** setting is enabled; unchecked clients have no game-disk clone/LUN to conflict with installer media.
- The existing iSCSI implementations are targetcli/LIO and internal configfs. Both backends and their shared ownership, active-session, and recovery protections must remain supported.
- Windows and Linux images already carry `os_type` metadata. Use the assigned image's authoritative metadata, not a client-provided OS string.

## Agreed direction

1. Super mode is persisted until an administrator saves or discards the work.
2. Enabling Super mode creates a client-exclusive writable ZFS clone from the client's assigned image/revision. It never exports the shared source image as a writable installer target.
3. The assigned image OS chooses the installer class: Windows selects WinPE media; Linux selects live Linux media.
4. **Save super client** promotes the working clone into a new managed image revision and returns the client to normal mode on that revision. Other clients stay pinned to their existing assignments.
5. **Discard super client** discards the working clone and restores the client's prior image assignment and normal writable storage.
6. The ISO library is an administrator-facing page for adding, validating, listing, selecting, and removing installer media.
7. During Super mode, the working image is LUN 0 and the read-only ISO is LUN 1 on the client's existing iSCSI target. iPXE attaches LUN 0 with `sanhook` and boots the installer from LUN 1 with `sanboot`.
8. **Conditional game-disk handling:** game-disk clones/LUNs exist only when the client's **Use game disk** setting is enabled. When enabled, Super mode reserves LUN 1 for installer media and temporarily unmaps that client's game-disk LUNs while retaining their clone data; Save or Discard restores the mappings. When disabled, no game-disk clone/LUN is provisioned or expected.
9. ISO files are added through browser upload into managed server storage, with streamed writes, configured size limits, checksum, and validation.
10. Existing clients already marked Super by the legacy shared-master implementation are not auto-converted or booted through the new flow. They require an explicit administrator conversion or exit operation while offline.

## Proposed design

### ISO library and selection

Add an administrator-only ISO management page using existing React routing, layout, authorization, form, and toast patterns. Administrators can add an ISO with a display name and media type (`Windows WinPE` or `Linux live`), inspect validation state and checksum, choose the default ISO for each media type, and remove unused media. Multiple ISO files may be cataloged, but one active default per media type is selected for the first release to keep client boot selection deterministic.

Store ISO files below a dedicated managed-media directory under the configured boot root, using generated safe filenames rather than user-supplied paths. Persist catalog metadata such as ID, name, media type, path, byte size, SHA-256, validation state, and timestamps. Upload and delete operations are administrator-only, size-limited, and safe against path traversal. Reject a media deletion while it is selected as a default or referenced by an active Super client. ISO backstores and mappings are read-only so a client cannot alter shared installer media.

Validation must verify that the file is a readable ISO and that its declared media type is plausible. The implementation must also demonstrate that the ISO boots from iPXE through the intended LUN 1 path on supported client firmware. A file that parses as an ISO but cannot boot through the supported path is not marked ready. Do not assume every arbitrary ISO is bootable through this workflow.

### LUN 1 and optional game disks

A game-disk clone/LUN is present only when the client's **Use game disk** setting is enabled; when unchecked, no game-disk clone or mapping is created. For enabled clients, temporarily unmap selected game-disk LUN(s) while Super mode reserves LUN 1 for the ISO. Keep the setting and client-owned clone data unchanged, then restore mappings on Save or Discard. This preserves game data while honoring the fixed installer LUN.

### Super-client storage and target layout

Persist enough Super-session metadata to recover safely after a process restart: client ID, prior image and snapshot/revision assignment, working-clone identity and source snapshot, selected ISO ID, operation state, and timestamps. A dedicated session/recovery record is required, not just `mode=super`, because Save and Discard span ZFS, image metadata, the iSCSI target, and client assignment state.

On activation:

1. Require an enabled client with a ready assigned image and a supported `os_type`.
2. Require the client to be offline and verify there are no active iSCSI sessions or other exports for the resources being changed.
3. Resolve the configured ISO matching the assigned image OS. Fail before mutation if no ready matching ISO is configured.
4. Create and verify a client-owned writable clone from the precise assigned image snapshot/revision. Preserve the original assignment in the Super-session record.
5. Attach the clone to the client target at LUN 0 and the catalog ISO as a read-only LUN 1. Preserve target IQN, initiator ACL, CHAP settings, and existing resource ownership rules.
6. Publish the Super-mode iPXE path only after both LUNs and target state have been verified.

Proposed Super-mode behavior for enabled clients is to temporarily remove selected game-disk LUN mappings from the target while retaining their client-owned ZFS clones and records. If **Use game disk** is unchecked, there is no game clone or mapping to remove or restore. For enabled clients, restore the mapping only after Save or Discard exits Super mode. Reconciliation must treat the temporary unmapping as intentional and must not recreate a game LUN that conflicts with installer LUN 1. This proposal preserves game data and the checkbox state; confirm it during spec review.

If preparation fails, roll back only resources created by that attempt. Never destroy the assigned master, a pre-existing client resource, or another client's storage. If rollback or cleanup fails, retain a durable recovery record and keep the client from entering an ambiguous boot state.

### Legacy Super clients

A client already persisted as `mode=super` before this feature may have its target pointed directly at the shared master and will have no new Super-session record. The upgrade must not silently auto-convert its storage or attach installer media to that legacy target. Detect this state and block PXE with an actionable administrator message. An administrator must take the client offline, verify no active sessions, then explicitly convert it to clone-backed Super mode or exit Super mode and rebuild normal client storage. Preserve the original assignment and use durable recovery records for either operation.

### Boot dispatch

For a client with `mode=super`, the per-client iPXE flow must:

1. Attach the verified LUN 0 working image as a secondary SAN drive with `sanhook --drive 0x81`, preserving the SAN description needed by the installer to find the target disk.
2. Boot the LUN 1 installer media from the same target using `sanboot --no-describe`, so the installer ISO boots without replacing the working-disk SAN description.
3. Leave the client in Super mode after installer exit, failure, or reboot. The server must not silently switch back to normal mode or boot the shared source image.

Normal mode keeps the current client boot behavior. Missing, mismatched, or unavailable LUNs/media fail closed with a useful client diagnostic and server-side structured logs. Both BIOS and UEFI support must be explicitly tested against the client's configured PXE mode; unsupported firmware/media combinations are rejected or reported, not silently redirected to another OS path.

### Save and discard lifecycle

Both actions require the client to be offline, no active target sessions, and an acquired per-client mutation lock. The UI must warn that Save publishes installer changes as a new image revision and that Discard permanently removes the working copy's changes. Discard requires explicit confirmation.

**Save:**

1. Validate the Super-session and confirm the client is offline with no active sessions.
2. Quiesce/remove the temporary ISO LUN mapping and ensure the working clone is not exported while it is promoted.
3. Snapshot the completed working clone, create a separate image-owned dataset from that sealed snapshot, and register it as a new managed image revision with OS metadata and provenance to the original assignment and Super session. The client-owned working clone is not itself converted into a shared writable master.
4. Rebuild the client's ordinary writable storage from the saved revision, restore game-disk LUN mappings only if **Use game disk** remains enabled, and update the client assignment/mode atomically with durable recovery state.
5. Remove the old Super working clone and clear the Super-session record only after the new normal target and assignment are verified.

**Discard:**

1. Validate the Super-session and confirm the client is offline with no active sessions.
2. Remove the ISO mapping and Super target attachments safely.
3. Destroy only the session-owned working clone, recreate ordinary client storage from the recorded prior assignment, and restore game-disk LUN mappings only if **Use game disk** remains enabled.
4. Restore the prior assignment, clear Super mode, and remove the session record only after target/storage verification.

If either action is interrupted, retain the operation state and recovery metadata. Retrying must be idempotent. Do not report success or clear the session while storage cleanup, promotion, or target restoration is incomplete.

## API and persistence contracts

- Add authenticated administrator endpoints for ISO catalog list, streamed browser upload, validation/status, per-OS default selection, and safe deletion. Enforce request size limits before persisting catalog metadata.
- Add an authenticated status and admin operation for legacy clients. Legacy `mode=super` clients without a Super-session record must be marked as requiring offline administrator handling and denied the new Super boot path. The administrator can explicitly convert them to a clone-backed session or exit Super mode and rebuild ordinary client storage.
- Normalize errors through the shared HTTP error contract (`code`, `message`, `operation_id`, `details`). Avoid returning server filesystem paths or command output that could reveal sensitive host details.
- Add a versioned migration for ISO metadata and Super-session/recovery state. Existing normal-mode clients remain unchanged. Legacy `mode=super` clients without a Super-session record are flagged as requiring explicit offline administrator handling; migration does not alter their storage or assignment.
- Keep uploaded ISO bytes and derived iSCSI file backstores outside SQLite. Back up/restore catalog metadata and media consistently, or fail validation clearly when a catalog entry's file is missing or its checksum changed.

## Safety and operational constraints

- Never write an installer directly to a shared master or a volume currently assigned to another client.
- Check active iSCSI sessions and all owned target/LUN mappings before detach, clone destruction, promotion, or discard.
- ISO LUN 1 is read-only and belongs to the specific client's existing target mapping. The backing ISO remains shared and immutable while mapped.
- Preserve both targetcli and configfs implementations, their shared validation, stable initiator ACLs, CHAP behavior, and transaction/rollback guarantees.
- Keep normal boot output and existing manual/diagnostic WinPE flows unchanged unless their callers are intentionally migrated and tested.
- Do not log ISO contents, CHAP secrets, or upload payloads. Apply restrictive permissions to stored media and runtime metadata.

## Acceptance criteria

1. An administrator can add Windows WinPE and Linux live installer ISOs, see their validation/checksum state, set the OS-specific defaults, and remove only unused media.
2. A Windows-assigned Super client receives WinPE media as LUN 1; a Linux-assigned Super client receives live Linux media as LUN 1.
3. In both cases, LUN 0 is a verified client-exclusive writable clone of the assigned image, and iPXE attaches LUN 0 then boots LUN 1.
4. Two Super clients based on one master have independent writable clones. Writes from either client never reach the shared master or the other client's clone.
5. Game-disk clone/LUN provisioning follows **Use game disk**. For enabled clients, selected game-disk mappings are temporarily removed only during Super mode, clone data and checkbox state remain intact, and mappings are restored afterward. Unchecked clients get no game-disk clone/LUN.
6. Save creates and records a new image revision, assigns it to the saving client, and leaves other clients on their previous revision.
7. Discard restores the exact prior assignment and normal storage without deleting shared or unrelated resources.
8. Disabled clients, online clients, clients with active target sessions, unsupported OS assignments, missing media, invalid media, and incomplete LUN mappings cannot enter or continue an unsafe Super boot path.
9. Both iSCSI backends pass fake-infrastructure tests for create, inspect, reconciliation, rollback, restart recovery, save, and discard. Automated tests do not modify live ZFS/iSCSI state.
10. Hardware boot validation is opt-in and records tested firmware mode, ISO checksum, client image revision, and observed LUN/boot results.

## Scope and non-goals

- This feature manages installer media and boots it for a human/operator-driven OS installation. Automated unattended Windows/Linux installation profiles are not part of this design.
- The initial ISO catalog has one active default per OS type. Per-client ISO selection, arbitrary boot-media transformations, and generic support for every vendor ISO are deferred.
- This workflow does not make a shared master writable and does not change assignments of other clients when one client saves a new revision.
- No generated ISO binary, production media, or application source changes are included in the specification deliverable.

## Open implementation verification

- Prove `sanhook` of LUN 0 followed by `sanboot` of the same target's LUN 1 boots supported WinPE and Linux media in each firmware mode claimed by the UI. iPXE command support alone is not sufficient evidence for a given ISO/firmware combination.
- Verify the chosen targetcli and configfs representations can map the same read-only ISO backstore as LUN 1 into multiple client targets without weakening ownership or reconciliation checks.
- Confirm promotion can create a managed immutable revision using existing image lifecycle APIs without changing other clients' pinned assignments.
- Verify legacy `mode=super` clients are denied the new boot path until an administrator explicitly converts or exits them offline.
