//! Fast mode: read the NTFS `$MFT` directly through a volume handle (`\\.\C:`) and
//! build the tree from file reference numbers — 10–30× faster than a directory walk
//! on volumes with millions of files (`docs/PLAN.md` §4.1).
//!
//! **Not implemented yet.** This module only exists behind the `mft` cargo feature so
//! the API and the fallback path are in place; [`scan`] always reports
//! [`MftUnavailable::NotImplemented`] and [`super::scan`] falls back to standard mode.

use std::path::Path;

use thiserror::Error;

use super::{ScanOptions, ScanTree};

/// Why the MFT reader cannot be used for a scan (the caller falls back to standard).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MftUnavailable {
    #[error("the NTFS MFT reader is not implemented yet")]
    NotImplemented,
}

/// Scan `root` by reading the volume's `$MFT`.
///
/// # Errors
/// Always [`MftUnavailable::NotImplemented`] for now.
pub fn scan(_root: &Path, _options: &ScanOptions) -> Result<ScanTree, MftUnavailable> {
    Err(MftUnavailable::NotImplemented)
}
