# Windows Driver Reporting and Trusted Catalog Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Let admins import signed vendor packages and let working managed Windows clients report their Boot NIC and contribute installed network-driver packages.

**Architecture:** Reuse the filesystem-backed network-driver catalog and INF parser. Add a client report contract bound to an active registered iSCSI session, and extend the Windows helper/tooling to export packages without putting credentials or package contents in logs.

**Tech Stack:** Rust 2021, Axum, Tokio, SQLite, existing ZIP/INF support, Windows `pnputil`, React API modules.

**Spec:** `docs/superpowers/specs/2026-10-12-managed-windows-network-drivers.md`.

## Global Constraints

- Reports are tied to a registered client and its active iSCSI session; a MAC address or request body client ID alone is insufficient.
- Only Windows network-adapter packages are accepted; retain their original INF, catalog, and payload files.
- Invalid signatures and incompatible device/OS/architecture metadata cannot enter a test revision.
- Do not add a shared credential to the common image or log credentials/package payloads.
- Keep both iSCSI target backends and their shared safety checks working.

---

### Task 1: Provision named initiator ACLs and read session peers

**Files:**
- Modify: `src-tauri/src/infrastructure/iscsi/mod.rs`
- Modify: `src-tauri/src/infrastructure/iscsi/targetcli.rs`
- Modify: `src-tauri/src/infrastructure/iscsi/configfs/engine.rs`
- Modify: client storage and iSCSI target specifications to carry the expected initiator IQN
- Modify: `src-tauri/src/infrastructure/pxe/ipxe.rs` only if a shared IQN builder is needed
- Test: the corresponding backend parser/unit tests

**Interfaces:**
- Client target specifications carry the same per-client initiator IQN emitted by iPXE (`iqn.2026-01.client:client.<slug>`).
- Both backends ensure a named ACL exists for that initiator and maps the target's LUNs, while retaining generated ACL compatibility. This is a separate additive operation: it may promote the active dynamic ACL in place, but must not change target, LUN, backstore, or CHAP configuration or weaken the existing guards on those mutations.
- Produces a shared operation that returns remote peer addresses for active sessions on one target's named ACL; existing boolean active-session checks keep their behavior.

- [x] Add tests that the provisioned initiator IQN matches the iPXE contract and that both backends ensure its explicit ACL without removing generated ACL compatibility.
- [x] Add a parser test where the named ACL has two active connections and assert both normalized IP addresses are returned.
- [x] Add a parser test for an inactive target and malformed session output; inactive yields an empty list and malformed output returns an error.
- [x] Run `cargo test --manifest-path src-tauri/Cargo.toml iscsi` and confirm the new parser tests fail before implementation.
- [x] Implement the separate additive named-ACL operation in both targetcli and configfs provisioners, invoke it during client target reconciliation, then read the named ACL's configfs session details.
- [x] Rerun the focused tests and add a fake-backend test showing the shared operation returns the peer list unchanged.
- [x] Document the opt-in LIO test that verifies the generated ACL is visible to configfs and returns the actual session peer address; do not run it on production targets.

### Task 2: Record authenticated Windows reports

**Files:**
- Create: `src-tauri/migrations/0007_windows_driver_reports.sql`
- Modify: `src-tauri/src/api/handlers/network_drivers.rs`
- Modify: `src-tauri/src/api/routes.rs`
- Modify: `src-tauri/src/infrastructure/pxe/driver_injection.rs`
- Test: `src-tauri/src/api/security_tests.rs` and driver catalog tests

**Interfaces:**
- Add an authenticated report request carrying client ID, Windows version/architecture, Boot NIC hardware IDs, and an optional exported package archive.
- Persist the latest report against the registered client. Reject it unless the request peer address matches an active session for that client's iSCSI target.
- Store harvested packages through the same safe ZIP extraction and INF inspection path as admin imports.

- [ ] Add tests for unknown client, no active target session, mismatched peer IP, and valid matching peer; all rejected cases must leave the report and catalog unchanged.
- [ ] Add catalog tests for a valid network INF, a non-network INF, missing catalog file, and malformed archive paths.
- [ ] Run the focused API and catalog tests and confirm they fail because report storage and peer binding do not exist.
- [ ] Add the migration and request/response types; constrain archive size using the existing API multipart/request limits.
- [ ] Implement the report endpoint with `ConnectInfo<SocketAddr>`, registered-client lookup, active-session peer verification, and existing catalog validation.
- [ ] Rerun the focused tests; verify response errors use the shared API error contract and do not expose credentials or archive paths.

### Task 3: Export and report from a working Windows client

**Files:**
- Modify: `tools/windows-servicer/Cargo.toml`
- Modify: `tools/windows-servicer/src/main.rs`
- Create: `tools/windows-servicer/src/windows_client_report.rs`
- Create: `tools/windows-servicer/client-report/`
- Modify: `tools/windows-servicer/install.ps1`
- Test: Windows helper unit tests and an opt-in Windows client integration procedure

**Interfaces:**
- Add a `report-client` helper mode that enumerates the Boot NIC, reports the running Windows version/architecture, and exports the corresponding third-party network-driver package with `pnputil /export-driver`.
- The scheduled startup task/service configuration invokes the reporter with a protected per-client configuration; no shared secret is embedded in the common image.

- [ ] Add parser tests for the report JSON contract and tests that `pnputil` export failures do not produce a successful report.
- [ ] Run `cargo test --manifest-path tools/windows-servicer/Cargo.toml windows_client_report` and confirm they fail before adding the command.
- [ ] Implement the helper command using bounded subprocess execution, temporary export directories, and structured JSON output; never log archive contents or credentials.
- [ ] Add installer/configuration support for enabling client reporting in a managed image while preserving existing worker-only installation behavior.
- [ ] Run helper formatting, clippy, and unit tests; document a Windows 10/11 test for exporting one signed NIC package and sending it to a fake report endpoint.
