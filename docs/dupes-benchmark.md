# Duplicates — benchmark (Phase 5)

DoD (`docs/PHASE5_DUPLICATES.md` §1): 100 GB of mixed photos/videos on an SSD — first run
< 5 min, re-run with the hash cache < 30 s.

**The 100 GB figure is extrapolated** from a smaller real sample, as agreed for this
phase: the reference machine is a laptop and the benchmark must stay short and light
(no long full-load runs). Numbers below are from that sample; the extrapolation is
linear in bytes read, which is what dominates a first run.

## Machine

- Laptop, Intel i9-13900HX, Windows 10 Pro 19045.
- Data on the system drive `C:`: NVMe SSD, Samsung 980 PRO 1 TB (PCIe 4.0).
- Windows Defender real-time protection **on** (every file opened is scanned, so the
  numbers include that cost).
- File cache not flushed between runs: part of the sample may have been served from
  RAM, so the throughput is an upper bound for this drive.
- Release build (`cargo bench -p nfd-dupes`, profile `bench` = `release`), 4 hashing
  threads in Windows background mode (low CPU and I/O priority).

## Sample

`%USERPROFILE%\Downloads` of the reference machine: 3,694 files, **17.7 GiB** — camera
photos and videos (`backup x100s`, 5.0 GiB), installers, archives, game files.

```
NFD_BENCH_DIR=%USERPROFILE%\Downloads cargo bench -p nfd-dupes
  scan: 3694 files, 17.69 GiB in 0.01 s
  cold: 19 candidates, 2 groups, 15.0 MiB wasted, read 26.2 MiB, 0 cache hits in 0.02 s
cached: 19 candidates, 2 groups, 15.0 MiB wasted, read 0.0 MiB, 19 cache hits in 0.00 s
  full hash of every file >= 1 MiB: 681 files, 17.42 GiB in 14.60 s = 1222 MiB/s -> 100 GB in 78 s
```

(`backup x100s` alone: 207 files, 4.98 GiB hashed in 1.57 s ≈ 3.2 GiB/s — mostly from the
file cache, so the slower whole-folder figure is the one used below.)

The real pipeline reads very little: only files that share their size with another one
are sampled (3 × 16 KiB), and only those whose samples also match are read whole. The
"full hash of every file" line is the **worst case**, where every file of at least
1 MiB has a same-size twin and is read completely.

## Extrapolation to 100 GB

| | Measured (17.7 GiB) | 100 GB, worst case | Goal |
|---|---|---|---|
| Scan (metadata) | 0.01 s for 3.7 k files | ~1–4 s for a media library of 20–50 k files (scanner: ~1 M entries in 3.6 s) | — |
| First run (every byte read once) | 14.6 s | ≈ 78 s at 1.2 GiB/s | < 5 min ✅ |
| Re-run with the hash cache | 0.00 s pipeline, 0 bytes read | scan + one SQLite lookup per candidate: a few seconds | < 30 s ✅ |

Even at a SATA SSD's ~500 MB/s the worst case stays at ~3.5 min. Spinning disks are read
one file at a time (`StorageDeviceSeekPenaltyProperty`) and are not covered by the goal.

`nfd dupes %USERPROFILE%\Downloads --min-size 1` with the history database as cache:
first run 0.1 s (40 MiB read), second run 0.0 s with all 1,630 candidates answered from
the cache.

## How to repeat

```
cargo bench -p nfd-dupes                                   # synthetic, ~100 MiB
NFD_BENCH_DIR=D:\Photos cargo bench -p nfd-dupes           # a real folder
nfd dupes D:\Photos --db %TEMP%\nfd-bench.db               # twice: cold, then cached
```
