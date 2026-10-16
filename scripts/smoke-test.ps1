<#
.SYNOPSIS
  Release smoke test: the portable CLI must run, and each installer (when present)
  must install silently, expose a working nfd.exe, and uninstall cleanly.
.PARAMETER Dist   Folder containing release artifacts.
.PARAMETER Version Version string, e.g. 1.0.0
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $Dist,
    [Parameter(Mandatory)] [string] $Version
)
$ErrorActionPreference = "Stop"
$app = "NANOFreeDisk"

function Step($msg) { Write-Host "`n==> $msg" -ForegroundColor Cyan }

# ── 1. portable zip ───────────────────────────────────────────────
Step "Portable zip"
$zip = Join-Path $Dist "$app-$Version-x64-portable.zip"
if (-not (Test-Path $zip)) { throw "missing $zip" }
$tmp = Join-Path ([IO.Path]::GetTempPath()) "nfd-smoke-$([guid]::NewGuid().ToString('N'))"
Expand-Archive -Path $zip -DestinationPath $tmp -Force
$nfd = Join-Path $tmp "nfd.exe"
if (-not (Test-Path $nfd)) { throw "nfd.exe missing in portable zip" }
$ver = & $nfd --version
Write-Host $ver
if ($ver -notmatch [regex]::Escape($Version)) { throw "nfd --version '$ver' does not contain $Version" }
& $nfd rules validate (Join-Path $tmp "rules")
if ($LASTEXITCODE -ne 0) { throw "rules validate failed" }
Remove-Item -Recurse -Force $tmp

# ── 2. MSI (silent install / uninstall) ───────────────────────────
$msi = Join-Path $Dist "$app-$Version-x64.msi"
if (Test-Path $msi) {
    Step "MSI silent install"
    $p = Start-Process msiexec -ArgumentList "/i `"$msi`" /qn /norestart /l*v msi-install.log" -Wait -PassThru
    if ($p.ExitCode -ne 0) { Get-Content msi-install.log -Tail 50; throw "msiexec install exit $($p.ExitCode)" }
    $installed = Get-ChildItem "${env:ProgramFiles}\$app", "${env:LOCALAPPDATA}\Programs\$app" -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $installed) { throw "install folder not found after MSI install" }
    Step "MSI silent uninstall"
    $p = Start-Process msiexec -ArgumentList "/x `"$msi`" /qn /norestart /l*v msi-uninstall.log" -Wait -PassThru
    if ($p.ExitCode -ne 0) { Get-Content msi-uninstall.log -Tail 50; throw "msiexec uninstall exit $($p.ExitCode)" }
} else { Write-Host "MSI not present — skipped" -ForegroundColor Yellow }

# ── 3. NSIS classic exe ───────────────────────────────────────────
$nsis = Join-Path $Dist "$app-$Version-x64-setup.exe"
if (Test-Path $nsis) {
    Step "NSIS silent install"
    $p = Start-Process $nsis -ArgumentList "/S" -Wait -PassThru
    if ($p.ExitCode -ne 0) { throw "NSIS install exit $($p.ExitCode)" }
    $un = Get-ChildItem "${env:ProgramFiles}\$app\uninstall.exe", "${env:LOCALAPPDATA}\Programs\$app\uninstall.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $un) { throw "uninstall.exe not found after NSIS install" }
    Step "NSIS silent uninstall"
    $p = Start-Process $un.FullName -ArgumentList "/S" -Wait -PassThru
    if ($p.ExitCode -ne 0) { throw "NSIS uninstall exit $($p.ExitCode)" }
} else { Write-Host "NSIS setup not present — skipped" -ForegroundColor Yellow }

# ── 4. Branded setup ──────────────────────────────────────────────
$branded = Join-Path $Dist "$app-Setup-$Version-x64.exe"
if (Test-Path $branded) {
    Step "Branded setup silent install"
    $dir = Join-Path ([IO.Path]::GetTempPath()) "nfd-branded-$([guid]::NewGuid().ToString('N'))"
    $p = Start-Process $branded -ArgumentList "/S", "/D=$dir" -Wait -PassThru
    if ($p.ExitCode -ne 0) { throw "branded setup exit $($p.ExitCode)" }
    if (-not (Test-Path (Join-Path $dir "nfd.exe"))) { throw "nfd.exe not found in $dir" }
    $reg = Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$app" -ErrorAction SilentlyContinue
    if (-not $reg) { throw "uninstall registry key missing" }
    Step "Branded setup silent uninstall"
    $p = Start-Process (Join-Path $dir "$app-Setup.exe") -ArgumentList "--uninstall", "/S" -Wait -PassThru
    if ($p.ExitCode -ne 0) { throw "branded uninstall exit $($p.ExitCode)" }
} else { Write-Host "Branded setup not present — skipped" -ForegroundColor Yellow }

Write-Host "`nSmoke test passed." -ForegroundColor Green
