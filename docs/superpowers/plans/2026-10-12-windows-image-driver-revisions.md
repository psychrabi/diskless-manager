# Windows Image Driver Revisions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Automatically create immutable Windows image revisions containing the reporting component and the compatible signed driver set gathered for managed clients.

**Architecture:** Extend managed Windows servicing to prepare a disposable clone from a ready snapshot, inject the compatible catalog packages without unsigned overrides, and publish a new revision only after servicing and cleanup succeed. Keep client assignments on their current revision until a new revision passes the boot-test gate.

**Tech Stack:** Rust 2021, Axum, Tokio, SQLx/SQLite, existing `ImageService`, ZFS snapshots, iSCSI servicing worker, DISM, React.

**Spec:** `docs/superpowers/specs/2026-10-12-managed-windows-network-drivers.md`.

## Global Constraints

- Never modify the source image or a revision already assigned to a client.
- Inject only packages whose parsed IDs, Windows version, and architecture match the target image/client set.
- Require Windows servicing to reject unsigned packages; never pass `/ForceUnsigned`.
- Preserve failed clones and targets when the worker may still be writing; do not discard cleanup errors.
- Publish a revision only after the worker disconnects and all owned temporary iSCSI resources are safely removed.
- Keep the last known-good revision available for rollback.

---

### Task 1: Persist image revision and driver-set provenance

**Files:**
- Create: `src-tauri/migrations/0008_windows_image_driver_revisions.sql`
- Modify: `src-tauri/src/core/image.rs`
- Modify: `src-tauri/src/persistence/repositories/image.rs`
- Test: `src-tauri/src/application/image_service.rs` and migration tests

**Interfaces:**
- Add revision provenance containing source image/snapshot, compatible driver package IDs, driver-set hash, lifecycle status (`preparing`, `awaiting-test`, `ready`, `failed`), and timestamps.
- A source image remains distinct from each published managed revision.

- [ ] Add repository tests for creating, listing, and transitioning revision records; reject duplicate revision IDs and invalid status transitions.
- [ ] Run the focused repository/migration tests and confirm they fail before the table and methods exist.
- [ ] Add the migration and typed revision model with database constraints for the four statuses.
- [ ] Implement repository operations and test transaction rollback when revision publication metadata fails.
- [ ] Run the focused Rust tests and confirm existing image listing and snapshot behavior remains unchanged.

### Task 2: Select compatible signed packages for an image

**Files:**
- Modify: `src-tauri/src/infrastructure/pxe/driver_selection.rs`
- Modify: `src-tauri/src/application/windows_servicing.rs`
- Modify: `src-tauri/src/infrastructure/pxe/windows_image_preparation.rs`
- Test: package selection and managed servicing tests in those modules

**Interfaces:**
- Add a deterministic selector for image OS version, architecture, and the known client Boot NIC IDs; preserve exact hardware-ID preference and stable package-ID tie breaking.
- Managed servicing accepts the selected catalog packages as a set and returns a candidate revision; DISM signature verification is part of adding the packages.

- [ ] Add tests showing incompatible architecture/OS and nonmatching IDs are omitted, compatible matches are selected, and duplicate package IDs are injected once.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml driver_selection` and `cargo test --manifest-path src-tauri/Cargo.toml windows_servicing` to confirm the new cases fail before implementation.
- [ ] Extend the selector and managed request to carry the compatible package set without changing existing manual preparation request behavior.
- [ ] Ensure candidate servicing never uses the unsigned-driver override and returns a failure if DISM rejects the package.
- [ ] Rerun focused tests; assert failure leaves the source image and existing revisions untouched.

### Task 3: Build and publish revisions automatically

**Files:**
- Modify: `src-tauri/src/application/windows_servicing.rs`
- Modify: `src-tauri/src/application/image_service.rs`
- Modify: `src-tauri/src/api/handlers/windows_servicing.rs`
- Modify: `src-tauri/src/api/routes.rs`
- Test: fake-backed managed servicing tests and API authorization tests

**Interfaces:**
- Add an application operation that builds an `awaiting-test` revision for each compatible Windows image when the trusted driver set changes.
- The operation uses the existing clone, CHAP target, remote Windows worker, safe release, snapshot, and publish sequence.

- [ ] Add fake-backed tests that a servicing success creates a distinct candidate revision and a DISM/worker/cleanup failure does not publish one.
- [ ] Run the focused servicing tests and confirm they fail because no automatic revision coordinator exists.
- [ ] Implement the coordinator and enforce the shared client mutation lock around image assignment/storage mutation.
- [ ] Make retry idempotent by driver-set hash so repeated reports do not create duplicate revisions.
- [ ] Rerun focused tests and verify manual Windows servicing endpoints keep their current behavior.
