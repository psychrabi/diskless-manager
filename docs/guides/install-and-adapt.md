# Install & Adapt: user guide

The **Install & Adapt** page (Infrastructure → Install & Adapt, administrators
only) installs an operating system onto a client's assigned disk and preserves
or restores a working Windows hardware identity.

Everything on this page requires the client to be **powered off**. The server
refuses to prepare an installation for a client that is online or still holds an
iSCSI session.

## 1. Add the installation ISO

Installer Media (Infrastructure → Installer Media):

1. Under **Upload an installer ISO**, enter a **Display name**, choose the
   **Media type**, and select the **ISO file**.
   - **Windows PE** – a bootable WinPE image you built yourself.
   - **Windows ISO** – Windows installation media; unattended Windows needs `sources/install.wim` on it.
   - **Linux ISO** – Ubuntu Server 24.04 amd64 live-server, or RHEL 9 x86_64 full DVD.
   - **Other ISO** – anything else you boot interactively.
2. Choose **Upload and validate**. The server checks the file size, structure,
   and SHA-256 checksum before the entry appears in the **Installer ISO catalog**.
   You may leave the page; the upload continues in the background.
3. Optionally choose **Set as default** on a catalog entry. The default ISO is
   used by the client form's preselection.

The catalog re-hashes the file every time an installation is prepared, so a
replaced or truncated ISO is rejected instead of being booted.

## 2. Assign the ISO to a client

Client Management (Infrastructure → Clients) → edit the client:

1. Under **Client boot mode**, select **OS installation** (instead of "OS boot").
2. Pick the image in **Select Image**. This is the disk the installer will
   write; the installer media is attached to the same target as read-only LUN 2.
3. Pick the ISO in **Installation image**.
4. Save.

The client form clears writeback, game-disk, and snapshot settings when you
switch to installation mode, because installation needs exclusive ownership of
the master volume. To install a client that is currently running a snapshot
rollback, first switch it back to "OS boot" and detach the snapshot.

## 3. Prepare the installation

Install & Adapt:

1. Select the client in the **Client** card. The card shows the assigned image,
   firmware, and current status.
2. In **OS installation**, choose the **Installation method**:

   | Method | Use it for | Requirements |
   | --- | --- | --- |
   | Interactive ISO boot | Any OS; you drive the installer console yourself | Any cataloged ISO and firmware. |
   | Windows installation | Unattended Windows onto the assigned disk | UEFI amd64, Windows ISO with `sources/install.wim`, and a deployed WinPE installer (see `src-tauri/src/infrastructure/pxe/installer/BUILD-WINPE.md`) |
   | Ubuntu unattended installation | Unattended Ubuntu Server | UEFI amd64, Ubuntu Server 24.04 amd64 live-server ISO |
   | RHEL / compatible unattended installation | Unattended RHEL 9 | UEFI amd64, RHEL 9 x86_64 full DVD |

3. For the Linux recipes, enter:
   - **Linux username** – lowercase letters, digits after the first character,
     `_` or `-`; at most 32 characters.
   - **SHA-512 password hash** – never a plaintext password. Generate one with
     `openssl passwd -6` (or `mkpasswd -m sha-512`) and paste the `$6$…` value.
4. Choose **Prepare installation** and confirm the warning. The confirmation
   states that the client's assigned installation disk will be erased.

Preparing a session does not touch the disk. It validates ownership, firmware,
media prerequisites, and disk size, then publishes a boot credential for this
client only. A second session cannot be prepared for the same client until the
first one is completed, and the client's installation assignment cannot be
changed while the session owns its disk. If the ISO is re-uploaded or the
disk is resized before the client boots, the server refuses to boot the session:
clear recovery and prepare again.

## 4. Boot the client

Power the client on and let it PXE boot. While a session is active the server
serves that session's installer instead of the normal boot image, and the
installer:

- checks that it is talking to the expected target and the expected disk size
  before partitioning anything, and
- reports progress back to this page.

The page refreshes every few seconds. Each session shows its state and
percentage, any recorded error, and an expandable **Installation events** list.

