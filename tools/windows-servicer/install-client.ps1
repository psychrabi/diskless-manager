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

New-Item -ItemType Directory -Force -Path $InstallDirectory, $DataDirectory, $driverDirectory | Out-Null
Copy-Item -LiteralPath $BinaryPath -Destination $destination -Force

$config = [ordered]@{
    server_url = $ServerUrl.TrimEnd('/')
    client_id = if ([string]::IsNullOrWhiteSpace($ClientId)) { $null } else { $ClientId.Trim() }
    export_package = $false
}
$config | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath $configPath -Encoding UTF8

# Restrict the client configuration and collected-driver workspace to SYSTEM and
# local administrators. The report protocol does not store a shared secret here.
& icacls.exe $DataDirectory /inheritance:r /grant:r "SYSTEM:(OI)(CI)(F)" "Administrators:(OI)(CI)(F)" | Out-Null

if ($RegisterStartupReport) {
    $taskName = "Diskless Manager Client Report"
    $arguments = "`"$destination`" report-client `"$configPath`""
    & schtasks.exe /Create /TN $taskName /SC ONSTART /RU SYSTEM /RL HIGHEST /TR $arguments /F | Out-Host
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to register the startup client-report task."
    }
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
