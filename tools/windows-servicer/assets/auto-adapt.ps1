<#
  NexBoot-compatible boot-time driver adaptation.
  This runs once as SYSTEM from the machine startup policy.
#>
$ErrorActionPreference = 'Stop'
$work = 'C:\ndadapt'
$done = Join-Path $work 'done.txt'
$exit = 0
function Log($m) { $m | Out-File (Join-Path $work 'adapt.log') -Append -Encoding utf8 }

function Get-InfBindings([string]$path) {
  $sections = @{}
  $current = ''
  foreach ($line in (Get-Content -LiteralPath $path)) {
    if ($line -match '^\s*\[([^]]+)\]') {
      $current = $Matches[1].Trim()
      if (-not $sections.ContainsKey($current)) { $sections[$current] = @() }
      continue
    }
    if ($current) { $sections[$current] += $line }
  }
  $bindings = @()
  foreach ($sectionName in $sections.Keys) {
    if ($sectionName -match '(?i)\.Services($|\.)') { continue }
    foreach ($line in $sections[$sectionName]) {
      if ($line -notmatch '^\s*[^;].*=\s*([^,]+),\s*(PCI\\[^;]+)$') { continue }
      $installSection = $Matches[1].Trim()
      $ids = $Matches[2] -split ',' | ForEach-Object { $_.Trim() } |
        Where-Object { $_ -match '^PCI\\VEN_[0-9A-Fa-f]{4}&DEV_[0-9A-Fa-f]{4}' }
      $serviceSections = $sections.Keys | Where-Object { $_ -ieq "$installSection.Services" -or $_ -like "$installSection.Services.*" }
      $services = @($serviceSections | ForEach-Object { $sections[$_] } |
        ForEach-Object { $_ } |
        Where-Object { $_ -match 'AddService\s*=\s*([^,\s]+)' } |
        ForEach-Object { $Matches[1] } | Sort-Object -Unique)
      if ($services.Count -eq 1) {
        foreach ($id in $ids) {
          $bindings += [pscustomobject]@{ Hwid = $id; Service = $services[0] }
        }
      }
    }
  }
  return $bindings
}

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
    if ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne 259) {
      throw "pnputil failed for $($inf.FullName) with exit code $LASTEXITCODE"
    }
  }

  foreach ($svc in @('msiscsi', 'iScsiPrt')) {
    $key = "HKLM:\SYSTEM\CurrentControlSet\Services\$svc"
    if (Test-Path $key) {
      Set-ItemProperty -Path $key -Name Start -Value 0 -Type DWord
    } elseif ($svc -eq 'msiscsi') {
      throw 'msiscsi service missing'
    }
  }

  foreach ($inf in $infs) {
    $svcNames = @(Select-String -Path $inf.FullName -Pattern 'AddService\s*=\s*([^,\s]+)' -AllMatches |
      ForEach-Object { $_.Matches } | ForEach-Object { $_.Groups[1].Value } | Sort-Object -Unique)
    $bindings = @(Get-InfBindings $inf.FullName)

    foreach ($svc in $svcNames) {
      $key = "HKLM:\SYSTEM\CurrentControlSet\Services\$svc"
      if (-not (Test-Path $key)) { throw "service $svc from $($inf.Name) is not registered after pnputil" }
      Set-ItemProperty -Path $key -Name Start -Value 0 -Type DWord
    }

    foreach ($binding in $bindings) {
      $cddbId = ($binding.Hwid -replace '\\', '#').ToUpper()
      if ($cddbId -match '^(PCI#VEN_[0-9A-F]{4}&DEV_[0-9A-F]{4})') {
        $cddbKey = "HKLM:\SYSTEM\CurrentControlSet\Control\CriticalDeviceDatabase\$($Matches[1])"
        if (-not (Test-Path $cddbKey)) { New-Item -Path $cddbKey -Force | Out-Null }
        Set-ItemProperty -Path $cddbKey -Name Service -Value $binding.Service -Type String
        Set-ItemProperty -Path $cddbKey -Name ClassGUID -Value '{4d36e972-e325-11ce-bfc1-08002be10318}' -Type String
      }
    }
  }
} catch {
  $exit = 1
  Log ("failure: " + $_.Exception.Message)
}

"OK=$($exit -eq 0) EXIT=$exit AT=$(Get-Date -Format o)" | Out-File $done -Encoding utf8

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

function Remove-AdaptRegistration([string]$gp) {
  $ini = Join-Path $gp 'Machine\Scripts\scripts.ini'
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
    if ($utf16) { [IO.File]::WriteAllText($ini, $newText, $enc) } else { [IO.File]::WriteAllText($ini, $newText, $enc) }
  }
  $startup = Join-Path $gp 'Machine\Scripts\Startup'
  Remove-Item -Force (Join-Path $startup 'adapt.ps1'), (Join-Path $startup 'ndadapt.cmd') -ErrorAction SilentlyContinue
  $gpt = Join-Path $gp 'gpt.ini'
  if (Test-Path $gpt) {
    $g = [IO.File]::ReadAllText($gpt)
    $g = [regex]::Replace($g, '(?m)^Version=(\d+)', { param($m) 'Version=' + ([int64]$m.Groups[1].Value + 1) })
    [IO.File]::WriteAllText($gpt, $g, [Text.Encoding]::Default)
  }
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