Unattended runs finish with the installer powered off. The session stops at
"installing"; completing it is a deliberate administrator step (see below).

## 5. Finish or recover

- **Cancel session** revokes the boot credential. It cannot stop an installer
  that is already writing, so power the client off afterwards.
- **Clear recovery** completes the server-side record. It is enabled for
  cancelled, failed, and recovery-required sessions, and it re-checks that the
  client is off and has no active storage session. The disk and the client's
  assignment are retained, so a failed installation can simply be retried.
- If the server restarts mid-installation, the session becomes
  **recovery_required**. Power the client off, then use **Clear recovery** and
  prepare a new session.

Cancelling or clearing recovery never deletes the disk, the assignment, or the
installation events.

## 6. Hardware identity

The **Hardware identity** card preserves a working Windows client so it can be
reapplied after a hardware or disk incident. All four actions need the client
**powered off**.

1. **Collect hardware profile** – asks the client to boot into WinPE inventory
   on its next boot and records the boot NIC hardware IDs, architecture, and
   firmware. The card then shows the observed profile.
2. **Capture identity** – creates an immutable baseline snapshot of this
   client's current master, keyed to the observed profile and the source image
   revision. Use it after Windows has booted and shut down cleanly on that
   machine.
3. **Apply identity** – builds a replacement disk for this client from the saved
   baseline and attaches it. The disk the client was using is kept for
   recovery. Apply refuses to run for another client, a different hardware
   profile, or a changed source revision.
4. **Remove identity** – restores the original client disk and deletes the
   cache.

A cache whose source image revision no longer matches the client's assignment is
shown as **stale**; apply is disabled until you capture again.

## Troubleshooting

| Message | What it means |
| --- | --- |
| `client must be powered off before preparing or closing installation` | The client is online or still holds an iSCSI session. Shut it down. |
| `client has active iSCSI sessions; power it off first` | An initiator login is still present on the target. |
| `client has no assigned installation media` | Switch the client to **OS installation** mode and pick an installation image. |
| `dedicated WinPE installer prerequisite missing` | Build and deploy the WinPE installer described in `installer/BUILD-WINPE.md`. |
| `Ubuntu recipe supports Ubuntu Server 24.04 amd64 live-server ISO only` | The uploaded ISO is a different release, architecture, or desktop image. |
| `RHEL recipe supports RHEL 9 x86_64 full DVD installation media only` | Use the full DVD installer, not a boot or minimal ISO. |
| `unattended recipes support UEFI amd64 only; use interactive mode for BIOS` | Switch the client's firmware to UEFI, or install interactively. |
| `installation requires an enabled, exclusive direct-master client without writeback, game disks or Super mode` | Remove the snapshot/writeback/game-disk/Super configuration first. |
| `client has an unfinished installation session; close recovery before retrying` | Finish the existing session with **Clear recovery**. |
| `remove or restore the client identity cache before changing Super mode` | Remove the identity cache, then retry the Super operation. |
| `installation source address does not match assigned client` | The client's IP changed after preparing; close recovery and prepare again. |
| `installation disk size changed` | The assigned volume was resized after preparing. Close recovery and prepare again. |
| `installation media revision changed` | The ISO was re-uploaded or replaced after preparing. Close recovery and prepare again. |
| `installation client assignment changed; recovery required` | The session's recorded target, media, address, or firmware no longer matches the client. Close recovery and prepare again. |
| `client has a reserved installation session; close offline recovery first` | The client's boot or installation assignment cannot change while a session owns its disk. Finish or clear the session first. |

## Known limits

- Unattended installation is UEFI/GPT only. BIOS clients keep normal and
  interactive boot.
- Windows unattended needs a WinPE installer you build and deploy; the server
  ships the instructions, not the media.
- ISO inspection on the server requires `7z`. `install.esd`-only Windows media
  is rejected; use an ISO with `sources/install.wim`.
- The identity cache covers boot NIC hardware IDs, architecture, and firmware.
  It does not rewrite NIC model configuration, and it is not portable between
  clients.
- Interactive ISO boot attaches the selected installer image as LUN 2.
