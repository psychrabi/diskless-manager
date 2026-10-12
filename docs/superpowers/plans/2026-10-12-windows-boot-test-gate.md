# Windows Boot-Test Gate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Discover a new client's Boot NIC before Windows starts, require a successful matching-hardware test for its image revision, and hold clients that lack a tested driver.

**Architecture:** Add a small WinPE inventory payload to the existing PXE flow, record its hardware IDs against the enrolled MAC, and use the authenticated Windows reporter to record test results. The application exposes revision/test state and prevents an untested image from being assigned to a new client.

**Tech Stack:** Rust 2021, Axum, Tokio, SQLx/SQLite, existing iPXE and enrollment flow, Windows ADK/WinPE, Windows client reporter, React.

**Spec:** `docs/superpowers/specs/2026-10-12-managed-windows-network-drivers.md`.

## Global Constraints

- Use preboot hardware IDs to match the Boot NIC; MAC is only the enrollment lookup key.
- A test is for a specific image revision and matching NIC hardware ID.
- A test passes only after the client reports the exact revision and Boot NIC after Windows starts and the server sees its iSCSI target session active.
- No failed, missing, or untested match may be silently booted as Windows.
- Existing clients remain assigned to their current image revision; newest tested revision is the default for new clients.
- Hardware boot tests are opt-in and use only a designated disposable test client.

---

### Task 1: Collect preboot NIC hardware IDs

**Files:**
- Modify: `src-tauri/src/infrastructure/pxe/ipxe.rs`
- Create: `tools/windows-servicer/client-report/winpe-inventory.ps1`
- Modify: `src-tauri/src/api/enroll.rs`
- Modify: `src-tauri/src/api/routes.rs`
- Test: iPXE script, inventory parser, and enrollment tests

**Interfaces:**
- Add a WinPE inventory request keyed by the observed client MAC and carrying Boot NIC PnP hardware IDs, Windows architecture, and firmware boot interface.
- Inventory data is pending until the client is assigned and authenticated; normalize and validate all IDs before saving.

- [ ] Add iPXE rendering tests showing the inventory path starts WinPE and returns to the normal client menu on completion/failure.
- [ ] Add inventory parser tests for multiple adapters, a missing Boot NIC, malformed IDs, and a secondary adapter being reported as nonboot.
- [ ] Run the focused PXE/enrollment tests and confirm the new cases fail before the inventory flow exists.
- [ ] Add the WinPE inventory script/source payload and enrollment route; do not commit generated `boot.wim` binaries.
- [ ] Document Windows ADK packaging and run the Linux-side script/rendering tests.

### Task 2: Record and evaluate image boot tests

**Files:**
- Create: `src-tauri/migrations/0009_windows_revision_boot_tests.sql`
- Modify: `src-tauri/src/application/windows_servicing.rs`
- Modify: `src-tauri/src/api/handlers/windows_servicing.rs`
- Modify: `src-tauri/src/api/routes.rs`
- Modify: `tools/windows-servicer/src/windows_client_report.rs`
- Test: revision state-machine, report authorization, and API tests

**Interfaces:**
- Store the designated test client, expected Boot NIC hardware ID, candidate revision, status, and result time.
- Mark a test passed only when the report identifies the exact candidate revision and Boot NIC and the server confirms that client's iSCSI target is active.
- A passing test makes that driver version eligible for new client assignments; failed tests keep the prior revision current.

- [ ] Add state-machine tests for candidate → passed, candidate → failed, wrong client, wrong revision, wrong NIC ID, and inactive iSCSI session.
- [ ] Run the focused test-state and report tests and confirm they fail before test state and endpoints exist.
- [ ] Add the migration and application operation; make result recording transactional with revision readiness.
- [ ] Add an admin-only endpoint to select a designated test client and start a candidate test without changing production client assignments.
- [ ] Extend the reporter payload with exact revision and Boot NIC identity; reject mismatches without changing revision status.
- [ ] Rerun focused tests; verify no client-secret or driver archive data appears in responses/logs.

### Task 3: Gate new client assignment and expose status

**Files:**
- Modify: `src-tauri/src/api/handlers/clients.rs`
- Modify: `src-tauri/src/api/handlers/images.rs`
- Modify: `src-tauri/src/api/routes.rs`
- Modify: `src/api/modules/networkDrivers.js`
- Modify: `src/api/modules/images.js`
- Modify: `src/components/ImageManagement/WindowsServicingModal.jsx`
- Modify: `src/components/ClientManagement/ClientFormModal.jsx`
- Test: client creation handler tests, API module tests, and modal tests

**Interfaces:**
- Before creating/publishing a Windows client boot reservation, require a preboot Boot NIC record and a ready image revision with a tested package for that ID.
- Return a validation error containing the missing PnP ID; do not create storage or publish a boot menu on failure.
- Show driver catalog status, candidate/test state, latest ready revision, and the missing-ID reason using existing modal, form, and toast patterns.

- [ ] Add a client creation test proving a missing/untested match creates neither client storage nor an iPXE reservation.
- [ ] Add a success test proving a tested match assigns the newest ready revision while an existing client remains pinned.
- [ ] Run the focused Rust and React tests and confirm the new cases fail before the gate/UI exist.
- [ ] Implement the server-side gate before any storage or DHCP/iPXE side effect; keep UI checks supplemental.
- [ ] Add UI/API module tests for pending, ready, failed, and missing-driver states.
- [ ] Run focused tests, then the full project checks listed in the parent rollout plan.
- [ ] Run the opt-in Windows 10/11 boot test only on the designated disposable test client and record the tested NIC ID and image revision.
