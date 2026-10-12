# Diskless Manager contributor guidelines

## Existing architecture

- This is a Linux management server with a browser UI, not a Tauri desktop app.
  `src-tauri/` is the historical directory name for the Rust server.
- Backend: Rust 2021, Axum, Tokio, SQLx/SQLite, serde/serde_json, and
  tracing/log. Keep HTTP handlers in `api`, orchestration in `application`,
  domain rules in `domain`, and system integrations in `infrastructure`.
- Frontend: React, JavaScript/JSX, Vite, Tailwind CSS, existing shadcn/Base UI
  components, React Hook Form, and Zod. Use Bun for frontend tooling.
  Reuse the existing components, auth context, theme, and toast patterns.
- Client storage uses ZFS and Linux LIO iSCSI, with targetcli and internal
  configfs backends. Preserve both backends and their shared safety checks.
- `tools/windows-servicer/` is a separate Rust executable for Windows.
- Deployment consists of the Rust executable, `dist/`, the systemd unit,
  and the installer under `packaging/`. Do not reintroduce Tauri packaging.

## Scope and implementation

- Make the smallest complete change that solves the requested problem.
  Trace callers, configuration, tests, and external side effects before editing.
- Preserve unrelated working-tree changes. Do not commit or deploy unless asked.
- Prefer existing helpers and installed dependencies. Add a dependency only
  when it substantially improves correctness or removes meaningful complexity.
- Keep functions focused. Use a request/configuration struct when several
  related parameters obscure the call. Avoid needless wrappers and boilerplate.
- Optimize demonstrated bottlenecks. Do not claim code is "fully optimized"
  without a defined workload and measurements. Parallelization, SIMD, Rayon,
  and specialized dataframe libraries are choices, not universal requirements.

## Rust and asynchronous work

- Use Rust naming conventions and rustfmt. Prefer borrowing where practical.
- Use `Result`, `thiserror` for typed errors, and `anyhow::Context` for
  orchestration failures. Never silently discard a data-loss or cleanup error.
- Do not use `.unwrap()` in production. Use explicit error propagation or
  a descriptive `.expect()` only for a proven invariant. Test assertions may
  use `.unwrap()` and `.expect()`.
- Use Tokio processes for asynchronous commands, or `spawn_blocking` for
  synchronous system operations, filesystem work, and CPU-heavy bcrypt work.
  Never run long blocking work directly on an async request worker.
- Preserve mutation locks, rollback, and recovery records across asynchronous
  boundaries. A timed-out request must not release a mutation lock while its
  detached operation is still changing client storage.
- Inject configuration and infrastructure through shared application state;
  do not create mutable global configuration caches. Immutable process-wide
  signing material and compile-time constants are allowed.
- Use structured tracing/log records for server diagnostics. CLI usage,
  machine-readable stdout protocols, and diagnostics before logging starts may
  write directly to stdout/stderr. Never log command secrets or input payloads.
- Document public contracts, non-obvious invariants, side effects, and failure
  behavior. Self-evident getters and field names do not need boilerplate prose.
- Avoid wildcard imports outside tests and intentional module re-exports.
  Keep unsafe code isolated and explain the safety invariants when it is needed.

## API and frontend contracts

- Keep handlers async and use Axum extractors and `AppState`.
- All HTTP API errors must use the shared `code`, `message`, `operation_id`,
  and `details` JSON contract. Typed handler return values are allowed;
  normalize legacy responses at the HTTP boundary.
- Enforce authentication, authorization, validation, request limits, and
  storage ownership checks on the server. UI restrictions are supplemental.
- Keep heavy computation and privileged operations in Rust. Render and manage
  user interaction in React. Preserve accessible controls and light/dark themes.
- Keep UI details out of implementation-specific workflows unless they help
  the user make a meaningful decision.

## Secrets and storage safety

- Never commit production credentials, keys, tokens, or real user data.
  `.env` must remain ignored. Synthetic test credentials are allowed.
- Configured secrets may come from environment variables or protected files.
  Generated JWT signing material and per-client CHAP credentials may persist
  in the protected runtime directory/database because unattended boot needs
  them. Preserve restrictive permissions and backups' existing protections.
- Redact secrets from `Debug`, API responses, logs, and command arguments.
  Explicit admin-only credential rotation responses are an intentional exception.
  Use a secret wrapper when it improves protection; do not add a crate merely
  to satisfy a dependency checklist.
- Never mount or alter a live client's disk on the server. Check active sessions,
  ownership, mount state, and pending recovery before destructive operations.
- Keep provisioning metadata atomic. Retain client/recovery records when
  cleanup fails, and make retries safe for resources already removed.

## Tests and verification

- Add behavioral regression tests for security, parsers, transactions, recovery,
  and other non-trivial changes. Do not mirror implementation details in tests.
- Use fake infrastructure for privileged operations and remote machines.
  Real in-memory SQLite and isolated temporary files are encouraged for
  transaction, migration, and filesystem integration tests.
- Tests must not modify live ZFS pools, iSCSI targets, system configuration,
  or client disks. Hardware tests must be explicitly opt-in.
- Run appropriate checks and inspect their output before claiming success:

  ```sh
  cargo fmt --manifest-path src-tauri/Cargo.toml --check
  cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
  cargo test --manifest-path src-tauri/Cargo.toml
  bun run lint
  bun run test --run
  bun run build
  ```

- When changing the Windows helper, also format, lint, and test its own
  Cargo manifest. Linux tests do not establish Windows runtime behavior.
- Report checks not run, ignored tests, known failures, and hardware limits.
  Do not weaken lint or test assertions merely to obtain a passing result.
- Run benchmarks only when relevant, in release mode, without competing
  benchmark runs or machine-specific flags that distort the comparison.
- Python, WASM, and terminal UI tooling requirements apply only if a requested
  change actually introduces those technologies. Do not add them to this stack
  or rebuild nonexistent packages.

## Agent skills

### Issue tracker

Issues are tracked in GitHub Issues with `gh`; PRs are not a triage surface. See `docs/agents/issue-tracker.md`.

### Triage labels

Use the default labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, and `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Use the single-context layout: root `GLOSSARY.md` and `docs/adr/`. See `docs/agents/domain.md`.
