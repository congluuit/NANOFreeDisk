//! Property tests for the SafetyGate and the Planner (PHASE3_CLEAN §3):
//!
//! * every path generated below a protected root is refused, for hand-picked items
//!   and for every rule level, whatever the case, separators, `.` segments or trailing
//!   dots/spaces;
//! * rules never reach user data or anything below `%SystemRoot%` outside the
//!   allow-list;
//! * a plan never holds two nested items, and every eligible candidate is covered by
//!   an item of the plan.

use std::path::PathBuf;

use nfd_core::RiskLevel;
use nfd_core::plan::{self, DeleteMode, PlanItem, PlanOptions};
use nfd_core::safety::{
    CONTAINER_TEMPLATES, Origin, PROTECTED_TEMPLATES, SafetyGate, USER_DATA_TEMPLATES,
};
use proptest::prelude::*;

fn resolver(var: &str) -> Option<String> {
    let base = if cfg!(windows) { "C:" } else { "" };
    Some(match var {
        "SYSTEMROOT" => format!("{base}/Windows"),
        "SYSTEMDRIVE" => base.to_owned(),
        "HOME" | "USERPROFILE" => format!("{base}/Users/nano"),
        "APPDATA" => format!("{base}/Users/nano/AppData/Roaming"),
        "LOCALAPPDATA" => format!("{base}/Users/nano/AppData/Local"),
        "TEMP" => format!("{base}/Users/nano/AppData/Local/Temp"),
        "PROGRAMDATA" => format!("{base}/ProgramData"),
        "PROGRAMFILES" => format!("{base}/Program Files"),
        "PROGRAMFILES_X86" => format!("{base}/Program Files (x86)"),
        _ => return None,
    })
}

fn expand(template: &str) -> String {
    let rest = template.strip_prefix("${").expect("template");
    let (var, tail) = rest.split_once('}').expect("template");
    format!("{}{tail}", resolver(var).expect("known var"))
}

fn origins() -> impl Strategy<Value = Origin> {
    prop_oneof![
        Just(Origin::Manual),
        Just(Origin::Rule {
            level: RiskLevel::Safe
        }),
        Just(Origin::Rule {
            level: RiskLevel::Caution
        }),
        Just(Origin::Rule {
            level: RiskLevel::Advanced
        }),
        Just(Origin::Rule {
            level: RiskLevel::Protected
        }),
    ]
}

/// One path segment that stays where it is: no `..`, no `:`; random case;
/// sometimes `.`; on Windows sometimes trailing dots/spaces (ignored by Win32).
fn segment() -> impl Strategy<Value = String> {
    prop_oneof![
        8 => "[A-Za-z0-9_ -]{1,10}".prop_map(|s| s.trim().to_owned()).prop_filter("non-empty", |s| !s.is_empty()),
        1 => Just(".".to_owned()),
        1 => "[A-Za-z]{1,6}[. ]{1,3}".prop_map(|s| if cfg!(windows) { s } else { s.trim_end_matches(['.', ' ']).to_owned() }),
    ]
}

/// Re-render `base/segments` with random separators and random letter case.
fn render(base: &str, segs: &[String], flips: &[bool], backslash: &[bool]) -> PathBuf {
    let mut out = String::new();
    let parts: Vec<&str> = base
        .split('/')
        .chain(segs.iter().map(String::as_str))
        .collect();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            let bs = cfg!(windows) && backslash.get(i).copied().unwrap_or(false);
            out.push(if bs { '\\' } else { '/' });
        }
        // case only matters on Windows; keep the protected root's case on Unix
        let flip = cfg!(windows) && flips.get(i).copied().unwrap_or(false);
        if flip {
            out.push_str(&part.to_uppercase());
        } else {
            out.push_str(part);
        }
    }
    PathBuf::from(out)
}

