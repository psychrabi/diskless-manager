# Verified image factory

The administrator-only **Image factory** page builds a new image without writing to a client's assigned image. Factory sessions create a private draft, install and adapt an OS onto it, snapshot it, then boot a disposable verification clone. The draft becomes assignable only after its normal boot reporter is accepted by the server.

## Prepare a factory image

1. Add and validate installer media in **Installer Media**.
2. In **Image factory**, select an enabled, powered-off client and validated media.
3. Choose Windows, Ubuntu, or RHEL, then set the final image name and size. Linux recipes also need an administrator username and SHA-512 password hash.
4. Confirm. The server checks the media, client ownership, firmware, size, and storage state before creating the draft and its isolated installer target.
5. PXE boot the client. The installer writes only to the new draft. Windows WinPE reports the boot NIC hardware IDs, receives a bounded driver bundle selected from the imported network-driver catalog, and runs the shared offline adaptation; Linux finalize writes client-agnostic iBFT boot configuration and installs the verification reporter. The installer acknowledges the server when adaptation finishes.
6. The server waits for the installer target to disconnect, saves a snapshot, and boots a disposable verification candidate. A successful normal OS report publishes the draft. A failed or timed-out attempt can be retried; it cannot be overridden.

Publishing leaves the client's existing assignment in place by default. If **Assign to this client after verification** was selected, the server waits for the client to power off, cleans up the verification resources, then assigns the published image through the normal storage reconciliation path and records the result in the session. If assignment fails, the image stays published and the failure is shown for administrator action; it is not rolled back.

## Manual ISO boot

Choose **Interactive ISO boot** to access an installer or recovery tool. The server attaches a new scratch disk at LUN 0 and the selected ISO read-only at LUN 1. Manual sessions never publish an image. Close the session after powering off the client; the scratch disk is discarded.

## Recovery

The session card shows progress, phase deadline, verification attempt, published image, assignment result, and event history. Cancel revokes the boot credential but cannot stop an installer already running. Power off the client before closing recovery or retrying cleanup. The server checks active iSCSI sessions and mounted storage before removing session-owned resources. A cleanup failure keeps the session reserved and exposes a retry action.

After a server restart, interrupted preparation/installer sessions require administrator recovery. Saving resumes automatically. Published cleanup resumes after restart and waits for the verification client to shut down.

## Limits

- Unattended recipes require UEFI amd64 and the documented supported Windows, Ubuntu, or RHEL media.
- Windows adaptation injects only boot NIC drivers that exist in the imported catalog. When nothing matches, adaptation continues with in-box drivers and verification is the gate.
- Live WinPE, ZFS, LIO, and firmware behavior requires an explicitly opted-in hardware test; automated checks use fakes.
