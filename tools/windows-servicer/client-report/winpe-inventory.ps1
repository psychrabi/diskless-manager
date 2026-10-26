#Requires -RunAsAdministrator
<#
.SYNOPSIS
  Collects the preboot NIC inventory inside WinPE and POSTs it to the server.

.DESCRIPTION
  Runs from WinPE startnet.cmd. Resolves the physical adapter used to route to
  the inventory server, gathers PnP hardware IDs for all present Net-class devices, and
  POSTs them keyed by the observed MAC to the enrollment listener's
  /inventory/{mac} route. The record stays pending until the client is
  assigned; matching always uses hardware IDs, never the MAC alone.

  The server URL is read from diskless-server.txt beside this script, baked
  into the WinPE image at build time (see BUILD-WINPE.md). Nothing is logged beyond the
  HTTP status.
#>
$ErrorActionPreference = "Stop"

$serverFile = Join-Path $PSScriptRoot "diskless-server.txt"
if (-not (Test-Path -LiteralPath $serverFile)) {
    Write-Host "diskless-server.txt is missing from the WinPE image; cannot report inventory."
    exit 1
}
$server = (Get-Content -LiteralPath $serverFile -Raw).Trim()
if ([string]::IsNullOrWhiteSpace($server)) {
    Write-Host "diskless-server.txt is empty; cannot report inventory."
    exit 1
}

$serverUri = [Uri]("http://$server")
$serverAddresses = [System.Net.Dns]::GetHostAddresses($serverUri.DnsSafeHost) |
    Where-Object { $_.AddressFamily -eq [System.Net.Sockets.AddressFamily]::InterNetwork }
if (-not $serverAddresses) {
    Write-Host "The inventory server has no reachable IPv4 address."
    exit 1
}
# DHCP may finish after wpeinit returns; wait at most one minute for the route.
$sourceAddress = $null
for ($attempt = 1; $attempt -le 30; $attempt++) {
    $udp = New-Object System.Net.Sockets.UdpClient
    try {
        $udp.Connect($serverAddresses[0], $serverUri.Port)
        $sourceAddress = $udp.Client.LocalEndPoint.Address.IPAddressToString
        if ($sourceAddress -eq "0.0.0.0") { throw "No IPv4 source address is ready." }
        break
    } catch {
        if ($attempt -eq 30) {
            Write-Host "Could not resolve the route to the inventory server after waiting for DHCP: $($_.Exception.Message)"
            exit 1
        }
    } finally {
        $udp.Dispose()
    }
    Start-Sleep -Seconds 2
}
$sourceInterfaces = @(Get-CimInstance -ClassName Win32_NetworkAdapterConfiguration -ErrorAction Stop |
    Where-Object { $_.IPEnabled -and $_.IPAddress -contains $sourceAddress })
if ($sourceInterfaces.Count -ne 1) {
    Write-Host "The route to the inventory server does not identify one network interface."
    exit 1
}
$adapters = @(Get-CimInstance -ClassName Win32_NetworkAdapter -ErrorAction Stop |
    Where-Object { $_.Index -eq $sourceInterfaces[0].Index -and $_.PhysicalAdapter })
if ($adapters.Count -ne 1 -or [string]::IsNullOrWhiteSpace($adapters[0].PNPDeviceID)) {
    Write-Host "The inventory server route does not map to one physical PnP adapter."
    exit 1
}
$boot = $adapters[0]
$mac = ($boot.MACAddress -replace "[-:]", "").ToLowerInvariant()
if ($mac -notmatch '^[0-9a-f]{12}$') {
    Write-Host "The routed adapter has no valid MAC address."
    exit 1
}

$devices = @(Get-CimInstance -ClassName Win32_PnPEntity -Filter "ClassGuid='{4d36e972-e325-11ce-bfc1-08002be10318}'" -ErrorAction Stop |
    Where-Object { $_.ConfigManagerErrorCode -eq 0 })
$bootDevices = @($devices | Where-Object { $_.PNPDeviceID -eq $boot.PNPDeviceID })
function Get-DeviceNicIds($device) {
    @((@($device.HardwareID) + @($device.CompatibleID)) |
        Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -Unique)
}
if ($bootDevices.Count -ne 1) {
    Write-Host "The routed adapter does not map to one present PnP network device."
    exit 1
}
$bootDevice = $bootDevices[0]
try {
    $bootIds = @(Get-DeviceNicIds $bootDevice)
} catch {
    Write-Host "Could not read Boot NIC hardware IDs: $($_.Exception.Message)"
    exit 1
}
if (-not $bootIds) {
    Write-Host "Boot NIC '$($boot.Description)' has no PnP hardware or compatible IDs."
    exit 1
}
# PCI SUBSYS is encoded as subsystem-device followed by subsystem-vendor.
$subvendor_id = ""
$subdevice_id = ""
foreach ($id in $bootIds) {
    if ($id -match 'SUBSYS_(?<device>[0-9A-Fa-f]{4})(?<vendor>[0-9A-Fa-f]{4})(?:&|$)') {
        $subvendor_id = $matches['vendor']
        $subdevice_id = $matches['device']
        break
    }
}
$secondaryIds = @()
foreach ($device in $devices | Where-Object { $_.PNPDeviceID -ne $bootDevice.PNPDeviceID }) {
    try { $secondaryIds += Get-DeviceNicIds $device } catch { Write-Host "Could not read secondary NIC IDs for '$($device.Name)'." }
}
$secondaryIds = @($secondaryIds | Select-Object -Unique)

$arch = $env:PROCESSOR_ARCHITECTURE
if ([string]::IsNullOrWhiteSpace($arch)) { $arch = "AMD64" }
$firmware = "unknown"
try {
    & wpeutil.exe UpdateBootInfo | Out-Null
    $firmwareType = (Get-ItemProperty -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Control' -Name PEFirmwareType -ErrorAction Stop).PEFirmwareType
    if ($firmwareType -eq 2) { $firmware = "uefi" }
    elseif ($firmwareType -eq 1) { $firmware = "bios" }
} catch {
    Write-Host "Firmware type detection failed; reporting as unknown."
}

$payload = @{
    boot_nic_ids = @($bootIds)
    secondary_ids = @($secondaryIds)
    architecture = $arch
    firmware = $firmware
    subvendor_id = $subvendor_id
    subdevice_id = $subdevice_id
} | ConvertTo-Json -Depth 3

$uri = "http://$server/inventory/$mac"
for ($attempt = 1; $attempt -le 3; $attempt++) {
    try {
        $response = Invoke-WebRequest -Uri $uri -Method Post -Body $payload `
            -ContentType "application/json" -UseBasicParsing -TimeoutSec 60
        if ($response.StatusCode -ge 400) {
            throw "Inventory rejected: $($response.StatusCode)."
        }
        Write-Host "Inventory accepted for $mac."
        exit 0
    } catch {
        if ($attempt -eq 3) {
            Write-Host "Inventory upload failed after $attempt attempts: $($_.Exception.Message)"
            exit 1
        }
        Start-Sleep -Seconds 5
    }
}
