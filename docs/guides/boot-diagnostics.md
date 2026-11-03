# Boot and iBFT diagnostics

`GET /api/clients/{id}/boot-diagnostics` (admin only) reports the boot inputs
the server will use on the client's next PXE request. It is read-only: it does
not start a boot, change storage, or alter the gateway.

## Fields

| Field | Meaning |
| --- | --- |
| `firmware` | The client's recorded PXE mode (`uefi` or `bios`). |
| `assigned_image` / `os_type` | The assigned image name and its OS when the catalog can classify it. |
| `target_iqn` | The iSCSI target persisted for this client. |
| `initiator_iqn` | The iPXE initiator IQN the boot script sets. |
| `keep_san` | Whether the script keeps the SAN attachment (`set keep-san 1`). |
| `chap_enabled` | Whether one-way CHAP credentials are emitted into the boot script. |
| `ibft_gateway` | The literal `set net0/gateway` value in the script. `0.0.0.0` means the Windows SAN-boot path overrides the DHCP gateway; `dhcp` means the script does not. |
| `boot_nic_ids` | Boot NIC hardware IDs from the client's most specific recorded report. |
| `hardware_architecture` / `hardware_firmware` | Architecture and firmware recorded with that report. `hardware_firmware` is empty for a Windows client report, which does not carry it. |
| `hardware_source` / `hardware_observed_at` | Which report supplied those rows and when. |
| `handoff` | The live iPXE-to-OS attempt (`attempt_id`, `phase`, age, remaining window, `active`) or `null`. |

The response always carries `read_only: true`, and the endpoint never returns
CHAP secrets or boot tokens.

Hardware rows use the same precedence as the Windows servicing path, so the
panel describes a client as soon as any of these exist:

1. `hardware_profiles` — an explicit identity observation (`identity profile`).
2. `windows_driver_reports` — a verified report from the booted Windows client (`windows client report`).
3. `windows_nic_inventory` — the MAC-keyed preboot inventory from the installer WinPE (`WinPE inventory`).

A client with none of these has never booted the installer or run the client
report tool. Recording inventory requires contact with that machine; the server
does not invent hardware IDs, because they select NIC drivers and key identity
baselines.

## iBFT gateway experiment (opt-in lab work)

The Windows SAN-boot path sets `net0/gateway 0.0.0.0` so Windows does not route
iSCSI traffic through the default gateway; Linux paths rely on DHCP. A
per-client gateway override is **not implemented**. Adding one belongs in an
isolated lab, because changing the iBFT gateway can make a bootable client
unreachable:

1. Use a disposable UEFI client and disposable image, never a production
   assignment. Record the existing diagnostics response.
2. Compare a boot with the current value against a boot with the gateway cleared
   or set to the DHCP-provided address, one change at a time, with `set keep-san`
   unchanged.
3. Verify the target sees the Windows initiator reconnect, and that the guest
   keeps the target reachable after the iSCSI session is established.
4. Roll back by restoring the original definition. A future override must be
   time-bounded and auto-expire so a forgotten experiment cannot persist.

Do not change production defaults from a single test machine. Record the exact
value, firmware, NIC model/revision, Windows build and result in the lab notes.

## Known limits

- The handoff phase is inferred from session presence, never from an OS
  heartbeat; `os_confirmed` is always false.
- SMART, scrub/TRIM and retention diagnostics are managed separately under
  `GET /api/system/storage/maintenance` and `GET /api/system/storage/retention`.
- A resident Windows agent, heartbeat reporting and remote actions remain
  unimplemented: they require a signed Windows service and a lab client, and
  must not be claimed as verified from unit tests.
