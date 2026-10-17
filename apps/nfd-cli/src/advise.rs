//! `nfd advise` — scan a folder and say, for what it holds, what can be deleted
//! safely, what needs care, what only the proper tool may shrink and what must stay,
//! with what happens if it is deleted (read-only; the same verdicts as the app's
//! Analyze view). `nfd scan --advice` prints the same after the scan tables.

use std::io::{self, Write};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Context;
use clap::Args;
use nfd_advice::{Action, Advice, Group, Knowledge, Reason, Verdict};
use nfd_core::model::{ChildIndex, subtree_ends};
use nfd_core::safety::SafetyGate;
use nfd_core::scanner::{ScanOptions, ScanTree};
use nfd_core::units::format_size;
use nfd_devtools::ai_leftovers::{self, Sources};
use nfd_devtools::process;
use nfd_platform::Platform;
use serde_json::json;

const SCHEMA: &str = "nanofreedisk.advice.v1";

#[derive(Args, Debug)]
pub struct AdviseArgs {
    /// Folder or drive to scan and judge (e.g. D:\ or %USERPROFILE%).
    path: PathBuf,

    /// Print JSON (schema `nanofreedisk.advice.v1`, see docs/schema/).
    #[arg(long)]
    json: bool,

    /// Language of the explanations: en or vi.
    #[arg(long, default_value = "en")]
    lang: String,

    /// Suggestions shown per section.
    #[arg(long, default_value_t = 12, value_name = "N")]
    top: usize,

    /// Skip the git / session check of AI agent worktrees: they stay "needs a check".
    #[arg(long)]
    no_git: bool,

    /// Rule packs folder [default: built-in].
    #[arg(long, value_name = "DIR")]
    rules: Option<PathBuf>,

    /// Worker threads for the scan [default: number of logical CPUs].
    #[arg(long, value_name = "N")]
    threads: Option<NonZeroUsize>,
}

/// How to judge and print.
#[derive(Debug, Clone)]
pub struct Options {
    pub lang: String,
    pub top: usize,
    pub git: bool,
    pub rules: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            lang: "en".into(),
            top: 12,
            git: true,
            rules: None,
        }
    }
}

