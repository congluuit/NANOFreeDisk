//! Dev Workspace (PLAN §13.2 group C, `docs/PHASE4_DEVCACHE.md`).
//!
//! * [`devcache`] — the DevCache rule packs grouped **vendor → app → folder**, with the
//!   🔒 Keep list and per-vendor thresholds (C1).
//! * [`unknown`] — cache-like folders no rule knows yet, marked 🟡 "unverified" (C1).
//!
//! Nothing here deletes anything: modules either produce
//! [`PlanItem`](nfd_core::plan::PlanItem)s for the Phase 3 Planner → SafetyGate →
//! Executor → UndoJournal path, or describe a command the user may run.

pub mod devcache;
pub mod fsutil;
pub mod unknown;

/// Lower-case, `/`-separated form of a path for prefix comparisons.
#[must_use]
pub fn norm(path: &str) -> String {
    let mut s = path.replace('\\', "/").to_lowercase();
    while s.ends_with('/') && s.len() > 1 {
        s.pop();
    }
    s
}

/// The literal part of an expanded rule path before its first glob ([`norm`]alized).
#[must_use]
pub fn literal_base(expanded: &str) -> String {
    let text = expanded.replace('\\', "/");
    let comps: Vec<&str> = text
        .split('/')
        .take_while(|c| !c.contains(['*', '?', '[', '{']))
        .collect();
    norm(&comps.join("/"))
}

/// `true` when `inner` equals `outer` or lies inside it (both [`norm`]alized).
#[must_use]
pub fn within(inner: &str, outer: &str) -> bool {
    inner == outer
        || (inner.len() > outer.len()
            && inner.starts_with(outer)
            && inner.as_bytes()[outer.len()] == b'/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn within_compares_whole_components() {
        assert!(within("c:/a/b", "c:/a"));
        assert!(within("c:/a", "c:/a"));
        assert!(!within("c:/ab", "c:/a"));
        assert_eq!(norm("C:\\Users\\X\\"), "c:/users/x");
    }
}
