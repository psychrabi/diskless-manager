# Test-gated Windows driver revisions

## Status

The assignment and default-image restrictions in this decision are superseded
by [ADR-0003](0003-advisory-windows-boot-tests.md). The catalog, servicing,
and boot-test reporting decisions remain in effect.

## Original decision

Manage Windows boot-network drivers in a signed package catalog populated by administrator imports and reports from working clients. Prepare new image revisions with compatible drivers and the client reporting component, but make a revision available to new clients only after a designated client with matching NIC hardware reports that exact revision and boot NIC after Windows starts and the server confirms its iSCSI session is active. Existing clients stay assigned to their current revision; new clients use the newest tested driver revision, with the last known-good revision retained for rollback. This favors reliable diskless boot over immediate rollout because an untested NIC driver can leave Windows unable to reach its iSCSI boot disk.
