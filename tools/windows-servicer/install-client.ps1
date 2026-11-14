[CmdletBinding()]
param(
    [string]$BinaryPath,
    [Parameter(Mandatory = $true)]
    [string]$ServerUrl,
    [string]$ClientId,
    [string]$InstallDirectory = (Join-Path $env:ProgramFiles "Diskless Manager"),
    [string]$DataDirectory = (Join-Path $env:ProgramData "Diskless Manager"),
    [switch]$RegisterStartupReport
)

$ErrorActionPreference = "Stop"

function Test-IsAdministrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

if (-not (Test-IsAdministrator)) {
    throw "Run this installer from an elevated PowerShell session."
}

if ([string]::IsNullOrWhiteSpace($BinaryPath)) {
    $BinaryPath = Join-Path $PSScriptRoot "diskless-windows-servicer.exe"
}
if (-not (Test-Path -LiteralPath $BinaryPath -PathType Leaf)) {
    $releaseCandidate = Join-Path $PSScriptRoot "target\release\diskless-windows-servicer.exe"
    if (Test-Path -LiteralPath $releaseCandidate -PathType Leaf) {
        $BinaryPath = $releaseCandidate
    }
    else {
        throw "diskless-windows-servicer.exe was not found. Pass -BinaryPath or place it beside install-client.ps1."
    }
}

$destination = Join-Path $InstallDirectory "diskless-windows-servicer.exe"
$driverDirectory = Join-Path $DataDirectory "Drivers"
$configPath = Join-Path $DataDirectory "client-report.json"

# Remove inherited and pre-existing grants before writing reporter configuration.
function Protect-ClientWorkspace([string]$Path) {
    $workspace = @(Get-Item -LiteralPath $Path) + @(Get-ChildItem -LiteralPath $Path -Force -Recurse)
    if (@($workspace | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }).Count) {
        throw "The client data directory must not contain symbolic links or junctions."
    }
    $identities = @(
        [Security.Principal.SecurityIdentifier]::new([Security.Principal.WellKnownSidType]::LocalSystemSid, $null),
        [Security.Principal.SecurityIdentifier]::new([Security.Principal.WellKnownSidType]::BuiltinAdministratorsSid, $null)
    )
    foreach ($item in $workspace) {
        $acl = Get-Acl -LiteralPath $item.FullName
        $acl.SetAccessRuleProtection($true, $false)
        foreach ($rule in @($acl.GetAccessRules($true, $false, [Security.Principal.SecurityIdentifier]))) {
            $acl.RemoveAccessRuleSpecific($rule)
        }
        $inheritance = if ($item.PSIsContainer) { 'ContainerInherit, ObjectInherit' } else { 'None' }
        foreach ($identity in $identities) {
            $rule = [Security.AccessControl.FileSystemAccessRule]::new(
                $identity, 'FullControl', $inheritance, 'None', 'Allow')
            $acl.AddAccessRule($rule)
        }
        $acl.SetOwner($identities[1])
        Set-Acl -LiteralPath $item.FullName -AclObject $acl
    }
}

New-Item -ItemType Directory -Force -Path $InstallDirectory, $DataDirectory, $driverDirectory | Out-Null
Protect-ClientWorkspace $DataDirectory
Copy-Item -LiteralPath $BinaryPath -Destination $destination -Force

$config = [ordered]@{
    server_url = $ServerUrl.TrimEnd('/')
    client_id = if ([string]::IsNullOrWhiteSpace($ClientId)) { $null } else { $ClientId.Trim() }
    export_package = $false
}
$config | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath $configPath -Encoding UTF8

if ($RegisterStartupReport) {
    $taskName = "Diskless Manager Client Report"
    $arguments = "`"$destination`" report-client `"$configPath`" --retry"
    & schtasks.exe /Create /TN $taskName /SC ONSTART /RU SYSTEM /RL HIGHEST /TR $arguments /F | Out-Host
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to register the startup client-report task."
    }
}

# Register the event-log source so DisklessManagerReporter service events
# render their message text instead of a description-lookup notice.
$eventSourceKey = "HKLM:\SYSTEM\CurrentControlSet\Services\EventLog\Application\DisklessManagerReporter"
New-Item -Path $eventSourceKey -Force | Out-Null
$eventMessageFile = Join-Path $env:SystemRoot "Microsoft.NET\Framework64\v4.0.30319\EventLogMessages.dll"
if (Test-Path -LiteralPath $eventMessageFile) {
    Set-ItemProperty -LiteralPath $eventSourceKey -Name EventMessageFile -Value $eventMessageFile
    Set-ItemProperty -LiteralPath $eventSourceKey -Name TypesSupported -Value 7
}

Write-Host "Installed Diskless Manager Windows client application:"
Write-Host "  $destination"
Write-Host "Client configuration:"
Write-Host "  $configPath"
Write-Host "Collected NIC driver directory:"
Write-Host "  $driverDirectory"
Write-Host ""
Write-Host "Launch the graphical worker application:"
Write-Host "  & `"$destination`""
Write-Host ""
Write-Host "Collect and upload all local third-party NIC drivers:"
Write-Host "  & `"$destination`" collect-nic `"$configPath`""
Write-Host "Install collected/known NIC drivers into this OS:"
Write-Host "  & `"$destination`" install-nic-drivers `"$driverDirectory`""
if ($RegisterStartupReport) {
    Write-Host "Startup reporting task registered: Diskless Manager Client Report"
}
Write-Host "Remove this installation with:"
Write-Host "  & `"$PSScriptRoot\uninstall-client.ps1`""
