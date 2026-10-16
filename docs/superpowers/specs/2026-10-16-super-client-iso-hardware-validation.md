# Super-client ISO hardware validation

This procedure is opt-in. Run it only in an isolated staging installation with disposable clients, test images, and storage. Never point it at production or a live user's client disk. The test client must install to its session-owned writable clone; the shared image remains read-only.

The procedure validates firmware and hardware behavior that automated tests cannot prove. It does not replace the fake-infrastructure test suite.

## Before testing

1. Record the application version/commit, selected iSCSI backend (`targetcli` or `configfs`), server address, test-client IDs/MACs, and the image revision assigned to each test client.
2. Use a disposable test pool and a second client assigned to the same source image. Keep that second client powered off and do not change its assignment during the test.
3. Keep test clients offline except when a step explicitly boots one. Confirm they have no unrelated iSCSI sessions, mounts, or pending storage recovery.
4. Upload each WinPE and Linux live ISO through **Installer Media**. Record its display name, media type, exact SHA-256 checksum, and size. Use media that is licensed and intended for the tested boot path.
5. For every test row, record the exact assigned source revision, client firmware mode (`BIOS` or `UEFI`), and whether **Use game disk** is enabled.
6. Prepare hardware for each claimed firmware mode. A machine that supports only UEFI cannot validate BIOS behavior.

## Firmware and media matrix

Run each applicable combination. Select the matching image OS and use an assigned disposable test image revision.

| iSCSI backend | Image / installer media | Firmware |
| --- | --- | --- |
| targetcli | Windows / WinPE | BIOS |
| targetcli | Windows / WinPE | UEFI |
| targetcli | Linux / Linux live | BIOS |
| targetcli | Linux / Linux live | UEFI |
| configfs | Windows / WinPE | BIOS |
| configfs | Windows / WinPE | UEFI |
| configfs | Linux / Linux live | BIOS |
| configfs | Linux / Linux live | UEFI |

Skip a row only when no test hardware supports that firmware mode; record it as **Not tested**. Do not infer a pass from a different mode, backend, ISO checksum, or image revision.

## Verify the installer boot

For each matrix row:

1. In the isolated staging UI, set the ISO under test as the default for its media type. Confirm its structural validation is `valid` and record its checksum.
2. Open the test client's context menu in **Client Management** and choose **Start verification-only session**. Confirm the warning that session writes are disposable. The client must be offline before starting.
3. Inspect the server's target state read-only. Confirm the session-owned writable image is LUN 0 and the selected ISO is LUN 1 with read-only mapping. Confirm the expected named ACL/CHAP configuration is retained. Do not change target state manually during this test.
4. Boot the test client in the recorded firmware mode. Confirm iPXE attaches the working disk first, boots the installer ISO, and the installer starts without a SAN or media error.
5. At the installer disk-selection screen, confirm the session disk is visible as the installation target. Do not select or modify any shared master or other client disk. Record the LUNs actually observed; a passing boot requires LUN 0 and LUN 1.
6. Power off the test client cleanly without starting an OS installation. In **Installer Media**, select this client and record the observed result on the ISO used by its active verification-only session. A failed attempt is recorded as failed; it must not be marked passed.
7. Confirm the displayed BIOS/UEFI verification state changes only for the tested ISO and firmware mode.

If the client cannot boot, record the exact firmware screen/error and leave the media unverified. Do not work around a failure by changing live target mappings or switching the test client to a shared writable image.

## Verify Save, Discard, game disk, and recovery

Run these only after the relevant ISO has passed the matching firmware verification and the test environment is isolated.

1. **Save:** Start a normal Super session on a disposable test client. Install or make a harmless, identifiable change to the LUN 0 session clone. Shut the client down, choose **Save Super Changes**, and verify the client returns to normal mode on a new managed revision. Verify the second client remains on its original revision.
2. **Discard:** Start a new Super session, make a different identifiable change, shut down, and choose **Discard Super Changes**. Verify the original assignment is restored and the discarded change is absent. Verify the second client remains unchanged.
3. **Game disk enabled:** Start a session for a disposable client whose game-disk preference is enabled. Verify its preference and clone remain present while its game LUN is temporarily unmapped, and that the LUN is restored after both Save and Discard paths.
4. **Game disk disabled:** Start a session for a disposable client with the preference disabled. Verify no game-disk clone or LUN is introduced before, during, or after the session.
5. **Restart recovery:** With the test client powered off and a durable Super session active, restart the management service. Confirm the session and LUN mappings are still reported correctly, then complete it through Save or Discard. If deliberately testing an interrupted transition, use only a staging test and record the resulting recovery state before retrying.
6. For every step, confirm a second client assigned to the same source image remains pinned to its original image revision.

Never interrupt a production service or test a recovery boundary on non-disposable storage.

## Result record

Keep one row per matrix combination and separate rows for Save, Discard, game-disk, and restart-recovery cases. Attach photos or logs with secrets removed.

| Field | Result |
| --- | --- |
| Date / operator | |
| Application version / commit | |
| iSCSI backend | |
| ISO name / type / SHA-256 | |
| Test client ID / MAC | |
| Assigned image revision before test | |
| Firmware mode | |
| PXE / iPXE / installer result | |
| LUN 0 visible and writable clone confirmed | |
| LUN 1 visible and read-only ISO confirmed | |
| CHAP / initiator ACL result | |
| Game-disk preference and mapping result | |
| Verification result recorded in UI | |
| Save / Discard result | |
| Service restart / recovery result | |
| Second client's revision unchanged | |
| Evidence / notes | |

Do not mark a firmware mode verified until the UI record is tied to the tested ISO checksum, test client, and assigned image revision. Clean up test clones and ISO media only after all sessions have completed and no active sessions or recovery records remain.
