# Shared iPXE Dispatcher Implementation Plan

> **For agentic workers:** Execute inline in this session, task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Return each client's managed iPXE boot script directly from the shared dispatcher and stop maintaining generated per-client boot menu files.

**Architecture:** Keep the current DHCP filename, shared `autoexec.ipxe`, and `/enroll/{mac}` endpoint. Make the existing boot-readiness flow return the final rendered script, preserve custom menu overrides, and retire generated-menu publication while keeping storage readiness and Super-session safety in their existing services.

**Tech Stack:** Rust 2021, Axum, Tokio, SQLx/SQLite, iPXE text renderers, Apache-served PXE files.

**Spec:** `docs/superpowers/specs/2026-10-17-shared-ipxe-dispatcher-design.md`

## Global Constraints

- Preserve the current shared DHCP filename and `/enroll/{mac}` path for existing clients.
- Preserve MAC validation, enabled-client checks, enrollment-window rules, owner checks, storage ownership checks, bounded mutation-lock waiting, and recovery handling.
- Keep custom per-client iPXE menus as explicit overrides; do not delete unrecognized menu files during migration.
- Keep personalized scripts out of caches and logs; responses that can contain target or CHAP settings use `Cache-Control: no-store`.
- Keep storage setup behind the existing storage service and retain both targetcli and configfs backends.
- Do not commit or deploy. No tests are added or run in this execution unless the user separately asks for verification.

---

## Files and responsibilities

- `src-tauri/src/api/enroll.rs`: select the dispatcher response and return the final script.
- `src-tauri/src/application/client_boot.rs`: ensure boot storage readiness and build the personalized normal/Super/custom response.
- `src-tauri/src/infrastructure/pxe/ipxe.rs`: maintain state-specific iPXE templates and point inventory failure back at the dispatcher.
- `src-tauri/src/infrastructure/dhcp/isc_dhcp.rs`: stop requiring a generated menu before publishing DHCP; retain custom-menu path safety and deletion cleanup.
- `src-tauri/src/services/http.rs`: publish the shared autoexec and remove only recognized stale generated menus; do not regenerate managed menus.
- `src-tauri/src/application/nvmeof_boot_service.rs`: stop generating a per-client menu as a prerequisite for NVMe/TCP preparation.
- `src-tauri/src/application/super_client_service.rs`: stop refreshing generated menu files; preserve custom-menu behavior and active-session validation.
- `src-tauri/src/api/handlers/client_enabled_update.rs`, `src-tauri/src/api/handlers/clients.rs`, and `src-tauri/src/api/handlers/client_delete.rs`: make database boot gates authoritative while retaining custom override cleanup on actual deletion.
- `src-tauri/src/application/client_lifecycle.rs`: remove menu-file side effects only where they serve as a boot gate; preserve deletion/recovery ordering and safety checks.
- `.gitignore`: allow this plan file, as the plans directory ignores new files by default.

## Task 1: Return the final script from boot readiness

**Files:**
- Modify: `src-tauri/src/application/client_boot.rs`
- Modify: `src-tauri/src/api/enroll.rs`

**Interface:** Change `ensure_client_boot_ready(state, id, requesting_mac)` from `Result<()>` to `Result<String>`. Keep its validation, mutation/recovery flow, and storage side effects. On successful readiness, return the current persisted custom override when applicable, otherwise render the normal or verified Super-session script from the existing boot reservation data.

- [x] Refactor `ensure_client_boot_ready` to return its selected script without publishing a manager-generated menu file.
- [x] Preserve custom-menu backup data while a Super session is active; on normal boot after an older Super session, restore any legacy backup safely before removing its backup record.
- [x] Update `enroll_client` to return the script produced by boot readiness and keep the existing retry response for lock, storage, or session errors.
- [x] Keep disabled, pending, unknown, enrollment-window, and one-shot inventory branches unchanged in meaning.

## Task 2: Keep dispatcher scripts uncached and make boot errors re-enter dispatch

**Files:**
- Modify: `src-tauri/src/api/enroll.rs`
- Modify: `src-tauri/src/infrastructure/pxe/ipxe.rs`
- Modify: `src-tauri/src/infrastructure/pxe/nvmeof.rs` only if direct script execution requires a variable-preservation correction.

- [x] Add `Cache-Control: no-store` to iPXE responses while retaining `text/plain; charset=utf-8`.
- [x] Change the inventory-failure fallback to return to `/enroll/${client-mac}`; do not refer to generated per-client menus.
- [x] Preserve the NVMe firmware capability flag and common autoexec re-entry behavior.
- [x] Remove the now-unused redirect renderer and assertions that require the success redirect, updating any source callers found by `rg`.

## Task 3: Stop publishing manager-generated per-client menus

**Files:**
- Modify: `src-tauri/src/infrastructure/dhcp/isc_dhcp.rs`
- Modify: `src-tauri/src/services/http.rs`
- Modify: `src-tauri/src/application/nvmeof_boot_service.rs`
- Modify: `src-tauri/src/application/super_client_service.rs`
- Modify: `src-tauri/src/application/client_boot.rs`

- [x] Make the DHCP publisher update/reload DHCP without requiring a `clients/<mac>.ipxe` file first.
- [x] Remove cached manager-menu regeneration from HTTP service setup; preserve the single generated `autoexec.ipxe` and custom files.
- [x] Add startup cleanup that removes only files carrying known manager-generated markers; preserve valid custom and unrecognized files.
- [x] Remove NVMe preparation's menu publication requirement.
- [x] Remove Super-session menu refresh writes; leave session state and its validation authoritative at dispatch time.
- [x] Retain client deletion's safe removal of that client's custom override and retain all DHCP/target rollback behavior.

## Task 4: Make the dispatcher the enabled-client boot gate

**Files:**
- Modify: `src-tauri/src/api/handlers/client_enabled_update.rs`
- Modify: `src-tauri/src/api/handlers/clients.rs`
- Modify: `src-tauri/src/api/handlers/client_delete.rs`
- Modify: `src-tauri/src/application/client_lifecycle.rs`
- Review: every `publish_client_menu`, `ensure_client_menu`, `publish_client_ipxe`, `remove_client_ipxe_menu`, and `client_mac_script_path` caller found with `rg`.

- [x] Remove enable/disable operations that delete or recreate generated menus as the authorization gate; persist enabled state only after the existing validation succeeds.
- [x] Keep deletion cleanup for custom overrides, but ensure cleanup failure does not delete the client record or weaken existing rollback behavior.
- [x] Remove lifecycle menu removal only when the per-client file was acting as a gate; keep per-client mutation locking and storage recovery protections.
- [x] Remove dead menu-generation helpers only after all production callers and tests/fixtures have been checked.
- [x] Confirm no production boot path falls back to a generated file or embeds client-specific CHAP data in a static manager-owned menu.

## Implementation review

- [x] Search every affected function and call site again; ensure error propagation, rollback, and safe-path validation were retained.
- [x] Inspect the working tree and run `git diff --check`.
- [x] Do not commit or deploy. Automated tests and hardware boot verification are deferred; Rust formatting and compile-only checks were run.
