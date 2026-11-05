<#
  NexBoot-compatible boot-time driver adaptation.
  This runs once as SYSTEM from the machine startup policy.
#>
$ErrorActionPreference = 'Stop'
$work = 'C:\ndadapt'
$done = Join-Path $work 'done.txt'
$exit = 0
$rebootRequired = $false
function Log($m) { $m | Out-File (Join-Path $work 'adapt.log') -Append -Encoding utf8 }

New-Item -ItemType Directory -Force -Path $work | Out-Null
Log ("start " + (Get-Date -Format o) + " as " + (whoami))

try {
  $drivers = Join-Path $work 'drivers'
  Remove-Item -Recurse -Force $drivers -ErrorAction SilentlyContinue
  New-Item -ItemType Directory -Force -Path $drivers | Out-Null
  $zip = Join-Path $work 'bundle.zip'
  if (-not (Test-Path -LiteralPath $zip -PathType Leaf)) { throw 'driver bundle is missing' }
  Expand-Archive -Path $zip -DestinationPath $drivers -Force
  $infs = @(Get-ChildItem -Path $drivers -Recurse -Filter *.inf -File -ErrorAction Stop)
  Log ("inf count = " + $infs.Count)
  if ($infs.Count -eq 0) { throw 'driver bundle contains no INF files' }

  $pnputilLog = Join-Path $work 'pnputil.log'
  foreach ($inf in $infs) {
    & pnputil.exe /add-driver $inf.FullName /install 2>&1 | Out-File $pnputilLog -Append -Encoding utf8
    if ($LASTEXITCODE -eq 3010) {
      $rebootRequired = $true
    } elseif ($LASTEXITCODE -eq 259) {
      Log "pnputil $($inf.Name) returned 259 (no matching device or a better driver already bound); continuing"
    } elseif ($LASTEXITCODE -ne 0) {
      throw "pnputil failed for $($inf.FullName) with exit code $LASTEXITCODE"
    }
  }

  $iscsiDriver = 'HKLM:\SYSTEM\CurrentControlSet\Services\iScsiPrt'
  if (Test-Path $iscsiDriver) {
    $driverType = (Get-ItemProperty -Path $iscsiDriver -Name Type).Type
    if ($driverType -ne 1 -and $driverType -ne 2) { throw 'iScsiPrt is not a driver service' }
    Set-ItemProperty -Path $iscsiDriver -Name Start -Value 0 -Type DWord
  }

} catch {
  $exit = 1
  Log ("failure: " + $_.Exception.Message)
}

"OK=$($exit -eq 0) EXIT=$exit REBOOT_REQUIRED=$rebootRequired AT=$(Get-Date -Format o)" | Out-File $done -Encoding utf8

function Add-StartupItems($out, $items) {
  $k = 0
  foreach ($it in ($items | Sort-Object N)) {
    if ($it.CmdLine -ieq 'ndadapt.cmd') { continue }
    $out.Add("$($k)CmdLine=$($it.CmdLine)")
    $out.Add("$($k)Parameters=$($it.Parameters)")
    $k++
  }
  $items.Clear()
}

function Write-AtomicPolicyFile([string]$path, [string]$text, [Text.Encoding]$encoding) {
  $temp = Join-Path (Split-Path -Parent $path) ('.diskless-' + [guid]::NewGuid().ToString('N') + '.tmp')
  try {
    $stream = [IO.File]::Open($temp, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {
      $writer = [IO.StreamWriter]::new($stream, $encoding)
      try { $writer.Write($text); $writer.Flush(); $stream.Flush($true) } finally { $writer.Dispose() }
    } finally { $stream.Dispose() }
    [IO.File]::Replace($temp, $path, $null)
  } finally {
    if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Force }
  }
}

function Remove-AdaptRegistration([string]$gp) {
  $ini = Join-Path $gp 'Machine\Scripts\scripts.ini'
  $newText = $null
  if (Test-Path $ini) {
    $bytes = [IO.File]::ReadAllBytes($ini)
    $utf16 = $bytes.Length -ge 2 -and $bytes[0] -eq 0xFF -and $bytes[1] -eq 0xFE
    $enc = if ($utf16) { New-Object Text.UnicodeEncoding($false, $true) } else { [Text.Encoding]::Default }
    $text = if ($utf16) { $enc.GetString($bytes, 2, $bytes.Length - 2) } else { $enc.GetString($bytes) }
    $out = New-Object System.Collections.Generic.List[string]
    $items = New-Object System.Collections.Generic.List[object]
    $inStartup = $false
    foreach ($line in ($text -split "`r?`n")) {
      if ($line -match '^\s*\[(.+)\]\s*$') {
        if ($inStartup) { Add-StartupItems $out $items }
        $inStartup = ($Matches[1] -ieq 'Startup')
        $out.Add($line); continue
      }
      if ($inStartup -and $line -match '^\s*(\d+)(CmdLine|Parameters)=(.*)$') {
        $n = [int]$Matches[1]
        $item = $items | Where-Object { $_.N -eq $n } | Select-Object -First 1
        if (-not $item) { $item = [pscustomobject]@{ N = $n; CmdLine = ''; Parameters = '' }; $items.Add($item) }
        $item.($Matches[2]) = $Matches[3]
        continue
      }
      if ($inStartup -and $line.Trim() -eq '') { continue }
      $out.Add($line)
    }
    if ($inStartup) { Add-StartupItems $out $items }
    $newText = (($out -join "`r`n").TrimEnd() + "`r`n")
  }
  $gpt = Join-Path $gp 'gpt.ini'
  if (Test-Path $gpt) {
    $gptBytes = [IO.File]::ReadAllBytes($gpt)
    $gptUtf16 = $gptBytes.Length -ge 2 -and $gptBytes[0] -eq 0xFF -and $gptBytes[1] -eq 0xFE
    $gptEncoding = if ($gptUtf16) { [Text.UnicodeEncoding]::new($false, $true) } else { [Text.Encoding]::Default }
    $g = [IO.File]::ReadAllText($gpt)
    $g = [regex]::Replace($g, '(?m)^Version=(\d+)', { param($m) 'Version=' + ([int64]$m.Groups[1].Value + 1) })
    Write-AtomicPolicyFile $gpt $g $gptEncoding
  }
  if ($null -ne $newText) {
    Write-AtomicPolicyFile $ini $newText $enc
  }
  $startup = Join-Path $gp 'Machine\Scripts\Startup'
  Remove-Item -Force (Join-Path $startup 'adapt.ps1'), (Join-Path $startup 'ndadapt.cmd') -ErrorAction SilentlyContinue
}

# Never remove the registration after a failed install. The next boot can retry.
if ($exit -eq 0) {
  try {
    Remove-AdaptRegistration 'C:\Windows\System32\GroupPolicy'
  } catch {
    Log ("registration cleanup failed; preserving retry registration: " + $_.Exception.Message)
  }
} else {
  Log 'installation failed; preserving startup registration for retry'
}
Log "install completed with exit=$exit"
exit $exit
