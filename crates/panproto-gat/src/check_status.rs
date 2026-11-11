//! The three-valued verdict of a checking report.
//!
//! A check either established that its subject passed, established that it
//! failed, or did not complete. Collapsing the third case into either of the
//! first two is what lets an unchecked subject read as a checked one, so every
//! checking report carries a [`CheckStatus`] rather than a `bool` or an empty
//! list of violations.
//!
//! ```
//! use panproto_gat::CheckStatus;
//!
//! let checks = [CheckStatus::Passed, CheckStatus::Incomplete, CheckStatus::Passed];
//! assert_eq!(CheckStatus::all(checks), CheckStatus::Incomplete);
//! assert_eq!(
//!     CheckStatus::Incomplete.and(CheckStatus::Failed),
//!     CheckStatus::Failed,
//! );
//! assert_eq!(CheckStatus::all([]), CheckStatus::Passed);
//! ```

use serde::{Deserialize, Serialize};

/// The verdict of a check: passed, failed, or did not complete.
///
/// Serialized in `snake_case` (`"passed"`, `"failed"`, `"incomplete"`).
///
/// Verdicts combine with [`CheckStatus::and`], which is Kleene's strong
/// conjunction with `Incomplete` read as "undefined" (Stanford Encyclopedia of
/// Philosophy 2026, "Many-Valued Logic", stated): a failure anywhere decides
/// the conjunction, an incomplete check otherwise leaves it undecided, and
/// only checks that all passed make it pass. [`CheckStatus::all`] folds a
/// sequence of verdicts with this conjunction, starting from `Passed`.
///
/// The derived [`Ord`] is the declaration order (`Passed < Failed <
/// Incomplete`) and exists so the type can key ordered maps and sort
/// deterministically. It is not the aggregation order: taking the maximum
/// or minimum of verdicts does not compute their conjunction, since the
/// conjunction of `Failed` and `Incomplete` is `Failed`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    /// Every check that ran completed and none found a violation.
    Passed,
    /// At least one check completed and found a violation.
    Failed,
    /// No check found a violation, but at least one did not complete.
    Incomplete,
}

impl CheckStatus {
    /// Kleene strong conjunction of two verdicts.
    ///
    /// `Failed` if either is `Failed`; otherwise `Incomplete` if either is
    /// `Incomplete`; otherwise `Passed`.
    ///
    /// ```
    /// use panproto_gat::CheckStatus::{Failed, Incomplete, Passed};
    ///
    /// assert_eq!(Passed.and(Passed), Passed);
    /// assert_eq!(Passed.and(Incomplete), Incomplete);
    /// assert_eq!(Incomplete.and(Failed), Failed);
    /// ```
    #[must_use]
    pub const fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::Failed, _) | (_, Self::Failed) => Self::Failed,
            (Self::Incomplete, _) | (_, Self::Incomplete) => Self::Incomplete,
            (Self::Passed, Self::Passed) => Self::Passed,
        }
    }

    /// The conjunction of every verdict in `statuses`, with unit `Passed`.
    ///
    /// An empty sequence yields `Passed`, since no check that ran found
    /// anything; a report states which checks ran, and folding their
    /// verdicts through this function is the only way it arrives at a
    /// pass.
    ///
    /// ```
    /// use panproto_gat::CheckStatus::{self, Failed, Incomplete, Passed};
    ///
    /// assert_eq!(CheckStatus::all([Passed, Failed, Incomplete]), Failed);
    /// ```
    #[must_use]
    pub fn all<I>(statuses: I) -> Self
    where
        I: IntoIterator<Item = Self>,
    {
        statuses.into_iter().fold(Self::Passed, Self::and)
    }

    /// Whether this verdict is [`CheckStatus::Passed`].
    #[must_use]
    pub const fn is_passed(self) -> bool {
        matches!(self, Self::Passed)
    }
}

#[cfg(test)]
mod tests {
    use super::CheckStatus::{self, Failed, Incomplete, Passed};

    #[test]
    fn check_status_serde_names() -> Result<(), serde_json::Error> {
        let all = [Passed, Failed, Incomplete];
        assert_eq!(
            serde_json::to_string(&all)?,
            r#"["passed","failed","incomplete"]"#
        );
        let back: Vec<CheckStatus> = serde_json::from_str(r#"["passed","failed","incomplete"]"#)?;
        assert_eq!(back, all);
        assert!(serde_json::from_str::<CheckStatus>(r#""Passed""#).is_err());
        Ok(())
    }

    #[test]
    fn check_status_and_is_kleene_strong_conjunction() {
        let table = [
            (Passed, Passed, Passed),
            (Passed, Failed, Failed),
            (Passed, Incomplete, Incomplete),
            (Failed, Passed, Failed),
            (Failed, Failed, Failed),
            (Failed, Incomplete, Failed),
            (Incomplete, Passed, Incomplete),
            (Incomplete, Failed, Failed),
            (Incomplete, Incomplete, Incomplete),
        ];
        for (a, b, expected) in table {
            assert_eq!(a.and(b), expected, "{a:?} and {b:?}");
        }
    }

    #[test]
    fn check_status_all_folds_and_with_unit_passed() {
        assert_eq!(CheckStatus::all([]), Passed);
        for s in [Passed, Failed, Incomplete] {
            assert_eq!(CheckStatus::all([s]), s);
        }
        assert_eq!(CheckStatus::all([Passed, Incomplete, Passed]), Incomplete);
        assert_eq!(CheckStatus::all([Incomplete, Failed, Passed]), Failed);
        assert_eq!(CheckStatus::all([Passed, Passed]), Passed);
        let xs = [Incomplete, Passed, Failed, Incomplete];
        assert_eq!(
            CheckStatus::all(xs),
            xs.iter().copied().fold(Passed, CheckStatus::and)
        );
    }

    #[test]
    fn check_status_order_is_declaration_order_not_aggregation() {
        assert!(Passed < Failed && Failed < Incomplete);
        // The maximum of Failed and Incomplete is Incomplete, but their
        // conjunction is Failed: the order must not be used to aggregate.
        assert_eq!(Failed.max(Incomplete), Incomplete);
        assert_eq!(Failed.and(Incomplete), Failed);
    }

    #[test]
    fn check_status_is_passed_only_for_passed() {
        assert!(Passed.is_passed());
        assert!(!Failed.is_passed());
        assert!(!Incomplete.is_passed());
    }
}
