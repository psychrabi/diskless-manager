# Pull Request Summary: NexBoot Driver Flow

## Suggested title

`Harden NexBoot boot-time NIC installation and make strategy selection safe`

## Branch

`feat/nexboot-driver-flow`

Latest commit:

```text
0fb293d Harden NexBoot deployment path
```

## Summary

This branch adds a selectable Windows NIC installation strategy while preserving the existing Offline DISM flow as the boot-critical path. The NexBoot-compatible behavior is now a hybrid path: drivers are injected and boot-armed offline first, then the NexBoot startup adaptation is staged for post-boot Plug and Play installation and compatibility handling.

The branch also hardens revision identity, Group Policy handling, helper capability negotiation, reporter installation, INF service mapping, and retry behavior.

## User-visible behavior

Windows Servicing settings expose:

- **Offline DISM (current)** — default and safest production path.
- **NexBoot boot-time adaptation (test)** — hybrid compatibility path that retains offline boot-critical preparation and adds NexBoot boot-time staging.

The selected strategy is persisted and included in managed Windows revision identity, so switching strategies cannot silently reuse an image produced by the other strategy.

## Main changes

### Safe boot-critical preparation

NexBoot mode no longer skips offline preparation. It still:

- Injects complete driver packages offline.
- Performs offline boot arming.
- Installs/updates the Windows reporter for committed images.

It additionally stages the NexBoot bundle and one-shot startup hook.

### Group Policy preservation

The staging implementation now merges the NexBoot startup entry into existing `scripts.ini` content. It preserves existing startup and shutdown registrations and supports UTF-16LE files with BOM.

The boot-time cleanup path removes only the NexBoot registration and files. It never deletes the entire Group Policy directory.

### Strategy-aware revisions

Driver revision hashing now includes the installation strategy. Offline DISM and NexBoot revisions are distinct even when they use the same image snapshot and driver package IDs.

### Helper capability negotiation

The helper protocol is now version 3 and advertises:

```json
"nexboot_boot_time": true
```

The server rejects NexBoot preparation when the worker is old or does not advertise support, preventing silent fallback to a different behavior.

### Safer boot script

`auto-adapt.ps1` now:

- Fails closed when the bundle is missing or empty.
- Requires at least one INF.
- Treats `pnputil` failures as installation failures.
- Preserves startup registration for retry after failure.
- Removes registration only after successful installation.
- Uses INF model/service relationships instead of assigning every hardware ID to every service.
- Skips unsafe ambiguous mappings rather than writing an incorrect CDDB service.

## Verification

Passed on the Linux sandbox:

- Windows helper tests: **38 passed, 0 failed**.
- NexBoot staging tests: **2 passed, 0 failed**.
- Windows servicing application tests: **14 passed, 0 failed**.
- Main backend compilation: passed.
- Windows helper compilation: passed.
- Rust formatting checks: passed.
- Git whitespace checks: passed.

The complete test report is available at:

[`docs/testing/NEXBOOT-TEST-REPORT.md`](../testing/NEXBOOT-TEST-REPORT.md)

## Remaining release gate

A real Windows worker/client execution was not possible in this session because no authorized Windows environment was available. The following must be completed before merging or deployment:

1. Run the updated helper on Windows and confirm protocol version 3 capabilities.
2. Run the hybrid NexBoot flow on a disposable Windows client.
3. Verify actual PowerShell execution under `SYSTEM`.
4. Verify DISM mounting, offline boot arming, and reporter installation.
5. Verify the client boots and reconnects to iSCSI without 0x7B.
6. Verify existing Group Policy startup/shutdown entries survive staging and cleanup.
7. Verify failure leaves a retryable startup registration.
8. Attach `adapt.log`, `pnputil.log`, `done.txt`, and boot results to the PR.

## Risk assessment

### Reduced risks

- Reduced 0x7B risk because offline boot preparation remains mandatory.
- Reduced image corruption risk because Group Policy entries are merged and cleanup is scoped.
- Reduced wrong-image risk because strategy participates in revision identity.
- Reduced old-helper compatibility risk because capability negotiation is explicit.
- Reduced retry loss because failed installation preserves its startup registration.

### Remaining risks

- Native PowerShell and Windows registry behavior still require hardware-in-the-loop verification.
- Actual INF model-section parsing must be validated against the vendor driver packages used in deployment.
- The full backend suite still contains an unrelated existing offline-replacement test failure.

## Review recommendation

Approve the code for staged Windows validation, not production deployment. Merge/deploy only after the real Windows worker/client test matrix passes on disposable hardware and the logs confirm successful boot, NIC initialization, iSCSI reconnection, reporter operation, and one-shot cleanup.
