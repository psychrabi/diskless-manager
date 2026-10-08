# Run inside an elevated Windows diskless master:
# .\restore-network.ps1 -Server 192.168.1.250 -Install
# Logs at C:\ProgramData\Diskless Manager\network-restore.log
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Server,
    [ValidateRange(1,65535)][int]$Port = 4237,
    [switch]$Install
)
$ErrorActionPreference = 'Stop'
$parsed = [System.Net.IPAddress]::None
if (-not [System.Net.IPAddress]::TryParse($Server, [ref]$parsed) -or
    $parsed.AddressFamily -ne [System.Net.Sockets.AddressFamily]::InterNetwork) {
    throw 'Server must be an IPv4 address'
}
$root = Join-Path $env:ProgramData 'Diskless Manager'
New-Item -ItemType Directory -Path $root -Force | Out-Null
$logFile = Join-Path $root 'network-restore.log'
if ($Install) {
    $destination = Join-Path $root 'restore-network.ps1'
    if ($MyInvocation.MyCommand.Path -ne $destination) {
        Copy-Item -LiteralPath $MyInvocation.MyCommand.Path -Destination $destination -Force
    }
    $arguments = ('-NoProfile -NonInteractive -ExecutionPolicy Bypass -File "{0}" -Server {1} -Port {2}' -f $destination,$Server,$Port)
    $action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument $arguments
    $trigger = New-ScheduledTaskTrigger -AtStartup
    $principal = New-ScheduledTaskPrincipal -UserId 'SYSTEM' -LogonType ServiceAccount -RunLevel Highest
    Register-ScheduledTask -TaskName 'DisklessManager-NetworkRestore' -Action $action -Trigger $trigger -Principal $principal -Force | Out-Null
    Write-Output "Installed startup task. Log: $logFile"
    exit 0
}
function Log([string]$message) {
    $entry = ('{0} {1}' -f (Get-Date -Format o),$message)
    Add-Content -LiteralPath $logFile -Value $entry
    Write-Output $entry
}
# Do not release/renew the DHCP lease or reset the NIC: this is the boot path.
$selected = $null
for ($attempt = 0; $attempt -lt 24 -and $null -eq $selected; $attempt++) {
    foreach ($adapter in @(Get-NetAdapter -ErrorAction SilentlyContinue | Where-Object { $_.Status -eq 'Up' })) {
        $mac = ($adapter.MacAddress -replace '[^0-9a-fA-F]', '').ToLowerInvariant()
        if ($mac.Length -ne 12) { continue }
        $url = 'http://{0}:{1}/boot/net-config/{2}' -f $Server,$Port,$mac
        try {
            $config = Invoke-RestMethod -Uri $url -Method Get -TimeoutSec 4
            if ($config.mac -ne $mac) { continue }
            $localIP = @(Get-NetIPAddress -AddressFamily IPv4 -InterfaceIndex $adapter.ifIndex -ErrorAction Stop |
                Where-Object { $_.IPAddress -eq $config.ip })
            if ($localIP.Count -eq 0) { continue }
            $selected = [pscustomobject]@{ Adapter = $adapter; Config = $config }
            break
        } catch { }
    }
    if ($null -eq $selected) { Start-Sleep -Seconds 5 }
}
if ($null -eq $selected) { throw 'No registered/reachable diskless NIC found; settings unchanged' }
$adapter = $selected.Adapter
$config = $selected.Config
$idx = [int]$adapter.ifIndex
Log "Verified $($config.mac) on $($adapter.Name), IPv4 $($config.ip)"
$gatewayAddress = [System.Net.IPAddress]::None
if ([System.Net.IPAddress]::TryParse([string]$config.gateway, [ref]$gatewayAddress) -and
    $gatewayAddress.AddressFamily -eq [System.Net.Sockets.AddressFamily]::InterNetwork -and
    $config.gateway -ne '0.0.0.0') {
    $existing = @(Get-NetRoute -DestinationPrefix '0.0.0.0/0' -InterfaceIndex $idx -ErrorAction SilentlyContinue |
        Where-Object { $_.NextHop -eq $config.gateway })
    if ($existing.Count -eq 0) {
        # Add a default route only; the directly connected iSCSI portal route
        # and established iSCSI session are left untouched.
        New-NetRoute -DestinationPrefix '0.0.0.0/0' -InterfaceIndex $idx -NextHop $config.gateway -RouteMetric 30 -PolicyStore ActiveStore -ErrorAction Stop | Out-Null
        Log "Added gateway $($config.gateway)"
    }
}
$dns = @($config.dns | Where-Object {
    $address = [System.Net.IPAddress]::None
    [System.Net.IPAddress]::TryParse([string]$_, [ref]$address) -and
        $address.AddressFamily -eq [System.Net.Sockets.AddressFamily]::InterNetwork
})
if ($dns.Count -gt 0) {
    Set-DnsClientServerAddress -InterfaceIndex $idx -ServerAddresses $dns -ErrorAction Stop
    Log "Restored DNS: $($dns -join ', ')"
}
Log 'Completed; no NIC reset, route flush, or DHCP renewal performed'