fn gate() -> SafetyGate {
    SafetyGate::new(&resolver)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn everything_below_a_protected_root_is_refused(
        idx in 0..PROTECTED_TEMPLATES.len(),
        segs in prop::collection::vec(segment(), 0..6),
        flips in prop::collection::vec(any::<bool>(), 16),
        backslash in prop::collection::vec(any::<bool>(), 16),
        origin in origins(),
    ) {
        let template = PROTECTED_TEMPLATES[idx];
        let user_data = USER_DATA_TEMPLATES.contains(&template);
        // The user may pick items *inside* Documents & co.; the folder itself and
        // every rule stay refused.
        prop_assume!(!(user_data && origin == Origin::Manual && segs.iter().any(|s| s != ".")));
        let path = render(&expand(template), &segs, &flips, &backslash);
        prop_assert!(
            gate().check_path(&path, origin).is_err(),
            "{} accepted for {:?}", path.display(), origin
        );
    }

    #[test]
    fn containers_and_their_ancestors_are_never_deleted_whole(
        idx in 0..CONTAINER_TEMPLATES.len(),
        up in 0usize..3,
        origin in origins(),
    ) {
        let mut path = PathBuf::from(expand(CONTAINER_TEMPLATES[idx]));
        for _ in 0..up {
            if !path.pop() { break; }
        }
        prop_assert!(gate().check_path(&path, origin).is_err(), "{}", path.display());
    }

    #[test]
    fn rules_stay_out_of_the_windows_folder(
        first in "[A-Za-z0-9]{1,12}",
        segs in prop::collection::vec(segment(), 0..4),
        level in prop_oneof![Just(RiskLevel::Safe), Just(RiskLevel::Caution), Just(RiskLevel::Advanced)],
    ) {
        let lower = first.to_lowercase();
        prop_assume!(!["temp", "logs", "minidump", "livekernelreports", "softwaredistribution"].contains(&lower.as_str()));
        let mut all = vec![first];
        all.extend(segs);
        let path = render(&expand("${SYSTEMROOT}"), &all, &[], &[]);
        prop_assert!(gate().check_path(&path, Origin::Rule { level }).is_err(), "{}", path.display());
        prop_assert!(gate().check_path(&path, Origin::Manual).is_err());
    }

    #[test]
    fn dotdot_cannot_climb_out_of_temp_into_protected(
        depth in 1usize..4,
        target in 0..PROTECTED_TEMPLATES.len(),
    ) {
        // ${TEMP}/a/../../.. lands at ${HOME}-level and climbs back down into a
        // protected root: still refused for rules.
        let temp = expand("${TEMP}");
        let protected = expand(PROTECTED_TEMPLATES[target]);
        let mut s = temp.clone();
        for _ in 0..depth { s.push_str("/x"); }
        for _ in 0..(depth + temp.matches('/').count()) { s.push_str("/.."); }
        let root_len = if cfg!(windows) { 2 } else { 0 };
        s.push_str(&protected[root_len..]);
        let path = PathBuf::from(&s);
        prop_assert!(
            gate().check_path(&path, Origin::Rule { level: RiskLevel::Safe }).is_err(),
            "{s}"
        );
    }

    #[test]
    fn plans_never_hold_nested_items(
        picks in prop::collection::vec(prop::collection::vec(0u8..4, 1..5), 1..24),
        levels in prop::collection::vec(0u8..4, 24),
    ) {
        let base = if cfg!(windows) { "C:/junk" } else { "/junk" };
        let names = ["a", "b", "a b", "A"];
        let candidates: Vec<PlanItem> = picks
            .iter()
            .enumerate()
            .map(|(n, segs)| {
                let mut path = PathBuf::from(base);
                for s in segs { path.push(names[*s as usize]); }
                let level = match levels[n] {
                    0 => RiskLevel::Safe,
                    1 => RiskLevel::Caution,
                    2 => RiskLevel::Advanced,
                    _ => RiskLevel::Protected,
                };
                PlanItem {
                    path,
                    origin: Origin::Rule { level },
                    rule_id: None,
                    is_dir: true,
                    bytes: 1,
                    files: 1,
                    size: 0,
                    mtime: 0,
                    requires_closed: Vec::new(),
                    mode: DeleteMode::Quarantine,
                    selected: false,
        keeper: None,
                }
            })
            .collect();
        let built = plan::build(candidates.clone(), PlanOptions { expert: true, include_caution: true, permanent: false });
        // Same as `safety::comparable`: case-insensitive on every OS (Windows
        // semantics), so `a` and `A` are one item on Unix too.
        let key = |p: &PathBuf| p.to_string_lossy().replace('\\', "/").to_lowercase();
        let inside = |inner: &str, outer: &str| inner == outer || inner.starts_with(&format!("{outer}/"));
        for (i, a) in built.items.iter().enumerate() {
            prop_assert!(a.level() != RiskLevel::Protected);
            for (j, b) in built.items.iter().enumerate() {
                if i != j {
                    prop_assert!(!inside(&key(&a.path), &key(&b.path)), "{:?} inside {:?}", a.path, b.path);
                }
            }
        }
        for c in candidates.iter().filter(|c| c.level() != RiskLevel::Protected) {
            prop_assert!(
                built.items.iter().any(|i| inside(&key(&c.path), &key(&i.path))),
                "{:?} lost", c.path
            );
        }
    }
}
