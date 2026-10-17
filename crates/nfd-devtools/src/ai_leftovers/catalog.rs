//! What AI coding agents leave behind per session (`docs/AI_LEFTOVERS.md` §2.1):
//! one entry per kind of leftover, where it lives, how risky it is to delete, and
//! what deleting it means (EN + VI), each checked against the tool's behaviour on
//! the `verified_on` date.
//!
//! Conversation history is never an entry: the SafetyGate refuses it everywhere
//! (`AGENT_HISTORY_TEMPLATES`, `AGENT_HISTORY_NAMES`, `AGENT_HISTORY_SUFFIXES`) and
//! the scan lists what it found of it as 🔒 kept.

use nfd_core::RiskLevel;
use serde::Serialize;

/// Where an entry is looked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scope {
    /// Inside a project folder (a session's working directory).
    Project,
    /// `%TEMP%`.
    Temp,
    /// A per-user tool folder (`%LOCALAPPDATA%\ms-playwright`).
    Global,
}

/// How an entry is found and cleaned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// A git worktree under `.claude/worktrees`: removed with `git worktree remove`.
    Worktree,
    /// A session's folder in `%TEMP%\claude\<project>\<session>`.
    SessionTemp,
    /// A folder (or cache file) inside a project.
    Folder,
    /// An old browser revision of Playwright (the newest of each browser stays).
    BrowserRevision,
}

/// One kind of leftover.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: &'static str,
    pub tool: &'static str,
    pub scope: Scope,
    pub kind: Kind,
    /// Project entries: a name (`.playwright-mcp`, `.aider.tags.cache.v*` — a `*` at
    /// the end matches any suffix) or a relative path (`.agent/screenshots`) looked
    /// for in the project and its sub-folders. Other entries: a description.
    pub pattern: &'static str,
    pub level: RiskLevel,
    /// A common name: only offered when git confirms nothing in it is tracked — 🟢
    /// when git ignores it, 🟡 when it is merely untracked; never without git.
    pub generic: bool,
    pub impact: &'static str,
    pub impact_vi: &'static str,
    pub verified_on: &'static str,
    /// Where the behaviour was checked.
    pub source: &'static str,
}

const fn project(
    id: &'static str,
    tool: &'static str,
    pattern: &'static str,
    generic: bool,
    impact: &'static str,
    impact_vi: &'static str,
    source: &'static str,
) -> Entry {
    Entry {
        id,
        tool,
        scope: Scope::Project,
        kind: Kind::Folder,
        pattern,
        level: RiskLevel::Safe,
        generic,
        impact,
        impact_vi,
        verified_on: "2026-10-17",
        source,
    }
}

/// The catalog.
pub const CATALOG: &[Entry] = &[
    Entry {
        id: "claude-code.worktree",
        tool: "Claude Code",
        scope: Scope::Project,
        kind: Kind::Worktree,
        pattern: ".claude/worktrees/*",
        level: RiskLevel::Safe,
        generic: false,
        impact: "A git worktree a Claude Code session worked in. Removed with `git worktree remove` only when it has no uncommitted or untracked changes and its branch is merged or pushed; its ignored files (build output) go to the quarantine first. The branch stays.",
        impact_vi: "Git worktree mà một phiên Claude Code đã làm việc. Chỉ gỡ bằng `git worktree remove` khi không còn thay đổi chưa commit / tệp chưa theo dõi và nhánh đã merge hoặc đã push; tệp bị git bỏ qua (build output) vào quarantine trước. Nhánh được giữ nguyên.",
        verified_on: "2026-10-17",
        source: "code.claude.com/docs/en/worktrees; Claude desktop app",
    },
    Entry {
        id: "claude-code.session-temp",
        tool: "Claude Code",
        scope: Scope::Temp,
        kind: Kind::SessionTemp,
        pattern: "%TEMP%\\claude\\<project>\\<session>",
        level: RiskLevel::Safe,
        generic: false,
        impact: "Scratchpad and background-task output of one session (scripts, logs, test builds). The conversation itself is kept in ~/.claude/projects.",
        impact_vi: "Scratchpad và output tác vụ nền của một phiên (script, log, bản build thử). Bản thân cuộc hội thoại vẫn nằm ở ~/.claude/projects.",
        verified_on: "2026-10-17",
        source: "Claude Code 2.1 (session id = transcript name)",
    },
    project(
        "playwright-mcp.output",
        "Playwright MCP",
        ".playwright-mcp",
        false,
        "Console logs, page snapshots, screenshots and traces the Playwright MCP server wrote while an agent drove the browser. Nothing reads them again.",
        "Log console, snapshot trang, ảnh chụp và trace mà Playwright MCP ghi khi agent điều khiển trình duyệt. Không gì đọc lại chúng.",
        "playwright.dev/mcp (--output-dir default)",
    ),
    project(
        "agent.screenshots",
        "Agent workspace (.agent)",
        ".agent/screenshots",
        true,
        "Screenshots an agent took to check its work.",
        "Ảnh chụp màn hình agent chụp để kiểm tra kết quả.",
        "reference machine (.agent/screenshots, 115 MB)",
    ),
    project(
        "agent.tmp",
        "Agent workspace (.agent)",
        ".agent/tmp",
        true,
        "Temporary files of an agent.",
        "Tệp tạm của agent.",
        "reference machine",
    ),
    project(
        "agent.logs",
        "Agent workspace (.agent)",
        ".agent/logs",
        true,
        "Logs an agent wrote while working.",
        "Log agent ghi trong lúc làm việc.",
        "reference machine",
    ),
    project(
        "codex.project-log",
        "Codex",
        ".codex/log",
        false,
        "Codex's own logs when CODEX_HOME points into the project. Its sessions, history and memories next to it are kept.",
        "Log của Codex khi CODEX_HOME trỏ vào dự án. Phiên, lịch sử và memory bên cạnh được giữ nguyên.",
        "developers.openai.com/codex/config-basic",
    ),
    project(
        "aider.tags-cache",
        "Aider",
        ".aider.tags.cache.v*",
        false,
        "Aider's repo-map cache; rebuilt on the next run. The chat history files next to it are kept.",
        "Cache repo-map của Aider; được dựng lại ở lần chạy sau. Các tệp lịch sử chat bên cạnh được giữ nguyên.",
        "aider.chat (repo map)",
    ),
    project(
        "generic.test-results",
        "Test runs (agents)",
        "test-results",
        true,
        "Output of test runs (Playwright traces, videos, screenshots of failures). Re-created by the next run.",
        "Kết quả các lần chạy test (trace, video, ảnh lỗi của Playwright). Lần chạy sau sẽ tạo lại.",
        "playwright.dev/docs/test-configuration (outputDir)",
    ),
    project(
        "generic.playwright-report",
        "Test runs (agents)",
        "playwright-report",
        true,
        "The HTML report of the last Playwright run.",
        "Báo cáo HTML của lần chạy Playwright gần nhất.",
        "playwright.dev/docs/test-reporters",
    ),
    project(
        "generic.screenshots",
        "Agent outputs",
        "screenshots",
        true,
        "Screenshots left by agents or tests.",
        "Ảnh chụp do agent hoặc test để lại.",
        "reference machine",
    ),
    project(
        "generic.scratch",
        "Agent outputs",
        "scratch",
        true,
        "A scratch folder agents use for throw-away files.",
        "Thư mục nháp agent dùng cho tệp dùng một lần.",
        "reference machine",
    ),
    Entry {
        id: "playwright.old-browser",
        tool: "Playwright",
        scope: Scope::Global,
        kind: Kind::BrowserRevision,
        pattern: "%LOCALAPPDATA%\\ms-playwright\\<browser>-<revision>",
        level: RiskLevel::Safe,
        generic: false,
        impact: "An older revision of a browser Playwright downloaded; the newest of each browser stays. A project pinned to an older Playwright downloads it again with `npx playwright install`.",
        impact_vi: "Bản cũ của trình duyệt Playwright đã tải; bản mới nhất của mỗi trình duyệt được giữ. Dự án ghim Playwright cũ sẽ tải lại bằng `npx playwright install`.",
        verified_on: "2026-10-17",
        source: "playwright.dev/docs/browsers (managing browser binaries)",
    },
];

