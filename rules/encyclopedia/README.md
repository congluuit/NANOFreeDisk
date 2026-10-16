# Folder Encyclopedia

One Markdown file per well-known folder: what it is, what happens if you delete it and
what to do instead, in English and Vietnamese. NANOFreeDisk shows the matching article
in the Details panel ("What is this?") and never adds a delete action because of it.

Format (validated by `nfd rules validate rules` and the `nfd-rules` tests):

```text
+++
id = "file-name-without-.md"
risk = "safe" | "caution" | "advanced" | "protected"
paths = ["${SYSTEMROOT}/WinSxS"]        # exact folders, rule-pack variables
names = ["node_modules"]               # or a folder/file name anywhere …
siblings = ["Cargo.toml"]              # … that sits next to all of these
volume_paths = ["pagefile.sys"]        # or a path below any drive root
rules = ["ai.ollama.models"]           # optional: rule ids that clean it safely
+++

## en
### Title
Text: paragraphs, **bold**, `code`. Say what it is, "If you delete it:", "Instead:".

## vi
### Tiêu đề
Nội dung tiếng Việt.
```

New files must also be listed in `builtin_files!` in `crates/nfd-rules/src/encyclopedia.rs`
(a test fails otherwise).
