# Startup apps (PLAN §13.2 E2)

The Startup view lists the programs that start when the user signs in to Windows
and turns them on or off exactly the way Task Manager's *Startup apps* page does.
Windows only: the view is hidden on macOS (`platform.ts`, feature `startup`).

Code: `crates/nfd-apps/src/startup.rs` (listing, flags, command and shortcut
parsing; tests in `startup/tests.rs`), `crates/nfd-platform/src/registry_write.rs`
(the one registry write), `apps/nanofreedisk/src-tauri/src/commands/startup.rs`,
`apps/nanofreedisk/src/lib/startup/` and `views/Startup.svelte`.

## Sources

| Origin (`id` prefix) | Where | Flag key (`…\Explorer\StartupApproved\…`) |
|---|---|---|
| `user-run` | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` | `HKCU …\Run` |
| `machine-run` | `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run` | `HKLM …\Run` |
| `machine-run32` | `HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run` | `HKLM …\Run32` |
| `user-folder` | `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup` | `HKCU …\StartupFolder` |
| `common-folder` | `%ProgramData%\Microsoft\Windows\Start Menu\Programs\Startup` | `HKLM …\StartupFolder` |

The flag value has the same name as the `Run` value, or the file name for the
Startup folders (`Tool.lnk`). An entry id is `<origin>|<that name>`.

For each entry the list shows the name, the command, the program it resolves to,
the publisher (the signer, else the file's `CompanyName` marked `*`, read only from
local drives), the source, the scope and the state.

**Resolving the program.** The command goes through the uninstaller's parser
(`nfd_apps::command::parse`: quotes, unquoted paths with spaces, `%VAR%`, bare
Windows tools), then `rundll32 x.dll,Entry` resolves to the DLL. A `.lnk` is read
as data (MS-SHLLINK: LinkInfo local base path, the environment-variable target
block, relative path and arguments); advertised Windows Installer shortcuts carry
no path and keep only their name. Nothing is ever run.

## The on / off flag

Task Manager keeps one `REG_BINARY` value per entry, 12 bytes:

- byte 0 even (`02`, `06`): allowed to run; odd (`03`, `07`): turned off,
- bytes 4 to 11: FILETIME of when it was turned off (zero when on).

No value means on. Turning an entry off writes `03 00 00 00` plus the current
FILETIME; turning it on writes `02` and zeros. The `Run` value and the shortcut
are never touched, so the change is fully reversible, here (Undo, or Turn on) or
in Task Manager, which reads the same flag. Every change is noted in the audit
log (`startup  disable  <id>`).

## Safety

- Nothing is deleted in this version; there is no Delete action.
- This user's entries (HKCU, the user Startup folder) change without elevation.
- All-users entries need administrator rights. They are listed with their state,
  marked "needs administrator rights", and offer a button that opens Task Manager
  on its Startup page (`Taskmgr.exe /0 /startup`). The backend refuses to write
  them (`needs-admin`) whatever the UI sends, and looks every entry up again by id
  instead of trusting paths from the UI.
- Entries whose program lives under `%SystemRoot%` are tagged *Part of Windows*;
  Windows Security (`SecurityHealthSystray.exe`, `MSASCuiL.exe`, the Windows
  Defender folders) and common antivirus folders are tagged *Security software*.
  Turning either off asks first.
- After every change an inline Undo puts the previous state back.

## Out of scope (for now)

Scheduled tasks with a logon or boot trigger, services, `RunOnce` keys, Store
apps' `StartupTask`, Group Policy `Run` lists and startup impact measurements.
The view says so and points to Task Scheduler and Services.
