<#
.SYNOPSIS
  Release smoke test (docs/PHASE8_INSTALLERS.md section 3): every package must install,
  run and uninstall without leaving anything behind.

.DESCRIPTION
  * Portable zip: nfd.exe runs, rules validate, data stays in <folder>\data
    (nothing written to the user profile), the app starts.
  * NSIS (per user): silent install -> app starts -> silent uninstall with /REMOVEDATA.
  * Branded setup (per user; plus "for everyone" when elevated): same cycle with
    the Explorer menu and a desktop shortcut.
  * MSI (per machine, needs admin; skipped otherwise): silent install -> app starts
    -> silent uninstall.
  After each uninstall nothing may remain: install folder, Apps & features entry,
  Start menu / desktop shortcuts, Explorer menu keys and (with /REMOVEDATA) the
  data folders.

  The user's own NANOFreeDisk data and Explorer menu keys are moved aside first and
  put back at the end, so the test can run on a developer machine.

.PARAMETER Dist     Folder containing the release artifacts.
.PARAMETER Version  Version string, e.g. 1.0.0
.PARAMETER Only     Run only these packages (portable, nsis, msi, setup).
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $Dist,
    [Parameter(Mandatory)] [string] $Version,
    [string[]] $Only = @("portable", "nsis", "msi", "setup")
)
$ErrorActionPreference = "Stop"
$app = "NANOFreeDisk"
$appId = "vn.io.nanodev.nanofreedisk"
$Dist = (Resolve-Path $Dist).Path

function Step($msg) { Write-Host "`n==> $msg" -ForegroundColor Cyan }
function Ok($msg) { Write-Host "    ok: $msg" -ForegroundColor Green }

$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)

$dataDirs = @("$env:LOCALAPPDATA\$appId", "$env:APPDATA\$appId", "$env:LOCALAPPDATA\$app\pawnio")
$shellKeys = foreach ($parent in "Directory\shell", "Directory\Background\shell", "Drive\shell") {
    foreach ($verb in "NANOFreeDisk", "NANOFreeDiskDupes") { "HKCU:\Software\Classes\$parent\$verb" }
}
$arpRoots = @(
    "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall",
    "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall",
    "HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"
)
$installDirs = @("$env:LOCALAPPDATA\$app", "$env:LOCALAPPDATA\Programs\$app", "$env:ProgramFiles\$app")
$shortcuts = @(
    "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\$app.lnk",
    "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\$app\$app.lnk",
    "$env:ProgramData\Microsoft\Windows\Start Menu\Programs\$app.lnk",
    "$env:ProgramData\Microsoft\Windows\Start Menu\Programs\$app\$app.lnk",
    "$([Environment]::GetFolderPath('Desktop'))\$app.lnk",
    "$env:PUBLIC\Desktop\$app.lnk"
)

function Get-ArpEntries {
    foreach ($r in $arpRoots) {
        Get-ChildItem $r -ErrorAction SilentlyContinue | ForEach-Object {
            $p = Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue
            if ($p.DisplayName -eq $app) { $_.Name }
        }
    }
}

function Get-Leftovers([switch] $WithData) {
    $left = @()
    $left += $installDirs | Where-Object { Test-Path $_ }
    $left += $shortcuts | Where-Object { Test-Path -LiteralPath $_ }
    $left += $shellKeys | Where-Object { Test-Path $_ }
    $left += @(Get-ArpEntries)
    if (Test-Path "HKCU:\Software\$app") { $left += "HKCU:\Software\$app" }
    $manu = "HKCU:\Software\NANO (nanodev.io.vn)"
    if (Test-Path $manu) { $left += $manu }
    if ($WithData) { $left += $dataDirs | Where-Object { Test-Path $_ } }
    $left
}

