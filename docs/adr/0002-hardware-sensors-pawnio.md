# ADR 0002 — Hardware sensors: driver-free by default, PawnIO opt-in

- Status: accepted (2026-10-16)
- Context: the Hardware Monitor view (`docs/HARDWARE_MONITOR.md`) must show CPU package/core temperatures, power, VID and real clocks like LibreHardwareMonitor. On Windows these come from model-specific registers and Super I/O ports, which user-mode code cannot read. `docs/PLAN.md` §13.3 ruled out shipping our own kernel driver.

## Options

1. **No driver at all** — GPU (D3DKMT/NVML), storage (SMART/NVMe), RAM, load, estimated clocks, ACPI thermal zones. No CPU core temperature or power on most machines.
2. **WinRing0** — what older tools used. Rejected: known exploitable driver, flagged by Microsoft Defender and blocked by the vulnerable-driver blocklist.
3. **PawnIO** — signed, scriptable driver that only runs signed modules and exposes them through IOCTLs; LibreHardwareMonitor, Fan Control and OpenRGB moved to it to replace WinRing0.
4. **Our own signed driver** — rejected: signing cost, stability risk, and it would duplicate PawnIO.

## Decision

Option 1 by default, **plus PawnIO as an opt-in** ("Advanced sensors"), used only from the elevated helper and only for reads. We install it with its official installer after checking the Authenticode signature; we never modify, rebuild or silently install it, and uninstalling NANOFreeDisk leaves it in place because other apps share it.

## Consequences

- The app works fully without admin or PawnIO; CPU/mainboard sensors show "needs Advanced sensors" with the reason.
- PawnIO is GPL-2.0 with an exception for independent programs that talk to it only through its IOCTL interface, which is how we use it; only official modules are loaded. Redistribution terms must be confirmed with the author and listed in `THIRD-PARTY-NOTICES` before the first release that bundles the installer.
- Must be verified on real machines before shipping: Defender stays quiet, behaviour with Memory Integrity (HVCI) on, installer size and silent-install flags.
