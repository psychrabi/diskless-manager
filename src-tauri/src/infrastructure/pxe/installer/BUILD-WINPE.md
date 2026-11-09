# Dedicated unattended Windows installation image

This path is separate from the inventory image, whose startup always reboots. On an ADK amd64 build machine, use `copype amd64`, mount `media/sources/boot.wim`, and add the WinPE-WMI, WinPE-NetFX, WinPE-Scripting, WinPE-PowerShell and WinPE-StorageWMI optional components (with matching language packages). Include the client's boot NIC driver. Use the existing `tools/windows-servicer/client-report/BUILD-WINPE.md` ADK mounting procedure, but do not copy its inventory startup script.

Publish the committed WIM as `boot/winpe/boot-installer.wim` under the configured HTTP boot root, alongside existing `wimboot`, `BCD`, and `boot.sdi`. After checking the modules in that image, create `boot/winpe/installer-v1.json` containing `{"architecture":"amd64","powershell":true,"storage_wmi":true}`. This protected administrator marker attests to image prerequisites; the runtime checks the actual cmdlets again before partitioning.

The published `BCD` must stay the WinPE ramdisk store that wimboot expects: a `{ramdiskoptions}` entry with `ramdisksdipath \Boot\boot.sdi`, and an osloader whose `device` and `osdevice` are both `ramdisk=[boot]\Boot\boot.wim,{ramdiskoptions}` with `winpe Yes`. Session scripts always inject the installer as `boot.wim`, so a BCD that names `boot-installer.wim`, or any path outside wimboot's virtual directories (`\`, `\Boot`, `\Sources`, `\EFI`, ...), is found unreadable once the WIM has loaded and Windows Boot Manager stops with `Status: 0xc000000f` / `Info: A required device isn't connected or can't be accessed`. The server rejects such a store (and a non-WIM `boot-installer.wim`) before creating a session, so a bad asset no longer strands the client in PXE. Verify with `bcdedit /store BCD /enum all` (the same store boots the inventory and provisioning images, so no per-image entry is needed), and use `imgstat` plus `wimboot pause=quiet` on a failing boot to confirm the fetched image was named `boot.wim`. To fix a store that fails the check, in an elevated Deployment Tools prompt:

```powershell
bcdedit /store C:\WinPE_amd64\media\boot\BCD /enum all
bcdedit /store C:\WinPE_amd64\media\boot\BCD /set {ramdiskoptions} ramdisksdipath \Boot\boot.sdi
bcdedit /store C:\WinPE_amd64\media\boot\BCD /set {default} device ramdisk=[boot]\Boot\boot.wim,{ramdiskoptions}
bcdedit /store C:\WinPE_amd64\media\boot\BCD /set {default} osdevice ramdisk=[boot]\Boot\boot.wim,{ramdiskoptions}
bcdedit /store C:\WinPE_amd64\media\boot\BCD /set {default} winpe Yes
```

Copy the corrected `BCD` to `boot/winpe/BCD` under the HTTP boot root, then retry creating the verified image.

Build the shared servicing helper for WinPE with `RUSTFLAGS="-C target-feature=+crt-static" cargo build --manifest-path tools/windows-servicer/Cargo.toml --target x86_64-pc-windows-gnu --release --no-default-features --features winpe`. The `winpe` build omits the desktop UI and uses the static CRT. Place the resulting `diskless-windows-servicer.exe` under `tools/windows-servicer/` in the HTTP boot root. The server advertises that file's size and SHA-256 in the per-session adaptation manifest and serves it on the scoped, client-address-bound installer route; the installer downloads and verifies it before servicing, so a WinPE image with a stale baked helper cannot silently service a clone with old code. A copy baked into the WinPE image remains a fallback for sessions where the server does not advertise one; keep it current and attest its version and SHA-256 in `installer-v1.json`.

The session iPXE script uses wimboot to inject `startnet.cmd` and the scoped installer script directly into WinPE's System32. Startup runs `wpeinit`, then the scoped script. The ISO must contain `sources/install.wim` (ESD-only and split WIM media are rejected). Image index 1 is applied; Windows OOBE remains interactive. The WIM must fit WinPE's X: RAM disk; insufficient memory fails before disk clearing. This version supports only UEFI amd64 Windows with an existing iBFT target and StorageWMI support. Use interactive mode for BIOS or other Windows media.

No generated ADK files or Windows binaries are distributed. Test on an isolated UEFI machine before production use.
