//! Folder Encyclopedia (PLAN §13 A5): "What is this folder? Can I delete it? What
//! happens if I do?" for well-known system, app and developer folders.
//!
//! Articles live in `rules/encyclopedia/*.md` so the community can add them by pull
//! request. Each file is TOML front matter between `+++` lines, then one section per
//! language (`## en`, `## vi` — both required) whose first `### ` heading is the title
//! and the rest is the text (paragraphs, `**bold**`, `` `code` ``):
//!
//! ```text
//! +++
//! id = "winsxs"
//! risk = "protected"
//! paths = ["${SYSTEMROOT}/WinSxS"]      # exact folders (rule-pack variables)
//! names = []                            # or: a folder/file name anywhere …
//! siblings = []                         # … next to all of these names
//! volume_paths = []                     # or: a path below any drive root
//! +++
//! ## en
//! ### Windows component store
//! What it is …
//! ## vi
//! ### Kho thành phần Windows
//! …
//! ```
//!
//! Articles are data only: they explain and point to the proper tool, they never add a
//! delete action. The desktop app embeds the files at build time ([`builtin`]).

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use nfd_core::RiskLevel;
use serde::{Deserialize, Serialize};

use crate::{ALLOWED_VARS, Error, Finding, expand, extract_vars};

/// Languages every article must provide.
pub const LANGUAGES: &[&str] = &["en", "vi"];

/// Title and text in one language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Text {
    pub title: String,
    /// Markdown paragraphs (blank-line separated; `**bold**` and `` `code` `` only).
    pub body: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Front {
    id: String,
    risk: RiskLevel,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    names: Vec<String>,
    #[serde(default)]
    siblings: Vec<String>,
    #[serde(default)]
    volume_paths: Vec<String>,
    /// Rule-pack rule ids that clean this folder safely (Phase 3/4), for a link.
    #[serde(default)]
    rules: Vec<String>,
}

/// One encyclopedia entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Article {
    pub id: String,
    pub risk: RiskLevel,
    pub paths: Vec<String>,
    pub names: Vec<String>,
    pub siblings: Vec<String>,
    pub volume_paths: Vec<String>,
    pub rules: Vec<String>,
    /// Per language (`en`, `vi`).
    pub text: BTreeMap<String, Text>,
    #[serde(skip)]
    pub source: Option<PathBuf>,
}

fn parse_error(source: Option<&Path>, message: String) -> Error {
    Error::Parse {
        path: source.map(Path::to_path_buf).unwrap_or_default(),
        source: Box::new(<toml::de::Error as serde::de::Error>::custom(message)),
    }
}

/// Parse one article.
///
/// # Errors
/// [`Error::Parse`] when the front matter is missing or invalid.
pub fn parse(text: &str, source: Option<&Path>) -> Result<Article, Error> {
    let text = text.replace("\r\n", "\n");
    let rest = text
        .strip_prefix("+++\n")
        .ok_or_else(|| parse_error(source, "missing `+++` front matter".into()))?;
    let end = rest
        .find("\n+++")
        .ok_or_else(|| parse_error(source, "unterminated `+++` front matter".into()))?;
    let front: Front = toml::from_str(&rest[..end]).map_err(|e| Error::Parse {
        path: source.map(Path::to_path_buf).unwrap_or_default(),
        source: Box::new(e),
    })?;
    let body = &rest[end + 4..];

    let mut texts = BTreeMap::new();
    let mut lang: Option<String> = None;
    let mut title = String::new();
    let mut lines: Vec<&str> = Vec::new();
    let mut flush = |lang: &Option<String>, title: &mut String, lines: &mut Vec<&str>| {
        if let Some(l) = lang {
            texts.insert(
                l.clone(),
                Text {
                    title: std::mem::take(title),
                    body: lines.join("\n").trim().to_owned(),
                },
            );
        }
        lines.clear();
    };
    for line in body.lines() {
        if let Some(l) = line.strip_prefix("## ") {
            flush(&lang, &mut title, &mut lines);
            lang = Some(l.trim().to_owned());
        } else if let Some(t) = line.strip_prefix("### ") {
            if title.is_empty() && lines.iter().all(|l| l.trim().is_empty()) {
                t.trim().clone_into(&mut title);
            } else {
                lines.push(line);
            }
        } else if lang.is_some() {
            lines.push(line);
        }
    }
    flush(&lang, &mut title, &mut lines);

    Ok(Article {
        id: front.id,
        risk: front.risk,
        paths: front.paths,
        names: front.names,
        siblings: front.siblings,
        volume_paths: front.volume_paths,
        rules: front.rules,
        text: texts,
        source: source.map(Path::to_path_buf),
    })
}

