## What

<!-- One or two sentences. Link the issue: Closes #123 -->

## Type

- [ ] `feat` / `fix` / `perf` / `refactor` / `docs` / `ci` / `chore`
- [ ] `rules` — rule pack change (fill in the section below)
- [ ] `safety` — touches `nfd-core::safety` (tests are mandatory)

## Checklist

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo nextest run --workspace`
- [ ] `cargo run -p nfd-cli -- rules validate ./rules`
- [ ] CHANGELOG entry under **Unreleased** (if user-visible)

## For rule changes

| Field | Value |
|---|---|
| App & version tested | |
| Folder(s) | |
| Level | 🟢 / 🟡 / 🟠 / 🔴 |
| What is lost / what is kept | |
| How verified | |

## Screenshots / logs

<!-- optional -->
