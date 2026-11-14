[CmdletBinding()]
param(
    [string]$InstallDirectory = (Join-Path $env:ProgramFiles "Diskless Manager"),
    [string]$DataDirectory = (Join-Path $env:ProgramData "Diskless Manager")
)

$ErrorActionPreference = "Stop"

function Test-IsAdministrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

if (-not (Test-IsAdministrator)) {
    throw "Run this uninstaller from an elevated PowerShell session."
}

# Remove both startup-task spellings and, if this machine ever booted a
# managed image, the one-shot reporter service the image registered.
foreach ($taskName in @("Diskless Manager Client Report", "DisklessManagerClientReport")) {
    & schtasks.exe /Delete /TN $taskName /F | Out-Null
    if ($LASTEXITCODE -eq 0) {
        Write-Host "Removed the startup report task '$taskName'."
    }
}

$service = Get-Service -Name DisklessManagerReporter -ErrorAction SilentlyContinue
if ($service) {
    & sc.exe delete DisklessManagerReporter | Out-Null
    Write-Host "Marked the DisklessManagerReporter service for deletion."
}

$eventSourceKey = "HKLM:\SYSTEM\CurrentControlSet\Services\EventLog\Application\DisklessManagerReporter"
if (Test-Path -LiteralPath $eventSourceKey) {
    Remove-Item -LiteralPath $eventSourceKey -Recurse
    Write-Host "Removed the DisklessManagerReporter event-log source."
}

$destination = Join-Path $InstallDirectory "diskless-windows-servicer.exe"
if (Test-Path -LiteralPath $destination) {
    Remove-Item -LiteralPath $destination -Force
    Write-Host "Removed $destination"
}

if (Test-Path -LiteralPath $DataDirectory) {
    Remove-Item -LiteralPath $DataDirectory -Recurse -Force
    Write-Host "Removed $DataDirectory (reporter configuration, logs, and collected drivers)."
}

if ((Test-Path -LiteralPath $InstallDirectory) -and -not (Get-ChildItem -LiteralPath $InstallDirectory -Force)) {
    Remove-Item -LiteralPath $InstallDirectory -Force
}

Write-Host "Diskless Manager client application removed."
Write-Host "Files locked by a running report are removed after the next reboot."
