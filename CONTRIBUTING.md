# Contributing to NANOFreeDisk

Thanks for helping make disk cleanup safe and understandable. This document covers the workflow, the coding rules and — most importantly — how to add or fix a **cache rule**.

## Ground rules

1. **Safety beats features.** A change that could make the app delete something it should not is rejected, no matter how useful.
2. Anything touching `crates/nfd-core/src/safety*` uses the `safety:` commit type and **must** include tests.
3. No telemetry, no network calls beyond the update check and documentation links.
4. Be kind — see [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## Development setup

```powershell
rustup toolchain install stable
cargo install cargo-nextest cargo-deny
git clone https://github.com/congluuit/NANOFreeDisk.git
cd NANOFreeDisk
cargo build --workspace
cargo nextest run --workspace
```

Before opening a PR, CI expects all of these to pass:

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo deny check
cargo run -p nfd-cli -- rules validate .\rules
```

## Adding or fixing a cache rule

Rules live in `rules/*.toml`, one pack per family (`ai-tools`, `ide-editors`, `package-managers`, `game-engines`, `windows-system`, …). A rule looks like this:

```toml
[[rule]]
id              = "vendor.app.what"            # unique, lowercase, dot-separated
vendor          = "Vendor"
app             = "App name"
category        = "ai-agent"                   # ai-agent | ai-app | ai-ide | ide | llm-runtime | package-manager | build | game-engine | browser | system | gpu
platform        = ["windows", "macos", "linux"]
paths           = ["${HOME}/.app/cache/**"]    # globs; variables: ${HOME} ${USERPROFILE} ${APPDATA} ${LOCALAPPDATA} ${PROGRAMDATA} ${TEMP} ${SYSTEMROOT}
exclude         = []                           # optional globs that win over `paths`
level           = "safe"                       # safe | caution | advanced | protected
regenerates     = true                         # does the app recreate it automatically?
min_age_days    = 1                            # skip files newer than this (optional)
requires_closed = ["app.exe"]                  # processes that must not be running (optional)
impact          = "One or two sentences a non-expert understands: what is lost, what is NOT lost."
docs            = "https://…"                  # optional
verified_on     = 2026-10-16                   # the date you actually tested it
```

Checklist for a rule PR:

- [ ] `impact` says clearly what the user loses **and** what they keep.
- [ ] Config, credentials, tokens, user documents and history the user may want are a **separate `protected` rule**, not an `exclude` buried in a safe rule.
- [ ] `verified_on` is the date you tested it; mention the app version in the PR description.
- [ ] `cargo run -p nfd-cli -- rules validate .\rules` passes (unique ids, valid levels, no path under a protected root for non-protected rules).
- [ ] Added the folder to the matching table in `docs/DEVTOOL_CACHE_CATALOG.md`.

When unsure between 🟢 and 🟡, choose 🟡. When unsure between 🟡 and 🔴, choose 🔴.

## Code style

- Rust stable, edition 2024, `cargo fmt` defaults (see `rustfmt.toml`), clippy with `-D warnings`.
- `unsafe` only inside `nfd-platform`, each block preceded by a `// SAFETY:` comment.
- Public items get a doc comment; errors use `thiserror` in libraries and `anyhow` in binaries.
- Keep OS-specific code behind the `Platform` trait so macOS/Linux ports stay possible.

## Commits and PRs

- [Conventional Commits](https://www.conventionalcommits.org/): `feat:`, `fix:`, `safety:`, `rules:`, `docs:`, `ci:`, `refactor:`, `perf:`, `test:`, `chore:`.
- One topic per PR; link the issue; fill in the PR template.
- Squash-merge into `main`. `main` must always be releasable.

## Reporting a destructive bug

If you believe the app deleted or could delete something it should not, **do not open a public issue** — follow [SECURITY.md](SECURITY.md).
