# Monitor overhead (docs/PHASE6_MONITOR.md DoD: < 2 % CPU): runs `nfd monitor` for
# a fixed time under a light, steady write workload (a few MB/s — not a stress test)
# and reports the monitor process's CPU time as a share of all logical processors,
# the way Task Manager counts it.
#
#   pwsh scripts/monitor-overhead.ps1 [-Nfd target\release\nfd.exe] [-Source counters|etw] [-Seconds 60]
param(
    [string]$Nfd = "target\release\nfd.exe",
    [ValidateSet("counters", "etw")][string]$Source = "counters",
    [int]$Seconds = 60
)
$ErrorActionPreference = "Stop"
if (-not (Test-Path $Nfd)) { throw "$Nfd not found (cargo build --release -p nfd-cli)" }
$dir = Join-Path ([IO.Path]::GetTempPath()) ("nfd-overhead-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory $dir | Out-Null
$out = Join-Path $dir "monitor.jsonl"
$p = Start-Process -FilePath $Nfd -ArgumentList @("monitor", "--source", $Source, "--seconds", $Seconds, "--json") `
    -RedirectStandardOutput $out -NoNewWindow -PassThru
$sw = [Diagnostics.Stopwatch]::StartNew()
# light workload: 4 MB every 500 ms into a rotating set of files (~8 MB/s)
$buf = New-Object byte[] (4MB)
(New-Object Random 1).NextBytes($buf)
$i = 0
while (-not $p.HasExited -and $sw.Elapsed.TotalSeconds -lt ($Seconds - 1)) {
    [IO.File]::WriteAllBytes((Join-Path $dir ("w{0}.bin" -f ($i % 8))), $buf)
    $i++
    Start-Sleep -Milliseconds 500
}
$p.WaitForExit()
$wall = $sw.Elapsed.TotalSeconds
$cpu = $p.TotalProcessorTime.TotalSeconds
$cores = [Environment]::ProcessorCount
$share = 100 * $cpu / ($wall * $cores)
$oneCore = 100 * $cpu / $wall
$lines = @(Get-Content $out | Where-Object { $_ })
$last = $lines[-1] | ConvertFrom-Json
"source $($last.source), $($lines.Count) samples in $([math]::Round($wall, 1)) s, $([math]::Round($i * 4)) MB written"
"monitor CPU time {0:N2} s -> {1:N3} % of {2} logical processors ({3:N2} % of one)" -f $cpu, $share, $cores, $oneCore
Remove-Item -Recurse -Force $dir
if ($share -ge 2) { throw "overhead {0:N2} % >= 2 %" -f $share }
