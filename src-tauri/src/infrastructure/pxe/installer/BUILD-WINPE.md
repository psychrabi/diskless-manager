# Dedicated unattended Windows installation image

This path is separate from the inventory image, whose startup always reboots. On an ADK amd64 build machine, use `copype amd64`, mount `media/sources/boot.wim`, and add the WinPE-WMI, WinPE-NetFX, WinPE-Scripting, WinPE-PowerShell and WinPE-StorageWMI optional components (with matching language packages). Include the client's boot NIC driver. Use the existing `tools/windows-servicer/client-report/BUILD-WINPE.md` ADK mounting procedure, but do not copy its inventory startup script.

Publish the committed WIM as `boot/winpe/boot-installer.wim` under the configured HTTP boot root, alongside existing `wimboot`, `BCD`, and `boot.sdi`. After checking the modules in that image, create `boot/winpe/installer-v1.json` containing `{"architecture":"amd64","powershell":true,"storage_wmi":true}`. This protected administrator marker attests to image prerequisites; the runtime checks the actual cmdlets again before partitioning.

The session iPXE script uses wimboot to inject `startnet.cmd` and `install.ps1` directly into WinPE's System32. Startup runs `wpeinit`, then the scoped script. The ISO must contain `sources/install.wim` (ESD-only and split WIM media are rejected). Image index 1 is applied; Windows OOBE remains interactive. The WIM must fit WinPE's X: RAM disk; insufficient memory fails before disk clearing. This version supports only UEFI amd64 Windows with an existing iBFT target and StorageWMI support. Use interactive mode for BIOS or other Windows media.

No generated ADK files or Windows binaries are distributed. Test on an isolated UEFI machine before production use.