/// Load every `*.md` file in `dir` (sorted by name).
///
/// # Errors
/// The folder or a file cannot be read, or an article does not parse.
pub fn load_dir(dir: &Path) -> Result<Vec<Article>, Error> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|source| Error::Io {
            path: dir.to_path_buf(),
            source,
        })?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("md")))
        .filter(|p| {
            !p.file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case("README.md"))
        })
        .collect();
    files.sort();
    files
        .iter()
        .map(|p| {
            let text = fs::read_to_string(p).map_err(|source| Error::Io {
                path: p.clone(),
                source,
            })?;
            parse(&text, Some(p))
        })
        .collect()
}

macro_rules! builtin_files {
    ($($name:literal),* $(,)?) => {
        /// File names compiled into [`builtin`] (a test keeps it equal to the folder).
        pub const BUILTIN_FILES: &[&str] = &[$($name),*];
        const BUILTIN_TEXT: &[&str] = &[$(include_str!(concat!("../../../rules/encyclopedia/", $name))),*];
    };
}

builtin_files![
    "appdata-packages.md",
    "cargo-registry.md",
    "derived-data-cache.md",
    "docker-wsl.md",
    "git-folder.md",
    "gradle-caches.md",
    "hiberfil.md",
    "node-modules.md",
    "npm-cache.md",
    "nuget-packages.md",
    "ollama.md",
    "package-cache.md",
    "pagefile.md",
    "pycache.md",
    "recycle-bin.md",
    "rust-target.md",
    "software-distribution.md",
    "system-volume-information.md",
    "temp.md",
    "unity-library.md",
    "windows-installer.md",
    "windows-old.md",
    "winsxs.md",
];

/// The articles shipped with this build (unparseable ones are skipped; the test
/// suite and `nfd rules validate` make sure there are none).
#[must_use]
pub fn builtin() -> Vec<Article> {
    BUILTIN_FILES
        .iter()
        .zip(BUILTIN_TEXT)
        .filter_map(|(name, text)| parse(text, Some(Path::new(name))).ok())
        .collect()
}

/// Check articles: ids unique and well-formed, both languages present with a title
/// and enough text, at least one way to match, known variables only.
#[must_use]
pub fn validate(articles: &[Article]) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut ids = HashSet::new();
    for a in articles {
        let mut push = |error: bool, message: String| {
            findings.push(Finding {
                pack: "encyclopedia".into(),
                rule: Some(a.id.clone()),
                error,
                message,
            });
        };
        if !ids.insert(a.id.as_str()) {
            push(true, "duplicate article id".into());
        }
        if a.id.is_empty()
            || !a
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            push(true, "id must be lowercase [a-z0-9-]".into());
        }
        if let Some(file) = a.source.as_ref().and_then(|p| p.file_stem()) {
            if file.to_string_lossy() != a.id {
                push(true, format!("file name must be `{}.md`", a.id));
            }
        }
        if a.paths.is_empty() && a.names.is_empty() && a.volume_paths.is_empty() {
            push(true, "needs paths, names or volume_paths".into());
        }
        if !a.siblings.is_empty() && a.names.is_empty() {
            push(true, "siblings only apply to names".into());
        }
        for path in &a.paths {
            if !path.starts_with("${") {
                push(true, format!("path must start with a variable: `{path}`"));
            }
            for var in extract_vars(path) {
                if !ALLOWED_VARS.contains(&var.as_str()) {
                    push(true, format!("unknown variable `${{{var}}}` in `{path}`"));
                }
            }
        }
        for p in a.names.iter().chain(&a.siblings).chain(&a.volume_paths) {
            if p.contains("${") || p.contains("..") || p.starts_with(['/', '\\']) {
                push(true, format!("`{p}` must be a plain relative name"));
            }
        }
        for lang in LANGUAGES {
            match a.text.get(*lang) {
                None => push(true, format!("missing `## {lang}` section")),
                Some(t) if t.title.is_empty() => {
                    push(true, format!("`## {lang}` has no `### title`"))
                }
                Some(t) if t.body.len() < 80 => {
                    push(true, format!("`## {lang}` text is too short"))
                }
                Some(_) => {}
            }
        }
        for lang in a.text.keys() {
            if !LANGUAGES.contains(&lang.as_str()) {
                push(false, format!("unexpected language section `{lang}`"));
            }
        }
    }
    findings
}

/// Every `rules = [...]` reference must name a rule of the loaded packs.
#[must_use]
pub fn validate_rule_links(articles: &[Article], packs: &[crate::Pack]) -> Vec<Finding> {
    let known: HashSet<&str> = packs
        .iter()
        .flat_map(|p| p.rules.iter().map(|r| r.id.as_str()))
        .collect();
    articles
        .iter()
        .flat_map(|a| {
            a.rules
                .iter()
                .filter(|r| !known.contains(r.as_str()))
                .map(|r| Finding {
                    pack: "encyclopedia".into(),
                    rule: Some(a.id.clone()),
                    error: true,
                    message: format!("unknown rule `{r}`"),
                })
        })
        .collect()
}

