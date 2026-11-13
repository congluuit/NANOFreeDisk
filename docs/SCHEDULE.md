# Automatic cleaning, low disk space warning, start with Windows

Settings, section **Automatic cleaning**. Three features, all **off by default**:

1. **Low disk space warning**: a notification when a fixed drive is almost full.
2. **Clean automatically**: Off, Weekly or Monthly. A small, fixed set of Safe junk
   is moved to the quarantine on a schedule.
3. **Start NANOFreeDisk with Windows (in the tray)**: so the two above can work
   without the user opening the app.

Code: `apps/nanofreedisk/src-tauri/src/schedule.rs` (pure decisions, time passed
in, tests in `schedule/tests.rs`), `src-tauri/src/commands/schedule.rs` (the
background thread, the clean, notifications, autostart), `tray.rs` (the "Free up
space" item), `src/lib/schedule/` (settings section, mock, helpers),
`src/lib/schedule.test.ts`, `locales/{en,vi}/schedule.ftl`.

## Where it runs

Inside the app's own process, while the window is open or the app waits in the
tray. There is **no Windows scheduled task, no service, and nothing runs
elevated**. If the app is not running when a clean is due, that clean is missed
and runs **once** a few minutes (3) after the next start; it never runs more than
once per period, however long the app was closed.

One background thread wakes 3 minutes after start and then every 5 minutes. Each
time it reads the free space of the fixed drives (the same volumes API the Home
view uses, no scan) and checks whether a clean is due. Both are cheap.

## Low disk space warning

- A drive counts as low below **10 % of its size or 10 GB, whichever is smaller**
  (both adjustable: 1 to 50 %, 1 to 10 000 GB). On a 1 TB drive that is 10 GB, on a
  64 GB drive 6.4 GB.
- One notification when a drive **falls** below the line. It stays quiet while the
  drive stays low, and alerts again only after the drive recovered (above the line
  plus a fifth of it, at least 1 GiB) and fell again, and never more often than
  once a day per drive.
- The notification goes through the same notification plugin as the drive health
  alerts, and the in-app toast is the drive health alert's "almost full" toast
  (`health-alert` event). While this warning is on, the drive health watch leaves
  low space to it, so a full drive is never reported twice.
- While a drive is low, the tray menu shows **Free up space**. It opens the app on
  Quick clean when that view exists, else on Junk files (Clean).
- Works on Windows and macOS.

## Automatic cleaning (Windows)

### What it may clean

Decided by `crate::schedule::qualifies` from the **built-in rule packs** only,
hard-coded, never from the settings file. A rule takes part only if all hold:

- verdict **Safe**,
- `regenerates = true` (the program rebuilds it by itself: temporary files, caches),
- not `admin` (automatic cleans never ask for administrator rights),
- not a guidance rule with a `tool` (WinSxS, restore points and the like are never
  deleted anyway),
- not the Recycle Bin,
- none of its `requires_closed` programs is running right now; if the process list
  cannot be read, every rule that needs a program closed is left out,
- its category is ticked in the picker (all are, by default).

After planning, every item is checked once more: it must be a Safe rule match of
one of those rules going to the quarantine, or it is dropped. Then the usual
executor runs it through the SafetyGate (locked files, links, changed files and
protected locations are skipped as in a manual clean).

On the current packs that means, for example, the user's `%TEMP%` (files older
than a day), browser and GPU shader caches, package manager caches. Not
`%SystemRoot%\Temp` (admin), not the thumbnail cache (Explorer is always running).

### What it never cleans

- anything Caution, Advanced or Protected, whatever a settings file says,
- anything that needs administrator rights,
- the Recycle Bin,
- duplicates, Tidy up items, anything picked in Analyze or Old & large,
- items of rules whose program is open,
- anything outside the built-in rule packs (a test `rules_dir` is never used).

The settings file can only **leave categories out**. A hand-edited or hostile
`schedule.json` that lists rule ids, levels, "expert", "admin" or "permanent" is
ignored field by field; a config that does not parse means everything is off
(`a_hostile_settings_file_cannot_widen_the_selection`).

### How it runs

