//! `nfd browsers sites | clean-site <host>`: the cache each website holds in each
//! browser profile, and removing only one site's entries (dry run unless --apply).
//! Never cookies, logins, Local Storage, IndexedDB, History, passwords or
//! extensions; the blockfile HTTP cache is listed but never edited (see
//! `nfd_browsers`).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Subcommand};
use nfd_browsers::{Env, Report, ScanOptions, StoreKind, Target, site};
use nfd_core::exec::ItemStatus;
use nfd_core::units::format_size;
use serde_json::json;

/// JSON schema id of `nfd browsers … --json`.
pub const SCHEMA: &str = "nanofreedisk.browsers.v1";

#[derive(Args, Debug)]
pub struct BrowsersArgs {
    #[command(subcommand)]
    pub action: BrowsersAction,
}

#[derive(Subcommand, Debug)]
pub enum BrowsersAction {
    /// Websites in each browser profile's caches, largest first (read-only).
    Sites(SitesArgs),
    /// Remove one website's cache entries (Code Cache, CacheStorage, Firefox cache2;
    /// dry run unless --apply). The browser must be closed.
    CleanSite(CleanSiteArgs),
}

#[derive(Args, Debug, Clone, Default)]
pub struct Common {
    /// Only this browser: chrome, edge, brave, vivaldi, opera, firefox.
    #[arg(long)]
    pub browser: Option<String>,
    #[arg(long)]
    pub json: bool,
    /// Read a fixture laid out as <dir>/Local, <dir>/Roaming (tests only).
    #[arg(long, hide = true)]
    pub fixture: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct SitesArgs {
    #[command(flatten)]
    pub common: Common,
    /// Only sites containing this text.
    #[arg(long)]
    pub search: Option<String>,
    /// Sites shown per profile.
    #[arg(long, default_value_t = 30)]
    pub top: usize,
}

#[derive(Args, Debug)]
pub struct CleanSiteArgs {
    /// Website (host or URL), e.g. youtube.com.
    pub host: String,
    #[command(flatten)]
    pub common: Common,
    /// Only this host, not its subdomains.
    #[arg(long)]
    pub exact: bool,
    /// Clean for real (quarantine, restorable for 7 days).
    #[arg(long)]
    pub apply: bool,
    #[arg(long, short = 'y')]
    pub yes: bool,
    /// Undo journal / history database [default: the desktop app's].
    #[arg(long)]
    pub db: Option<PathBuf>,
}

fn env(c: &Common) -> Env {
    c.fixture.as_deref().map_or_else(Env::detect, Env::fixture)
}

fn check_browser(c: &Common) -> anyhow::Result<()> {
    if let Some(b) = &c.browser {
        if nfd_browsers::browser(b).is_none() {
            let known: Vec<&str> = nfd_browsers::BROWSERS.iter().map(|b| b.id).collect();
            anyhow::bail!("unknown browser `{b}` (known: {})", known.join(", "));
        }
    }
    Ok(())
}

const fn store_name(k: StoreKind) -> &'static str {
    match k {
        StoreKind::HttpCache => "http",
        StoreKind::CodeCacheJs => "js",
        StoreKind::CodeCacheWasm => "wasm",
        StoreKind::CacheStorage => "sw",
        StoreKind::FirefoxCache => "cache2",
    }
}

fn print(report: &Report, search: Option<&str>, top: usize) {
    if report.browsers.is_empty() {
        println!("No browser caches found.");
        return;
    }
    for b in &report.browsers {
        println!(
            "{}{}",
            b.name,
            if b.running {
                format!("  (running: close {} before cleaning)", b.exe)
            } else {
                String::new()
            }
        );
        for p in &b.profiles {
            println!("  {} — {}", p.name, p.path.display());
            for s in &p.stores {
                println!(
                    "    {:<6} {:>10}  {} entries{}{}",
                    store_name(s.kind),
                    format_size(s.bytes),
                    s.entries,
                    if s.cleanable {
                        ""
                    } else {
                        "  (listed only: blockfile)"
                    },
                    s.error
                        .as_deref()
                        .map(|e| format!("  error: {e}"))
                        .unwrap_or_default()
                );
            }
            let sites: Vec<_> = p
                .sites
                .iter()
                .filter(|s| search.is_none_or(|q| s.site.contains(&q.to_lowercase())))
                .collect();
            for s in sites.iter().take(top) {
                let stores: Vec<&str> = s.stores.iter().map(|k| store_name(*k)).collect();
                println!(
                    "    {:>10}  {:>10} cleanable  {}  [{}]",
                    format_size(s.bytes),
                    format_size(s.cleanable_bytes),
                    s.site,
                    stores.join(",")
                );
            }
            if sites.len() > top {
                println!("    … {} more", sites.len() - top);
            }
        }
    }
}

