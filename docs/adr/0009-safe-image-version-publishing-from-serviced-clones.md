# ADR 0009 — Safe image version publishing from serviced clones

**Status:** Accepted (publishing only; explicit client migration, pinning and comment endpoints are not implemented)  
**Date:** 2026-11-01  
**Related:** ADR 0004 (non-destructive restore-point apply), ADR 0008 (offline Windows personalization), handoff `docs/architecture/WRITEBACK-SPARES-2026-10-31.md`  
**Decisions:** Image versions are a first-class assignable metadata record, created from a verified serviced clone. Existing masters, snapshots, and existing clones are never renamed or promoted in place by default. Client assignment is intentional and explicit, not implicit mass-migration.

## Context

Operational workflows want a safe path from "a client clone has been serviced/personalized/validated" to "this image can now be assigned to other clients as a new version, with provenance." That path must be non-destructive to source history, explicit about which versions exist, and safe to retry across partial failures.

## Decision

- Introduce an **image version** record separate from the underlying ZFS dataset and the existing `Image` asset record. A version points at one verified asset and carries provenance, optional user comment, optional pin/lock, and optional ordering.
- A version is created from a **verified** serviced clone (or verified master/snapshot-derived asset) only. “Verified” here means the asset has passed the server-side verification path appropriate to its current lifecycle status; this ADR does not redefine that status machine.
- Publishing creates a new version; it does **not** rename, promote-in-place, or otherwise alter source snapshots/masters as a side effect. Where ZFS promotion/clone-origin removal is needed for storage layout, it is an explicit, separately gated storage operation, not part of version creation.
- Version creation is independent of client assignment. Existing clients remain on their current assignment until an admin explicitly moves them.
- Retention/capacity/preview and maintenance/observability are separate concerns and are specified in companion docs; they must never be gated on version publishing, and version creation must not trigger automatic snapshot destruction.

## Non-acceptable alternatives

- Do not make “publish” mean “swap the master dataset out from under existing clients.”
- Do not infer version identity from raw ZFS snapshot names alone.
- Do not auto-delete source snapshots or dormant writebacks as part of publishing.

## Consequences

- New tables/fields for versions, version-asset linkage, optional pinning, provenance, ordering, and last-used/assignment metadata where needed.
- New admin API for listing versions, creating a version from a verified asset, pinning/locking, commenting, and explicit client migration.
- Existing image APIs continue to work; version creation reuses existing asset verification where possible.