The same path as a manual Clean of the default Safe picks: rule match, Planner,
SafetyGate, Executor, journal and audit log. Destination is always the
**quarantine** (never permanent), the operation is journaled with kind
**`clean-scheduled`** and shows up in Cleanup history as *Automatic clean*, where
it can be restored for 7 days like any clean. Afterwards a notification says
"Cleaned X automatically. See Cleanup history." Nothing is announced when there was
nothing to clean.

- **Weekly** = at least 7 days between runs, **Monthly** = at least 30 days. The
  first automatic clean is one period after it was turned on.
- **Only when the PC has been idle** (on by default): no keyboard or mouse input
  for 5 minutes (`GetLastInputInfo`). A due clean waits for that.
- A clock that moves backwards neither starts a run early nor blocks the next one:
  times in the future are pulled back to "now".
- A run that finds nothing, or whose rules do not load, still counts as the
  period's run (no retry loop).

### Run now

**Run now** first shows exactly what would be cleaned (per rule, with the total).
**Clean these now** then cleans that preview, not a new scan; a preview older than
15 minutes must be shown again. It counts as the period's automatic clean.

## Start with Windows (in the tray)

Own implementation, no plugin: this user's `Run` key gets one value,

```
HKCU\Software\Microsoft\Windows\CurrentVersion\Run
  NANOFreeDisk = "C:\...\nanofreedisk.exe" --tray
```

written with `nfd_platform::registry_write`, removed with
`nfd_platform::registry::delete_value`. Only that value is ever touched. The
Startup view lists it as *NANOFreeDisk*. If it was turned off in Task Manager
(its `StartupApproved\Run` flag), the section says so; turning it on here sets the
flag back to "on" the way Task Manager does. When the exe moved (portable copy,
update), the value is pointed at the new path on the next start.

Why not `tauri-plugin-autostart`: it would be one more dependency whose minor
version must follow the pinned Tauri 2.11 and MSRV 1.85, for what is one registry
value (Windows) or one LaunchAgent file (macOS). The app already writes `HKCU`
values for the Explorer menu and the Startup view.

On macOS the same option writes `~/Library/LaunchAgents/vn.io.nanodev.nanofreedisk.plist`
(`RunAtLoad`, arguments `<exe> --tray`).

With `--tray` the window starts hidden. Closing the window then hides it again
(the app keeps running in the tray); **Quit** in the tray menu ends it. Started
normally, closing the window quits the app as before. Opening the app again (Start
menu, shortcut) while it waits in the tray brings the window up.

## Where the state is kept

`schedule.json` in the local data folder (`%LOCALAPPDATA%\vn.io.nanodev.nanofreedisk`,
portable: `data` next to the exe), next to `health-state.json`:

- `config`: frequency, idle only, left-out categories, low space on, threshold,
- `status`: when automatic cleaning was turned on, the last run (time, bytes,
  items, journal id), the low space state per drive,
- `texts`: notification and tray texts in the UI language.

Each part is read on its own, so a broken `config` does not lose the history.

## How to turn it off

- Settings, Automatic cleaning: set **Clean automatically** to Off, untick the low
  disk space warning, untick **Start NANOFreeDisk with Windows**.
- Or quit the app from the tray: nothing runs while it is closed.
- Or turn the *NANOFreeDisk* entry off in Task Manager's Startup apps (or in the
  app's Startup view).
- The `Run` value is removed when the option is turned off. The uninstallers do
  not remove it yet: turn the option off before uninstalling; a leftover value that
  points to a deleted exe does nothing.

## Prior art

- Windows **Storage Sense** (Settings, System, Storage): runs on a schedule (every
  day, week, month or "during low free disk space"), cleans temporary files, the
  Recycle Bin and Downloads after N days. This app deliberately does less: never
  the Recycle Bin, never Downloads, only rules that regenerate, and everything goes
  to a 7-day quarantine first.
- **BleachBit** has no built-in scheduler; it documents running its command line
  from Task Scheduler or cron. This app avoids scheduled tasks so nothing runs when
  the user has quit it and nothing runs with other rights.

## Not done yet

- "Only on AC power" (laptops), and a before / after report per run.
- A notification action button: desktop notifications cannot carry one here, so
  "Free up space" lives in the tray menu.