/// Lower-case, `\`-separated, without trailing separators (except `c:\`).
fn norm(path: &str) -> String {
    let s = path.replace('/', "\\").to_lowercase();
    let t = s.trim_end_matches('\\');
    if t.len() == 2 && t.ends_with(':') {
        format!("{t}\\")
    } else {
        t.to_owned()
    }
}

/// Path below its drive root (`c:\$recycle.bin` → `$recycle.bin`), normalized.
fn below_volume(path: &str) -> Option<&str> {
    let b = path.as_bytes();
    (b.len() > 3 && b[1] == b':' && b[2] == b'\\').then(|| &path[3..])
}

/// A match found by [`Matcher::lookup`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit<'a> {
    pub article: &'a Article,
    /// The looked-up path is inside the matched folder rather than the folder itself.
    pub inside: bool,
    /// The matched folder (the path itself or an ancestor).
    pub depth_up: usize,
}

/// Articles prepared for lookups on this machine (variables expanded once).
#[derive(Debug)]
pub struct Matcher {
    articles: Vec<Article>,
    expanded: Vec<Vec<String>>,
}

impl Matcher {
    /// Expand every article's `paths` with `resolve` (unresolvable ones are dropped).
    #[must_use]
    pub fn new(articles: Vec<Article>, resolve: &impl Fn(&str) -> Option<String>) -> Self {
        let expanded = articles
            .iter()
            .map(|a| {
                a.paths
                    .iter()
                    .filter_map(|p| expand(p, resolve))
                    .map(|p| norm(&p))
                    .collect()
            })
            .collect();
        Self { articles, expanded }
    }

    #[must_use]
    pub fn articles(&self) -> &[Article] {
        &self.articles
    }

    fn matches_here(&self, i: usize, path: &str, exists: &impl Fn(&Path) -> bool) -> bool {
        let a = &self.articles[i];
        if self.expanded[i].iter().any(|p| p == path) {
            return true;
        }
        if let Some(rel) = below_volume(path) {
            if a.volume_paths.iter().any(|v| norm(v) == rel) {
                return true;
            }
        }
        let name = path.rsplit('\\').next().unwrap_or(path);
        if a.names.iter().any(|n| n.to_lowercase() == name) {
            let parent = path.rsplit_once('\\').map_or("", |(p, _)| p);
            return a
                .siblings
                .iter()
                .all(|s| exists(&Path::new(&format!("{parent}\\")).join(s)));
        }
        false
    }

    /// The article for `path` itself only (no ancestor), if any.
    #[must_use]
    pub fn at(&self, path: &Path, exists: &impl Fn(&Path) -> bool) -> Option<&Article> {
        let cur = norm(&path.to_string_lossy());
        (0..self.articles.len())
            .find(|&i| self.matches_here(i, &cur, exists))
            .map(|i| &self.articles[i])
    }

