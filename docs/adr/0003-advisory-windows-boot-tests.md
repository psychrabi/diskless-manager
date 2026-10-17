# Advisory Windows boot tests

## Status

Accepted

## Context

ADR-0001 made matching-hardware boot tests a prerequisite for assigning Windows
driver revisions and selecting them as the default image. This prevented
administrators from assigning an available image to a client or using it for
installation, even when they intentionally wanted to test that image.

## Decision

Allow administrators to assign any valid image and select it as the default,
regardless of Windows driver revision status, NIC inventory, or boot-test
results. Keep the requested image and snapshot as the assignment. A successful
boot test continues to update diagnostic revision readiness, but does not
control assignment availability or rewrite the selected image.

Keep storage ownership, installer-media validation, and offline-client safety
checks. NIC inventory remains useful for selecting drivers during Windows
servicing. Administrators are responsible for confirming that an assigned
Windows image has a boot NIC driver; an image without one can still fail with
`INACCESSIBLE_BOOT_DEVICE`.

This decision supersedes the image-assignment and default-image restrictions
in ADR-0001 and the corresponding acceptance criteria in the managed Windows
network-driver specification. Boot-test reporting and revision readiness
remain in place for diagnosis.

## Consequences

- An image can be assigned or selected as the default before any client has
  booted it successfully.
- Boot-test results remain visible, but cannot prevent an assignment.
- Assigning an image with an incompatible or missing boot NIC driver can still
  leave a client unable to boot.