function Assert-Clean([string] $what, [switch] $WithData) {
    # uninstallers may finish removing themselves a moment after they exit
    for ($i = 0; $i -lt 30; $i++) {
        $left = @(Get-Leftovers -WithData:$WithData)
        if ($left.Count -eq 0) { Ok "$what left nothing behind"; return }
        Start-Sleep -Milliseconds 500
    }
    throw "$what left: $($left -join '; ')"
}

function Start-AppBriefly([string] $exe, [string] $label) {
    if (-not (Test-Path $exe)) { throw "$label`: $exe missing" }
    $p = Start-Process $exe -PassThru
    Start-Sleep -Seconds 8
    if ($p.HasExited) { throw "$label`: the app exited early (code $($p.ExitCode))" }
    Get-Process nanofreedisk -ErrorAction SilentlyContinue | Stop-Process -Force
    # the WebView2 child processes outlive a killed host for a moment and keep the
    # profile open; wait for ours so later deletes do not hit locked files
    for ($i = 0; $i -lt 30; $i++) {
        $webviews = @(Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe'" -ErrorAction SilentlyContinue |
                Where-Object { $_.CommandLine -match 'nanodev|NANOFreeDisk|nfd-portable' })
        if ($webviews.Count -eq 0) { break }
        if ($i -eq 10) { $webviews | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue } }
        Start-Sleep -Milliseconds 500
    }
    Start-Sleep -Seconds 1
    Ok "$label started"
}

function Wait-Gone([string] $path, [int] $seconds = 60) {
    for ($i = 0; $i -lt $seconds * 2; $i++) {
        if (-not (Test-Path $path)) { return }
        Start-Sleep -Milliseconds 500
    }
    throw "$path still present after $seconds s"
}

function Invoke-Quiet([string] $exe, [string[]] $argList) {
    # native stderr must not stop the script (Windows PowerShell wraps it as an error)
    $ErrorActionPreference = "Continue"
    & $exe @argList 2>&1 | Out-Null
    $LASTEXITCODE
}

function Fake-ShellMenu([string] $exe) {
    # what the app writes when the Explorer menu is turned on in Settings
    foreach ($k in $shellKeys) {
        New-Item -Path "$k\command" -Force | Out-Null
        Set-ItemProperty -Path "$k\command" -Name "(default)" -Value "`"$exe`" `"%V`""
    }
}

# -- keep the developer's own data out of harm's way -----------------
if (Get-Process nanofreedisk -ErrorAction SilentlyContinue) { throw "close NANOFreeDisk before running the smoke test" }
if (@(Get-ArpEntries).Count -gt 0) { throw "NANOFreeDisk is installed here already; uninstall it first" }
$backup = Join-Path ([IO.Path]::GetTempPath()) "nfd-smoke-backup-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Force $backup | Out-Null
$moved = @()
foreach ($d in $dataDirs) {
    if (Test-Path $d) {
        $aside = Join-Path $backup "data$($moved.Count)"
        Move-Item $d $aside
        $moved += , @($d, $aside)
    }
}
# the PawnIO module folder's parent is also the NSIS install folder
if ((Test-Path "$env:LOCALAPPDATA\$app") -and -not (Get-ChildItem "$env:LOCALAPPDATA\$app" -Force)) {
    Remove-Item "$env:LOCALAPPDATA\$app"
}
$i = 0
foreach ($k in $shellKeys) {
    if (Test-Path $k) {
        $file = Join-Path $backup "shell$i.reg"
        & reg.exe export ($k -replace '^HKCU:', 'HKCU') $file /y | Out-Null
        $i++
    }
}

