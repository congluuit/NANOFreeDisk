# ADR 0003 — Starting the elevated helper without a UAC prompt (opt-in)

- Status: accepted (2026-10-21)
- Context: ADR 0002, `docs/HARDWARE_MONITOR.md` §4 / §6, `crates/nfd-helper`

## Context

Advanced sensors (PawnIO), the Monitor's file-level detail (kernel ETW) and the full
drive-health read (SMART thresholds, USB bridges, the volume dirty bit) need
administrator rights. The app never runs elevated: it starts its own executable as
`nanofreedisk.exe --elevated-helper <pipe> <app pid>` through UAC (`ShellExecuteEx`
"runas"), and that helper answers a closed set of pipe requests until the app exits.
With "Turn on automatically when the app opens" (`advancedAuto`) the user sees one
UAC prompt every time the app starts.

The usual way to drop that prompt is a scheduled task with "run with highest
privileges" that the user may start. As-is that would be a silent elevation path:
NANOFreeDisk installs per user into `%LOCALAPPDATA%\Programs\NANOFreeDisk`, which the
user — and so any program running as the user — can write. Whoever swaps that exe
gets their code run as administrator by the task, with no prompt.

## Threat model

**Attacker:** code running as the same account at medium integrity (malware, a
compromised app or script). Out of scope: code that is already administrator, other
administrators, physical access.

**Must never happen:** that code getting code execution at high integrity without a
UAC prompt. The ways a task-started elevated process can be hijacked, and what closes
each one here:

