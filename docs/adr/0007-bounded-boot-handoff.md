# Bounded iPXE-to-OS boot handoff

## Status

Accepted

## Context

ADR 0002 starts the non-persistent reset timer whenever the server observes no
iSCSI session for a client. iPXE closes its session the moment the OS kernel or
Windows opens its own to the same LUN, so during every boot there is a gap with
no session. Long installers and first Windows boots widen that gap past the
reset delay, and the sweep then deletes the volume the client is booting from.

The server cannot tell an iPXE session from an OS session: LIO reports the same
initiator IQN for both.

## Decision

Record a persisted, bounded boot handoff per client in `client_boot_handoffs`.

- The handoff is armed immediately before the boot script leaves the server,
  with the lifecycle fingerprint of the revision that was served.
- Phases are `script_served`, `session_observed` and `expired`. Phases stop at
  "a session was observed"; a verified OS boot needs a client agent and is never
  inferred from a session.
- While an attempt is live and the session is confirmed absent, the lifecycle
  suspends the offline deadline instead of starting it. Every other safety gate
  is unchanged: a live iSCSI session keeps the client online, and destructive
  steps still require a confirmed disconnection, no NVMe export and a passing
  ownership check.
- Repeated PXE requests inside an open window keep the original attempt and its
  deadline, so a boot loop cannot hold storage indefinitely. A closed window
  starts a new attempt.
- Expiry is bounded from `started_at`
  (`client_lifecycle.boot_handoff_seconds`, 30-900, default 300). A client still
  connected when the window closes is simply marked observed. A client with a
  confirmed-absent session gets one boot-failure diagnostic per attempt, which
  the existing boot-failure and alarm surfaces already report.
- Unknown session state never expires an attempt: a failed or unanswerable LIO
  query leaves the attempt open, because treating unknown state as idle is what
  this record exists to prevent.
- A confirmed session, or a deleted client, clears the record.
- `GET /api/clients/{id}/boot-handoff` exposes the live attempt, its phase, age
  and remaining time for diagnostics, and states explicitly that the boot is
  inferred rather than confirmed by the guest.

## Consequences

- A boot no longer races the reset timer, including installer and first-boot
  cases that legitimately take minutes.
- A client that never reaches the OS stops suspending its deadline when the
  window closes and is reported as a boot failure instead of silently staying
  online.
- The window is bookkeeping, not an authorization: a failed write only logs a
  warning, because storage has already been published and deletion still
  requires the full set of confirmed-absent checks.
- Clients without an iPXE boot through the server never arm a handoff and keep
  the ADR 0002 behaviour unchanged.
