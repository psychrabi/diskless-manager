# Runs only in the dedicated WinPE installer image. Never execute on the server.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$Base = '@@URL@@'
$ExpectedIqn = '@@IQN@@'
$ExpectedBytes = [UInt64]'@@BYTES@@'
$AdaptExisting = @@ADAPT_EXISTING@@
$Disk = $null
$TemporaryMounts = @()
function Report([string]$State, [int]$Progress, [string]$Message) {
    Invoke-RestMethod -Uri "$Base/report" -Method Post -ContentType 'application/json' -Body (@{state=$State;progress=$Progress;message=$Message} | ConvertTo-Json -Compress) | Out-Null
}
function Run([string]$File, [string[]]$Arguments) {
    & $File @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$File failed with exit code $LASTEXITCODE" }
}
try {
    if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { throw 'Windows unattended installation requires amd64 WinPE' }
    foreach ($Command in @('Get-IscsiSession', 'Get-IscsiConnection', 'Get-Disk', 'Initialize-Disk', 'New-Partition', 'Format-Volume')) {
        if (-not (Get-Command $Command -ErrorAction SilentlyContinue)) { throw "Dedicated WinPE requires WinPE-StorageWMI and PowerShell: $Command unavailable" }
    }
    $Servicer = 'X:\Windows\System32\diskless-windows-servicer.exe'
    New-Item -ItemType Directory -Path 'X:\drivers' -Force | Out-Null
    # The iBFT session attached by iPXE must identify the disk; local SATA/NVMe disks never qualify.
    $Sessions = @(Get-IscsiSession | Where-Object TargetNodeAddress -EQ $ExpectedIqn)
    if ($Sessions.Count -ne 1) { throw 'Expected exactly one assigned iSCSI target session' }
    # Use the iSCSI initiator address to identify the firmware boot adapter.
    # PNPDeviceID is an instance path; DISM boot arming needs HardwareID and
    # CompatibleID values to build a valid CriticalDeviceDatabase key.
    $BootConnections = @(Get-IscsiConnection -IscsiSession $Sessions[0] | Where-Object { $_.InitiatorAddress })
    $InitiatorAddresses = @($BootConnections | Select-Object -ExpandProperty InitiatorAddress -Unique)
    if ($InitiatorAddresses.Count -ne 1) { throw 'Expected one iSCSI boot NIC address' }
    $BootConfigs = @(Get-CimInstance Win32_NetworkAdapterConfiguration | Where-Object { $_.IPEnabled -and $_.IPAddress -contains $InitiatorAddresses[0] })
    if ($BootConfigs.Count -ne 1) { throw 'The iSCSI boot address does not identify one network adapter' }
    $BootAdapters = @(Get-CimInstance Win32_NetworkAdapter | Where-Object { $_.Index -eq $BootConfigs[0].Index -and $_.PhysicalAdapter -and $_.PNPDeviceID })
    if ($BootAdapters.Count -ne 1) { throw 'The iSCSI boot address does not identify one physical PnP adapter' }
    $BootDevices = @(Get-CimInstance Win32_PnPEntity -Filter "ClassGuid='{4d36e972-e325-11ce-bfc1-08002be10318}'" | Where-Object { $_.PNPDeviceID -eq $BootAdapters[0].PNPDeviceID })
    if ($BootDevices.Count -ne 1) { throw 'The iSCSI boot adapter has no unique network PnP device' }
    $NicIds = @(@($BootDevices[0].HardwareID) + @($BootDevices[0].CompatibleID) | Where-Object { $_ } | Select-Object -Unique)
    if (-not $NicIds) { throw 'The iSCSI boot adapter has no PnP hardware or compatible IDs' }
    $NicMac = $BootAdapters[0].MACAddress
    $SessionDisks = @($Sessions[0] | Get-Disk)
    $Disks = @($SessionDisks | Where-Object { $_.BusType -eq 'iSCSI' -and $_.Size -eq $ExpectedBytes })
    for ($Attempt = 0; $Disks.Count -ne 1 -and $Attempt -lt 30; $Attempt++) {
        Start-Sleep -Seconds 1
        $SessionDisks = @($Sessions[0] | Get-Disk)
        $Disks = @($SessionDisks | Where-Object { $_.BusType -eq 'iSCSI' -and $_.Size -eq $ExpectedBytes })
    }
    if ($Disks.Count -ne 1) {
        $Seen = ($SessionDisks | ForEach-Object { "Disk$($_.Number):BusType=$($_.BusType),Size=$($_.Size),IsBoot=$($_.IsBoot),IsSystem=$($_.IsSystem)" }) -join '; '
        throw "Expected exactly one iSCSI disk with recorded size $ExpectedBytes, found $($Disks.Count) (session disks: $Seen)"
    }
    if ($Disks[0].IsBoot -or $Disks[0].IsSystem) { throw 'Refusing a boot or system disk' }
    # Fail before fetching Setup payload or clearing a disk if firmware identity differs.
    $Firmware = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control').PEFirmwareType
    if ($Firmware -ne 2) { throw 'Windows unattended installation requires UEFI amd64' }
    $Disk = $Disks[0]
    Report 'installing' 10 'Verified target IQN, iSCSI bus, UEFI and exact disk size'
    if ($AdaptExisting) {
        $Disk | Set-Disk -IsOffline $false
        $Disk | Set-Disk -IsReadOnly $false
        $Candidates = @()
        # Enumerate partitions only through the exact session-owned clone disk.
        foreach ($Partition in @(Get-Partition -DiskNumber $Disk.Number)) {
            $Root = $null
            if ($Partition.DriveLetter) {
                $Root = "$($Partition.DriveLetter):\"
            } elseif ($Partition.Type -ne 'Reserved' -and $Partition.Type -ne 'System') {
                $Root = "X:\adapt-mount-$($Partition.PartitionNumber)\"
                New-Item -ItemType Directory -Path $Root -Force | Out-Null
                $Mount = @{ DiskNumber = $Disk.Number; PartitionNumber = $Partition.PartitionNumber; AccessPath = $Root }
                # Record before mounting so partial mount failures are cleaned up too.
                $TemporaryMounts += $Mount
                Add-PartitionAccessPath @Mount
            }
            if ($Root -and (Test-Path "${Root}Windows\System32\config\SYSTEM" -PathType Leaf) -and (Test-Path "${Root}Windows\System32\ntoskrnl.exe" -PathType Leaf)) {
                $Candidates += $Root
            }
        }
        if ($Candidates.Count -ne 1) { throw 'Expected exactly one existing Windows partition on the private clone' }
        $WindowsRoot = $Candidates[0]
    } else {
    Invoke-WebRequest -UseBasicParsing -Uri "$Base/install.wim" -OutFile 'X:\install.wim'
    # Download completes before the first destructive instruction; a missing source leaves the disk untouched.
    $Again = @($Sessions[0] | Get-Disk | Where-Object { $_.Number -eq $Disk.Number -and $_.BusType -eq 'iSCSI' -and $_.Size -eq $ExpectedBytes })
    if ($Again.Count -ne 1 -or $Again[0].IsBoot -or $Again[0].IsSystem) { throw 'Target identity changed before partition' }
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
        foreach ($Service in @('iScsiPrt')) {
            $Key = "Registry::HKEY_LOCAL_MACHINE\DM_INSTALL_SYSTEM\$ControlSet\Services\$Service"
            if (-not (Test-Path $Key)) { throw "Windows source lacks required $Service service" }
            $ServiceType = (Get-ItemProperty -Path $Key -Name Type).Type
            if ($ServiceType -ne 1 -and $ServiceType -ne 2) { throw "$Service is not a driver service" }
            Set-ItemProperty $Key Start 0
        }
    } finally { [GC]::Collect(); Run 'reg.exe' @('unload','HKLM\DM_INSTALL_SYSTEM') }
    Run 'bcdboot.exe' @("${WindowsRoot}Windows",'/s',"$($Esp.DriveLetter):",'/f','UEFI')
    }
    Report 'adapting' 78 'Arming the installed Windows volume for firmware iBFT boot and normal-boot verification'
    $Manifest = Invoke-RestMethod -Uri "$Base/adaptation" -Method Post -ContentType 'application/json' -Body (@{ boot_nic_ids = $NicIds; boot_nic_mac = $NicMac } | ConvertTo-Json -Compress)
    # The server is the source of truth for the servicing helper so a WinPE image
    # with an older baked copy cannot silently service a clone with stale code.
    if ($Manifest.servicer_url) {
        try {
            Invoke-WebRequest -UseBasicParsing -Uri $Manifest.servicer_url -OutFile $Servicer
            if ((Get-Item $Servicer).Length -ne [Int64]$Manifest.servicer_bytes) { throw 'Windows servicer size does not match the manifest' }
            $Stream = [IO.File]::OpenRead($Servicer)
            try { $Actual = ([BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash($Stream))).Replace('-','').ToLower() } finally { $Stream.Dispose() }
            if ($Actual -ne $Manifest.servicer_sha256) { throw 'Windows servicer checksum does not match the manifest' }
        } catch {
            Remove-Item $Servicer -Force -ErrorAction SilentlyContinue
            throw
        }
    } elseif (-not (Test-Path $Servicer -PathType Leaf)) {
        throw 'WinPE shared Windows servicer is missing and the server did not advertise one'
    }
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
    foreach ($Mount in $TemporaryMounts) { Remove-PartitionAccessPath @Mount }
    $TemporaryMounts = @()
    $Disk | Set-Disk -IsOffline $true
    Report 'installed' 88 'Installation and offline adaptation completed; waiting for server save and verification boot'
    wpeutil shutdown
} catch {
    foreach ($Mount in $TemporaryMounts) {
        try { Remove-PartitionAccessPath @Mount } catch { Write-Warning 'Could not remove temporary Windows mount' }
    }
    if ($Disk) {
        try { $Disk | Set-Disk -IsOffline $true } catch { Write-Warning 'Could not offline the private target disk' }
    }
    # Diagnostics deliberately avoid command arguments, CHAP credentials and token URLs.
    $Detail = $_.Exception.Message
    if ([string]::IsNullOrWhiteSpace($Detail)) { $Detail = 'unknown installer error' }
    $Detail = ($Detail -split "`r?`n")[0]
    $Detail = $Detail -replace 'https?://\S+', '<url>'
    if ($Detail.Length -gt 400) { $Detail = $Detail.Substring(0, 400) }
    $Failure = "Windows installation failed: $Detail. The private draft is retained for recovery; inspect the session event."
    try { Report 'failed' 10 $Failure } catch { Write-Warning 'Could not record installer failure' }
    Write-Error $Failure -ErrorAction Continue
    Write-Warning 'Windows installation stopped. Do not retry against a different disk; contact the administrator.'
    exit 1
}