try {
    # -- 1. portable zip ---------------------------------------------
    if ($Only -contains "portable") {
        Step "Portable zip"
        $zip = Join-Path $Dist "$app-$Version-x64-portable.zip"
        if (-not (Test-Path $zip)) { throw "missing $zip" }
        $tmp = Join-Path ([IO.Path]::GetTempPath()) "nfd-portable-$([guid]::NewGuid().ToString('N'))"
        Expand-Archive -Path $zip -DestinationPath $tmp -Force
        foreach ($f in "nfd.exe", "nanofreedisk.exe", "portable.flag", "LICENSE") {
            if (-not (Test-Path (Join-Path $tmp $f))) { throw "$f missing in the portable zip" }
        }
        $nfd = Join-Path $tmp "nfd.exe"
        $ver = & $nfd --version
        if ($ver -notmatch [regex]::Escape($Version)) { throw "nfd --version '$ver' does not contain $Version" }
        if ((Invoke-Quiet $nfd @("rules", "validate", (Join-Path $tmp "rules"))) -ne 0) { throw "rules validate failed" }
        if ((Invoke-Quiet $nfd @("scan", (Join-Path $tmp "rules"), "--save", "--json")) -ne 0) { throw "nfd scan --save failed" }
        if (-not (Test-Path (Join-Path $tmp "data\history.db"))) { throw "portable history not in data\" }
        Start-AppBriefly (Join-Path $tmp "nanofreedisk.exe") "portable app"
        if (-not (Test-Path (Join-Path $tmp "data\EBWebView"))) { throw "portable WebView2 profile not in data\EBWebView" }
        $leak = $dataDirs | Where-Object { Test-Path $_ }
        if ($leak) { throw "portable mode wrote to the user profile: $leak" }
        Ok "data stays next to the exe"
        for ($i = 0; (Test-Path $tmp) -and $i -lt 20; $i++) {
            try { Remove-Item -Recurse -Force $tmp -ErrorAction Stop } catch { Start-Sleep -Milliseconds 500 }
        }
        if (Test-Path $tmp) { throw "could not remove $tmp" }
    }

    # -- 2. NSIS (per user) ------------------------------------------
    $nsis = Join-Path $Dist "$app-$Version-x64-setup.exe"
    if ($Only -contains "nsis" -and (Test-Path $nsis)) {
        Step "NSIS silent install (per user)"
        $p = Start-Process $nsis -ArgumentList "/S" -Wait -PassThru
        if ($p.ExitCode -ne 0) { throw "NSIS install exit $($p.ExitCode)" }
        $dir = "$env:LOCALAPPDATA\$app"
        foreach ($f in "nanofreedisk.exe", "nfd.exe", "uninstall.exe") {
            if (-not (Test-Path "$dir\$f")) { throw "$f missing in $dir" }
        }
        if (-not (Get-ArpEntries)) { throw "no Apps & features entry" }
        $ver = & "$dir\nfd.exe" --version
        if ($ver -notmatch [regex]::Escape($Version)) { throw "installed nfd --version '$ver'" }
        Start-AppBriefly "$dir\nanofreedisk.exe" "NSIS app"
        Fake-ShellMenu "$dir\nanofreedisk.exe"
        Step "NSIS silent uninstall (/REMOVEDATA)"
        Start-Process "$dir\uninstall.exe" -ArgumentList "/S", "/REMOVEDATA" -Wait | Out-Null
        Wait-Gone "$dir\nanofreedisk.exe"
        Assert-Clean "NSIS" -WithData
    } elseif ($Only -contains "nsis") { Write-Host "NSIS setup not present - skipped" -ForegroundColor Yellow }

    # -- 3. MSI (per machine) ----------------------------------------
    $msi = Join-Path $Dist "$app-$Version-x64.msi"
    if ($Only -contains "msi" -and (Test-Path $msi) -and $isAdmin) {
        Step "MSI silent install"
        $p = Start-Process msiexec -ArgumentList "/i `"$msi`" /qn /norestart /l*v `"$env:TEMP\nfd-msi-install.log`"" -Wait -PassThru
        if ($p.ExitCode -ne 0) { Get-Content "$env:TEMP\nfd-msi-install.log" -Tail 50; throw "msiexec install exit $($p.ExitCode)" }
        $dir = "$env:ProgramFiles\$app"
        foreach ($f in "nanofreedisk.exe", "nfd.exe") {
            if (-not (Test-Path "$dir\$f")) { throw "$f missing in $dir" }
        }
        Start-AppBriefly "$dir\nanofreedisk.exe" "MSI app"
        Fake-ShellMenu "$dir\nanofreedisk.exe"
        Step "MSI silent uninstall"
        $p = Start-Process msiexec -ArgumentList "/x `"$msi`" /qn /norestart /l*v `"$env:TEMP\nfd-msi-uninstall.log`"" -Wait -PassThru
        if ($p.ExitCode -ne 0) { Get-Content "$env:TEMP\nfd-msi-uninstall.log" -Tail 50; throw "msiexec uninstall exit $($p.ExitCode)" }
        Assert-Clean "MSI"
        # a quiet MSI uninstall keeps user data by design (docs/PHASE8_INSTALLERS.md section 2.1)
        foreach ($d in $dataDirs) { if (Test-Path $d) { Remove-Item -Recurse -Force $d } }
    } elseif ($Only -contains "msi") { Write-Host "MSI skipped (missing or not running as administrator)" -ForegroundColor Yellow }

    # -- 4. Branded setup --------------------------------------------
    $branded = Join-Path $Dist "$app-Setup-$Version-x64.exe"
    if ($Only -contains "setup" -and (Test-Path $branded)) {
        $modes = @(@{ Name = "per user"; Args = @(); Dir = "$env:LOCALAPPDATA\Programs\$app" })
        if ($isAdmin) { $modes += @{ Name = "for everyone"; Args = @("/ALLUSERS"); Dir = "$env:ProgramFiles\$app" } }
        foreach ($m in $modes) {
            Step "Branded setup silent install ($($m.Name))"
            $p = Start-Process $branded -ArgumentList (@("/S", "/DESKTOP", "/SHELLMENU") + $m.Args) -Wait -PassThru
            if ($p.ExitCode -ne 0) { throw "branded setup exit $($p.ExitCode)" }
            $dir = $m.Dir
            foreach ($f in "nanofreedisk.exe", "nfd.exe", "uninstall.exe", "install.manifest") {
                if (-not (Test-Path "$dir\$f")) { throw "$f missing in $dir" }
            }
            if (-not (Get-ArpEntries)) { throw "no Apps & features entry" }
            if (-not (Test-Path $shellKeys[0])) { throw "Explorer menu not registered" }
            if (-not ($shortcuts | Where-Object { Test-Path -LiteralPath $_ })) { throw "no shortcut created" }
            Start-AppBriefly "$dir\nanofreedisk.exe" "branded app"
            Step "Branded setup silent uninstall (/REMOVEDATA)"
            $p = Start-Process "$dir\uninstall.exe" -ArgumentList "--uninstall", "/S", "/REMOVEDATA" -Wait -PassThru
            if ($p.ExitCode -ne 0) { throw "branded uninstall exit $($p.ExitCode)" }
            Wait-Gone "$dir\uninstall.exe" 15
            Assert-Clean "branded setup ($($m.Name))" -WithData
        }
    } elseif ($Only -contains "setup") { Write-Host "Branded setup not present - skipped" -ForegroundColor Yellow }

    Write-Host "`nSmoke test passed." -ForegroundColor Green
}
finally {
    Get-Process nanofreedisk -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    foreach ($pair in $moved) {
        if (Test-Path $pair[0]) { Remove-Item -Recurse -Force $pair[0] }
        New-Item -ItemType Directory -Force (Split-Path $pair[0]) | Out-Null
        Move-Item $pair[1] $pair[0]
    }
    Get-ChildItem $backup -Filter *.reg -ErrorAction SilentlyContinue | ForEach-Object { & reg.exe import $_.FullName 2>$null }
    Remove-Item -Recurse -Force $backup -ErrorAction SilentlyContinue
}
