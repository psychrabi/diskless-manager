# Resolve the boot NIC from the iSCSI session that carries the Windows system
# disk, so an extra attached data disk does not make the inventory ambiguous.
$connected = @(Get-IscsiSession | Where-Object { $_.IsConnected })
if ($connected.Count -eq 0) { throw 'No connected iSCSI session' }
$systemDisk = @(Get-Partition -DriveLetter $env:SystemDrive.TrimEnd(':') | Get-Disk)
if ($systemDisk.Count -ne 1) { throw 'Could not uniquely resolve the Windows system disk' }
$bootSessions = @($connected | Where-Object {
    @(($_ | Get-Disk -ErrorAction SilentlyContinue) | Where-Object { $_.Number -eq $systemDisk[0].Number }).Count -gt 0
})
if ($bootSessions.Count -eq 1) {
    $targetSessions = $bootSessions
} elseif ($connected.Count -eq 1) {
    # The disk association can be missing for an iBFT boot disk; a single
    # connected session is still unambiguous.
    $targetSessions = $connected
} else {
    throw 'Could not identify the iSCSI session carrying the Windows system disk'
}
$connections = @(Get-IscsiConnection -IscsiSession $targetSessions[0])
$sessions = @($connections | Select-Object -ExpandProperty InitiatorAddress -Unique)
if ($sessions.Count -ne 1) { throw 'Expected one iSCSI boot source address' }
$ips = @(Get-NetIPAddress -IPAddress $sessions[0] -ErrorAction Stop)
if ($ips.Count -ne 1) { throw 'Could not uniquely resolve iSCSI boot interface' }
$a = @(Get-NetAdapter -InterfaceIndex $ips[0].InterfaceIndex -Physical)
if ($a.Count -ne 1) { throw 'Could not uniquely resolve iSCSI boot adapter' }
$d = Get-PnpDevice -InstanceId $a[0].PnPDeviceID -ErrorAction Stop
$ids = @((Get-PnpDeviceProperty -InstanceId $d.InstanceId -KeyName 'DEVPKEY_Device_HardwareIds' -ErrorAction Stop).Data)
$signed = @(Get-CimInstance Win32_PnPSignedDriver | Where-Object { $_.DeviceID -eq $d.InstanceId } | Select-Object -ExpandProperty InfName -Unique)
if ($signed.Count -gt 1) { throw 'Could not uniquely resolve Boot NIC published INF' }
$inf = if ($signed.Count -eq 1) { $signed[0] } else { '' }
$o = [pscustomobject]@{
    Name = $a[0].Name
    MacAddress = $a[0].MacAddress
    InterfaceDescription = $a[0].InterfaceDescription
    Status = $a[0].Status
    ServiceName = [string]$a[0].ServiceName
    PnPDeviceID = $d.InstanceId
    HardwareIds = [string[]]$ids
    PublishedInf = $inf
    TargetIqn = $targetSessions[0].TargetNodeAddress
}
$o | ConvertTo-Json -Depth 3 -Compress
