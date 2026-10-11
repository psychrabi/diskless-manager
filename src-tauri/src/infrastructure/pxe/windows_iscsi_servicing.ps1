$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::InputEncoding = [Text.UTF8Encoding]::new($false)
$request = [Console]::In.ReadToEnd() | ConvertFrom-Json
$connection = $request.connection
$target = $connection.target_iqn
$diskNumber = $null
$originalGuid = $null
$identity = $null
$connected = $false
$mounts = @()
$directory = Join-Path $env:ProgramData ('Diskless Manager\staging\volume-' + [guid]::NewGuid())
$journal = Join-Path $directory 'lease.json'
$result = $null
$failure = $null
$releaseErrors = @()
try {
    if (@(Get-IscsiSession | Where-Object TargetNodeAddress -eq $target).Count) {
        throw 'Servicing target is already connected; refusing to reuse a session'
    }
    New-Item -ItemType Directory -Path $directory | Out-Null
    if (!(Get-IscsiTargetPortal | Where-Object { $_.TargetPortalAddress -eq $connection.portal -and $_.TargetPortalPortNumber -eq 3260 })) {
        New-IscsiTargetPortal -TargetPortalAddress $connection.portal -TargetPortalPortNumber 3260 | Out-Null
    }
    Update-IscsiTargetPortal -TargetPortalAddress $connection.portal -TargetPortalPortNumber 3260
    $connected = $true
    Connect-IscsiTarget -NodeAddress $target -TargetPortalAddress $connection.portal -TargetPortalPortNumber 3260 -AuthenticationType ONEWAYCHAP -ChapUsername $connection.chap_username -ChapSecret $connection.chap_password -IsPersistent $false | Out-Null
    $disks = @()
    for ($attempt = 0; $attempt -lt 30; $attempt++) {
        $disks = @(Get-IscsiSession | Where-Object TargetNodeAddress -eq $target | Get-Disk)
        if ($disks.Count) { break }
        Start-Sleep -Milliseconds 500
    }
    if ($disks.Count -ne 1) { throw 'Servicing target must expose exactly one disk' }
    $disk = $disks[0]
    if ($disk.IsBoot -or $disk.IsSystem -or $disk.BusType -ne 'iSCSI') { throw 'Refusing to service a boot/system or non-iSCSI disk' }
    if ($disk.Size -ne $connection.expected_size_bytes -or $disk.PartitionStyle -ne 'GPT') { throw 'Clone size or GPT partition style did not match' }
    $diskNumber = $disk.Number
    $originalGuid = $disk.Guid
    @{target=$target;disk=$diskNumber;original_guid=$originalGuid} | ConvertTo-Json | Set-Content $journal
    Set-Disk -Number $diskNumber -IsOffline $true
    Set-Disk -Number $diskNumber -IsReadOnly $false
    $geometry = @{disk_number=$diskNumber;size_bytes=$disk.Size;sector_size=$disk.LogicalSectorSize}
    $captured = ($geometry | ConvertTo-Json -Compress) | & $request.executable gpt-capture
    if ($LASTEXITCODE -ne 0) { throw ('GPT capture failed: ' + ($captured | ConvertFrom-Json).error) }
    $identity = ($captured | ConvertFrom-Json).result
    @{target=$target;disk=$diskNumber;identity=$identity} | ConvertTo-Json -Depth 10 -Compress | Set-Content $journal
    if (@(Get-Disk | Where-Object { $_.Number -ne $diskNumber -and $_.Guid -eq $originalGuid }).Count) {
        # Windows can also regenerate partition GUIDs; restore the saved GPT after dismounting.
        Set-Disk -Number $diskNumber -Guid ([guid]::NewGuid().ToString('B'))
    }
    Set-Disk -Number $diskNumber -IsOffline $false
    $windowsRoots = @()
    foreach ($partition in @(Get-Partition -DiskNumber $diskNumber | Where-Object GptType -eq '{ebd0a0a2-b9e5-4433-87c0-68b6b72699c7}')) {
        $path = (Join-Path $directory ('partition-' + $partition.PartitionNumber)) + '\'
        New-Item -ItemType Directory -Path $path | Out-Null
        Add-PartitionAccessPath -DiskNumber $diskNumber -PartitionNumber $partition.PartitionNumber -AccessPath $path
        $mounts += @{partition=$partition.PartitionNumber;path=$path}
        $mapped = @(Get-Partition | Where-Object { $_.AccessPaths -contains $path })
        if ($mapped.Count -ne 1 -or $mapped[0].DiskNumber -ne $diskNumber) { throw 'Mount path does not uniquely identify the clone partition' }
        if (Test-Path (Join-Path $path 'Windows\System32\config\SYSTEM')) { $windowsRoots += $path }
    }
    if ($windowsRoots.Count -ne 1) { throw 'Clone must contain exactly one accessible Windows installation' }
    $request.preparation.image_path = $windowsRoots[0]
    $request.preparation.iscsi = $null
    $payload = $request.preparation | ConvertTo-Json -Depth 30 -Compress
    $response = $payload | & $request.executable prepare-volume
    if ($LASTEXITCODE -ne 0) {
        $message = ($response | ConvertFrom-Json).error
        throw ('Offline volume preparation failed: ' + $message)
    }
    $result = ($response | ConvertFrom-Json).result
} catch {
    $failure = 'iSCSI servicing step at line ' + $_.InvocationInfo.ScriptLineNumber + ': ' + $_.Exception.Message
} finally {
    foreach ($mount in $mounts) {
        try { Remove-PartitionAccessPath -DiskNumber $diskNumber -PartitionNumber $mount.partition -AccessPath $mount.path }
        catch { $releaseErrors += $_.Exception.Message }
    }
    if ($null -ne $diskNumber) {
        try {
            $disk = Get-Disk -Number $diskNumber
            $owned = @(Get-IscsiSession | Where-Object TargetNodeAddress -eq $target | Get-Disk)
            if ($owned.Count -ne 1 -or $owned[0].Number -ne $diskNumber) { throw 'Disk no longer belongs to the servicing session; refusing release writes' }
            if ($disk.IsBoot -or $disk.IsSystem) { throw 'Disk unexpectedly became a boot/system disk; refusing release writes' }
            Set-Disk -Number $diskNumber -IsOffline $true
            if ($null -ne $identity) {
                $geometry.identity = $identity
                $restored = ($geometry | ConvertTo-Json -Depth 10 -Compress) | & $request.executable gpt-restore
                if ($LASTEXITCODE -ne 0) { throw ('GPT restore failed: ' + ($restored | ConvertFrom-Json).error) }
                Update-Disk -Number $diskNumber
                if ((Get-Disk -Number $diskNumber).Guid -ne $originalGuid) { throw 'Original GPT disk ID was not restored' }
            }
        } catch { $releaseErrors += $_.Exception.Message }
    }
    if ($connected -and !$releaseErrors.Count) {
        try {
            if (@(Get-IscsiSession | Where-Object TargetNodeAddress -eq $target).Count) {
                Disconnect-IscsiTarget -NodeAddress $target -Confirm:$false
            }
            if (@(Get-IscsiSession | Where-Object TargetNodeAddress -eq $target).Count) { throw 'Servicing session remains connected' }
        } catch { $releaseErrors += $_.Exception.Message }
    }
    if (!$releaseErrors.Count -and (Test-Path $directory)) { Remove-Item $directory -Recurse -Force }
}
if ($releaseErrors.Count) { $failure = ($failure + '; release failed: ' + ($releaseErrors -join '; ') + '; lease: ' + $journal) }
@{ok=(!$failure);result=$result;error=$failure;released=(!$releaseErrors.Count)} | ConvertTo-Json -Depth 100 -Compress
if ($failure) { exit 1 }
