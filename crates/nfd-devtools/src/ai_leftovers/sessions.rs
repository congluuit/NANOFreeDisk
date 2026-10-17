//! Who made a leftover (`docs/AI_LEFTOVERS.md` §2.3): the AI sessions on this
//! machine, read — never written — from where the agents keep them:
//!
//! * Claude Code transcripts `~/.claude/projects/<project>/<session>.jsonl`
//!   (`CLAUDE_CONFIG_DIR`): working directory, branch, start, title (`custom-title`,
//!   `summary`, `agent-name`), last activity = last write of the transcript;
//! * Claude desktop sessions `%APPDATA%\Claude\claude-code-sessions\*\*\local_*.json`
//!   (and the Microsoft Store build's folder): title, `lastActivityAt`, and whether
//!   the session is archived — a session that is not archived is **open** and may be
//!   continued, so what it uses is kept;
//! * Codex rollouts `~/.codex/sessions/**/rollout-*.jsonl` (first line only).
//!
//! These files are 🔒 conversation history: they are opened read-only and only their
//! first / last bytes are read. Anything unreadable is skipped.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use nfd_platform::fsinfo;
use serde::Serialize;
use serde_json::Value;

use crate::{norm, within};

/// Bytes read from the start of a transcript (working directory, branch, start).
const HEAD: u64 = 64 * 1024;
/// Bytes read from the end of a transcript (title).
const TAIL: u64 = 256 * 1024;
/// Most session files read per source.
const MAX_FILES: usize = 20_000;

/// One AI session.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// The agent's session id (the transcript's name).
    pub id: String,
    pub tool: &'static str,
    pub title: Option<String>,
    /// Working directory.
    pub cwd: Option<PathBuf>,
    pub branch: Option<String>,
    /// Unix seconds (0 = unknown).
    pub started: i64,
    pub last_active: i64,
    /// Known to the Claude desktop app and not archived there.
    pub open: bool,
}

/// Where the sessions are read from (tests point these into a fixture).
#[derive(Debug, Clone, Default)]
pub struct SessionSources {
    /// `~/.claude/projects` folders.
    pub claude_projects: Vec<PathBuf>,
    /// `claude-code-sessions` folders of the desktop app.
    pub desktop: Vec<PathBuf>,
    /// `~/.codex/sessions` folders.
    pub codex: Vec<PathBuf>,
}

impl SessionSources {
    /// The real locations: `resolve` maps `HOME`, `APPDATA`, `LOCALAPPDATA`; `env`
    /// reads `CLAUDE_CONFIG_DIR` / `CODEX_HOME`.
    #[must_use]
    pub fn detect(
        resolve: &dyn Fn(&str) -> Option<String>,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Self {
        let mut s = Self::default();
        let home = resolve("HOME").map(PathBuf::from);
        if let Some(dir) = env("CLAUDE_CONFIG_DIR").filter(|d| !d.trim().is_empty()) {
            s.claude_projects.push(PathBuf::from(dir).join("projects"));
        }
        if let Some(h) = &home {
            s.claude_projects.push(h.join(".claude").join("projects"));
        }
        if let Some(dir) = env("CODEX_HOME").filter(|d| !d.trim().is_empty()) {
            s.codex.push(PathBuf::from(dir).join("sessions"));
        }
        if let Some(h) = &home {
            s.codex.push(h.join(".codex").join("sessions"));
        }
        if let Some(a) = resolve("APPDATA") {
            s.desktop
                .push(PathBuf::from(a).join("Claude").join("claude-code-sessions"));
        }
        if let Some(l) = resolve("LOCALAPPDATA") {
            let packages = PathBuf::from(l).join("Packages");
            for name in crate::fsutil::subdirs(&packages) {
                if name.to_lowercase().starts_with("claude_") {
                    s.desktop.push(
                        packages
                            .join(name)
                            .join("LocalCache")
                            .join("Roaming")
                            .join("Claude")
                            .join("claude-code-sessions"),
                    );
                }
            }
        }
        for list in [&mut s.claude_projects, &mut s.desktop, &mut s.codex] {
            let mut seen = Vec::new();
            list.retain(|p| {
                let k = norm(&p.to_string_lossy());
                let fresh = !seen.contains(&k);
                seen.push(k);
                fresh && p.is_dir()
            });
        }
        s
    }
}

/// Days since 1970-01-01 of a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Unix seconds of an RFC 3339 UTC time (`2026-10-16T16:40:34.762Z`); offsets other
/// than `Z` / `+00:00` are applied.
#[must_use]
pub fn parse_time(text: &str) -> Option<i64> {
    let t = text.trim();
    if t.len() < 19 {
        return None;
    }
    let num = |a: usize, b: usize| t.get(a..b)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, mi, s) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || s > 60 {
        return None;
    }
    let mut rest = &t[19..];
    if let Some(r) = rest.strip_prefix('.') {
        rest = r.trim_start_matches(|c: char| c.is_ascii_digit());
    }
    let offset = match rest {
        "" | "Z" | "z" => 0,
        o if o.len() == 6 && (o.starts_with('+') || o.starts_with('-')) => {
            let oh: i64 = o.get(1..3)?.parse().ok()?;
            let om: i64 = o.get(4..6)?.parse().ok()?;
            let v = oh * 3600 + om * 60;
            if o.starts_with('+') { v } else { -v }
        }
        _ => return None,
    };
    Some(days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + s - offset)
}