pub fn run(args: AdviseArgs) -> anyhow::Result<ExitCode> {
    let path = std::path::absolute(&args.path).unwrap_or_else(|_| args.path.clone());
    let tree = crate::scan::scan_path(
        &path,
        ScanOptions {
            threads: args.threads,
            ..ScanOptions::default()
        },
    )?;
    let opts = Options {
        lang: args.lang,
        top: args.top,
        git: !args.no_git,
        rules: args.rules,
    };
    if args.json {
        let (advice, _, _) = judge(&tree, &opts)?;
        let s = advice.suggestions(&tree);
        let out = json!({
            "schema": SCHEMA,
            "version": env!("CARGO_PKG_VERSION"),
            "root": tree.root,
            "scannedAt": tree.started_at,
            "totals": s.totals,
            "reclaim": s.reclaim,
            "enriched": s.enriched,
            "groups": s.groups,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(ExitCode::SUCCESS);
    }
    print_for(&tree, &opts)?;
    Ok(ExitCode::SUCCESS)
}

/// The advice of `tree`, refined by the AI leftovers check when `opts.git`.
fn judge(tree: &ScanTree, opts: &Options) -> anyhow::Result<(Advice, ChildIndex, Vec<u32>)> {
    let platform = nfd_platform::Native;
    let resolve = platform.resolver();
    let k = match &opts.rules {
        Some(dir) => {
            let packs = nfd_rules::load_dir(dir).context("loading the rule packs")?;
            Knowledge::new(
                &packs,
                nfd_rules::encyclopedia::builtin(),
                &resolve,
                nfd_advice::current_platform(),
            )
        }
        None => Knowledge::builtin(&resolve).context("loading the built-in rule packs")?,
    };
    let children = ChildIndex::build(&tree.entries);
    let ends = subtree_ends(&tree.entries);
    let now = nfd_rules::matcher::now();
    let mut advice = nfd_advice::advise(tree, &children, &ends, &k, now);
    if opts.git && !advice.agent_projects.is_empty() {
        eprintln!(
            "checking {} project(s) with AI agent worktrees with git…",
            advice.agent_projects.len()
        );
        let env = |k: &str| std::env::var(k).ok();
        let sources = Sources::detect(&resolve, &env);
        let gate = SafetyGate::new(&resolve);
        let report = ai_leftovers::scan(
            &ai_leftovers::Options {
                roots: advice.agent_projects.clone(),
                ..ai_leftovers::Options::default()
            },
            &sources,
            process::git().as_deref(),
            &gate,
            None,
        );
        advice.apply_ai(tree, &children, &ends, &report);
    }
    Ok((advice, children, ends))
}

/// Print the advice of `tree` as text.
pub fn print_for(tree: &ScanTree, opts: &Options) -> anyhow::Result<()> {
    let (advice, _, _) = judge(tree, opts)?;
    let s = advice.suggestions(tree);
    let vi = opts.lang.eq_ignore_ascii_case("vi");
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let t = s.totals;
    let line = |en: &str, vi_text: &str| {
        if vi {
            vi_text.to_owned()
        } else {
            en.to_owned()
        }
    };
    writeln!(out)?;
    writeln!(
        out,
        "{} {}",
        line("Advice for", "Gợi ý cho"),
        tree.root.display()
    )?;
    writeln!(
        out,
        "  {}: {}  ({} {} · {} {})",
        line("Can be freed", "Có thể giải phóng"),
        format_size(s.reclaim),
        line("safe", "an toàn"),
        format_size(t.safe),
        line("needs care", "thận trọng"),
        format_size(t.caution),
    )?;
    writeln!(
        out,
        "  {} {} · {} {} · {} {}",
        line("Guide only", "Chỉ hướng dẫn"),
        format_size(t.guide),
        line("Keep", "Không được xóa"),
        format_size(t.keep),
        line("Unknown", "Chưa rõ"),
        format_size(t.unknown),
    )?;
    if !s.enriched
        && advice
            .units
            .iter()
            .any(|u| u.blocked.as_deref() == Some("git-check"))
    {
        writeln!(
            out,
            "  {}",
            line(
                "AI agent worktrees were not checked with git (--no-git).",
                "Worktree của AI agent chưa được kiểm tra bằng git (--no-git)."
            )
        )?;
    }
    let sections: [(&[Verdict], &str, &str); 4] = [
        (&[Verdict::Safe], "Safe to delete", "Có thể xóa an toàn"),
        (&[Verdict::Caution], "Needs care", "Thận trọng"),
        (
            &[Verdict::Guide],
            "Only with the proper tool",
            "Chỉ hướng dẫn",
        ),
        (
            &[Verdict::Keep, Verdict::Unknown],
            "Big but keep",
            "Lớn nhưng nên giữ",
        ),
    ];
    for (verdicts, en, vi_title) in sections {
        let groups: Vec<&Group> = s
            .groups
            .iter()
            .filter(|g| verdicts.contains(&g.verdict))
            .filter(|g| g.verdict != Verdict::Unknown || g.reason == Reason::Personal)
            .take(opts.top)
            .collect();
        if groups.is_empty() {
            continue;
        }
        writeln!(out, "\n{}", if vi { vi_title } else { en })?;
        for g in groups {
            let where_ = g.location.as_deref().unwrap_or("");
            let count = if g.count > 1 {
                format!(" ({})", g.count)
            } else {
                String::new()
            };
            writeln!(
                out,
                "  {:>10}  {}{}{}",
                format_size(g.bytes),
                label(g, vi),
                if where_.is_empty() {
                    String::new()
                } else {
                    format!(" — {where_}")
                },
                count
            )?;
            writeln!(
                out,
                "              {}",
                g.impact.get(&opts.lang).replace('`', "")
            )?;
            if let Some(b) = &g.blocked {
                writeln!(out, "              [{}: {b}]", line("not now", "chưa thể"))?;
            } else if let Some(how) = how(g, vi, &tree.root) {
                writeln!(out, "              → {how}")?;
            }
            if g.hard {
                writeln!(
                    out,
                    "              🔒 {}",
                    line(
                        "hard protection: never deleted",
                        "bảo vệ cứng: không bao giờ bị xóa"
                    )
                )?;
            } else if g.overridable {
                writeln!(
                    out,
                    "              {}",
                    line(
                        "only the app's judgment: \"Delete anyway…\" in Analyze moves it to the quarantine",
                        "chỉ là đánh giá của ứng dụng: \"Vẫn xóa…\" trong Phân tích sẽ chuyển vào khu cách ly"
                    )
                )?;
            }
        }
    }
    writeln!(
        out,
        "\n{}",
        line(
            "Nothing was deleted. Clean from the app's Analyze view, or with `nfd clean`, `nfd repos clean`, `nfd ai-leftovers clean`.",
            "Chưa xóa gì. Dọn trong màn hình Phân tích của ứng dụng, hoặc bằng `nfd clean`, `nfd repos clean`, `nfd ai-leftovers clean`."
        )
    )?;
    Ok(())
}

/// Short name of a group.
fn label(g: &Group, vi: bool) -> String {
    let app = g.app.clone().unwrap_or_default();
    let src = g.source.clone().unwrap_or_default();
    let pick = |en: String, vi_text: String| if vi { vi_text } else { en };
    match g.reason {
        Reason::Rule => pick(format!("{app} ({src})"), format!("{app} ({src})")),
        Reason::KeepRule => pick(format!("{app} (🔒 {src})"), format!("{app} (🔒 {src})")),
        Reason::BuildOutput => pick(
            format!("Build output ({src})"),
            format!("Kết quả build ({src})"),
        ),
        Reason::AgentWorktree => pick("AI agent worktrees".into(), "Worktree của AI agent".into()),
        Reason::AgentLeftover => pick(format!("{app} leftovers"), format!("Phần thừa của {app}")),
        Reason::AgentHistory => pick(
            format!("{app} conversation history"),
            format!("Lịch sử hội thoại {app}"),
        ),
        Reason::System => pick("Windows".into(), "Windows".into()),
        Reason::Programs => pick("Installed programs".into(), "Chương trình đã cài".into()),
        Reason::Protected => pick(format!("Protected ({src})"), format!("Được bảo vệ ({src})")),
        Reason::Personal => pick("Personal media".into(), "Ảnh, video, nhạc cá nhân".into()),
        Reason::Explained => g
            .title
            .as_ref()
            .map_or(src, |t| t.get(if vi { "vi" } else { "en" }).to_owned()),
        Reason::OldLarge => pick("Old large files".into(), "Tệp lớn và cũ".into()),
        Reason::OldInstaller => pick(
            "Old installers in Downloads".into(),
            "Bộ cài cũ trong Downloads".into(),
        ),
        Reason::Duplicate => pick("Duplicate copies".into(), "Bản sao trùng lặp".into()),
    }
}

/// The right way to free a group.
fn how(g: &Group, vi: bool, root: &Path) -> Option<String> {
    let pick = |en: &str, vi_text: &str| Some(if vi { vi_text } else { en }.to_owned());
    match g.action {
        Action::Clean => pick(
            "clean with the rule, into the quarantine (7 days to restore)",
            "dọn theo quy tắc, vào khu cách ly (7 ngày để khôi phục)",
        ),
        Action::Quarantine => pick(
            "move into the quarantine (7 days to restore)",
            "chuyển vào khu cách ly (7 ngày để khôi phục)",
        ),
        Action::AiLeftovers => pick(
            "`git worktree remove` / quarantine (nfd ai-leftovers clean)",
            "`git worktree remove` / quarantine (nfd ai-leftovers clean)",
        ),
        Action::RecycleBin => pick("Recycle Bin, by hand", "Thùng rác, tự chọn"),
        Action::Store => Some(format!(
            "{} ({})",
            if vi {
                "lệnh của trình quản lý gói: nfd stores"
            } else {
                "the package manager's own command: nfd stores"
            },
            g.store.as_deref().unwrap_or_default()
        )),
        Action::Duplicates => pick("review in nfd dupes", "xem lại bằng nfd dupes"),
        Action::Guide => Some(format!(
            "{}: nfd explain \"{}\"",
            if vi {
                "chỉ hướng dẫn"
            } else {
                "guide only"
            },
            root.join(g.location.as_deref().unwrap_or_default())
                .display()
        )),
        Action::None => None,
    }
}
