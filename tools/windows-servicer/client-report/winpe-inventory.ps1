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
$udp = New-Object System.Net.Sockets.UdpClient
try {
    $udp.Connect($serverAddresses[0], $serverUri.Port)
    $sourceAddress = $udp.Client.LocalEndPoint.Address.IPAddressToString
} catch {
    Write-Host "Could not resolve the route to the inventory server: $($_.Exception.Message)"
    exit 1
} finally {
    $udp.Dispose()
}
$sourceInterfaces = @(Get-NetIPAddress -IPAddress $sourceAddress -ErrorAction Stop)
if ($sourceInterfaces.Count -ne 1) {
    Write-Host "The route to the inventory server does not identify one network interface."
    exit 1
}
$adapters = @(Get-NetAdapter -InterfaceIndex $sourceInterfaces[0].InterfaceIndex -Physical -ErrorAction Stop)
if ($adapters.Count -ne 1 -or $adapters[0].Status -ne "Up" -or [string]::IsNullOrWhiteSpace($adapters[0].PnPDeviceID)) {
    Write-Host "The inventory server route does not map to one active physical PnP adapter."
    exit 1
}
$boot = $adapters[0]
$mac = ($boot.MacAddress -replace "[-:]", "").ToLowerInvariant()

$devices = Get-PnpDevice -Class Net -PresentOnly -ErrorAction SilentlyContinue
$bootDevices = @($devices | Where-Object { $_.InstanceId -eq $boot.PnPDeviceID })
function Get-DeviceNicIds($device) {
    $keys = @("DEVPKEY_Device_HardwareIds", "DEVPKEY_Device_CompatibleIds")
    $properties = Get-PnpDeviceProperty -InstanceId $device.InstanceId -KeyName $keys -ErrorAction Stop
    @($properties | Where-Object { $_.KeyName -in $keys } | ForEach-Object { $_.Data } | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -Unique)
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
    Write-Host "Boot NIC '$($boot.InterfaceDescription)' has no PnP hardware or compatible IDs."
    exit 1
}
$secondaryIds = @()
foreach ($device in $devices | Where-Object { $_.InstanceId -ne $bootDevice.InstanceId }) {
    try { $secondaryIds += Get-DeviceNicIds $device } catch { Write-Host "Could not read secondary NIC IDs for '$($device.FriendlyName)'." }
}
$secondaryIds = @($secondaryIds | Select-Object -Unique)

$arch = $env:PROCESSOR_ARCHITECTURE
if ([string]::IsNullOrWhiteSpace($arch)) { $arch = "AMD64" }
$firmware = "unknown"
try {
    $firmwareType = (Get-ComputerInfo -Property BiosFirmwareType -ErrorAction Stop).BiosFirmwareType
    if ($firmwareType -eq "Uefi") { $firmware = "uefi" }
    elseif ($firmwareType -eq "Bios") { $firmware = "bios" }
} catch {
    Write-Host "Firmware type detection failed; reporting as unknown."
}

$payload = @{
    boot_nic_ids = @($bootIds)
    secondary_ids = @($secondaryIds)
    architecture = $arch
    firmware = $firmware
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