fn read_range(path: &Path, from_end: bool, len: u64) -> Option<String> {
    let mut f = File::open(path).ok()?;
    let size = f.metadata().ok()?.len();
    let mut buf = Vec::new();
    if from_end && size > len {
        f.seek(SeekFrom::Start(size - len)).ok()?;
        f.take(len).read_to_end(&mut buf).ok()?;
        // drop the partial first line
        let cut = buf
            .iter()
            .position(|b| *b == b'\n')
            .map_or(buf.len(), |i| i + 1);
        buf.drain(..cut);
    } else {
        f.take(len).read_to_end(&mut buf).ok()?;
        if size > len {
            // drop the partial last line
            let cut = buf.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
            buf.truncate(cut);
        }
    }
    Some(String::from_utf8_lossy(&buf).into_owned())
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn clean_title(t: String) -> String {
    let t: String = t.chars().filter(|c| !c.is_control()).take(120).collect();
    t.trim().to_owned()
}

/// A Claude Code transcript.
fn claude_transcript(path: &Path) -> Option<Session> {
    let id = path.file_stem()?.to_string_lossy().into_owned();
    let info = fsinfo::file_info(path).ok()?;
    let mut s = Session {
        id,
        tool: "Claude Code",
        last_active: info.mtime,
        ..Session::default()
    };
    let head = read_range(path, false, HEAD).unwrap_or_default();
    for line in head.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if s.started == 0 {
            if let Some(t) = str_field(&v, "timestamp").and_then(|t| parse_time(&t)) {
                s.started = t;
            }
        }
        if s.cwd.is_none() {
            if let Some(cwd) = str_field(&v, "cwd") {
                s.cwd = Some(PathBuf::from(cwd));
                s.branch = str_field(&v, "gitBranch");
            }
        }
        if s.cwd.is_some() && s.started != 0 {
            break;
        }
    }
    // a title the user or the app gave the session
    let side = path.with_extension("").join("custom-title.json");
    if let Some(t) = std::fs::read_to_string(&side)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| str_field(&v, "customTitle"))
    {
        s.title = Some(clean_title(t));
    }
    if s.title.is_none() {
        let tail = read_range(path, true, TAIL).unwrap_or_default();
        let mut fallback = None;
        for line in tail.lines().rev() {
            if !line.contains("\"custom-title\"")
                && !line.contains("\"summary\"")
                && !line.contains("\"agent-name\"")
            {
                continue;
            }
            let Ok(v) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            match v.get("type").and_then(Value::as_str) {
                Some("custom-title") => {
                    s.title = str_field(&v, "customTitle").map(clean_title);
                    break;
                }
                Some("summary") if fallback.is_none() => {
                    fallback = str_field(&v, "summary").map(clean_title);
                }
                Some("agent-name") if fallback.is_none() => {
                    fallback = str_field(&v, "agentName").map(clean_title);
                }
                _ => {}
            }
        }
        if s.title.is_none() {
            s.title = fallback;
        }
    }
    if s.started == 0 {
        s.started = info.mtime;
    }
    Some(s)
}

/// Every Claude Code transcript under `projects` (`<project>/<session>.jsonl`).
fn claude_sessions(projects: &Path, out: &mut Vec<Session>) {
    for dir in crate::fsutil::subdirs(projects) {
        let Ok(items) = fsinfo::read_dir(&projects.join(&dir), false) else {
            continue;
        };
        for item in items {
            if out.len() >= MAX_FILES {
                return;
            }
            let name = item.name.to_string_lossy();
            let is_file = item.info.as_ref().is_ok_and(|i| !i.is_dir);
            if is_file && name.to_lowercase().ends_with(".jsonl") {
                if let Some(s) = claude_transcript(&projects.join(&dir).join(&*name)) {
                    out.push(s);
                }
            }
        }
    }
}

