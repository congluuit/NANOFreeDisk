# DevCache view (Phase 4, docs/PHASE4_DEVCACHE.md). Loaded together with app.ftl and
# clean.ftl; every key must exist in vi/devcache.ftl too.

dev-title = Dev caches
dev-intro = Caches of AI tools, IDEs, package managers and game engines by vendor, the toolchains you have installed, build output in your repositories and caches no rule knows yet. Cleaning moves items to the quarantine: restore them from Activity for 7 days.
dev-tab-cache = Caches
dev-tab-toolchains = Toolchains
dev-tab-repos = Repositories
dev-tab-stores = Package stores
dev-tab-unknown = Unknown folders
dev-scanning = Looking for developer caches… (read-only)
dev-nothing = No developer caches found.
dev-kept-count = { $count } kept on purpose
dev-kept = Kept:
dev-kept-hint = Config, sign-in, extensions or your own data: NANOFreeDisk never cleans this.
dev-safe-all-hint = If you click: every Safe developer cache of every vendor moves to the quarantine. The apps rebuild them; you can restore them from Activity for 7 days.
dev-safe-part = safe { $size }
dev-safe-vendor = Clean safely ({ $size })
dev-safe-vendor-hint = If you click: the Safe caches of { $vendor } move to the quarantine (restorable from Activity for 7 days).
dev-over = over { $size }
dev-over-hint = Bigger than the reminder threshold set in Settings → Dev caches.
dev-over-title = Developer caches are growing
dev-over-body = Over the threshold: { $list }. Open NANOFreeDisk → Dev caches to review them.
dev-over-toast = Developer caches over their threshold: { $list }
dev-last-write = last change { $date }
dev-running = { $apps } is running — close it first, or the files it keeps open are skipped.
dev-needs-closed = Close { $apps } first
dev-open-items = Show folders
dev-reveal = Show in Explorer

## Toolchains (C2)
dev-tools-note = Installed SDKs, runtimes and editors with the proper way to remove each. NANOFreeDisk never deletes a toolchain folder: copy the command or use the app named.
dev-tools-loading = Measuring toolchains… (read-only)
dev-tools-none = No toolchains found.
dev-tool-default = default
dev-tool-default-hint = The default / active version
dev-guide-windows-apps = Remove in Settings → Apps → Installed apps
dev-guide-unity-hub = Remove in Unity Hub → Installs
dev-guide-epic-launcher = Remove in Epic Games Launcher → Unreal Engine → Library
dev-guide-visual-studio-installer = Modify or remove in Visual Studio Installer
dev-guide-ide = Remove in the IDE or tool that downloaded it
dev-guide-compact-vhd = Shrink it: free space inside first, then wsl --shutdown and Optimize-VHD -Mode Full

## Repositories (C3, C9)
dev-repos-root = Folder with your projects
dev-repos-pick = Folder to search for repositories
dev-repos-choose = Choose…
dev-repos-scan = Find repositories
dev-repos-scan-hint = If you click: the folder is searched (read-only) for git repositories and projects and their build output is measured.
dev-repos-policy = Build output is found by project files (Cargo.toml → target, package.json → node_modules…). Generic names like build or dist are only offered when git ignores them, folders with tracked files never, and projects active in the last { $days } days are not pre-selected.
dev-repos-loading = Searching for repositories… (read-only)
dev-repos-summary = { $count } project(s) in { $folders } folders
dev-repos-no-git = git was not found: generic folders (build, dist, bin, obj) are not offered.
dev-repos-none = No repositories or projects found here.
dev-repo-no-git = no git
dev-repo-dirty = uncommitted changes
dev-repo-dirty-hint = git status shows changes that are not committed (build output is unaffected)
dev-repo-recent = active recently
dev-repo-recent-hint = Used in the last { $days } days: its build output is not pre-selected
dev-repo-stale = stale
dev-repo-commit = last commit { $date }:
dev-repo-activity = last activity { $date }
dev-gc = Guide only:
dev-gc-hint = git gc packs the repository; it cannot be undone, so NANOFreeDisk only shows the command.
dev-art-tracked = Not offered: git tracks files in it
dev-art-not-ignored = Not offered: git does not ignore it (or git is missing)
dev-art-impact-safe = Build output: the next build recreates it.
dev-art-impact-caution = Rebuilding takes long (re-import, shader compile or a large build), or an environment must be recreated.
dev-kind-rust = Rust
dev-kind-node = Node.js
dev-kind-python = Python
dev-kind-dotnet = .NET
dev-kind-cmake = CMake
dev-kind-gradle = Gradle
dev-kind-maven = Maven
dev-kind-flutter = Flutter / Dart
dev-kind-unity = Unity
dev-kind-unreal = Unreal Engine
dev-kind-godot = Godot
dev-kind-terraform = Terraform
dev-kind-ruby = Ruby
dev-repos-selected = { $count } folder(s) selected · { $size }
dev-repos-clean = Move to quarantine
dev-repos-clean-hint = If you click: you see the list first, then the selected build output moves to the quarantine (restorable from Activity for 7 days).
dev-repos-confirm-title = Move this build output to the quarantine?
dev-repos-confirm-body = { $count } folder(s), { $size }. The next build recreates them.

