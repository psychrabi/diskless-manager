<#Requires -RunAsAdministrator
<#
.SYNOPSIS
  Collects the preboot NIC inventory inside WinPE and POSTs it to the server.

.DESCRIPTION
  Runs from WinPE startnet.cmd. Determines the Boot NIC (first up physical
  adapter), gathers PnP hardware IDs for all present Net-class devices, and
  POSTs them keyed by the observed MAC to the enrollment listener's
  /inventory/{mac} route. The record stays pending until the client is
  assigned; matching always uses hardware IDs, never the MAC alone.

  The server URL is read from X:\diskless-server.txt, baked into the WinPE
  image at build time (see BUILD-WINPE.md). Nothing is logged beyond the
  HTTP status.
#>
$ErrorActionPreference = "Stop"

$serverFile = "X:\diskless-server.txt"
if (-not (Test-Path -LiteralPath $serverFile)) {
    Write-Host "diskless-server.txt is missing from the WinPE image; cannot report inventory."
    exit 1
}
$server = (Get-Content -LiteralPath $serverFile -Raw).Trim()
if ([string]::IsNullOrWhiteSpace($server)) {
    Write-Host "diskless-server.txt is empty; cannot report inventory."
    exit 1
}

$adapters = Get-NetAdapter -Physical | Where-Object { $_.Status -eq "Up" }
if (-not $adapters) {
    Write-Host "No usable physical network adapter found."
    exit 1
}
$boot = $adapters | Select-Object -First 1
$mac = ($boot.MacAddress -replace "[-:]", "").ToLowerInvariant()

$devices = Get-PnpDevice -Class Net -PresentOnly -ErrorAction SilentlyContinue
$bootIds = @($devices | Where-Object {
    $_.FriendlyName -eq $boot.InterfaceDescription
} | Select-Object -ExpandProperty InstanceId)
if (-not $bootIds) {
    Write-Host "Boot NIC '$($boot.InterfaceDescription)' not found in the PnP inventory."
    exit 1
}
$secondaryIds = @($devices | Where-Object {
    $_.FriendlyName -ne $boot.InterfaceDescription
} | Select-Object -ExpandProperty InstanceId)

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
try {
    $response = Invoke-WebRequest -Uri $uri -Method Post -Body $payload `
        -ContentType "application/json" -UseBasicParsing -TimeoutSec 60
    if ($response.StatusCode -ge 400) {
        Write-Host "Inventory rejected: $($response.StatusCode)."
        exit 1
    }
    Write-Host "Inventory accepted for $mac."
} catch {
    Write-Host "Inventory upload failed: $($_.Exception.Message)"
    exit 1
}
