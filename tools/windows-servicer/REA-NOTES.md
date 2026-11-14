# CCBoot client reference: static REA investigation

Date: 2026-11-14. No downloaded executable or driver was run or installed. No repository source or REA registration was modified.

## Target provenance

The official download page https://www.ccboot.com/download.htm linked to https://update.youngzsoft.com/icafecloud/update/ccbootcloudsetup.exe. Downloaded installer: `/tmp/ccbootcloudsetup.exe`, SHA-256 `be9553809d54a77e462f72b2f3326bc88c2e7ce0524a022bc84563f950b951ab`. Existing innoextract identified it as CCBoot 2026 Build 1101 (Inno setup data 5.6.0) and extracted only the client, PnP EXE/DLL, and packaged registry data to `/tmp/ccboot-rea-reference/`. This extraction is ordinary shell-tool evidence, not REA evidence.

## REA evidence

The cached public CLI `/home/rabistha/.npm/_npx/af752b90495482d7/node_modules/.bin/rea` reports version 6.8.0. Engine-free PE inspection commands completed successfully. Full JSON output envelopes are retained in `/tmp/`:

- `ccboot-client-rea-imports.json`: `ev_a34df49c8729f0592ab071db979ee0c625d1b5e454c86a13020b1717f7c74356`; `inspect_pe_imports`; SHA-256 `28bcbe5bd4df824ce3db1efa5a31ece8bb3bc57071b1bc84d51385aed660d3eb`.
- `ccboot-client-rea-resources.json`: `ev_ea6fdbfb9ee8f90fa96741de54050c04934303ad7244840ad74c8e173b63b046`; `inspect_pe_resources`; SHA-256 `28bcbe5bd4df824ce3db1efa5a31ece8bb3bc57071b1bc84d51385aed660d3eb`.
- `ccboot-pnp-dll-rea-exports.json`: `ev_6b9739ee0a2195a1e12f575c9e4aaa927b95436e8f0de9417861ae76cea8e8f8`; `inspect_native_exports`; SHA-256 `33f4fbfa9c972b4985de4352f1c9713cc7b33069e0d140c3df21d93e6390de18`.
- `ccboot-pnp-rea-imports.json`: `ev_8d53a4ca805afbefff5a167a47dbf6e9dc74ace9834fc62b3533db01165359a3`; `inspect_pe_imports`; SHA-256 `73d917c5e633359a2211b472b866154b718cde3ec86c4d8c09d908d7a4896b25`.
- `ccboot-pnp-rea-resources.json`: `ev_dd0adc2b7ce742109bb4e4e10ded1cc9493a5510a5c677752844e80381c6ca2d`; `inspect_pe_resources`; SHA-256 `73d917c5e633359a2211b472b866154b718cde3ec86c4d8c09d908d7a4896b25`.

## Windows policy basis

Microsoft documents `BootFlags=0x1` (`CM_SERVICE_NETWORK_BOOT_LOAD`) as network-boot driver promotion in [Specifying Driver Load Order](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/specifying-driver-load-order). The local preparation workflow reuses Diskless Manager's existing planner, limits NIC changes to the selected installed PCI Ethernet device, preserves unrelated BootFlags, and exposes a preview. This is our implementation choice, not a recovered CCBoot algorithm. Existing service binaries and driver packages remain Windows-managed.

## Findings and reconstruction boundaries

Observed in REA import records: CCBootClient declares `newdev.dll!UpdateDriverForPlugAndPlayDevicesW` and `SETUPAPI.dll!SetupDiCreateDeviceInfoList`. CCBootPNP declares SetupAPI operations for INF parsing, device instance enumeration, and device registry properties; it declares registry hive save/load/unload operations too. Its DLL exports `CxPnpInstall` and `CxPnpUnstall`. These are shipped import/export table declarations; they do not prove runtime reachability, actual arguments, ordering, or effects.

Official documented behavior: https://www.ccboot.com/wikis-add-nic-drivers-to-image-by-local.htm describes collecting NIC drivers from an installed Windows client into a Drivers folder, then merging the collection into the server image. https://www.ccboot.com/wiki-ccboot-client-interface.htm (explicitly old version) distinguishes installing client support and NIC PnP installation for hardware variants. These sources support a workflow with separate driver installation/collection and client boot preparation actions, not exact CCBoot implementation equivalence.

Design inference: reuse the project's own Windows native driver preparation and network boot code, expose those actions through its existing client GUI, retain client reporting/export settings, and provide explicit reboot/image-capture guidance. No proprietary drivers, cache hooks, UI resources, or proprietary implementation code need redistribution.

Unknown: the exact native CCBoot boot preparation algorithm, kernel interactions, native runtime behavior, and physical iSCSI boot success. Resource inventory leaves non-icon payloads opaque. No imported function was called. Static metadata is not a successful hardware boot test.

## REA environment limitations

`doctor --client codex --json` reports no configured native Ghidra/Hopper/IDA/Kuna engine, stale Codex registration, and Node 22.23.1 below REA 6.8's supported patch range. Nevertheless the above engine-free inspections completed with success envelopes. Doctor output is `/tmp/nexboot-rea-cached-doctor.json`. No native decompilation was attempted without a provider. Installing/updating unrelated runtimes or repairing MCP config was not needed for the successful static subset, and was not performed.
