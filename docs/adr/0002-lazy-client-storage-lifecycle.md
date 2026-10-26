# Lazy client writeback lifecycle

## Status

Accepted

## Context

Non-persistent clients currently keep an iSCSI target and writable ZFS clones after shutdown. The offline lifecycle waits for a configured delay, then replaces the boot writeback clone and resets selected game-disk clones while leaving the target provisioned. Administrators also need a way to discard a client's clones from the edit dialog.

PXE enrollment currently treats a persisted target IQN as proof that the target is ready and immediately redirects to the per-client boot menu. That assumption fails once an offline client’s target and clones are deleted.

## Decision

Use lazy provisioning for per-client storage. For non-persistent clients, after the existing offline delay has elapsed and the server confirms there are no active iSCSI sessions or NVMe exports, remove the client iSCSI target, its boot writeback clone, and all selected per-client game-disk clones. Preserve the client record, image and snapshot assignment, target IQN, CHAP credentials, and game-disk selection so the storage can be recreated.

Add a destructive **Delete Writeback** action to the edit-client dialog. It is available for any existing client with a boot writeback clone, including persistent clients. It uses the same no-session and no-NVMe-export safety checks and removes the target and both boot and game-disk clones. It does not change the client’s persistent setting or source assignment. The confirmation states that changes in all deleted clones are lost and that storage is recreated from its configured sources on the next PXE boot.

When an enabled, assigned client requests its enrollment boot script, the server checks and provisions its storage before returning the normal boot-menu redirect. Provisioning ensures the boot clone, selected game clones, iSCSI target/LUNs, initiator ACL and CHAP, and per-client boot menu are ready. Repeated concurrent PXE requests for the same client are serialized with client mutations and provisioning is idempotent. The client is never redirected to a target known to be missing or partially provisioned.

If cleanup or provisioning fails, persist enough operation state to resume safely. Destructive cleanup checks for active iSCSI sessions and NVMe exports before each destructive step. On boot-time provisioning failure, return a retryable enrollment script instead of the normal redirect; a later PXE request retries recovery/provisioning. Existing client assignments and source snapshots remain authoritative.

New deletion journals use an explicit `kind` and `version` envelope. Decoding
continues to accept untagged deletion journals and legacy `OfflineReplacement`
journals, using the `staged` field only to distinguish those old records. This
compatibility is required for supported upgrades and restores that can carry
pending operations written by earlier releases. Unknown journal kinds or
versions fail closed so the coordinator cannot misinterpret a recovery record.

This lifecycle decision does not change the existing `mode=super` toggle.
Super-client ISO installation is future work covered by its separate design
specification; that implementation will replace the legacy toggle behavior.

## Consequences

- Non-persistent boot and game clone changes are discarded after the offline delay.
- Manual deletion discards those changes immediately after the client is confirmed offline, even when the client is configured as persistent.
- PXE enrollment becomes a storage-provisioning boundary and can take longer than the current redirect-only request.
- If storage cannot be restored, the client remains in a retryable boot flow and its assignment remains intact for repair.
- Existing persistent clients retain their clones unless an administrator invokes the new action.
- Pending deletion journals identify their operation format explicitly; legacy records remain readable across supported upgrades and restores.

## Alternatives considered

- **Reset clones in place after the delay.** Rejected because it retains the target and backstores and does not reclaim the per-client clone allocations.
- **Delete only the boot clone.** Rejected because the selected per-client game clones would continue consuming storage and their data is also non-persistent client state.
- **Redirect first and provision asynchronously.** Rejected because the client could reach a stale boot menu before its target and backing volumes exist.
- **Delete while the client may still be connected.** Rejected because it risks removing storage beneath an active initiator.

## Contention and diagnostics

The existing global mutation lock remains the safety boundary for shared storage resources. PXE readiness waits up to two seconds to acquire it, then returns the retry script without leaving queued provisioning work. Once started, detached provisioning retains the lock until its storage work finishes, even if the HTTP request is cancelled. This bounds waiting requests; it does not make provisioning across clients parallel.

The background sweep considers non-persistent snapshot clients and clients with outstanding lifecycle state. It skips a busy mutation lock and retries on a later sweep rather than queueing ahead of boot or administrator work.

Boot provisioning failures have independent persisted counters and error text in `client_boot_failures`. The Pending Recovery page displays these diagnostics separately from interrupted destructive operations. Boot diagnostics do not change deletion retry deadlines, and successful readiness clears them. Boot failures retry on the next PXE request; the manual recovery action applies only to interrupted storage journals.
