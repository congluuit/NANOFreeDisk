//! Programs (PLAN §13.2 E1, `docs/UNINSTALLER.md`): the uninstaller and the
//! software junk cleaner.
//!
//! * [`inventory`] — installed programs: the registry's `Uninstall` keys and the
//!   Store packages, as "Apps & features" lists them.
//! * [`icons`] — each program's icon (read as data, local drives only).
//! * [`command`] — the vendor's uninstall command, split and checked.
//! * [`leftovers`] — what an uninstalled program left behind: folders, shortcuts,
//!   registry keys and values, each with the evidence that ties it to the program.
//! * [`junk`] — logs, caches, crash dumps and old updates of installed programs.
//! * [`remove`] — carrying out a leftover removal: files through Planner →
//!   SafetyGate → Executor (quarantine), registry keys after a `.reg` backup;
//!   all-users items in one elevated run ([`elevated`]).
//! * [`runner`] — starting the vendor's uninstaller and waiting for it (Windows).
//!
//! Nothing here deletes a file outside the Phase 3 path, and nothing is gone for
//! good: files go to the quarantine or the Recycle Bin, registry keys to `.reg`
//! backups.

pub mod command;
pub mod elevated;
pub mod fsutil;
pub mod icons;
pub mod inventory;
pub mod junk;
pub mod leftovers;
pub mod names;
pub mod registry;
pub mod remove;
pub mod runner;

use serde::{Deserialize, Serialize};

pub use inventory::Program;

/// Who a program, a leftover or a junk item belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scope {
    /// This user (their profile, `HKEY_CURRENT_USER`).
    User,
    /// Every user of the machine (Program Files, ProgramData, `HKEY_LOCAL_MACHINE`):
    /// changing it needs administrator rights.
    Machine,
}

/// How a program was installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    /// A setup program registered in `Uninstall`.
    Win32,
    /// Windows Installer (`msiexec`).
    Msi,
    /// A Store / MSIX package.
    Store,
}
