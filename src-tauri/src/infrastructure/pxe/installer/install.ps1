# Runs only in the dedicated WinPE installer image. Never execute on the server.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$Base = '@@URL@@'
$ExpectedIqn = '@@IQN@@'
$ExpectedBytes = [UInt64]'@@BYTES@@'
function Report([string]$State, [int]$Progress, [string]$Message) {
    Invoke-RestMethod -Uri "$Base/report" -Method Post -ContentType 'application/json' -Body (@{state=$State;progress=$Progress;message=$Message} | ConvertTo-Json -Compress) | Out-Null
}
function Run([string]$File, [string[]]$Arguments) {
    & $File @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$File failed with exit code $LASTEXITCODE" }
}
try {
    if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { throw 'Windows unattended installation requires amd64 WinPE' }
    foreach ($Command in @('Get-IscsiSession', 'Get-Disk', 'Initialize-Disk', 'New-Partition', 'Format-Volume')) {
        if (-not (Get-Command $Command -ErrorAction SilentlyContinue)) { throw "Dedicated WinPE requires WinPE-StorageWMI and PowerShell: $Command unavailable" }
    }
    $Servicer = 'X:\Windows\System32\diskless-windows-servicer.exe'
    if (-not (Test-Path $Servicer -PathType Leaf)) { throw 'WinPE shared Windows servicer is missing' }
    New-Item -ItemType Directory -Path 'X:\drivers' -Force | Out-Null
    # The firmware-provided boot adapter is the only NIC that needs drivers.
    $NicIds = @(Get-CimInstance Win32_NetworkAdapter -Filter 'PhysicalAdapter=True' | Where-Object { $_.PNPDeviceID } | Select-Object -ExpandProperty PNPDeviceID -Unique)
    $NicMac = @(Get-CimInstance Win32_NetworkAdapterConfiguration | Where-Object { $_.IPEnabled -eq $true -and $_.MACAddress } | Select-Object -First 1 -ExpandProperty MACAddress)
    $NicMac = if ($NicMac.Count -eq 1) { $NicMac[0] } else { $null }
    # The iBFT session attached by iPXE must identify the disk; local SATA/NVMe disks never qualify.
    $Sessions = @(Get-IscsiSession | Where-Object TargetNodeAddress -EQ $ExpectedIqn)
    if ($Sessions.Count -ne 1) { throw 'Expected exactly one assigned iSCSI target session' }
    $Disks = @($Sessions[0] | Get-Disk | Where-Object { $_.BusType -eq 'iSCSI' -and $_.Size -eq $ExpectedBytes })
    if ($Disks.Count -ne 1) { throw 'Expected exactly one iSCSI disk with the recorded size' }
    $Disk = $Disks[0]
    # Fail before fetching Setup payload or clearing a disk if firmware identity differs.
    $Firmware = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control').PEFirmwareType
    if ($Firmware -ne 2) { throw 'Windows unattended installation requires UEFI amd64' }
    Report 'installing' 10 'Verified target IQN, iSCSI bus, UEFI and exact disk size'
    Invoke-WebRequest -UseBasicParsing -Uri "$Base/install.wim" -OutFile 'X:\install.wim'
    # Download completes before the first destructive instruction; a missing source leaves the disk untouched.
    $Again = @($Sessions[0] | Get-Disk | Where-Object { $_.Number -eq $Disk.Number -and $_.BusType -eq 'iSCSI' -and $_.Size -eq $ExpectedBytes })
    if ($Again.Count -ne 1) { throw 'Target identity changed before partition' }
    $Disk | Set-Disk -IsOffline $false
    $Disk | Set-Disk -IsReadOnly $false
    $Disk | Clear-Disk -RemoveData -RemoveOEM -Confirm:$false
    $Disk | Initialize-Disk -PartitionStyle GPT
    $Esp = New-Partition -DiskNumber $Disk.Number -Size 260MB -GptType '{c12a7328-f81f-11d2-ba4b-00a0c93ec93b}' -AssignDriveLetter
    $Esp | Format-Volume -FileSystem FAT32 -Confirm:$false | Out-Null
    New-Partition -DiskNumber $Disk.Number -Size 16MB -GptType '{e3c9e316-0b5c-4db8-817d-f92df00215ae}' | Out-Null
    $Windows = New-Partition -DiskNumber $Disk.Number -UseMaximumSize -AssignDriveLetter
    $Windows | Format-Volume -FileSystem NTFS -Confirm:$false | Out-Null
    Report 'installing' 35 'Assigned target partitioned; applying Windows image index 1'
    $WindowsRoot = "$($Windows.DriveLetter):\"
    Run 'dism.exe' @('/Apply-Image','/ImageFile:X:\install.wim','/Index:1',"/ApplyDir:$WindowsRoot")
    # Keep the iBFT boot-start storage path enabled in the deployed operating system.
    Run 'reg.exe' @('load','HKLM\DM_INSTALL_SYSTEM',"${WindowsRoot}Windows\System32\config\SYSTEM")
    try {
        $Select = (Get-ItemProperty 'Registry::HKEY_LOCAL_MACHINE\DM_INSTALL_SYSTEM\Select').Default
        $ControlSet = 'ControlSet{0:d3}' -f $Select
        foreach ($Service in @('iScsiPrt','MSiSCSI')) {
            $Key = "Registry::HKEY_LOCAL_MACHINE\DM_INSTALL_SYSTEM\$ControlSet\Services\$Service"
            if (-not (Test-Path $Key)) { throw "Windows source lacks required $Service service" }
            Set-ItemProperty $Key Start 0
        }
    } finally { [GC]::Collect(); Run 'reg.exe' @('unload','HKLM\DM_INSTALL_SYSTEM') }
    Run 'bcdboot.exe' @("${WindowsRoot}Windows",'/s',"$($Esp.DriveLetter):",'/f','UEFI')
    Report 'adapting' 78 'Arming the installed Windows volume for firmware iBFT boot and normal-boot verification'
    $Manifest = Invoke-RestMethod -Uri "$Base/adaptation" -Method Post -ContentType 'application/json' -Body (@{ boot_nic_ids = $NicIds; boot_nic_mac = $NicMac } | ConvertTo-Json -Compress)
    if ($Manifest.driver_bundle_url) {
        Invoke-WebRequest -UseBasicParsing -Uri $Manifest.driver_bundle_url -OutFile 'X:\drivers.zip'
        if ((Get-Item 'X:\drivers.zip').Length -ne [Int64]$Manifest.driver_bundle_bytes) { throw 'Driver bundle size does not match the manifest' }
        $Stream = [IO.File]::OpenRead('X:\drivers.zip')
        try { $Actual = ([BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash($Stream))).Replace('-','').ToLower() } finally { $Stream.Dispose() }
        if ($Actual -ne $Manifest.driver_bundle_sha256) { throw 'Driver bundle checksum does not match the manifest' }
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        Remove-Item 'X:\drivers' -Recurse -Force
        [IO.Compression.ZipFile]::ExtractToDirectory('X:\drivers.zip', 'X:\drivers')
        Remove-Item 'X:\drivers.zip' -Force
    }
    $AdaptRequest = @{
        image_path = $WindowsRoot.TrimEnd('\')
        driver_root = 'X:\drivers'
        commit = $true
        boot_arm = $Manifest.boot_arm
        reporter = $Manifest.reporter
    } | ConvertTo-Json -Depth 8 -Compress
    $AdaptRequest | & $Servicer adapt-volume | Set-Content -Path 'X:\adapt-result.json' -Encoding ASCII
    if ($LASTEXITCODE -ne 0) { throw 'Shared Windows adaptation failed' }
    $AdaptResult = Get-Content -Raw 'X:\adapt-result.json' | ConvertFrom-Json
    if ($AdaptResult.ok -ne $true) { throw 'Shared Windows adaptation did not commit' }
    Remove-Item 'X:\drivers' -Recurse -Force -ErrorAction SilentlyContinue
    Report 'installed' 88 'Installation and offline adaptation completed; waiting for server save and verification boot'
    wpeutil shutdown
} catch {
    # Diagnostics deliberately avoid command arguments, CHAP credentials and token URLs.
    try { Report 'failed' 10 'Windows installation failed; inspect the WinPE console and retain the assigned disk for recovery' } catch { Write-Warning 'Could not record installer failure' }
    Write-Error 'Windows installation stopped. Do not retry against a different disk; contact the administrator.'
    exit 1
}
