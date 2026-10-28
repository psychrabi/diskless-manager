# ADR 0005: Verified image factory

- **Status:** Accepted
- **Date:** 2026-10-28

## Decision

Install & Adapt creates a session-owned draft image and a session-owned iSCSI installer target. It never writes to the client's currently assigned image or normal target. Factory images remain hidden until a disposable verification clone boots through the normal OS path and reports from the live iSCSI peer for the exact attempt target. Only that report can publish the draft.

Interactive ISO boot remains a manual session with a scratch disk. Closing it discards the scratch disk; it does not create a published image.

## Consequences

- A durable session row owns draft, snapshot, candidate, target, and cleanup identifiers. The row continues reserving client storage until cleanup succeeds.
- Normal PXE dispatch handles unfinished installation sessions before normal storage readiness. Saving waits; verification boots only its candidate; stopped sessions fail closed.
- Verification failures can be retried with a new target and attempt number. Administrators cannot override verification.
- Publish-only preserves the previous assignment. Opt-in assignment runs after the verification client is off and post-publish cleanup succeeds; it updates the client through the normal storage reconciliation path and records success or a visible, non-destructive failure.
- Automated tests use isolated SQLite and fake storage/infrastructure. They do not establish behavior on live ZFS, LIO, firmware, WinPE, or client hardware.
