# NexBoot upstream review — 2026-10-25

Reviewed upstream `waterlemon0108/nexboot` at
[`f3add0dec65e8ba7c2e34f423dedc753b6f0141c`](https://github.com/waterlemon0108/nexboot/commit/f3add0dec65e8ba7c2e34f423dedc753b6f0141c).
The existing Diskless Manager adaptation documents do not pin an upstream
commit, so this is a source comparison, not a claim that every difference
was introduced after our last review. Upstream's substantive October 24
update is [`fb74ba2`](https://github.com/waterlemon0108/nexboot/commit/fb74ba293888c7f9ed8fd55680c9295c67badc1e),
following its October 4 snapshot `18c0672`. Later October 24 commits change
test isolation/formatting/timezone assertions and publish only Debian packages.

## What upstream added

- **Installation from ISO:** Windows, Ubuntu 24.04 and Anaconda-based
  RHEL-family full-DVD installation, with durable installation sessions,
  progress/log reporting, installation-disk identity checks, and rejection of
  offline media missing required packages. Its ISO installation workflow is
  UEFI/GPT; this is not a reason to disable Diskless Manager's existing BIOS
  client support. See the pinned [support matrix](https://github.com/waterlemon0108/nexboot/blob/f3add0dec65e8ba7c2e34f423dedc753b6f0141c/README.md#client-images)
  and [installation service](https://github.com/waterlemon0108/nexboot/blob/f3add0dec65e8ba7c2e34f423dedc753b6f0141c/internal/control/install/service.go).
- **Windows hardware identity:** hardware profiles, WinPE capture/adaptation
  sessions, and per-terminal identity clones separate NIC model configuration
  from device-instance identity. This is a new storage/application subsystem,
  not a driver ZIP update. See [identity preparation](https://github.com/waterlemon0108/nexboot/blob/f3add0dec65e8ba7c2e34f423dedc753b6f0141c/internal/control/identity/service.go)
  and [hardware observation](https://github.com/waterlemon0108/nexboot/blob/f3add0dec65e8ba7c2e34f423dedc753b6f0141c/internal/control/hwprofile/hints.go).
- **INF binding:** binds a matching model's DDInstall section to its own
  service declarations, resolves architecture decorations, and records a
  matching x64 model's minimum Windows build. See [binding.go](https://github.com/waterlemon0108/nexboot/blob/f3add0dec65e8ba7c2e34f423dedc753b6f0141c/internal/driverinf/binding.go),
  [MinBuild](https://github.com/waterlemon0108/nexboot/blob/f3add0dec65e8ba7c2e34f423dedc753b6f0141c/internal/driverinf/inf.go)
  and [runtime driver notes](https://github.com/waterlemon0108/nexboot/blob/f3add0dec65e8ba7c2e34f423dedc753b6f0141c/internal/control/install/runtime_drivers.go).
- **Safer Super saves:** inspect Windows hibernation and dirty SYSTEM hives
  before saving, and replay supported transaction logs before accepting a
  dirty hive. See [superhive.go](https://github.com/waterlemon0108/nexboot/blob/f3add0dec65e8ba7c2e34f423dedc753b6f0141c/internal/storage/local/superhive.go)
  and [bounded hive-log recovery](https://github.com/waterlemon0108/nexboot/blob/f3add0dec65e8ba7c2e34f423dedc753b6f0141c/internal/winreg/hvlog.go).
- **UDF content inspection:** reads a bounded ECMA-167 subset to locate WIM
  file extents/version metadata inside Windows installation media, with a
  `7z` fallback. See [udf.go](https://github.com/waterlemon0108/nexboot/blob/f3add0dec65e8ba7c2e34f423dedc753b6f0141c/internal/control/install/udf.go).

## Existing Diskless Manager coverage

`windows_inf.rs`, `driver_selection.rs` and the Windows servicing worker
already provide normalized hardware IDs, architecture filtering, complete
driver packages, associated-function-driver checks, deterministic selection,
and native DISM installation. `windows_boot_arming.rs` refuses dirty hives by
default, exposes optional crash-dump suppression, and avoids inventing vendor
service keys. `windows_nexboot_adaptation.rs` preserves unrelated Group Policy
entries and keeps failed startup work retryable. These protections should
remain rather than importing upstream's entire implementation.

## Bounded improvements implemented

1. **Keep service precedence within the same DDInstall group.** Current
   `windows_inf::service_sections_for_architecture` ranks all service sections
   globally. An unrelated model's architecture-specific section can suppress
   another model's generic section. Adapt the model-scoped precedence idea,
   retaining refusal when multiple applicable declarations disagree. The Rust
   parser now applies architecture precedence independently per install group;
   its regression test covers unrelated generic and decorated models.
2. **Do not broaden subsystem-specific CDDB mappings.** The startup adapter
   script should keep the INF's actual hardware-ID declarations rather than
   synthesizing generic VEN/DEV keys from SUBSYS-only models. Do not import
   upstream Binding's permissive VEN/DEV-only fallback. The startup script now
   validates the complete PCI ID and preserves SUBSYS/REV/CC components in the
   registry key. A Windows PowerShell test extracts only the pure ID helper,
   checks distinct subsystem IDs and rejects registry-path suffixes without
   executing the startup script or touching the registry.
3. **Defer Windows-build selection until image build metadata is reliable.**
   Upstream's minimum-build notes are useful, but our selector's free-form
   `os_version` is not sufficient to infer an exact Windows build safely.

No-BOM UTF-16 INF decoding is another small compatibility gap, but it already
existed in upstream's October 4 parser and is not an October 24 addition.

## Changes that need a separate implementation scope

The unattended installer, WinPE session service, hardware-profile identity
clone cache, and Linux-host hive-log replay each introduce persistent state,
recovery and privileged storage operations. Replacing our current lifecycle
with those components would require their own contracts and hardware checks.
Likewise, upstream HA/catalogue replication and Debian-only release packaging
do not map directly onto this single-server Rust/Fedora deployment.

Catalog upload does not require extracting UDF files: our existing bounded
ISO9660/El Torito validator can accept ISO9660/UDF bridge media with valid
boot metadata. Media without that descriptor chain remain unsupported;
adding UDF directory/WIM inspection would belong to a future ISO installer.

## Media-type review

The pending implementation already includes Windows ISO, WinPE, Linux ISO,
and Other ISO throughout its enum, API, catalog and migration. It also
contains separate pending changes for a global default, arbitrary client
media selection and background uploads. Preserve those existing edits when
reviewing this task.

The review found that migration `0022` escaped SQLx's transaction through
explicit `COMMIT`/`BEGIN`. A rollback regression demonstrated that this could
leave schema/data changes committed without migration bookkeeping. The
repair changes the checked type column within SQLx's transaction and keeps
the parent table intact, preserving media IDs and foreign-key references.
The requested UI label/order is Windows PE, Windows ISO, Linux ISO, Other ISO.

## Source and verification boundaries

Upstream is licensed [AGPL-3.0](https://github.com/waterlemon0108/nexboot/blob/f3add0dec65e8ba7c2e34f423dedc753b6f0141c/LICENSE).
Use the review to guide independently written changes in this repository;
do not transplant its Go/PowerShell implementation under an assumed MIT
license. This report reflects static source/history inspection. No upstream
installer, Windows payload, or privileged storage operation was executed.

## Verification of the local adaptations

- Rust formatting and Clippy (`--all-targets --all-features -- -D warnings`)
  pass for both the server and Windows worker manifests.
- Server tests: 561 unit tests, 14 integration tests and 9 documentation tests
  pass. Three tests remain opt-in/platform-limited: real LIO configuration,
  real ISC DHCP validation, and the Windows PowerShell ID-helper test.
- Worker tests on Linux: 49 pass; its Windows PowerShell test is ignored.
  These results do not establish Windows runtime behavior.
- Frontend: 152 tests across 37 files pass with Vitest's thread pool and two
  workers. Lint exits successfully with 21 existing warnings; production
  build passes. A stale image-page child mock was updated without changing
  its layout/loading assertions.
- Migration rollback and INF model-scoping regressions were observed failing
  before their fixes and passing afterward. The server's socket tests needed
  a rerun outside the sandbox's loopback-binding restriction; the worker's
  test build needed the compiler cache disabled to avoid writes outside the
  writable workspace. No assertions were weakened for these restrictions.

No deployment, real client boot, live ZFS/iSCSI changes, Windows driver
installation, or registry mutation was performed during this review.
