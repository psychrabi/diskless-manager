# NexBoot Driver-Installation Validation Report

**Branch:** `feat/nexboot-driver-flow`  
**Validated commit:** `0fb293d` (`Harden NexBoot deployment path`)  
**Remote ref:** `origin/feat/nexboot-driver-flow` → `0fb293d`  
**Validation date:** 2026-10-17  
**Validation environment:** Linux Manus Sandbox

## Executive summary

The automated validation for the Windows worker and NexBoot integration passes. The current branch compiles, the Windows worker test suite passes completely, and the focused backend tests for staging, Group Policy merging, Windows preparation, and revision strategy identity pass.

A real Windows worker/client execution could not be performed in this run because no authorized Windows environment or Windows client was available in the session. Therefore, PowerShell parsing, DISM behavior, Windows registry behavior, Group Policy processing, `pnputil` behavior, and an actual iSCSI boot/0x7B test remain **not executed**. The branch should not be marked production-validated until that hardware-in-the-loop test is completed.

## Results

| Area | Command/check | Result |
|---|---|---:|
| Windows helper unit tests | `cargo test --manifest-path tools/windows-servicer/Cargo.toml -- --nocapture` | **38 passed, 0 failed** |
| Windows helper compilation | `cargo check --manifest-path tools/windows-servicer/Cargo.toml` | **Passed** |
| NexBoot staging tests | `cargo test --manifest-path src-tauri/Cargo.toml infrastructure::pxe::windows_nexboot_adaptation::tests -- --nocapture` | **2 passed, 0 failed** |
| Windows servicing application tests | `cargo test --manifest-path src-tauri/Cargo.toml application::windows_servicing::tests -- --nocapture` | **14 passed, 0 failed** |
| Rust formatting | `cargo fmt ... -- --check` for both crates | **Passed** |
| Git whitespace validation | `git diff --check` | **Passed** |
| Real Windows helper execution | Native `capabilities` / `prepare` | **Not run: no authorized Windows environment** |
| Real Windows PowerShell execution | `auto-adapt.ps1` under SYSTEM | **Not run: no authorized Windows client** |
| Real iSCSI boot test | Boot new image and reconnect root disk | **Not run: no authorized Windows client** |

The full backend suite was also run previously. It had one unrelated existing failure in `application::storage_service::offline_replacement::tests::cleanup_can_be_retried_after_client_clone_is_already_absent`; the NexBoot-focused tests remained green.

## What the automated tests cover

### Windows worker coverage

The worker test suite covers the existing helper behavior, including:

- Windows image preparation validation.
- Offline driver injection and INF parsing.
- Windows boot-arming plans.
- Dirty-hive refusal.
- Windows client reporting and report retries.
- iSCSI servicing validation.
- GPT identity handling.
- NexBoot bundle staging.
- Group Policy startup registration merging.

### NexBoot-specific coverage

The new tests verify that:

1. A driver bundle is created and staged under the NexBoot working directory.
2. `adapt.ps1` and `ndadapt.cmd` are copied into the image.
3. Existing `[Startup]` entries remain present when the NexBoot entry is added.
4. Existing `[Shutdown]` sections remain present.
5. NexBoot staging does not overwrite `scripts.ini` wholesale.
6. Driver installation strategy affects revision identity.
7. Offline DISM and NexBoot revisions do not share the same reuse hash.
8. The helper advertises protocol version 3 and NexBoot capability.

## Deployment-safety behavior verified by code and tests

The current implementation uses a hybrid path for NexBoot mode:

1. Offline driver injection still occurs.
2. Offline boot arming still occurs.
3. The reporter is still installed for committed images.
4. NexBoot boot-time staging is added as a secondary compatibility mechanism.
5. The startup registration is removed only after successful boot-time installation.
6. Failed installation leaves the registration in place for retry.
7. Missing bundles, extraction failures, zero-INF bundles, `pnputil` failures, and missing iSCSI services fail closed.
8. Ambiguous INF service mappings do not generate unsafe CDDB assignments.
9. Older helpers are rejected because they do not report protocol-version-3 NexBoot capability.

## Required real-Windows test

This is the remaining release gate. Use a disposable Windows 10/11 client and a disposable image clone. Do not use a production image.

### A. Install and verify the helper

On the Windows servicing worker, from an elevated PowerShell prompt:

```powershell
Set-ExecutionPolicy -Scope Process Bypass
.\install.ps1 -BinaryPath .\diskless-windows-servicer.exe -ConfigureOpenSsh
& "C:\Program Files\Diskless Manager\diskless-windows-servicer.exe" capabilities
```

The capability response must report:

```json
{
  "protocol_version": 3,
  "available": true,
  "elevated": true,
  "iscsi_volume_servicing": true,
  "nexboot_boot_time": true
}
```

### B. Verify driver inventory

On the disposable managed client:

```powershell
& "C:\Program Files\Diskless Manager\diskless-windows-servicer.exe" report-client
```

Confirm that the server records the Boot NIC hardware ID and that the matching signed package exists in the network-driver catalog.

### C. Test Offline DISM baseline

Select **Offline DISM (current)**, prepare a fresh image revision, and record:

- Preparation result.
- Client boot result.
- NIC availability.
- iSCSI reconnection.
- Additional reboot requirement.
- Reporter status.

### D. Test hybrid NexBoot mode

Select **NexBoot boot-time adaptation (test)** and use a new source snapshot or new driver revision. Verify that the preparation result completes offline injection and boot arming before publication.

After boot, inspect:

```powershell
Get-Content C:\ndadapt\adapt.log
Get-Content C:\ndadapt\pnputil.log
Get-Content C:\ndadapt\done.txt
pnputil /enum-drivers
Get-Service msiscsi, iScsiPrt
Test-Path "C:\Windows\System32\GroupPolicy\Machine\Scripts\Startup\ndadapt.cmd"
```

Expected results:

- `done.txt` reports `OK=True EXIT=0`.
- At least one INF was extracted and installed.
- The Boot NIC driver is present.
- iSCSI services are available and boot-start configuration is applied.
- `ndadapt.cmd` is removed after success.
- Existing non-NexBoot startup registrations remain.
- A second reboot does not execute NexBoot again.
- The reporter remains installed and can report the client.

### E. Failure/retry tests

Run each against a disposable clone:

1. Remove or corrupt `bundle.zip`; verify failure and retained startup registration.
2. Use a bundle without INF files; verify failure and retained startup registration.
3. Force a `pnputil` failure; verify failure and retained startup registration.
4. Include an INF with multiple service declarations; verify no unsafe cross-product CDDB mapping is written.
5. Put existing startup and shutdown entries in `scripts.ini`; verify they survive staging and cleanup.
6. Switch from Offline DISM to NexBoot and verify a new revision is created rather than reusing the prior strategy’s revision.
7. Reuse the same strategy and package set and verify the intended revision reuse behavior.

## Release decision

**Automated gate:** PASS  
**Real Windows gate:** BLOCKED / NOT EXECUTED  
**Production deployment:** Do not approve until the real Windows worker/client test above is completed and its logs are attached to the release record.