/// The catalog entry `id`.
#[must_use]
pub fn entry(id: &str) -> Option<&'static Entry> {
    CATALOG.iter().find(|e| e.id == id)
}

/// The tool whose conversation history a protected name (`AGENT_HISTORY_NAMES` or a
/// `folder` of `AGENT_HISTORY_SUFFIXES`) belongs to.
#[must_use]
pub fn history_tool(hit: &str) -> &'static str {
    let hit = hit.to_lowercase();
    if hit.starts_with(".aider") {
        "Aider"
    } else if hit.starts_with(".specstory") {
        "SpecStory"
    } else if hit.starts_with(".codex") {
        "Codex"
    } else if hit.starts_with(".claude") {
        "Claude Code"
    } else if hit.starts_with(".gemini") {
        "Gemini CLI"
    } else if hit.starts_with(".qwen") {
        "Qwen Code"
    } else if hit.starts_with(".continue") {
        "Continue"
    } else if hit.starts_with(".copilot") {
        "GitHub Copilot CLI"
    } else {
        "AI agent"
    }
}

/// `true` when a project entry's single-name `pattern` matches `name`.
#[must_use]
pub fn name_matches(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => {
            name.len() > prefix.len() && name.to_lowercase().starts_with(&prefix.to_lowercase())
        }
        None => name.eq_ignore_ascii_case(pattern),
    }
}

#[cfg(test)]
mod tests {
    use nfd_core::safety::is_protected_template;

    use super::*;

    #[test]
    fn catalog_is_consistent_and_never_reaches_history() {
        let mut ids: Vec<&str> = CATALOG.iter().map(|e| e.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), CATALOG.len(), "duplicate ids");
        for e in CATALOG {
            assert!(!e.impact.is_empty() && !e.impact_vi.is_empty(), "{}", e.id);
            assert_ne!(e.impact, e.impact_vi, "{}: not translated", e.id);
            assert_eq!(e.verified_on.len(), 10, "{}", e.id);
            assert!(e.level.is_deletable(), "{}", e.id);
            if e.scope == Scope::Project {
                let template = format!("${{HOME}}/proj/{}", e.pattern);
                assert!(!is_protected_template(&template), "{}", e.id);
            }
        }
    }

    #[test]
    fn names_match_case_insensitively_with_a_trailing_star() {
        assert!(name_matches(".aider.tags.cache.v*", ".aider.tags.cache.v4"));
        assert!(!name_matches(".aider.tags.cache.v*", ".aider.tags.cache.v"));
        assert!(name_matches("test-results", "Test-Results"));
        assert!(!name_matches("test-results", "test-results2"));
        assert_eq!(history_tool(".aider.chat.history.md"), "Aider");
        assert_eq!(history_tool(".codex"), "Codex");
    }
}
