# Offline personalization of job-owned Windows clones

## Status

Accepted (opt-in; applied only to offline volume servicing)

## Context

One Windows base image serves many clients. It also inherits the identity it was
captured with: one computer name, possibly a static IPv4 configuration, and
`MountedDevices` drive-letter reservations for disks that a given client does
not have. Booting that image unchanged produces duplicate hostnames, clients
fighting over one address, and drive letters pointing at missing volumes.

Diskless Manager already prepares a clone and services it offline through the
Windows worker: driver injection, boot arming and reporter installation all run
on a SYSTEM hive owned by a servicing job, with dirty-hive refusal and a
job-unique iSCSI target. Personalization is the missing step in that pipeline.

## Decision

Add an opt-in `WindowsPersonalizationPlan` to offline volume servicing.

- The plan carries an optional computer name, an opt-in conversion of static
  IPv4 interfaces to DHCP, and an opt-in removal of non-`C:` drive-letter
  reservations. Every field defaults to "do nothing"; an empty plan is rejected
  before any clone is created.
- Computer names are validated as NetBIOS names (1-15 characters, letters,
  digits and hyphens, not all digits, not a reserved device name) and written
  uppercase to `ControlSetXXX\Control\ComputerName\ComputerName` plus the
  `Hostname` and `NV Hostname` values under `Services\Tcpip\Parameters`.
  `ActiveComputerName` is left alone: Windows derives it at boot.
- DHCP conversion only considers interfaces that carry a static IPv4 address
  and are not already DHCP. It sets `EnableDHCP=1` and clears `IPAddress`,
  `SubnetMask` and `DefaultGateway` for those interfaces. Interfaces with no
  static IPv4 address are reported and left untouched.
- Drive-letter cleanup removes only `\DosDevices\<letter>:` values where the
  letter is not `C:`. Volume identities, EFI entries and every other
  `MountedDevices` value are kept.
- The plan is executed by the trusted Windows worker on an offline hive, using
  the same `reg.exe` load/unload contract as reporter installation. There is no
  Linux `hivexregedit` shortcut and no in-place edit of the source image.
- A dry run returns the exact diff without writing. Inspection and application
  share one hive load, so a plan is never diffed against one state and written
  to another.
- Personalization is refused for WIM master preparation and for the running
  installation. A worker that does not advertise `personalization` refuses the
  plan before the image is cloned, so no revision is published with an identity
  the operator did not get.

## Consequences

- A serviced clone can be published as an identity-neutral image (DHCP, cleaned
  drive letters) or renamed for a single-purpose master.
- Static-to-DHCP conversion is opt-in per servicing run: a network that depends
  on static configuration for a specific interface is unaffected unless the
  operator asks for it. The boot NIC must still reach the iSCSI target through
  DHCP or an iBFT-provided address; this change does not alter boot arming.
- The diff is part of the API result, so an operator can review the planned
  changes before committing, and a partially applied cleanup is reported as a
  warning rather than silently dropped.
- Per-client plans and automatic personalization of a client's boot clone or
  prewarmed spare are **not** implemented. `prepare_spare_clone` and the boot
  provisioning path remain the extension points, and applying personalization
  there needs a measured cost on the boot critical path first.
