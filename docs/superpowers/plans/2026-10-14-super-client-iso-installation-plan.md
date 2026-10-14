# Super-client ISO Installation Implementation Plan

> **For agentic workers:** Use the `subagent-driven-development` or `executing-plans` skill when implementing. Complete the checkboxes in order and keep this plan updated. This plan does not authorize commits, deployment, production ISO uploads, or changes to live client storage.

**Goal:** Allow an administrator to install or service a client's assigned OS image through a client-specific writable Super-client session booted from a managed WinPE or Linux live ISO, then explicitly Save or Discard the work.

**Architecture:** Add managed ISO catalog and durable Super-session state through the existing Rust/Axum/SQLite application layers. Extend both iSCSI backends with safe read-only file-backed ISO LUN support. Publish mode-aware iPXE boot menus that keep the assigned working image at LUN 0 and installer media at LUN 1. Build Save/Discard as recoverable application workflows that preserve shared-image isolation and conditionally suppress/restore game-disk LUN mappings. Add admin ISO management and client lifecycle controls in the existing React UI.

**Tech Stack:** Rust 2021, Axum, Tokio, SQLx/SQLite, serde, ZFS, existing targetcli and configfs iSCSI backends, iPXE, React, Vite, Tailwind, Bun.

**Spec:** `docs/superpowers/specs/2026-10-14-super-client-iso-installation-design.md`

## Global Constraints

- Preserve unrelated existing working-tree changes. Make the smallest complete changes in the implementation phase.
- Keep API errors on the shared `code`, `message`, `operation_id`, `details` contract. Require authentication and enforce administrator authorization server-side for ISO management.
- Never write to a shared master image from a client session. Super mode must use an isolated client-owned writable clone and a durable session/recovery record.
- Do not mount or alter live client disks on the server. Check active sessions, ownership, mount state, pending recovery, and mutation locks before storage changes.
- Preserve both targetcli and internal configfs backends, CHAP behavior, rollback, retry safety, and error reporting. File-backed ISO LUN support must be implemented and tested for both backends, not silently routed through block-device-only code.
- Only expose the game-disk clone as a LUN when `Use game disk` is enabled. When enabled, preserve its clone and setting but temporarily unmap its LUN during Super mode, then restore it on Save or Discard.
- A Super session remains active until an explicit Save or Discard succeeds. A failed or interrupted operation must leave a durable state that can be safely retried or recovered.
- Legacy `mode=super` without a new session record must not receive a Super boot menu. Require an explicit administrator action while the client is offline to convert/exit it; do not auto-convert.
- ISO upload must be streamed with a configured maximum, staged atomically, validated, checksummed, and stored with restrictive permissions. Never log upload contents or secret values.
- Use fake infrastructure and isolated temporary storage for tests. Regular tests must not touch real ZFS pools, iSCSI targets, system configuration, or client disks. Hardware boot testing is opt-in.
- Do not commit or deploy unless explicitly requested.

## Implementation Tasks

### 1. Add persistence models, migration, repositories, and upload configuration

**Touch points:** `src-tauri/migrations/`, `src-tauri/src/persistence/repositories/`, `src-tauri/src/core/config.rs`, application state/service wiring.

- [ ] Add the next ordered SQLite migration after the current latest migration for managed installer media and durable Super-session/recovery state.
- [ ] Persist ISO identity, display name, media type (`winpe` or `linux_live`), managed path, byte size, checksum, default status, and timestamps. Enforce at most one default ISO per media type in database/application logic.
- [ ] Persist Super-session identity, client, prior assigned image/revision, working clone, selected installer media, lifecycle state, and timestamps needed for restart recovery and idempotent Save/Discard. Add uniqueness/foreign-key constraints appropriate to the existing schema.
- [ ] Add typed repository APIs with transaction-aware operations. Keep file deletion outside database transactions and make cleanup retryable.
- [ ] Add typed installer-media upload configuration with a documented default maximum size and validation. Apply a route-specific request limit rather than relying on the global 16 MiB limit; retain an explicit streaming byte-count cap as defense in depth.
- [ ] Add migration and repository tests using isolated SQLite, including uniqueness/default rules, foreign keys, and resumable session state.

