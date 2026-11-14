$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
if ([Environment]::OSVersion.Version.Major -lt 10) { throw 'Windows 10 or 11 is required.' }
# Ethernet PCI adapters have an installed device instance that can survive image capture.
$adapters = @(Get-NetAdapter -Physical | Where-Object {
    $_.InterfaceType -eq 6 -and $_.PnPDeviceID -like 'PCI\*'
} | ForEach-Object {
    $device = Get-ItemProperty -LiteralPath ('HKLM:\SYSTEM\CurrentControlSet\Enum\' + $_.PnPDeviceID)
    [ordered]@{
        id = $_.PnPDeviceID
        name = $_.Name
        description = $_.InterfaceDescription
        mac = $_.MacAddress
        status = [string]$_.Status
        service = [string]$device.Service
        hardware_ids = @($device.HardwareID)
    }
})
ConvertTo-Json -InputObject $adapters -Depth 4 -Compress
