#Requires -RunAsAdministrator
<#
.SYNOPSIS
  Registers the managed-client driver reporter as a SYSTEM startup task.

.DESCRIPTION
  Writes the per-client reporter configuration (server URL + client ID, no
  secrets) under ProgramData with a SYSTEM/Administrators-only ACL, then
  registers a scheduled task that runs
  `diskless-windows-servicer report-client <config>` at startup. The server
  authenticates each report against the client's live iSCSI session, so no
  credential is embedded in the common image.

.PARAMETER ServerUrl
  Base URL of the Diskless Manager server, e.g. http://192.0.2.1:8080.

.PARAMETER ClientId
  The managed client ID this machine is registered as.

.PARAMETER BinaryPath
  Path to diskless-windows-servicer.exe. Defaults to the installed location.

.PARAMETER NoExport
  Report Boot NIC IDs only; skip `pnputil /export-driver` package upload.

.PARAMETER ImageRevision
  Candidate revision this client is designated to test. Only set on the
  disposable test client.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$ServerUrl,
    [Parameter(Mandatory)]
    [string]$ClientId,
    [string]$ImageRevision = "",
    [string]$BinaryPath = (Join-Path $env:ProgramFiles "Diskless Manager\diskless-windows-servicer.exe"),
    [switch]$NoExport
)

$ErrorActionPreference = "Stop"

$configDir = Join-Path $env:ProgramData "Diskless Manager"
$configPath = Join-Path $configDir "client-report.json"
New-Item -ItemType Directory -Force -Path $configDir | Out-Null

$config = @{
    server_url     = $ServerUrl.TrimEnd("/")
    client_id      = $ClientId
    export_package = -not $NoExport.IsPresent
}
if (-not [string]::IsNullOrWhiteSpace($ImageRevision)) {
    $config["image_revision"] = $ImageRevision
}
$config | ConvertTo-Json -Depth 2 | Set-Content -LiteralPath $configPath -Encoding UTF8

$acl = Get-Acl -LiteralPath $configPath
$acl.SetAccessRuleProtection($true, $false)
foreach ($account in @("NT AUTHORITY\SYSTEM", "BUILTIN\Administrators")) {
    $rule = [Security.AccessControl.FileSystemAccessRule]::new(
        $account, "FullControl", "Allow")
    $acl.AddAccessRule($rule)
}
Set-Acl -LiteralPath $configPath -AclObject $acl

$action = New-ScheduledTaskAction -Execute $BinaryPath -Argument "report-client `"$configPath`""
$trigger = New-ScheduledTaskTrigger -AtStartup
$principal = New-ScheduledTaskPrincipal -UserId "NT AUTHORITY\SYSTEM" -RunLevel Highest
Register-ScheduledTask -TaskName "DisklessManagerClientReport" `
    -Action $action -Trigger $trigger -Principal $principal -Force | Out-Null

Write-Host "Reporter registered. Config: $configPath"