/// A desktop-app session (`local_*.json`): (cli session id, session).
fn desktop_session(path: &Path, archived: &[String]) -> Option<(Option<String>, Session)> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    let local = str_field(&v, "sessionId")?;
    let ms = |k: &str| v.get(k).and_then(Value::as_i64).map_or(0, |t| t / 1000);
    let is_archived = v
        .get("isArchived")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || archived.iter().any(|a| a == &local);
    let cli = str_field(&v, "cliSessionId");
    Some((
        cli.clone(),
        Session {
            id: cli.unwrap_or(local),
            tool: "Claude Code",
            title: str_field(&v, "title").map(clean_title),
            cwd: str_field(&v, "cwd").map(PathBuf::from),
            branch: None,
            started: ms("createdAt"),
            last_active: ms("lastActivityAt"),
            open: !is_archived,
        },
    ))
}

fn desktop_sessions(root: &Path, out: &mut Vec<(Option<String>, Session)>) {
    for account in crate::fsutil::subdirs(root) {
        for org in crate::fsutil::subdirs(&root.join(&account)) {
            let dir = root.join(&account).join(&org);
            let archived: Vec<String> = std::fs::read_to_string(dir.join("archived-sessions.idx"))
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                .and_then(|v| v.get("archived").cloned())
                .and_then(|a| serde_json::from_value(a).ok())
                .unwrap_or_default();
            let Ok(items) = fsinfo::read_dir(&dir, false) else {
                continue;
            };
            for item in items {
                let name = item.name.to_string_lossy();
                if name.starts_with("local_") && name.ends_with(".json") && out.len() < MAX_FILES {
                    if let Some(s) = desktop_session(&dir.join(&*name), &archived) {
                        out.push(s);
                    }
                }
            }
        }
    }
}

/// The first line of a Codex rollout.
fn codex_rollout(path: &Path) -> Option<Session> {
    let info = fsinfo::file_info(path).ok()?;
    let head = read_range(path, false, HEAD)?;
    let v: Value = serde_json::from_str(head.lines().next()?).ok()?;
    let meta = v
        .get("payload")
        .filter(|_| v.get("type").and_then(Value::as_str) == Some("session_meta"));
    let meta = meta.unwrap_or(&v);
    let id = str_field(meta, "id")?;
    let started = str_field(meta, "timestamp")
        .or_else(|| str_field(&v, "timestamp"))
        .and_then(|t| parse_time(&t))
        .unwrap_or(info.mtime);
    Some(Session {
        id,
        tool: "Codex",
        title: None,
        cwd: str_field(meta, "cwd").map(PathBuf::from),
        branch: meta.get("git").and_then(|g| str_field(g, "branch")),
        started,
        last_active: info.mtime,
        open: false,
    })
}

fn codex_sessions(dir: &Path, depth: usize, out: &mut Vec<Session>) {
    let Ok(items) = fsinfo::read_dir(dir, false) else {
        return;
    };
    for item in items {
        if out.len() >= MAX_FILES {
            return;
        }
        let Ok(info) = item.info else { continue };
        if info.link != fsinfo::LinkKind::None {
            continue;
        }
        let path = dir.join(&item.name);
        let name = item.name.to_string_lossy().to_lowercase();
        if info.is_dir && depth < 4 {
            codex_sessions(&path, depth + 1, out);
        } else if !info.is_dir && name.starts_with("rollout-") && name.ends_with(".jsonl") {
            if let Some(s) = codex_rollout(&path) {
                out.push(s);
            }
        }
    }
}

/// Every session of `sources`, desktop-app facts merged into the transcripts.
#[must_use]
pub fn load(sources: &SessionSources) -> Vec<Session> {
    let mut sessions = Vec::new();
    for p in &sources.claude_projects {
        claude_sessions(p, &mut sessions);
    }
    let mut desktop = Vec::new();
    for d in &sources.desktop {
        desktop_sessions(d, &mut desktop);
    }
    let mut by_id: HashMap<String, usize> = sessions
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id.to_lowercase(), i))
        .collect();
    for (cli, d) in desktop {
        match cli.and_then(|c| by_id.get(&c.to_lowercase()).copied()) {
            Some(i) => {
                let s = &mut sessions[i];
                if d.title.is_some() {
                    s.title = d.title;
                }
                s.open |= d.open;
                s.last_active = s.last_active.max(d.last_active);
                if s.started == 0 || (d.started > 0 && d.started < s.started) {
                    s.started = d.started;
                }
                if s.cwd.is_none() {
                    s.cwd = d.cwd;
                }
            }
            None => {
                by_id.insert(d.id.to_lowercase(), sessions.len());
                sessions.push(d);
            }
        }
    }
    for c in &sources.codex {
        codex_sessions(c, 0, &mut sessions);
    }
    sessions
}

