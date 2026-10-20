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

## Verified facts (2026-10-16, on the reference i9-13900HX, Windows 10 22H2)

- **Installer:** `PawnIO_setup.exe` 2.2.0 from `github.com/namazso/PawnIO.Setup/releases` (linked from pawnio.eu), 3,410,960 bytes, SHA-256 `1f519a22e47187f70a1379a48ca604981c4fcf694f4e65b734aaa74a9fba3032`. Authenticode valid: signer `CN=namazso.eu, O=namazso` (GLOBALTRUST 2015 CodeSigning 1), timestamped by Microsoft.
- **Silent install:** `PawnIO_setup.exe -install -silent` (self-elevates; exit 0 here; 2.2.0 returns `ERROR_SUCCESS_REBOOT_REQUIRED` when a restart is needed). Uninstall: `"C:\Program Files\PawnIO\uninstall.exe" -uninstall`. The flags are not documented by the author; they come from package managers (winget id `namazso.PawnIO`) and were tested here.
- **What it installs:** kernel service `PawnIO` (demand start, running after install) from `DriverStore\...\PawnIO.sys` (136,504 bytes), signed by *Microsoft Windows Hardware Compatibility Publisher* (attestation-signed — the Official edition; the Unrestricted edition is unsigned). `C:\Program Files\PawnIO\` holds PawnIOLib.dll / .h, PawnIOUtil.exe and the uninstaller.
- **Defender:** no detection during or after install.
- **Memory Integrity (HVCI):** off on this machine, so not verified yet — still to test on a PC with Memory Integrity on (the driver is Microsoft-signed, which HVCI requires; the vulnerable-driver blocklist targets WinRing0, not PawnIO).
- **Access:** the device is admin-only by default (2.2.0 can optionally expose it to non-admins; "not recommended"), hence the elevated helper.
- **Interface we use:** device `\Device\PawnIO`, IOCTLs `CTL_CODE(41394, 0x821 / 0x841, METHOD_BUFFERED, FILE_ANY_ACCESS)` for load / execute — taken from `pawnio_um.h`. NANOFreeDisk talks to the driver only through these IOCTLs (the GPL-2.0 exception) and does not load PawnIOLib.dll (LGPL-2.1).
- **Modules:** official signed builds in `github.com/namazso/PawnIO.Modules/releases` (0.2.11, LGPL-2.1). We pin the SHA-256 of `IntelMSR.bin` (`d6ed85d6…92e5f`) and call only `ioctl_read_msr`; the module itself also exposes `ioctl_write_msr` (power limit, VR mailbox), which NANOFreeDisk never calls.
- **Redistribution:** the GPL / LGPL terms allow it, with license text and a source link. Bundling the installer or the modules still waits for the author's confirmation (admin@namazso.eu) and an entry in `THIRD-PARTY-NOTICES`; until then the app links to pawnio.eu and looks for modules in `<app>\pawnio\` or `%LOCALAPPDATA%\NANOFreeDisk\pawnio\`.
- **One-click setup (2026-10-21):** at the user's request the app downloads the two official release assets itself — `PawnIO.Setup` 2.2.0 `PawnIO_setup.exe` and `PawnIO.Modules` 0.2.11 `release_0_2_11.zip` (SHA-256 `43608cb8…81f4`, 69,582 bytes) — with Windows' own `curl.exe` over HTTPS only (redirects too), accepts each only when its SHA-256 matches the pinned value (`nfd_sensors::win::pawnio::{SETUP_DOWNLOAD, MODULES_DOWNLOAD}`), runs the installer through UAC with `-install -silent` (exit 3010 → asks for a restart), and stores the module (hash checked again) in `%LOCALAPPDATA%\NANOFreeDisk\pawnio\`. Nothing is redistributed by us; a new upstream release is adopted only by changing the pinned URL + hash after checking it.
- **Turn on automatically (2026-10-21):** setting `advancedAuto` (on after the first successful manual start, untickable next to "Advanced sensors on"). The first view that shows temperatures calls `advanced_sensors_start { auto: true }`; the backend tries once per app session, only when PawnIO runs and the module is present, never under `NFD_NO_AUTO_ELEVATE` (gui-smoke). It is still one UAC prompt per app start — removing it (a scheduled task that starts the helper) needs its own security design: the per-user install folder is user-writable, so a task pointing at it would be a silent elevation path.
- **PL4:** there is no architectural MSR for PL4 on Raptor Lake (HWiNFO reads it via MMIO); upstream LibreHardwareMonitor does not show PL1/PL2/PL4 either. We show PL1 / PL2 from `MSR_PKG_POWER_LIMIT`; PL4 is left out until a read-only source is confirmed.
