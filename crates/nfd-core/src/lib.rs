//! NANOFreeDisk core library.
//!
//! UI-agnostic building blocks: the entry model and the [`scanner`] that produces it,
//! the [`RiskLevel`] vocabulary shared with rule packs, and the [`safety`] module that
//! owns the hard-coded list of locations the application must never delete.
//!
//! Phase 3 adds the [`plan`]ner and the SafetyGate ([`safety::gate`]); the executor
//! lives with the undo journal in `nfd-history`. See `docs/PHASE3_CLEAN.md`.

pub mod health;
pub mod insights;
pub mod model;
pub mod plan;
pub mod query;
pub mod risk;
pub mod safety;
pub mod scanner;
pub mod shortcut;
pub mod units;

pub use risk::RiskLevel;
