<#
.SYNOPSIS
  Build the four release packages of one version into a folder
  (docs/PHASE8_INSTALLERS.md section 1). Used by release.yml and locally.

.DESCRIPTION
  dist\NANOFreeDisk-<v>-x64.msi            MSI (WiX via Tauri, English UI)
  dist\NANOFreeDisk-<v>-x64-vi-VN.msi      MSI with the Vietnamese UI
  dist\NANOFreeDisk-<v>-x64-setup.exe      NSIS (per user)
  dist\NANOFreeDisk-Setup-<v>-x64.exe      branded setup (egui)
  dist\NANOFreeDisk-<v>-x64-portable.zip   portable zip
  plus the updater signatures (*.sig) when TAURI_SIGNING_PRIVATE_KEY is set: NSIS
  and MSI from Tauri, the branded setup and the portable zip signed here (their
  copies update from them, see apps/nanofreedisk/src-tauri/src/commands/update.rs).

.PARAMETER Out        Output folder (created; existing package files are replaced).
.PARAMETER Version    Defaults to workspace.package.version in Cargo.toml.
.PARAMETER SkipBuild  Only assemble from what target\release already holds.
#>
[CmdletBinding()]
param(
    [string] $Out = "dist",
    [string] $Version,
    [switch] $SkipBuild
)
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$app = "NANOFreeDisk"
if (-not $Version) {
    $Version = (Select-String -Path "$root\Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
}
$rel = "$root\target\release"
New-Item -ItemType Directory -Force $Out | Out-Null
$Out = (Resolve-Path $Out).Path

function Native([string] $exe, [string[]] $argList, [string] $dir = $root) {
    Push-Location $dir
    try {
        $ErrorActionPreference = "Continue"
        & $exe @argList
        if ($LASTEXITCODE -ne 0) { throw "$exe $($argList -join ' ') failed ($LASTEXITCODE)" }
    } finally { Pop-Location }
}

if (-not $SkipBuild) {
    Native cargo @("build", "--release", "-p", "nfd-cli", "-p", "nanofreedisk-setup")
    $configs = @("--config", "src-tauri/tauri.bundle.conf.json")
    if ($env:TAURI_SIGNING_PRIVATE_KEY) {
        # updater artifacts (.sig) only when the signing key is available
        $configs += @("--config", "src-tauri/tauri.updater.conf.json")
    }
    Native pnpm (@("tauri", "build", "--bundles", "msi,nsis") + $configs) "$root\apps\nanofreedisk"
}

# MSI (one per WiX language) and NSIS
$bundle = "$rel\bundle"
Copy-Item "$bundle\msi\${app}_${Version}_x64_en-US.msi" "$Out\$app-$Version-x64.msi" -Force
Copy-Item "$bundle\msi\${app}_${Version}_x64_vi-VN.msi" "$Out\$app-$Version-x64-vi-VN.msi" -Force
Copy-Item "$bundle\nsis\${app}_${Version}_x64-setup.exe" "$Out\$app-$Version-x64-setup.exe" -Force
foreach ($sig in @(
        @("$bundle\nsis\${app}_${Version}_x64-setup.exe.sig", "$Out\$app-$Version-x64-setup.exe.sig"),
        @("$bundle\msi\${app}_${Version}_x64_en-US.msi.sig", "$Out\$app-$Version-x64.msi.sig"))) {
    if (Test-Path $sig[0]) { Copy-Item $sig[0] $sig[1] -Force }
}

# files shared by the portable zip and the branded setup
$stage = Join-Path ([IO.Path]::GetTempPath()) "nfd-stage-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Force $stage | Out-Null
try {
    Copy-Item "$rel\nanofreedisk.exe", "$rel\nfd.exe", "$root\LICENSE", "$root\README.md" $stage

    # branded setup: stub + payload (the stub itself becomes uninstall.exe)
    Native "$rel\nanofreedisk-setup-pack.exe" @("$rel\nanofreedisk-setup.exe", $stage, "$Out\$app-Setup-$Version-x64.exe")

    # portable: data next to the exe (portable.flag), rule packs for `nfd rules validate`
    Copy-Item -Recurse "$root\rules" "$stage\rules"
    "NANOFreeDisk portable: settings, history and quarantine live in .\data next to the exe." |
        Out-File -Encoding ascii "$stage\portable.flag"
    $zip = "$Out\$app-$Version-x64-portable.zip"
    if (Test-Path $zip) { Remove-Item $zip }
    Compress-Archive -Path "$stage\*" -DestinationPath $zip
} finally {
    Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
}
if ($env:TAURI_SIGNING_PRIVATE_KEY) {
    foreach ($f in @("$Out\$app-Setup-$Version-x64.exe", $zip)) {
        Remove-Item "$f.sig" -ErrorAction SilentlyContinue
        Native pnpm @("tauri", "signer", "sign", $f) "$root\apps\nanofreedisk"
    }
}

Get-ChildItem $Out | Format-Table Name, Length -AutoSize
