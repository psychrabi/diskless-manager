# Managed Windows Driver Rollout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Make new Windows 10/11 diskless clients use an iSCSI boot image revision containing a signed, boot-tested driver for their Boot NIC.

**Architecture:** Keep the existing network-driver catalog and managed Windows servicing flow. Add authenticated client reporting and preboot NIC inventory, then build immutable revisions from compatible packages and gate new client assignment on a matching-hardware boot test.

**Tech Stack:** Rust 2021, Axum, Tokio, SQLx/SQLite, existing network-driver INF parser and Windows servicing worker, React/JavaScript, Bun.

**Spec:** `docs/superpowers/specs/2026-10-12-managed-windows-network-drivers.md`; decision record: `docs/adr/0001-test-gated-windows-driver-revisions.md`.

## Global Constraints

- Manage network adapter drivers only; require the Boot NIC driver, while secondary NIC drivers remain optional.
- Accept administrator-imported and working-client-collected packages; require a valid Windows signature and matching device ID, OS, and architecture.
- Do not use DISM's unsigned-driver override.
- Do not mutate an image revision already assigned to a client.
- Publish a candidate for new clients only after the designated matching-hardware test reports the exact revision and Boot NIC and the server confirms the iSCSI session.
- Preserve the last known-good revision and hold clients without a tested Boot NIC match before Windows boot.
- Authenticate reports to the registered client and its active iSCSI session; MAC or caller-supplied client ID alone is not authentication.
- Keep privileged operations in Rust, client package data and credentials out of logs, use fake infrastructure in tests, and make real hardware tests explicitly opt-in.
- Do not commit or deploy unless requested.

---

## Workstream order

These plans are sequential because the later work consumes the earlier contracts:

1. [Client driver reporting and trusted catalog](./2026-10-12-windows-driver-reporting.md)
2. [Automatic image revisions](./2026-10-12-windows-image-driver-revisions.md)
3. [Preboot inventory and boot-test gate](./2026-10-12-windows-boot-test-gate.md)

### Integration acceptance

- [ ] A working managed client can report its Boot NIC and export a signed network driver package into the catalog.
- [ ] A preboot inventory for a new client selects only packages matching its Boot NIC, Windows version, and architecture.
- [ ] A candidate Windows image is serviced as a new revision; its source and existing client assignments remain unchanged.
- [ ] A matching designated test client can report a successful boot for the exact candidate revision, with an active iSCSI session confirmed by the server.
- [ ] New client creation refuses an untested or missing Boot NIC driver and explains which hardware ID needs a package.
- [ ] New clients use the newest tested revision; the prior known-good revision remains available.
- [ ] Windows ADK/WinPE hardware verification is documented and opt-in; no generated WinPE image or live client disk is part of automated tests.

### Final checks

Run the checks required by `AGENTS.md` after the integrated work is complete:

```sh
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
bun run lint
bun run test --run
bun run build
cargo fmt --manifest-path tools/windows-servicer/Cargo.toml --check
cargo clippy --manifest-path tools/windows-servicer/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path tools/windows-servicer/Cargo.toml
```

Run a real boot only on the designated disposable test client, after a candidate image and matching NIC have been selected.
