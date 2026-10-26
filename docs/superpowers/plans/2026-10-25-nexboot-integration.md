# NexBoot-inspired lifecycle integration implementation plan

> For agentic workers: use subagent-driven-development or executing-plans, preserving the file ownership below.

**Goal:** Add usable installer sessions, per-client hardware identity caching, and native Windows save safety to the existing server.
**Architecture:** Reuse current ISO/client/Super storage workflows, Windows servicing worker, application state and shared safety checks. Independent modules own persistence/orchestration; root integrates routers, observation hooks and React controls.
**Tech stack:** Existing Rust/Axum/SQLx/Tokio, React/Bun and Windows PowerShell/native registry tools.
**Spec:** `docs/architecture/NEXBOOT-INTEGRATION-2026-10-25.md`.

## Global constraints

- No verbatim upstream code; no dependencies unless materially required.
- Preserve BIOS support, configfs and targetcli, shared locks, retained recovery records and protected credentials.
- All destructive operations validate ownership/offline/session state; native hive replay only on an offline candidate through the worker.
- No live infrastructure tests or deployment. Tests use fake backends, temporary files and SQLite.

## Tasks

- [x] Installer owner: migration 0023, new installer domain/repository/application/API modules and boot payload templates. Durable transitions, authenticated management and scoped progress reporting. Windows/Ubuntu/RHEL unattended recipes with disk identity and media prerequisite checks; explicit errors for unsupported combinations; interactive ISO path. Export router/service interfaces for root; do not edit central routes/state/mod files or existing client form.
- [x] Identity owner: migration 0024 and new hardware profile/cache repository/application/API modules. Stable profile observation, immutable client-bound source/profile keyed cache capture/apply, stale detection and restart/retry safety. Reuse image/storage services; export router and observation hooks for root. Do not edit central routes/state/mod files or frontend.
- [x] Windows safety owner: native offline hibernation/SYSTEM hive inspection/replay module, worker/remote protocol and Super save integration. Candidate-only recovery, private backups, deterministic job targets and verified release. Own SuperClientService, Windows worker and remote servicing files; coordinate additional shared file edits with root.
- [x] Root: review each contract, add central exports/routes/state and enroll observation hook, add React session/cache controls and authenticated API modules, update docs and migrations allowlist, integrate recovery/startup behavior.
- [x] For each task add behavioral regression tests and observe meaningful failures before fixes where feasible. Run formatting, full backend/frontend/worker checks, inspect platform-specific skips, and document actual supported workflows and remaining hardware limits.
