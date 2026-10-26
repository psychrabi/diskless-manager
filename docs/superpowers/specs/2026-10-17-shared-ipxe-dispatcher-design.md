# Shared iPXE boot dispatcher

## Status

Approved by the user on 2026-10-17. Implementation is complete; compile-only
checks passed, with automated test execution and hardware validation pending.
The user approved returning managed boot scripts from the shared dispatcher
while preserving administrator-authored per-client menus as overrides.

## Goal

Make the shared iPXE entry point the only managed boot-policy entry point.
Every client should ask the server for its current boot decision, and the
server should return the final iPXE script for that client. Managed boot menus
should no longer need to be generated and maintained as separate files under
`clients/<mac>.ipxe`.

## Existing behavior

- DHCP directs clients to the same `autoexec.ipxe`.
- `autoexec.ipxe` calls the dedicated listener at `/enroll/{mac}`.
- The listener handles unknown/disabled clients, inventory requests, and
  assigned-client storage readiness.
- On success, it returns a small script that chains to the generated
  `clients/<mac>.ipxe` file.
- Client provisioning, service startup, NVMe preparation, enable/disable, and
  Super-session transitions also publish or remove those per-client files.
- Custom per-client menus are preserved by recognizing files without the
  manager-generated markers. Super-session code backs them up and restores
  them across a session.

The shared entry point and server-side boot decision already exist. The main
change is to return the final managed script from that decision instead of
redirecting to a generated menu file.

## Approaches considered

1. **Keep generated files and improve their refresh path.** This is the
   smallest change, but clients still depend on per-client menu generation and
   stale files can disagree with current database state.
2. **Return a generated menu file's contents from the dispatcher.** This
   removes the second HTTP request but keeps generated files and their
   lifecycle, so it does not provide the shared-dispatcher model.
3. **Render the managed boot script from persisted state in the dispatcher.**
   This removes manager-generated menu files and their publish/remove lifecycle.
   It touches more callers but makes the database and current session state
   authoritative. This is the selected approach.

## Design

### Shared request and server decision

Keep the DHCP filename and shared `autoexec.ipxe` flow. Keep the existing
`/enroll/{mac}` path so already-published dispatcher scripts continue working.
After parsing the MAC, the listener selects one response in this order:

1. Reject invalid MACs and deny disabled registered clients.
2. Consume a pending inventory request and return the WinPE inventory script.
3. For an unknown MAC, apply the current enrollment-window rules, register it
   as pending when allowed, and return the pending or closed script.
4. For an enabled registered client without an assigned image, return the
   pending script.
5. For an assigned client, acquire the existing per-client mutation lock,
   perform the existing storage-readiness and recovery checks, then return the
   normal, custom, or Super-session boot script.

The dispatcher remains the only normal PXE route. Boot-script generation does
not add a second storage-provisioning path; it reuses the current readiness
operation and its safety checks. The bounded lock wait continues to return the
existing retry script.

### Managed scripts and custom overrides

Use the existing iPXE renderers and persisted client, target, CHAP, firmware,
image, and Super-session data to build the final managed script in memory.
Inventory, pending, disabled, closed, and retry scripts remain distinct where
their boot behavior differs.

Administrator-authored files at `clients/<mac>.ipxe` remain explicit
overrides for otherwise bootable clients. The server checks registration,
enabled state, inventory state, and storage readiness before serving an
override. Generated files identified by the existing manager markers are
ignored as boot policy. Client deletion may still remove its custom override.

Super-session state is selected from the persisted session on each request.
An active, verified session returns its installer script; invalid or
recovering sessions fail closed. Normal operation returns the custom override
when present, otherwise the current managed boot menu. Existing backup records
for custom menus must remain recoverable during rollout; completing a Super
session must not discard the original custom script.

### iPXE behavior and compatibility

- A successful `/enroll/{mac}` response contains the final script, not a chain
  to `clients/<mac>.ipxe`.
- The inventory-failure path re-enters the shared dispatcher instead of
  chaining to a generated per-client file. The one-shot inventory request has
  already been consumed, so the next response follows current client state.
- The NVMe-capable firmware continues to re-enter the shared `autoexec.ipxe`;
  its runtime capability flag remains intact across dispatch.
