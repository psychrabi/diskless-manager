$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$principal = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Run the Windows client as administrator.'
}
$changes = @([Console]::In.ReadToEnd() | ConvertFrom-Json)
$mutex = [Threading.Mutex]::new($false, 'Global\DisklessManagerLocalBootPreparation')
$locked = $false
$machine = [Microsoft.Win32.Registry]::LocalMachine
try {
    try { $locked = $mutex.WaitOne(0) } catch [Threading.AbandonedMutexException] { $locked = $true }
    if (-not $locked) { throw 'Another diskless preparation is running.' }
    $original = @()
    foreach ($change in $changes) {
        if ($change.key -notmatch '^ControlSet[0-9]{3}\\' -or $change.key.Contains('..')) { throw 'Invalid preparation registry key.' }
        $path = 'SYSTEM\' + $change.key
        $key = $machine.OpenSubKey($path)
        try {
            $exists = $null -ne $key -and $key.GetValueNames() -contains $change.name
            $savedValue = $null
            if ($exists) { $savedValue = $key.GetValue($change.name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) }
            $original += [pscustomobject]@{
                path = $path; name = $change.name; key_existed = $null -ne $key; existed = $exists
                kind = $(if ($exists) { [string]$key.GetValueKind($change.name) } else { 'String' })
                value = $savedValue
            }
        } finally { if ($key) { $key.Dispose() } }
    }
    # Create the journal with restrictive ACLs from the outset, including standalone use.
    $directory = Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) ('DisklessManager-BootBackup-' + [Guid]::NewGuid().ToString('N'))
    $acl = [Security.AccessControl.DirectorySecurity]::new()
    $acl.SetAccessRuleProtection($true, $false)
    foreach ($sid in @('S-1-5-18', 'S-1-5-32-544')) {
        $identity = [Security.Principal.SecurityIdentifier]::new($sid)
        $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($identity, 'FullControl', 'ContainerInherit, ObjectInherit', 'None', 'Allow'))
    }
    $acl.SetOwner([Security.Principal.SecurityIdentifier]::new('S-1-5-32-544'))
    [IO.DirectoryInfo]::new($directory).Create($acl)
    $backup = Join-Path $directory 'registry-before.clixml'
    $original | Export-Clixml -LiteralPath ($backup + '.tmp') -Depth 8
    Move-Item -LiteralPath ($backup + '.tmp') -Destination $backup
    try {
        foreach ($change in $changes) {
            $key = $machine.CreateSubKey('SYSTEM\' + $change.key)
            try {
                $kind = switch ($change.value.type) {
                    'dword' { [Microsoft.Win32.RegistryValueKind]::DWord }
                    'string' { [Microsoft.Win32.RegistryValueKind]::String }
                    'expand_string' { [Microsoft.Win32.RegistryValueKind]::ExpandString }
                    'multi_string' { [Microsoft.Win32.RegistryValueKind]::MultiString }
                    default { throw 'Unsupported preparation value type.' }
                }
                $value = $change.value.value
                if ($kind -eq 'DWord') { $value = [BitConverter]::ToInt32([BitConverter]::GetBytes([uint32]$value), 0) }
                if ($kind -eq 'MultiString') { $value = [string[]]$value }
                $key.SetValue($change.name, $value, $kind)
                $key.Flush()
            } finally { if ($key) { $key.Dispose() } }
        }
    } catch {
        $failure = $_.Exception.Message
        $rollbackErrors = @()
        [array]::Reverse($original)
        foreach ($before in $original) {
            try {
                $key = $machine.OpenSubKey($before.path, $true)
                if ($null -eq $key) {
                    if ($before.existed) { throw ('Registry key disappeared: ' + $before.path) }
                    continue
                }
                $removeEmpty = $false
                try {
                    if ($before.existed) {
                        $restoreValue = $before.value
                        if ($before.kind -eq 'MultiString') { $restoreValue = [string[]]$restoreValue }
                        if ($before.kind -eq 'Binary') { $restoreValue = [byte[]]$restoreValue }
                        $key.SetValue($before.name, $restoreValue, [Microsoft.Win32.RegistryValueKind]$before.kind)
                    } else { $key.DeleteValue($before.name, $false) }
                    $key.Flush()
                    $removeEmpty = -not $before.key_existed -and $key.ValueCount -eq 0 -and $key.SubKeyCount -eq 0
                } finally { $key.Dispose() }
                if ($removeEmpty) { $machine.DeleteSubKey($before.path, $false) }
            } catch { $rollbackErrors += $_.Exception.Message }
        }
        if ($rollbackErrors.Count) { throw "Preparation failed: $failure. Rollback incomplete: $($rollbackErrors -join '; '). Recovery journal: $backup" }
        throw "Preparation failed: $failure. Original registry values restored. Recovery journal: $backup"
    }
    @{ backup_path = $backup } | ConvertTo-Json -Compress
} finally {
    if ($locked) { $mutex.ReleaseMutex() }
    $mutex.Dispose()
}
