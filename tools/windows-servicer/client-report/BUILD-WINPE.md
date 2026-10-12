# Building the WinPE inventory image (opt-in, Windows ADK required)

The inventory image (`boot-inventory.wim`) is built by the administrator with
the Windows ADK. No generated WinPE image is committed to this repository.

## Prerequisites

- Windows 10/11 machine with the Windows ADK and WinPE add-on installed.
- The Diskless Manager server address (the inventory POSTs to the enrollment
  listener on port 4237).
- `wimboot` served by the boot server (unchanged from normal provisioning).

## Build steps

```powershell
# 1. Stage a WinPE working set (amd64 example).
copype amd64 C:\winpe-inventory

# 2. Mount the image.
Dism /Mount-Image /ImageFile:C:\winpe-inventory\media\sources\boot.wim /Index:1 /MountDir:C:\winpe-inventory\mount

# 3. Add networking + PowerShell + secure boot support.
Dism /Add-Package /Image:C:\winpe-inventory\mount /PackagePath:"C:\Program Files (x86)\Windows Kits\10\Assessment and Deployment Kit\Windows Preinstallation Environment\amd64\WinPE_OCs\WinPE-WMI.cab"
Dism /Add-Package /Image:C:\winpe-inventory\mount /PackagePath:".../WinPE_OCs\WinPE-NetFX.cab"
Dism /Add-Package /Image:C:\winpe-inventory\mount /PackagePath:".../WinPE_OCs\WinPE-PowerShell.cab"
Dism /Add-Package /Image:C:\winpe-inventory\mount /PackagePath:".../WinPE_OCs\WinPE-SecureBootCmdlets.cab"

# 4. Stage the inventory payload (this directory).
Copy-Item .\winpe-inventory.ps1 C:\winpe-inventory\mount\Windows\System32\
Copy-Item .\winpe-startnet.cmd C:\winpe-inventory\mount\Windows\System32\startnet.cmd

# 5. Bake the server address as host:port of the enrollment listener.
"192.0.2.1:4237" | Set-Content C:\winpe-inventory\mount\Windows\System32\diskless-server.txt -NoNewline

# 6. Commit and serve as boot-inventory.wim next to the install boot.wim.
Dism /Unmount-Image /MountDir:C:\winpe-inventory\mount /Commit
Copy-Item C:\winpe-inventory\media\sources\boot.wim <boot-root>\boot\winpe\boot-inventory.wim
```

Notes:

- `diskless-server.txt` holds `host:port` of the enrollment listener
  (default port 4237). It carries no credential: the inventory record stays
  pending until the client is assigned, and matching uses hardware IDs.
- Keep this image separate from the install `boot.wim`: its `startnet.cmd`
  always reboots after reporting, so a client can never stall in WinPE.
- The reporter selects the one physical adapter Windows routes to the
  inventory server and maps it by PnP device ID. It fails closed if that route
  cannot be mapped uniquely, and retries the inventory POST three times before
  rebooting.
- Verify on the designated disposable test client only: request an inventory
  boot for its record, confirm the `windows_nic_inventory` row appears, and
  confirm the next boot returns to the normal menu.