**Done when:** migrations apply to a fresh and existing test database, repositories round-trip the records, and configuration rejects invalid upload limits.

### 2. Implement safe ISO upload and catalog operations

**Touch points:** new `src-tauri/src/application/installer_media_service.rs` (or equivalent), infrastructure filesystem/ISO validation helper, `AppState` wiring.

- [ ] Stream multipart upload chunks to a uniquely named temporary file under the managed-media filesystem, enforcing the configured size limit without buffering the entire ISO in memory.
- [ ] Validate the upload as an ISO image using a bounded, documented structural check; reject empty, malformed, truncated, and unsupported media. Verify the declared media type against supported policy without assuming every live Linux image has identical metadata.
- [ ] Compute a cryptographic checksum while streaming or during bounded file processing; persist size and checksum only after successful validation.
- [ ] Atomically move validated files into the managed-media directory, set restrictive permissions, and insert catalog metadata with compensating cleanup if persistence fails.
- [ ] Implement list, upload, set-default, and delete operations. Refuse deletion while any active/recoverable Super session references the ISO. Do not follow symlinks or permit caller-selected filesystem paths.
- [ ] Test size boundary, upload interruption cleanup, malformed media, checksum, duplicate/default transitions, permissions where testable, and deletion refusal while referenced.

**Done when:** service tests prove failed uploads leave no catalog entry or partial managed ISO and successful uploads are stable across process restart.

### 3. Extend the iSCSI model and both backends for read-only ISO LUNs

**Touch points:** `src-tauri/src/infrastructure/iscsi/model.rs`, `targetcli.rs`, configfs engine and tests, storage service LUN assembly.

- [ ] Introduce an explicit backing type that distinguishes existing block-device LUNs from regular-file ISO LUNs. Validate backing type, path, LUN number, and read-only intent at the shared model boundary.
- [ ] Implement targetcli fileio backstores for ISO files and internal configfs file-backed `iblock`/fileio-equivalent objects supported by the existing backend design. Keep current block-device creation unchanged.
- [ ] Ensure the ISO is presented read-only through both backstores and mapped ACLs. Ensure writable client image LUN 0 semantics remain unchanged.
- [ ] Support a target with working-image LUN 0 and installer ISO LUN 1, while avoiding collisions with game LUN assignments. The Super target should omit game LUNs during the session; normal storage retains existing game LUN numbering behavior.
- [ ] Preserve cleanup and rollback on partial backstore/target creation, including file-backed resource cleanup without deleting the managed ISO file.
- [ ] Add fake-command and fake-configfs tests for fileio creation, read-only presentation, LUN ordering, invalid paths/types, partial failure rollback, and unchanged block-LUN behavior.

**Done when:** both backends pass equivalent behavioral coverage and neither backend can expose the managed ISO as writable.

### 4. Build the Super-session application workflow and recovery state machine

**Touch points:** new `src-tauri/src/application/super_client_service.rs` (or lifecycle extension), `image_service.rs`, `storage_service.rs`, `ApplicationServices`, `AppState`.

