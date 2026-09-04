# Non-Persistent Client Reset and Disk Totals Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reset non-persistent client clones after a configurable offline interval and expose separate disk speed and cumulative-total columns.

**Architecture:** A durable background coordinator observes authoritative iSCSI sessions, persists disconnect deadlines, and invokes rollback-safe storage replacement only after the global delay. LIO counters provide both instantaneous rates and cumulative totals; the React table and settings UI consume the extended contracts.

**Tech Stack:** Rust, Tokio, Axum, SQLx/SQLite, ZFS/LIO abstractions, React 19, Vite, Vitest, Zod.

**Spec:** Approved in the current task conversation; the user explicitly requested no separate specification document.

## Global Constraints

- Offline means no active iSCSI session, not failed ping.
- The global delay defaults to 5 minutes and applies only to clients with `keep_writeback = false`.
- Reconnection cancels a pending reset; persistent clients are never reset.
- Retain the existing clone when replacement fails and retry with bounded backoff.
- Preserve internal restore-point and boot-disk data while hiding their table columns.
- Preserve all existing uncommitted workspace changes.

---

### Task 1: Settings and durable reset state

**Files:**
- Modify: `src-tauri/src/core/config.rs`
- Modify: `src-tauri/src/state/app_state.rs`
- Create: `src-tauri/migrations/0002_client_offline_reset.sql`
- Modify: `src/schema/index.js`
- Create: `src/components/SettingsManagement/Forms/ClientLifecycleForm.jsx`
- Modify: `src/components/SettingsManagement/index.jsx`
- Modify: `src/components/SettingsManagement/ConfigForm.jsx`
- Modify: `src/hooks/useSettings.js`

**Interfaces:**
- Produces: `ClientLifecycleConfig { non_persistent_reset_delay_minutes: u32 }` with default `5` and validated range `1..=1440`.
- Produces: durable client columns `offline_since` and `reset_retry_after` containing RFC3339 timestamps.

- [ ] Write Rust and React tests for defaulting, validation, form rendering, and settings persistence behavior.
- [ ] Run focused tests and confirm they fail because lifecycle settings do not exist.
- [ ] Add the config type, migration, lifecycle form, schema, and settings update hook.
- [ ] Re-run focused tests and confirm they pass.

### Task 2: Offline reset coordinator

**Files:**
- Create: `src-tauri/src/application/client_lifecycle.rs`
- Modify: `src-tauri/src/application/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/api/handlers/clients.rs`

**Interfaces:**
- Produces: a pure lifecycle decision function covering disconnect, reconnect, due reset, persistence changes, and retry deadlines.
- Produces: `run_client_lifecycle_coordinator(AppState)` background loop independent of dashboard/WebSocket clients.
- Consumes: `target_has_active_sessions`, `StorageService::replace_client_storage`, global lifecycle settings, and persisted timestamps.

- [ ] Write failing state-machine tests for disconnect, cancellation, persistence, due reset, restart recovery, and bounded retry.
- [ ] Run the focused Rust tests and verify the expected missing-module/API failures.
- [ ] Implement the pure decisions and database state transitions.
- [ ] Add the background loop and start it with application startup.
- [ ] Clear pending state when a client becomes persistent or storage configuration changes.
- [ ] Re-run focused tests and confirm they pass.

### Task 3: Rollback-safe clone replacement

**Files:**
- Modify: `src-tauri/src/application/storage_service.rs`

**Interfaces:**
- Produces: `replace_client_storage(&ClientStorage, &ClientStorageSpec) -> Result<ClientStorage>`.
- Guarantees: stage and verify a clean clone first; restore the original dataset and iSCSI target if switching fails; delete backup only after success.

- [ ] Add fake-backend tests proving successful replacement and rollback after provisioning failure.
- [ ] Run the focused tests and verify they fail because replacement is absent.
- [ ] Implement staged clone creation, dataset renames, iSCSI switching, rollback, and cleanup.
- [ ] Re-run the focused tests and confirm they pass.

### Task 4: Cumulative disk metrics and table layout

**Files:**
- Modify: `src-tauri/src/metrics.rs`
- Modify: `src-tauri/src/api/handlers/ws.rs`
- Modify: `src/components/ClientManagement/ClientTableHeader.jsx`
- Modify: `src/components/ClientManagement/ClientTableRow.jsx`
- Modify: `src/components/ClientManagement/ClientTableHeader.test.jsx`
- Create: `src/components/ClientManagement/ClientTableRow.test.jsx`

**Interfaces:**
- Extends: `Throughput` with `total_read_bytes: u64` and `total_write_bytes: u64`.
- Displays: Disk Read Speed, Total Disk Read, Disk Write Speed, Total Disk Write.

- [ ] Write failing Rust tests for LIO totals and counter reset behavior.
- [ ] Write failing React tests for four disk columns, formatted totals, and hidden internal columns.
- [ ] Run focused tests and confirm the new assertions fail.
- [ ] Extend metrics payloads and add human-readable byte formatting.
- [ ] Update table header and row cells.
- [ ] Re-run focused tests and confirm they pass.

### Task 5: Verification

**Files:**
- Verify all files changed by Tasks 1-4.

- [ ] Run targeted Rust tests for config, lifecycle, storage, metrics, and WebSocket mapping.
- [ ] Run the complete Rust test suite and Clippy with warnings denied.
- [ ] Run targeted Vitest files, the complete frontend test suite, lint, and production build.
- [ ] Review the final diff to ensure unrelated user changes were preserved and no internal columns remain visible.