## Package stores (C4)
dev-stores-note = Each package manager's own cleanup command: it knows what is still in use, so no project breaks. Commands run only after you see the exact command line; commands that cannot be undone are guide-only.
dev-stores-loading = Looking for package managers…
dev-stores-none = No package manager with a cleanup command found.
dev-undo-redownload = Undo: the tool downloads what it needs again
dev-undo-none = Cannot be undone — guide only
dev-store-missing = The program is not on PATH; only the folder is measured.
dev-store-run = Run
dev-store-run-hint = If you click: a dialog shows the exact command to run; nothing runs until you confirm.
dev-store-guide = Guide only
dev-store-guide-hint = What this command deletes cannot be brought back, so NANOFreeDisk never runs it: copy it and run it yourself if you are sure.
dev-store-confirm-title = Run the { $tool } cleanup?
dev-store-confirm-body = The store holds { $size }. This exact command will run:
dev-store-confirm-note = It runs without a shell and without input, stops after 10 minutes, and is recorded in Activity (it cannot be restored: the tool downloads what it needs again).
dev-store-confirm-go = Run command
dev-store-running = Running { $id }…
dev-store-done = Done — freed { $size }
dev-store-failed = The command did not finish successfully.
dev-store-recorded = Recorded in Activity.
dev-store-output = Output

## Unknown folders
dev-unknown-note = Folders that look like caches but no rule describes yet. They are never cleaned automatically — check them yourself or propose a rule.
dev-unknown-loading = Looking for unknown caches… (read-only)
dev-unknown-none = No unknown cache folders of 50 MB or more.
dev-unknown-count = { $count } unknown folder(s)
dev-unverified = Unverified
dev-unknown-why = Looks like a cache: { $signals }
dev-unknown-dot = hidden tool folder
dev-propose = Propose a rule
dev-propose-hint = If you click: GitHub opens a pre-filled rule proposal with the folder written as a variable (no user name) and its size.

## Settings
settings-devcache = Dev caches
settings-dev-alerts = Remind me when a vendor's caches pass its threshold
hint-dev-alerts = If you turn this on: the Dev caches scan (and one check a minute after start) shows a notification for vendors over their threshold.
settings-dev-thresholds-hint = Reminder thresholds per vendor (0 = none).
settings-dev-threshold = Threshold for { $vendor }
hint-dev-threshold = If you change this: the vendor is flagged, and you are reminded, once its caches pass this size.
settings-dev-remove = Remove threshold
settings-dev-vendor = Vendor (e.g. Gradle)
settings-dev-add = Add
settings-dev-keep-recent = Keep build output of projects active in the last (days)
hint-dev-keep-recent = If you change this: build output of projects used this recently is listed but not pre-selected (0 = off).
settings-dev-stale = Flag projects without activity for (days)
hint-dev-stale = If you change this: repositories idle this long are marked stale.