- [ ] Define explicit session states and allowed transitions for entering, active, saving, discarding, recovery-required, and completed workflows. Document idempotency and resource ownership invariants.
- [ ] On entry, validate client exists, has an assigned ready image, media type matches authoritative `os_type`, is offline with no active iSCSI session, and has no conflicting mutation or recovery operation.
- [ ] Acquire the existing mutation lock and create a durable session record before long-running changes. Clone the assigned image/revision into a unique client-owned writable working image. Never use the shared master as the writable target.
- [ ] Create the Super iSCSI target with working image LUN 0 and selected ISO LUN 1. If `Use game disk` is enabled, unmap game LUNs while preserving existing game clones and client preference; if disabled, do not create/expose game LUNs.
- [ ] Publish the Super session state and boot reservation only after storage is verified. On failure, rollback what is safe and retain recovery/session records for any incomplete cleanup.
- [ ] Save: snapshot the working clone, create a separate image-owned dataset/revision from that snapshot using existing image-service semantics, publish/register it as a managed image revision, assign it only to this client, rebuild normal client storage, and restore game LUNs only when enabled. Keep the session and working clone until the new assignment and normal target are verified; then clean up session-owned resources idempotently.
- [ ] Discard: restore the exact prior assignment, rebuild normal storage, conditionally restore game LUNs, then remove only resources owned by this session. Do not delete the prior image or retained game clone.
- [ ] Add startup/retry recovery that reconciles durable session states with image and iSCSI resources without releasing mutation protection while operations are still active.
- [ ] Add service tests with fake image/iSCSI infrastructure for entry validation, assignment isolation, game-disk on/off behavior, successful Save/Discard, each injected failure boundary, retry after restart, and ownership-safe cleanup.

**Done when:** each state transition is retryable, preserves the prior assignment until Save commits, and tests prove other clients remain pinned to their existing image revisions.

### 5. Add explicit handling for legacy Super-mode clients

**Touch points:** client lifecycle/application service, handlers, boot readiness, client management UI.

- [ ] Detect `mode=super` clients lacking a durable Super-session record and classify them as legacy/unmanaged Super state.
- [ ] Block Super-specific boot publication and new Super operations for those clients with a clear actionable error. Do not silently infer a working clone or media selection.
- [ ] Add an administrator-only explicit conversion/exit operation that requires the client to be offline, checks active sessions and recovery/mount state, and moves it to a valid managed normal assignment or an explicitly selected supported workflow.
- [ ] Preserve records and report recovery-required status if the conversion cannot safely complete.
- [ ] Add tests for legacy detection, online refusal, authorization, no automatic conversion, and successful explicit offline handling.

**Done when:** a legacy client cannot accidentally boot a Super menu or have its storage rewritten without explicit administrator action.

### 6. Make iPXE reservations and scripts Super-session aware

**Touch points:** `src-tauri/src/infrastructure/dhcp/isc_dhcp.rs`, `src-tauri/src/infrastructure/pxe/ipxe.rs`, `src-tauri/src/application/client_boot.rs`, `src-tauri/src/api/enroll.rs`.

- [ ] Extend boot reservation/menu input with validated mode and installer-media information from the durable session, not from client-supplied request fields.
- [ ] Generate the Super iPXE script to attach the verified working-image LUN 0 as a secondary SAN drive using `sanhook --drive 0x81`, then boot installer LUN 1 from the same target using `sanboot --no-describe`. Preserve the SAN description needed by the installer to find the target disk, and verify the command sequence with both supported media classes and firmware modes. Preserve existing normal Windows/Linux boot behavior and CHAP handling.
- [ ] Ensure generated per-client menus are refreshed when a session starts, saves, discards, or recovers. Do not let `ensure_client_menu` return a stale ordinary menu when the authoritative session state changed.
- [ ] Validate enrollment/readiness against active or recovery-required Super state. Missing ISO, inconsistent storage, or legacy state must fail safely and produce the shared API error response.
- [ ] Add script tests for Windows WinPE and Linux live ISO, exact LUN/drive ordering, CHAP behavior, normal path regression, missing media, and stale-menu prevention.

**Done when:** generated script content selects the installer only for a valid active session and normal boot output remains unchanged for ordinary clients.

### 7. Expose authenticated API endpoints

**Touch points:** new handlers under `src-tauri/src/api/handlers/`, `handlers/mod.rs`, `routes.rs`, `api/security_tests.rs`.

