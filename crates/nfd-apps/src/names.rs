//! Program and folder names made comparable (`docs/UNINSTALLER.md` §2.1): the
//! leftovers and the software junk are tied to a program by the name of the folder
//! or key they sit in, so `Python 3.12.1 (64-bit)`, `python` and `Python312` must
//! meet, while `Common`, `Data` or `Microsoft` must never be enough on their own.

/// Words too common to tie a folder or key to one program.
pub const GENERIC: &[&str] = &[
    "app",
    "apps",
    "application",
    "applications",
    "bin",
    "cache",
    "caches",
    "cfg",
    "common",
    "common files",
    "config",
    "configuration",
    "crashdumps",
    "data",
    "default",
    "desktop",
    "documents",
    "downloads",
    "files",
    "help",
    "installer",
    "installers",
    "launcher",
    "lib",
    "local",
    "locallow",
    "log",
    "logs",
    "microsoft",
    "packages",
    "plugins",
    "profile",
    "profiles",
    "program",
    "programs",
    "roaming",
    "settings",
    "setup",
    "shared",
    "software",
    "temp",
    "tmp",
    "tool",
    "tools",
    "uninstall",
    "uninstaller",
    "update",
    "updater",
    "updates",
    "user",
    "user data",
    "users",
    "windows",
];

/// Exe names that say nothing about the program they belong to.
pub const GENERIC_EXES: &[&str] = &[
    "app",
    "crashpad_handler",
    "crashreporter",
    "elevate",
    "helper",
    "installer",
    "launcher",
    "maintenancetool",
    "notification_helper",
    "setup",
    "squirrel",
    "uninst",
    "uninstall",
    "uninstaller",
    "unins000",
    "unins001",
    "update",
    "updater",
];

/// Company-form words dropped from publisher names (`Acme Software, Inc.` → `acme`).
const COMPANY_WORDS: &[&str] = &[
    "inc",
    "incorporated",
    "ltd",
    "limited",
    "llc",
    "corp",
    "corporation",
    "co",
    "company",
    "gmbh",
    "ag",
    "sa",
    "sas",
    "bv",
    "nv",
    "ab",
    "as",
    "oy",
    "kk",
    "plc",
    "pty",
    "srl",
    "sro",
    "spa",
    "jsc",
    "software",
    "technologies",
    "technology",
    "the",
];

/// Architecture words dropped from names.
const ARCH_WORDS: &[&str] = &[
    "x64", "x86", "amd64", "arm64", "win64", "win32", "64bit", "32bit", "x86_64",
];

/// A dotted version (`3.12.1`, `v2.0`, `14.38.33130`, `1.2.3-beta`).
fn is_version(token: &str) -> bool {
    let t = token.strip_prefix('v').unwrap_or(token);
    let core: &str = t.split(['-', '+']).next().unwrap_or(t);
    core.contains('.')
        && core
            .split('.')
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// Lower case, versions / architecture / `™ ® ©` / brackets dropped, words
/// separated by single spaces: `Python 3.12.1 (64-bit)` → `python`,
/// `7-Zip 23.01 (x64)` → `7 zip`, `Notepad++` → `notepad`.
#[must_use]
pub fn normalize(name: &str) -> String {
    // whole-token passes first (versions keep their dots), then the separators
    let lower = name.to_lowercase().replace(['™', '®', '©'], "");
    let mut tokens: Vec<String> = Vec::new();
    for raw in
        lower.split(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | '[' | ']' | ',' | '_'))
    {
        let raw = raw.trim_matches(|c: char| !c.is_alphanumeric());
        if raw.is_empty() || is_version(raw) {
            continue;
        }
        for part in raw.split(|c: char| !c.is_alphanumeric()) {
            if !part.is_empty() {
                tokens.push(part.to_owned());
            }
        }
    }
    // `64 bit`, `32-bit` written as two tokens
    let mut out: Vec<String> = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        let t = tokens[i].as_str();
        let joined = tokens.get(i + 1).map(|n| format!("{t}{n}"));
        if joined
            .as_deref()
            .is_some_and(|j| ARCH_WORDS.contains(&j) && (t == "64" || t == "32"))
        {
            i += 2;
            continue;
        }
        if !ARCH_WORDS.contains(&t) {
            out.push(t.to_owned());
        }
        i += 1;
    }
    // a trailing major version (`Python 3`, `Firefox 120`) when words remain
    while out.len() > 1
        && out
            .last()
            .is_some_and(|t| t.chars().all(|c| c.is_ascii_digit()))
    {
        out.pop();
    }
    out.join(" ")
}

/// [`normalize`] without the spaces (`visual studio code` → `visualstudiocode`).
#[must_use]
pub fn compact(name: &str) -> String {
    normalize(name).replace(' ', "")
}