| Path | Closed by |
|---|---|
| Replace the binary the task runs | the task runs a copy in an administrator-only folder under Program Files |
| Change the task's action or arguments | the task's security descriptor lets the account read and run it, never change or delete it |
| DLL planting / search order | the copy's folder holds only the copy; every static import of `nanofreedisk.exe` is a Windows DLL (checked with `dumpbin /dependents`, no delay-loads), found in System32 before `PATH`; the helper calls `SetDefaultDllDirectories(System32)` first |
| Environment variables in the action (the `SilentCleanup` / `%windir%` bypass) | the action is an absolute, quoted path with no `%`, no `$(ArgN)` placeholder; registration refuses a path containing either |
| Argument injection | the task's only argument is the fixed `--elevated-helper-task`; that mode requires exactly `[exe, flag]` and helper modes are checked before every other command-line mode |
| Make the helper load attacker bytes | PawnIO modules are loaded only when their SHA-256 is one of the pinned official builds (ADR 0002), hashed on the buffer that is loaded |
| Make the helper copy attacker bytes | the copy is made only from the helper's own image: at start the helper opens the file its image is mapped from — found by the image's current name, which Windows updates on rename, and checked again once the handle is open — with read-only sharing, so it can no longer be renamed or rewritten |
| Make the helper open attacker paths (pipes, UNC) | requests carry drive indices `< 64`, drive letters `X:\` and a folder string used only as an event filter — nothing the helper opens |

**What the helper exposes — the invariant that must stay true.** The helper only
reads, and the caller never chooses *what* is read beyond these:

- **MSRs** through PawnIO's official, hash-pinned `IntelMSR` / `AMDFamily17` modules:
  the helper picks the sensor registers, the module's allow-list refuses others, and
  `ioctl_write_msr` is never called. Hardware is read at most every 200 ms; faster
  requests get the previous values, so a caller cannot turn the RAPL energy counters
  into a high-rate power side channel (PLATYPUS, CVE-2020-8694).
- **Kernel ETW file / disk activity**: process ids and names, file names, byte counts —
  never file contents.
- **SMART and the volume dirty bit** with the fixed read commands of
  `nfd_health::cmd`.
- **Two parameterless requests added by this ADR**, `TaskRegister` / `TaskRemove`,
  which can only install or remove the helper's own task and copy for the account it
  runs as (see Decision).

There is no request that runs code, writes hardware, reads file contents, ends a
process, or names a file, register or command to use. Every new request must keep
this list true; a request that needs more is a reason to drop the no-prompt start,
not to widen it.

**What the no-prompt start changes.** Today only the app that started the helper can
talk to it, and each start needs consent. With the task, any program running as the
user can have a helper started and talk to it — directly, or through a genuine
`nanofreedisk.exe` it starts and injects into (same-user code can always do that).
The peer checks on the pipe (same account, same executable hash) keep stale builds
and casual impersonation out; they are consistency checks, not the boundary. The
boundary is the read-only protocol above, and the opt-in accepts that such code can
read, without a prompt: sensor values, SMART data, volume dirty bits, and file names
other processes touch — including other accounts' processes on a shared PC. The
option's hint says this in the UI.

## Options

1. **Keep UAC only** (status quo, stays the default). No new surface. One prompt per
   app start with `advancedAuto`, which also trains the user to approve, every day, a
   prompt for a file in a user-writable folder.
2. **Task pointing at the per-user install.** Rejected: swapping the exe is a silent
   elevation path.
3. **Task pointing at an administrator-only copy of the helper in Program Files**,
   started on demand, fixed arguments, run-only for the account; the helper accepts
   only a pipe server running as the same account whose executable hashes like its
   own; the app accepts only a client whose image is the copy. **Chosen.**
4. **A Windows service** (LocalSystem or a virtual account) with a pipe. Needs the
   same administrator-only binary, but runs as SYSTEM (more than the helper needs),
   stays registered in the Service Control Manager, must authorize clients from
   every session itself, and needs a service control handler. A bug would be a
   SYSTEM bug. No advantage over 3.
5. **Install the whole app per machine** (Program Files) and point the task at it.
   Would remove the copy, but changes the installer model (NSIS and the branded setup
   default to per user, PHASE8). The branded setup's "Everyone" mode already installs
   there; option 3 works the same for both by always using its own copy.
6. **An auto-elevating COM object.** Only Windows' own signed binaries can
   auto-elevate (`COMAutoApprovalList`). Not available.
7. **Open PawnIO's device to non-administrators** (an option of PawnIO 2.2, "not
   recommended" by its author). Rejected: it would hand the driver — module loading
   included — to every program; the helper exists to keep PawnIO admin-only.

## Decision

Option 3, **opt-in, off by default**: "Start without the Windows prompt" next to
"Turn on automatically when the app opens" in Hardware → Sensors.

- **Copy:** `<Program Files>\NANOFreeDisk Helper\<account SID>\nanofreedisk.exe`.
  Program Files comes from `FOLDERID_ProgramFiles`, never from an environment
  variable. The elevated helper creates both folders with an explicit, protected ACL
  (owner Administrators; SYSTEM and Administrators full control; the account — and,
  on the parent only, Users — read & execute; nothing inherited) and refuses a
  reparse point. One copy per account, because per-user installs can be different
  versions.
- **Bytes:** the helper copies its own image from the handle it locked at start
  (proven to be the running image, see the table above) and checks the copy's
  SHA-256 before registering the task.
- **Task** `\NANOFreeDisk Helper-<SID>` (root folder, OneDrive-style name): no
  trigger, started on demand only; principal = the account, `InteractiveToken`,
  `HighestAvailable`; action `"<copy>" --elevated-helper-task`, working directory the
  copy's folder; parallel instances, no time limit; security descriptor
  `D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FRFX;;;<SID>)`. Registered with
  `<System32>\schtasks.exe /create /xml` (System32 from `GetSystemDirectory`), the XML
  written inside the protected folder and deleted afterwards.
- **Start:** the app checks that the copy hashes like its own executable (else the
  copy is *stale*: next point), creates `\\.\pipe\nanofreedisk-helper-task-<SID>`
  (first instance only, ACL owner / Administrators / SYSTEM), runs
  `schtasks /run`, waits up to 10 s and accepts only a client whose image path is the
  copy. The task-mode helper runs only from the copy's path, opens that pipe, and
  accepts only a server running as its own account whose image hashes like its own.
  Anything unexpected falls back to the UAC path.
- **Protocol 4:** adds `TaskRegister` and `TaskRemove` (no parameters). Register is
  a no-op re-registration when the helper is itself the copy.
- **Lifecycle:**
  - *On:* needs the helper (at most the one prompt that starts it); the helper
    registers.
  - *App updated:* the copy no longer matches, so the next start goes through UAC
    and the new helper renews the copy and the task — one prompt per update.
  - *Off:* the helper (started through the task while it is current, so no prompt)
    deletes the task, then the copy; a copy that is running is renamed and deleted at
    the next restart.
  - *Uninstall:* `--uninstall-cleanup` removes the account's task and copy — through
    the task without a prompt when current, through UAC after a message when stale
    (never in a silent uninstall); an elevated cleanup (the branded setup's
    per-machine uninstall) removes every account's.
- **Tests:** under `NFD_NO_AUTO_ELEVATE` (gui-smoke) the app never uses, registers or
  removes the task; `helper_task_set` is refused.

## Consequences

- Users who opt in get no prompt at start; nothing changes for anyone else.
- Residual risks, accepted:
  - Same-account code can read sensors, SMART and system-wide file activity without
    a prompt (privacy of other accounts' file names on a shared PC). Stated in the
    option's hint.
  - The copy is exactly the image of the helper the user approved in the UAC prompt
    that started it. That prompt itself trusts a file in a user-writable folder,
    which same-account code could have swapped *before* the prompt — true of every
    UAC prompt for a per-user install, with or without this option. What the design
    prevents is later changes to the user folder reaching the elevated task.
  - Leftovers when the app folder is deleted by hand or a silent uninstall finds a
    stale copy: the task can only run the administrator-only copy, for that account,
    with the read-only protocol. Remove it in Task Scheduler and delete
    `Program Files\NANOFreeDisk Helper`.
  - Assumes Program Files keeps Windows' default permissions (if it does not, every
    program installed there is already replaceable).
  - On a PC where UAC is set to "elevate without prompting" for administrators the
    option changes nothing visible; it still keeps the elevated binary out of the
    user folder.