- [ ] Add admin-only ISO endpoints for upload, list, set-default, and delete. Apply a route-specific body limit, stream-level size cap, and shared error response normalization.
- [ ] Add client Super lifecycle endpoints for enter/start, status, Save, and Discard, plus the explicit legacy offline conversion/exit operation. Enforce ownership and authorization on the server.
- [ ] Make lifecycle endpoints reject conflicting active sessions, online clients, invalid OS/media pairing, invalid client assignment, and in-progress recovery with stable error codes/details.
- [ ] Ensure API responses never expose filesystem paths or secret CHAP credentials. Return only necessary ISO metadata and lifecycle state.
- [ ] Add handler/security tests for unauthenticated, non-admin, invalid-size, invalid-media, wrong-client ownership, active-session, and successful requests. Verify all errors use the shared contract.

**Done when:** API tests verify both authorization and observable lifecycle outcomes without invoking privileged real infrastructure.

### 8. Add the ISO management page and API client module

**Touch points:** `src/api/modules/`, API module exports, `src/router/router.jsx`, `src/components/layouts/navigation.js`, `Sidebar.jsx`, new page/components and tests.

- [ ] Add an API module following existing `apiRequest` conventions for catalog operations and multipart upload progress/state.
- [ ] Add an admin-only ISO management page to upload WinPE and Linux live ISOs, view validation/checksum/size/default status, choose a default per type, and delete only unused media.
- [ ] Display configured upload maximum and useful validation/error state. Keep UI restrictions supplemental to backend authorization.
- [ ] Add lazy route and navigation entry using existing auth/theme/component patterns; hide the entry from non-admins consistently with existing protected navigation.
- [ ] Add UI/API-module tests for upload result, media type selection, default change, referenced-delete refusal, loading/error states, and admin-only navigation.

**Done when:** an administrator can manage installer media without shell access, while a non-admin cannot reach the operation through direct API calls.

### 9. Add client controls for starting, saving, and discarding Super mode

**Touch points:** `src/components/ClientManagement/ClientTableRow.jsx`, `ContextMenu.jsx`, `src/hooks/useClientActions.js`, relevant client components/tests.

- [ ] Add the start-Super action only for clients with a compatible assigned image and available default installer media. Show clear unavailable/recovery/legacy states.
- [ ] For active sessions, show status plus explicit Save and Discard actions. Require confirmation before Discard and explain that it removes session changes while retaining the prior assignment.
- [ ] Keep Save/Discard disabled while online or while another mutation/recovery is active; treat UI state as advisory and enforce again server-side.
- [ ] Reflect game-disk behavior accurately: the checkbox/clone preference remains enabled during Super even though the game LUN is temporarily unmapped, and it is restored on completion.
- [ ] Add component/hook tests for available/unavailable media, online state, pending recovery, Save, Discard confirmation, legacy state, and game-disk preference retention.

**Done when:** operators can understand and control the full lifecycle without conflating normal image assignment, game-disk preference, and temporary Super-session state.

### 10. Run integrated verification and document opt-in hardware validation

- [ ] Run `cargo fmt --manifest-path src-tauri/Cargo.toml --check`.
- [ ] Run `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml` and report ignored tests or environment limitations.
- [ ] Run `bun run lint`, `bun run test --run`, and `bun run build`.
- [ ] Add an opt-in hardware verification procedure covering both targetcli and configfs, Windows WinPE and Linux live ISO, LUN0/LUN1 mapping, CHAP, Save/Discard, game-disk enabled/disabled, restart recovery, and a second client remaining on its original revision. Do not run against production or live client storage without explicit authorization.
- [ ] Inspect final diff to confirm only intended implementation files change during the later implementation task, review all cleanup/error paths, and report any checks that could not run.

## Dependency Order

```text
Persistence and configuration
        |
        +--> ISO upload/catalog service
        |
        +--> iSCSI file-backed LUN support
                    |
                    +--> Super-session lifecycle and recovery
                                  |
                                  +--> iPXE/enrollment integration
                                  +--> lifecycle API
                                  +--> legacy-state handling
                                            |
                                            +--> ISO management UI
                                            +--> client controls
                                                      |
                                                      +--> integrated verification
```