- Existing generated menu files are no longer read as managed policy. During
  rollout, remove only files with recognized manager-generated markers. Leave
  unrecognized/custom files untouched.
- Keep the existing `/inventory/{mac}` upload endpoint and DHCP reservation
  behavior.

### File and service lifecycle

Remove manager-generated menu publication from client provisioning, normal
boot readiness, cached-menu regeneration, NVMe preparation, and Super-session
refresh. The DHCP publisher should manage DHCP reservations without requiring a
per-client menu file to exist first. Enable/disable decisions are enforced by
the database check in the dispatcher, not by deleting or recreating a menu.

Continue to retain custom menu files and the existing safe path checks. Keep
client-deletion cleanup for the matching custom file. Do not expose generated
scripts as static files after migration. The shared `autoexec.ipxe` remains a
single published file.

### Response headers, errors, and security

Return iPXE scripts as `text/plain; charset=utf-8` with `Cache-Control:
no-store`, because a response can contain client-specific target and CHAP
settings. Do not log script bodies or credentials. Preserve the current MAC
validation, enabled-client gate, enrollment-window gate, owner validation,
storage ownership checks, bounded lock wait, and structured server diagnostics.

The dispatcher listener remains unauthenticated for firmware compatibility.
As today, it must be reachable only on the intended PXE network. A client MAC
is a routing key, not proof of identity; no stronger identity claim is added by
this design. CHAP remains supported as it is today.

If a lookup, storage readiness, or session-validation step fails, return the
existing retry/deny iPXE response and log a structured server-side cause. Never
return partial scripts or silently fall through to an older generated file.

## Rollout and compatibility

1. Publish the updated common `autoexec.ipxe`; DHCP reservations do not need a
   new filename or per-client configuration.
2. Keep `/enroll/{mac}` available and switch its success response to a final
   script, so clients using the current shared dispatcher do not need a
   firmware or DHCP change.
3. Stop creating new manager-generated per-client files. Remove only files
   whose content has a recognized manager-generated marker; preserve custom or
   unrecognized files.
4. Preserve custom-menu backup data until it has been restored or is no longer
   associated with an active Super session. Do not clear backup metadata as a
   side effect of a failed response.

## Acceptance criteria

1. All clients continue to enter through the same DHCP filename and
   `autoexec.ipxe`.
2. The dispatcher returns the appropriate final script for unknown, disabled,
   pending, inventory-requested, normally assigned, active Super-session, and
   recovery-blocked clients.
3. Normal boot requests no longer require a generated
   `clients/<mac>.ipxe` file or a second HTTP fetch.
4. Inventory collection, target readiness, CHAP, firmware mode, custom menus,
   and Super-session behavior remain correct.
5. Disabling a client prevents a boot through the dispatcher without relying
   on menu-file deletion. Deleting a client still removes its custom override
   safely.
6. Existing manager-generated files are never used as stale fallback policy;
   custom/unrecognized files are preserved.
7. Personalized responses are not cached, and secrets/script contents are
   absent from logs.
8. Both targetcli and configfs provisioning remain compatible because target
   creation stays behind the existing storage service.

## Verification plan

- Add focused regression coverage for each dispatcher branch, direct delivery
  of a managed script, override precedence, one-shot inventory, Super-session
  selection, and no-store response headers.
- Verify callers no longer require manager-generated files before DHCP or boot.
- Run the repository's Rust formatting, lint, and test checks, plus frontend
  checks if any frontend code is touched.
- Hardware validation remains opt-in: test UEFI and BIOS only where the
  installed clients and boot firmware are available, including inventory and
  Windows iSCSI boot with and without CHAP.

## Scope and non-goals

- Do not change the Windows servicing, driver catalog, installer-media
  validation, client enrollment UI, or DHCP address-allocation policy.
- Do not add a new client configuration format or a new dispatcher listener.
- Do not remove administrator-authored menu overrides.
- Do not copy NSBoot's request-time target creation. Storage preparation stays
  in the existing application/storage service and its recovery model.
- This design centralizes policy delivery; it does not eliminate the distinct
  scripts required for different boot states.
