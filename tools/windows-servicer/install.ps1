[CmdletBinding()]
param(
    [string]$BinaryPath = (Join-Path $PSScriptRoot "diskless-windows-servicer.exe"),
    [string]$InstallDirectory = (Join-Path $env:ProgramFiles "Diskless Manager"),
    [switch]$ConfigureOpenSsh
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

if (-not (Test-Path -LiteralPath $BinaryPath -PathType Leaf)) {
    $releaseCandidate = Join-Path $PSScriptRoot "target\release\diskless-windows-servicer.exe"
    if (Test-Path -LiteralPath $releaseCandidate -PathType Leaf) {
        $BinaryPath = $releaseCandidate
    }
    else {
        throw "diskless-windows-servicer.exe was not found. Pass -BinaryPath or place the executable beside install.ps1."
    }
}

$destination = Join-Path $InstallDirectory "diskless-windows-servicer.exe"
$stagingDirectory = Join-Path $env:ProgramData "Diskless Manager\staging"

New-Item -ItemType Directory -Force -Path $InstallDirectory | Out-Null
New-Item -ItemType Directory -Force -Path $stagingDirectory | Out-Null

$sourceFull = [IO.Path]::GetFullPath($BinaryPath)
$destinationFull = [IO.Path]::GetFullPath($destination)
if (-not $sourceFull.Equals($destinationFull, [StringComparison]::OrdinalIgnoreCase)) {
    Copy-Item -LiteralPath $BinaryPath -Destination $destination -Force
}

if ($ConfigureOpenSsh) {
    $capability = Get-WindowsCapability -Online | Where-Object Name -Like "OpenSSH.Server*"
    if (-not $capability) {
        throw "Windows OpenSSH Server capability is not available on this system."
    }

    if ($capability.State -ne "Installed") {
        Write-Host "Installing Windows OpenSSH Server..."
        Add-WindowsCapability -Online -Name $capability.Name | Out-Null
    }

    Set-Service -Name sshd -StartupType Automatic
    Start-Service -Name sshd

    $firewallRule = Get-NetFirewallRule -Name "OpenSSH-Server-In-TCP" -ErrorAction SilentlyContinue
    if (-not $firewallRule) {
        $firewallParams = @{
            Name = "OpenSSH-Server-In-TCP"
            DisplayName = "OpenSSH Server (sshd)"
            Enabled = "True"
            Direction = "Inbound"
            Protocol = "TCP"
            Action = "Allow"
            LocalPort = 22
        }
        New-NetFirewallRule @firewallParams | Out-Null
    }
    else {
        Enable-NetFirewallRule -Name "OpenSSH-Server-In-TCP"
    }
}

Write-Host "Installed Windows servicing helper to:"
Write-Host "  $destination"
Write-Host ""
Write-Host "Capability check:"
& $destination capabilities
if ($LASTEXITCODE -ne 0) {
    throw "The Windows servicing helper capability check failed."
}

Write-Host ""
if ($ConfigureOpenSsh) {
    Write-Host "OpenSSH Server is configured and listening through the Windows firewall."
}
else {
    Write-Host "OpenSSH was not modified. Use -ConfigureOpenSsh if this machine still needs an SSH server."
}
Write-Host "Configure key-based SSH access from the Diskless Manager server before using remote servicing."
