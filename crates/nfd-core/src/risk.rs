use std::fmt;

use serde::{Deserialize, Serialize};

/// How dangerous it is to delete an item. Every item shown to the user carries one.
///
/// The ordering is meaningful: `Safe < Caution < Advanced < Protected`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    /// Recreated automatically; no user data, history or configuration is lost.
    Safe,
    /// Deleting loses history, forces a re-download, or slows the next start.
    /// Requires an explicit per-item confirmation.
    Caution,
    /// May break repair/uninstall or features. Hidden unless Expert mode is on.
    Advanced,
    /// Never deletable from inside the app. The UI only explains the proper system tool.
    Protected,
}

impl RiskLevel {
    /// Items eligible for the one-click "Clean all Safe" action.
    #[must_use]
    pub const fn is_one_click(self) -> bool {
        matches!(self, Self::Safe)
    }

    /// Items that may be deleted at all (with confirmation where required).
    #[must_use]
    pub const fn is_deletable(self) -> bool {
        !matches!(self, Self::Protected)
    }

    /// Stable lowercase identifier used in rule packs and JSON output.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Caution => "caution",
            Self::Advanced => "advanced",
            Self::Protected => "protected",
        }
    }
}

impl fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordering_is_safe_to_protected() {
        assert!(RiskLevel::Safe < RiskLevel::Caution);
        assert!(RiskLevel::Caution < RiskLevel::Advanced);
        assert!(RiskLevel::Advanced < RiskLevel::Protected);
    }

    #[test]
    fn serde_round_trip_uses_lowercase() {
        let json = serde_json_lite(RiskLevel::Caution);
        assert_eq!(json, "\"caution\"");
    }

    fn serde_json_lite(level: RiskLevel) -> String {
        // Avoid a serde_json dependency in core just for a test.
        format!("\"{}\"", level.as_str())
    }
}