/// A publisher's name made comparable: [`normalize`] without company forms
/// (`Acme Software, Inc.` → `acme`).
#[must_use]
pub fn publisher_key(name: &str) -> String {
    normalize(name)
        .split(' ')
        .filter(|w| !COMPANY_WORDS.contains(w))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A normalized name specific enough to tie a folder or key to one program: at least
/// three characters without spaces, not a [`GENERIC`] word, not only digits.
#[must_use]
pub fn distinctive(norm: &str) -> bool {
    let c: String = norm.chars().filter(|c| !c.is_whitespace()).collect();
    c.chars().count() >= 3
        && !GENERIC.contains(&norm)
        && !GENERIC.contains(&c.as_str())
        && !c.chars().all(|ch| ch.is_ascii_digit())
}

/// The names a folder or key of `program` may carry, normalized and distinctive:
/// the display name (with and without the publisher's name in front), the install
/// folder's name and the main exe names.
#[must_use]
pub fn program_keys(
    display: &str,
    publisher: Option<&str>,
    install_dir_name: Option<&str>,
    exe_stems: &[String],
) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    let mut push = |n: String| {
        if distinctive(&n) && !keys.contains(&n) {
            keys.push(n);
        }
    };
    let full = normalize(display);
    push(full.clone());
    push(full.replace(' ', ""));
    if let Some(p) = publisher.map(publisher_key).filter(|p| !p.is_empty()) {
        if let Some(rest) = full.strip_prefix(&format!("{p} ")).map(str::trim) {
            if !rest.is_empty() {
                push(rest.to_owned());
                push(rest.replace(' ', ""));
            }
        }
    }
    if let Some(dir) = install_dir_name {
        let n = normalize(dir);
        push(n.replace(' ', ""));
        push(n);
    }
    for stem in exe_stems {
        let lower = stem.to_lowercase();
        if GENERIC_EXES.contains(&lower.as_str()) || lower.starts_with("unins") {
            continue;
        }
        let n = normalize(stem);
        if n.replace(' ', "").chars().count() >= 4 {
            push(n);
        }
    }
    keys
}

/// `true` when the folder / key name `name` is one of `keys` (normalized, with or
/// without spaces).
#[must_use]
pub fn matches(name: &str, keys: &[String]) -> bool {
    let n = normalize(name);
    if n.is_empty() {
        return false;
    }
    let c = n.replace(' ', "");
    keys.iter().any(|k| *k == n || *k == c)
}

/// A weaker tie: `name` starts with one of `keys` followed by more words
/// (`Acme Tool Beta` for `acme tool`). Keys of at least five characters only.
#[must_use]
pub fn starts_with_key(name: &str, keys: &[String]) -> bool {
    let n = normalize(name);
    keys.iter().any(|k| {
        k.chars().count() >= 5
            && n.len() > k.len()
            && n.starts_with(k.as_str())
            && n.as_bytes()[k.len()] == b' '
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_architecture_and_marks_go() {
        assert_eq!(normalize("Python 3.12.1 (64-bit)"), "python");
        assert_eq!(normalize("7-Zip 23.01 (x64)"), "7 zip");
        assert_eq!(normalize("Notepad++ (64-bit x64)"), "notepad");
        assert_eq!(
            normalize("Mozilla Firefox (x64 en-US)"),
            "mozilla firefox en us"
        );
        assert_eq!(
            normalize("Microsoft Visual C++ 2015-2022 Redistributable (x64) - 14.38.33130"),
            "microsoft visual c 2015 2022 redistributable"
        );
        assert_eq!(normalize("Zalo™"), "zalo");
        assert_eq!(normalize("Node.js"), "node js");
        assert_eq!(normalize("Firefox 120"), "firefox");
        assert_eq!(normalize("v2.0.1"), "");
        assert_eq!(normalize("Steam"), "steam");
        assert_eq!(normalize("Git version 2.45.1"), "git version");
        assert_eq!(compact("Visual Studio Code"), "visualstudiocode");
    }

    #[test]
    fn publishers_lose_their_company_form() {
        assert_eq!(publisher_key("Acme Software, Inc."), "acme");
        assert_eq!(publisher_key("Microsoft Corporation"), "microsoft");
        assert_eq!(publisher_key("Valve Corp."), "valve");
        assert_eq!(
            publisher_key("The Git Development Community"),
            "git development community"
        );
    }

    #[test]
    fn generic_words_are_never_enough() {
        for w in [
            "common",
            "data",
            "Microsoft",
            "logs",
            "Update",
            "ab",
            "2024",
            "User Data",
        ] {
            assert!(!distinctive(&normalize(w)), "{w}");
        }
        for w in ["zalo", "steam", "vlc", "discord", "7 zip"] {
            assert!(distinctive(&normalize(w)), "{w}");
        }
    }

    #[test]
    fn keys_of_a_program() {
        let keys = program_keys(
            "Microsoft Visual Studio Code (User)",
            Some("Microsoft Corporation"),
            Some("Microsoft VS Code"),
            &["Code".into(), "unins000".into(), "Update".into()],
        );
        assert!(keys.contains(&"microsoft visual studio code user".to_owned()));
        assert!(keys.contains(&"visual studio code user".to_owned()));
        assert!(keys.contains(&"microsoft vs code".to_owned()));
        assert!(keys.contains(&"code".to_owned()));
        assert!(!keys.iter().any(|k| k == "unins000" || k == "update"));
        assert!(matches("Code", &keys));
        assert!(matches("MicrosoftVSCode", &keys));
        assert!(!matches("Microsoft", &keys));
    }

    #[test]
    fn folder_names_match_with_or_without_spaces() {
        let keys = program_keys("Acme Photo Studio 5", Some("Acme Ltd"), None, &[]);
        assert!(matches("Acme Photo Studio", &keys));
        assert!(matches("AcmePhotoStudio", &keys));
        assert!(matches("Photo Studio", &keys));
        assert!(!matches("Acme", &keys));
        assert!(starts_with_key("Acme Photo Studio Beta", &keys));
        assert!(!starts_with_key("Acme Photo Studios", &keys));
    }
}