/// The sessions, searchable by id, folder and time.
#[derive(Debug, Clone, Default)]
pub struct SessionIndex {
    pub sessions: Vec<Session>,
    cwds: Vec<Option<String>>,
}

/// How a leftover relates to its session (what the UI shows).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRef {
    pub id: String,
    pub tool: &'static str,
    pub title: Option<String>,
    pub last_active: i64,
    pub open: bool,
    /// Active within the keep window.
    pub recent: bool,
}

impl SessionIndex {
    #[must_use]
    pub fn new(sessions: Vec<Session>) -> Self {
        let cwds = sessions
            .iter()
            .map(|s| s.cwd.as_ref().map(|c| norm(&c.to_string_lossy())))
            .collect();
        Self { sessions, cwds }
    }

    #[must_use]
    pub fn by_id(&self, id: &str) -> Option<&Session> {
        self.sessions.iter().find(|s| s.id.eq_ignore_ascii_case(id))
    }

    /// Sessions whose working directory is `dir` or inside it.
    pub fn within<'a>(&'a self, dir: &Path) -> impl Iterator<Item = &'a Session> + 'a {
        let d = norm(&dir.to_string_lossy());
        self.sessions
            .iter()
            .zip(&self.cwds)
            .filter(move |(_, c)| c.as_deref().is_some_and(|c| within(c, &d)))
            .map(|(s, _)| s)
    }

    /// The session of `project` (its own folder, not a worktree below it) that was
    /// running when something last written at `newest` was written: the latest one
    /// whose [start − 5 min, last activity + 10 min] holds `newest`.
    #[must_use]
    pub fn at(&self, project: &Path, newest: i64) -> Option<&Session> {
        let wt = norm(&project.join(".claude").join("worktrees").to_string_lossy());
        self.within(project)
            .filter(|s| {
                s.cwd
                    .as_ref()
                    .is_none_or(|c| !within(&norm(&c.to_string_lossy()), &wt))
            })
            .filter(|s| newest >= s.started - 300 && newest <= s.last_active + 600)
            .max_by_key(|s| s.started)
    }

    /// Distinct working directories of all sessions.
    #[must_use]
    pub fn folders(&self) -> Vec<PathBuf> {
        let mut seen = Vec::new();
        let mut out = Vec::new();
        for s in &self.sessions {
            if let Some(c) = &s.cwd {
                let k = norm(&c.to_string_lossy());
                if !seen.contains(&k) {
                    seen.push(k);
                    out.push(c.clone());
                }
            }
        }
        out
    }
}

/// `s` as seen by a leftover: `recent` when active within `keep_days` of `now`.
#[must_use]
pub fn reference(s: &Session, now: i64, keep_days: u32) -> SessionRef {
    SessionRef {
        id: s.id.clone(),
        tool: s.tool,
        title: s.title.clone(),
        last_active: s.last_active,
        open: s.open,
        recent: keep_days > 0 && now - s.last_active < i64::from(keep_days) * 86_400,
    }
}