    /// The article for `path` itself, else for its nearest ancestor that has one.
    /// `exists` checks sibling markers (e.g. a Unity `Library` next to `Assets`).
    #[must_use]
    pub fn lookup(&self, path: &Path, exists: &impl Fn(&Path) -> bool) -> Option<Hit<'_>> {
        let mut cur = norm(&path.to_string_lossy());
        let mut up = 0;
        loop {
            if let Some(i) = (0..self.articles.len()).find(|&i| self.matches_here(i, &cur, exists))
            {
                return Some(Hit {
                    article: &self.articles[i],
                    inside: up > 0,
                    depth_up: up,
                });
            }
            let (parent, _) = cur.rsplit_once('\\')?;
            if parent.is_empty() || parent.ends_with(':') {
                return None;
            }
            cur = parent.to_owned();
            up += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "+++\nid = \"sample\"\nrisk = \"protected\"\npaths = [\"${SYSTEMROOT}/WinSxS\"]\n+++\n\n## en\n### Windows component store\n\nFirst paragraph with **bold** and `code`, long enough to pass validation easily.\n\nSecond paragraph.\n\n## vi\n### Kho thành phần Windows\n\nĐoạn đầu tiên đủ dài để vượt qua bước kiểm tra độ dài của bài viết bách khoa.\n";

    fn resolve(var: &str) -> Option<String> {
        match var {
            "SYSTEMROOT" => Some(r"C:\Windows".into()),
            "HOME" => Some(r"C:\Users\me".into()),
            "LOCALAPPDATA" => Some(r"C:\Users\me\AppData\Local".into()),
            _ => None,
        }
    }

    #[test]
    fn parses_front_matter_and_sections() {
        let a = parse(SAMPLE, Some(Path::new("sample.md"))).expect("parse");
        assert_eq!(a.id, "sample");
        assert_eq!(a.risk, RiskLevel::Protected);
        assert_eq!(a.text["en"].title, "Windows component store");
        assert!(a.text["en"].body.starts_with("First paragraph"));
        assert!(a.text["en"].body.ends_with("Second paragraph."));
        assert_eq!(a.text["vi"].title, "Kho thành phần Windows");
        assert!(validate(&[a]).is_empty());
        // CRLF files parse the same
        let crlf = parse(&SAMPLE.replace('\n', "\r\n"), None).expect("crlf");
        assert_eq!(crlf.text["vi"].title, "Kho thành phần Windows");
    }

    #[test]
    fn rejects_broken_articles() {
        assert!(parse("no front matter", None).is_err());
        assert!(parse("+++\nid = \"x\"\n", None).is_err());
        assert!(parse("+++\nid = \"x\"\nrisk = \"safe\"\nbogus = 1\n+++\n", None).is_err());
        let a = parse(
            "+++\nid = \"Bad_Id\"\nrisk = \"safe\"\npaths = [\"C:/abs\", \"${NOPE}/x\"]\nsiblings = [\"a\"]\n+++\n## en\n### T\nshort\n",
            Some(Path::new("other.md")),
        )
        .expect("parse");
        let messages: Vec<String> = validate(&[a.clone(), a])
            .into_iter()
            .map(|f| f.message)
            .collect();
        for expected in [
            "duplicate article id",
            "id must be lowercase [a-z0-9-]",
            "file name must be `Bad_Id.md`",
            "siblings only apply to names",
            "path must start with a variable: `C:/abs`",
            "unknown variable `${NOPE}` in `${NOPE}/x`",
            "`## en` text is too short",
            "missing `## vi` section",
        ] {
            assert!(
                messages.iter().any(|m| m == expected),
                "{expected} not in {messages:?}"
            );
        }
    }

    #[test]
    fn builtin_articles_are_valid_and_complete() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules/encyclopedia");
        let on_disk = load_dir(&dir).expect("load");
        let mut names: Vec<String> = on_disk
            .iter()
            .filter_map(|a| {
                a.source
                    .as_ref()?
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
            })
            .collect();
        names.sort();
        assert_eq!(
            names, BUILTIN_FILES,
            "update builtin_files! in encyclopedia.rs"
        );
        let findings = validate(&on_disk);
        assert!(findings.is_empty(), "{findings:#?}");
        let packs = crate::load_dir(&dir.join("..")).expect("packs");
        let links = validate_rule_links(&on_disk, &packs);
        assert!(links.is_empty(), "{links:#?}");
        let mut bogus = on_disk[0].clone();
        bogus.rules = vec!["no.such.rule".into()];
        assert_eq!(validate_rule_links(&[bogus], &packs).len(), 1);
        assert_eq!(builtin().len(), BUILTIN_FILES.len());
        // never claims a protected location is anything but protected/advanced
        for a in &on_disk {
            for p in &a.paths {
                if crate::is_protected_template(p) {
                    assert!(
                        !a.risk.is_deletable() || a.risk == RiskLevel::Advanced,
                        "{} {p}",
                        a.id
                    );
                }
            }
        }
    }

    #[test]
    fn lookup_by_path_volume_name_and_ancestor() {
        let m = Matcher::new(builtin(), &resolve);
        let none = |_: &Path| false;
        let all = |_: &Path| true;
        let id = |p: &str, e: &dyn Fn(&Path) -> bool| {
            m.lookup(Path::new(p), &|x| e(x))
                .map(|h| (h.article.id.clone(), h.inside))
        };
        assert_eq!(
            id(r"c:\windows\winsxs", &none),
            Some(("winsxs".into(), false))
        );
        assert_eq!(
            id(r"C:\Windows\WinSxS\amd64_x\f.dll", &none),
            Some(("winsxs".into(), true))
        );
        assert_eq!(
            id(r"D:\$Recycle.Bin", &none),
            Some(("recycle-bin".into(), false))
        );
        assert_eq!(
            id(r"E:\pagefile.sys", &none),
            Some(("pagefile".into(), false))
        );
        assert_eq!(
            id(r"D:\code\app\node_modules\lodash", &none),
            Some(("node-modules".into(), true))
        );
        // a Unity Library needs Assets + ProjectSettings next to it
        assert_eq!(
            id(r"D:\Games\Proj\Library", &all),
            Some(("unity-library".into(), false))
        );
        assert_eq!(id(r"D:\Books\Library", &none), None);
        assert_eq!(
            id(r"C:\Users\me\.ollama\models", &none),
            Some(("ollama".into(), true))
        );
        assert_eq!(id(r"D:\nothing\here", &none), None);
        assert_eq!(id(r"C:\", &none), None);
    }
}
