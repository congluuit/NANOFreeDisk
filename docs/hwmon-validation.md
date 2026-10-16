# Hardware sensors — validation against LibreHardwareMonitor

DoD (`docs/HARDWARE_MONITOR.md` §8): values within ±2 °C and ±5 % power / clocks of LibreHardwareMonitor on the reference machine.

## Setup (2026-10-16)

- **Machine:** System76 Adder WS, Intel Core i9-13900HX (8 P + 16 E cores), RTX 4050 Laptop, Samsung 980 PRO + PM981a NVMe, on AC, Windows 10 22H2.
- **Drivers / modules:** PawnIO 2.2.0 (Official) with IntelMSR 0.2.11.
- **Reference:** LibreHardwareMonitor v0.9.6, the official GitHub release `LibreHardwareMonitor.zip` (SHA-256 `086d9f1b…c001`, matching the release digest), elevated, web server on 127.0.0.1:8085.
- **App:** release build, Advanced sensors on through the elevated helper.
- **Method:** `scripts/hw-compare-lhm.mjs` reads LHM's `data.json` and the app's `sensors_snapshot` together once a second and compares the means over the run. Both tools average power over their own ~1 s window. The script also prints the mean of the per-pair differences, which shows how much a value moves between two reads taken a few milliseconds apart.
- **Load:** 2 × 16 busy threads in separate processes, one pinned to the P-core threads and one to the E-cores (Windows otherwise keeps background load on E-cores only). The load settled for 20 s, then 40 rounds were taken.

## Results under steady load (40 rounds): 83 / 87 within tolerance

| Sensor | LHM | NANOFreeDisk | Diff |
|---|---|---|---|
| CPU Package temperature | 75.4 °C | 76.7 °C | +1.4 °C ✓ |
| Core Max / Core Average | 75.1 / 69.4 °C | 76.5 / 69.1 °C | +1.5 / −0.3 °C ✓ |
| 24 per-core temperatures | — | — | all within ±2 °C ✓ |
| CPU Package power | 55.1 W | 54.7 W | −0.9 % ✓ |
| CPU Cores power | 43.7 W | 43.4 W | −0.8 % ✓ |
| 24 per-core VIDs | ~1.0–1.1 V | — | all within ±5 % ✓ |
| Bus speed (TSC-derived) | 100.80 MHz | 100.80 MHz | 0 ✓ |
| 20 of 24 per-core clocks | — | — | within ±5 % ✓ |
| 4 per-core clocks | — | — | 5.2–5.8 % ✗ (per-pair noise ~300 MHz) |
| CPU Total load | 66.9 % | 63.5 % | −3.4 points ✓ |
| NVIDIA temperature / power / clocks, NVMe temperatures, memory load | — | — | identical or within 0.1 ✓ |

## Idle (40 rounds)

- Package, Core Max and Core Average temperatures, GPU, disks and loads are within tolerance.
- Per-core instantaneous clocks and VIDs, and a few P-core temperatures, are **not comparable at idle**. Parked cores flip between 800 MHz and full turbo between two reads (per-pair differences of 1.5–2.6 GHz), and each tool's own MSR reads wake the core it reads. Package power showed −2 W (−8 %) with ±9 W per-pair noise, for the same reason.
- The meaningful idle clock is the APERF-based **Effective clock** (`cpu-clock-eff`). LHM does not report it; HWiNFO does.

## Changes the comparison led to

- **Bus clock** is now measured from the TSC and `MSR_PLATFORM_INFO`, as LHM does (100.8 MHz here), instead of an assumed 100 MHz.
- **Two read passes per sample:** thermal status first, then perf status and APERF. Reading everything in one visit returned the wake-up ratio and VID of half-idle P-cores, 8–19 % below LHM. With two passes the gap closed to the noise level.

## Still to validate

- At least one AMD machine (slice 5).
- A PC with Memory Integrity (HVCI) on.

## AMD and mainboard Super I/O (slice 5)

- **AMD Zen (family 17h–1Ah)** is implemented with the official AMDFamily17 module (SHA-256 pinned): Tctl / Tdie (k10temp offset table), per-CCD temperatures, package and per-core RAPL power, per-core VID (SVI2 VID from the hardware P-state), and effective clocks. Only `ioctl_read_msr` and `ioctl_read_smn` are called; the SMN read writes the host bridge's index register, the same access k10temp and LibreHardwareMonitor use. **Not yet verified on AMD hardware**: the decoding is unit-tested against the k10temp / LHM formulas, but the ±2 °C / ±5 % check against LibreHardwareMonitor still has to be run on an AMD PC with `scripts/hw-compare-lhm.mjs` (the LHM sensor ids are `/amdcpu/0/...`; the pairs in the script need adjusting).
- **Super I/O (fans, mainboard voltages, LpcIO)** is **not shipped**. Reading a chip means entering its configuration mode by writing magic bytes to ports 0x2E / 0x4E, and on laptops (the reference machine included) those ports often belong to the embedded controller, where a stray write can hang the EC or power the machine off. HARDWARE_MONITOR.md §3.2 asks for a table of chips verified on real boards, and none could be verified here, so the table is empty and nothing is probed. Next step: a desktop with a known Nuvoton / ITE chip, a chip-ID allow-list, and the `Access_ISABUS.HTP.Method` mutex.