/// The session to show for a set (most recently active), and whether any of them
/// keeps the leftover (open or recent).
#[must_use]
pub fn strongest<'a>(
    list: impl IntoIterator<Item = &'a Session>,
    now: i64,
    keep_days: u32,
) -> Option<SessionRef> {
    let refs: Vec<SessionRef> = list
        .into_iter()
        .map(|s| reference(s, now, keep_days))
        .collect();
    let open = refs.iter().any(|r| r.open);
    let recent = refs.iter().any(|r| r.recent);
    let mut best = refs.into_iter().max_by_key(|r| r.last_active)?;
    best.open |= open;
    best.recent |= recent;
    Some(best)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn parses_rfc3339() {
        assert_eq!(parse_time("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_time("2026-10-16T16:40:34.762Z"), Some(1_792_168_834));
        assert_eq!(
            parse_time("2026-10-16T23:40:34+07:00"),
            parse_time("2026-10-16T16:40:34Z")
        );
        assert_eq!(parse_time("yesterday"), None);
        assert_eq!(parse_time("2026-13-01T00:00:00Z"), None);
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(path, text).expect("write");
    }

    #[test]
    fn reads_transcripts_desktop_sessions_and_rollouts() {
        let dir = tempfile::tempdir().expect("tmp");
        let root = dir.path();
        let projects = root.join("claude/projects");
        let wt = root.join("repo/.claude/worktrees/brave-fox");
        let wt_s = wt.to_string_lossy().replace('\\', "\\\\");
        write(
            &projects.join("D--repo/11111111-aaaa.jsonl"),
            &format!(
                "{{\"type\":\"queue-operation\",\"timestamp\":\"2026-10-16T10:00:00Z\"}}\n\
                 {{\"type\":\"user\",\"cwd\":\"{wt_s}\",\"gitBranch\":\"cong/brave-fox\",\"timestamp\":\"2026-10-16T10:00:01Z\"}}\n\
                 {{\"type\":\"summary\",\"summary\":\"Fix the build\"}}\n"
            ),
        );
        write(
            &projects.join("D--repo/22222222-bbbb.jsonl"),
            "{\"type\":\"user\",\"cwd\":\"/elsewhere\"}\n{\"type\":\"custom-title\",\"customTitle\":\"Old title\"}\n{\"type\":\"custom-title\",\"customTitle\":\"Phase 9\"}\n",
        );
        let desk = root.join("desk/acc/org");
        write(
            &desk.join("local_aaa.json"),
            &format!(
                "{{\"sessionId\":\"local_aaa\",\"cliSessionId\":\"11111111-aaaa\",\"cwd\":\"{wt_s}\",\"title\":\"Brave fox work\",\"createdAt\":1792000000000,\"lastActivityAt\":1792100000000,\"isArchived\":false}}"
            ),
        );
        write(
            &desk.join("local_bbb.json"),
            "{\"sessionId\":\"local_bbb\",\"cwd\":\"/x\",\"title\":\"Desktop only\",\"lastActivityAt\":1000,\"isArchived\":false}",
        );
        write(
            &desk.join("archived-sessions.idx"),
            "{\"v\":1,\"archived\":[\"local_bbb\"]}",
        );
        write(
            &root.join("codex/2026/10/16/rollout-2026-10-16T10-00-00-cccc.jsonl"),
            "{\"timestamp\":\"2026-10-16T10:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"cccc\",\"cwd\":\"/repo\",\"timestamp\":\"2026-10-16T10:00:00Z\",\"git\":{\"branch\":\"main\"}}}\n",
        );
        let sources = SessionSources {
            claude_projects: vec![projects],
            desktop: vec![root.join("desk")],
            codex: vec![root.join("codex")],
        };
        let all = load(&sources);
        assert_eq!(all.len(), 4, "{all:#?}");
        let a = all.iter().find(|s| s.id == "11111111-aaaa").expect("a");
        assert_eq!(
            a.title.as_deref(),
            Some("Brave fox work"),
            "desktop title wins"
        );
        assert!(a.open);
        assert_eq!(a.branch.as_deref(), Some("cong/brave-fox"));
        assert_eq!(a.started, 1_792_000_000, "desktop knows the earlier start");
        assert_eq!(a.cwd.as_deref(), Some(wt.as_path()));
        let b = all.iter().find(|s| s.id == "22222222-bbbb").expect("b");
        assert_eq!(b.title.as_deref(), Some("Phase 9"), "last custom title");
        assert!(!b.open, "CLI only: open is unknown");
        let d = all
            .iter()
            .find(|s| s.id == "local_bbb")
            .expect("desktop only");
        assert!(!d.open, "archived");
        let c = all.iter().find(|s| s.tool == "Codex").expect("codex");
        assert_eq!(c.branch.as_deref(), Some("main"));

        let index = SessionIndex::new(all);
        assert_eq!(index.within(&root.join("repo")).count(), 1);
        assert_eq!(
            index.at(&root.join("repo"), 1_792_050_000),
            None,
            "a worktree's session is not the project's"
        );
        let r = strongest(index.within(&wt), 1_792_100_000 + 86_400, 7).expect("ref");
        assert!(r.open && r.recent);
        let r = strongest(index.within(&wt), 1_792_100_000 + 30 * 86_400, 7).expect("ref");
        assert!(!r.recent);
    }
}