/// `nfd browsers`.
pub fn run(args: BrowsersArgs) -> anyhow::Result<ExitCode> {
    match args.action {
        BrowsersAction::Sites(a) => sites(&a),
        BrowsersAction::CleanSite(a) => clean_site(&a),
    }
}

fn sites(a: &SitesArgs) -> anyhow::Result<ExitCode> {
    check_browser(&a.common)?;
    let report = nfd_browsers::scan(
        &env(&a.common),
        &ScanOptions {
            browser: a.common.browser.clone(),
            ..ScanOptions::default()
        },
    );
    if a.common.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "schema": SCHEMA, "report": report }))?
        );
    } else {
        print(&report, a.search.as_deref(), a.top);
        println!(
            "Clean one site with `nfd browsers clean-site <host> --apply` (close the browser first)."
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn clean_site(a: &CleanSiteArgs) -> anyhow::Result<ExitCode> {
    check_browser(&a.common)?;
    let host = site::normalize_host(&a.host)
        .ok_or_else(|| anyhow::anyhow!("`{}` is not a website host", a.host))?;
    let env = env(&a.common);
    let targets: Vec<Target> = nfd_browsers::BROWSERS
        .iter()
        .filter(|b| {
            a.common
                .browser
                .as_deref()
                .is_none_or(|id| b.id.eq_ignore_ascii_case(id))
        })
        .map(|b| Target {
            browser: b.id.into(),
            profile: None,
            site: host.clone(),
            subdomains: !a.exact,
        })
        .collect();
    let p = nfd_browsers::plan(&env, &targets, ScanOptions::default().max_entries);
    if a.common.json && !a.apply {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &json!({ "schema": SCHEMA, "dryRun": true, "site": host, "plan": p })
            )?
        );
        return Ok(ExitCode::SUCCESS);
    }
    for exe in &p.running {
        eprintln!("{exe} is running — close it first; nothing is cleaned in that browser.");
    }
    for r in &p.refused {
        eprintln!("not planned ({}): {}", r.reason, r.path.display());
    }
    if p.listed_only > 0 && !a.common.json {
        println!(
            "{} of {host} sits in the blockfile HTTP cache: it is listed only and stays (that cache is cleaned only as a whole: `nfd clean --rule browser.<browser>.cache`).",
            format_size(p.listed_only)
        );
    }
    if p.plan.items.is_empty() {
        println!("Nothing to clean for {host}.");
        return Ok(if p.running.is_empty() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        });
    }
    if !a.apply {
        println!(
            "Dry run: would move {} in {} cache entr{} of {host} to the quarantine ({}{}). Add --apply to do it.",
            format_size(p.bytes),
            p.entries,
            if p.entries == 1 { "y" } else { "ies" },
            p.by_store
                .iter()
                .map(|s| format!("{} {}", store_name(s.kind), s.entries))
                .collect::<Vec<_>>()
                .join(", "),
            if p.index_files > 0 {
                format!(
                    "; {} index file(s) go too, the browser rebuilds them",
                    p.index_files
                )
            } else {
                String::new()
            }
        );
        return Ok(ExitCode::SUCCESS);
    }
    if !crate::quarantine::confirm(&p.plan, a.yes)? {
        return Ok(ExitCode::FAILURE);
    }
    // the browser could have been started since the plan was made
    let running = nfd_browsers::Env {
        running: if a.common.fixture.is_some() {
            env.running.clone()
        } else {
            Env::detect().running
        },
        ..env.clone()
    };
    let (op, journal_ok) = crate::quarantine::execute_verified(
        &p.plan,
        "browser-site",
        a.db.clone(),
        a.common.json,
        &mut |item| match item
            .requires_closed
            .iter()
            .find(|exe| running.is_running(exe))
        {
            Some(exe) => Err(nfd_core::exec::Refusal {
                code: "app-running",
                message: format!("{exe} is running"),
            }),
            None => Ok(()),
        },
    );
    if a.common.json {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &json!({ "schema": SCHEMA, "site": host, "operation": op, "journal": journal_ok })
            )?
        );
    } else {
        crate::quarantine::print_outcome(&op, journal_ok);
    }
    Ok(if op.count(ItemStatus::Failed) == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
