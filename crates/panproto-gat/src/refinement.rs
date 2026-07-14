//! Refinement types: sorts constrained by predicates.
//!
//! A `RefinedSort` pairs a base sort (e.g., "string") with constraints
//! (e.g., `maxLength(300)`), creating a subsort. The subsort relationship
//! determines whether constraint changes are breaking.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// A sort refined by constraints, creating a subsort.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RefinedSort {
    /// The base sort name (e.g., "string", "int").
    pub base: Arc<str>,
    /// Constraints that narrow the sort.
    pub constraints: Vec<RefinementConstraint>,
}

/// A single refinement constraint on a sort.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RefinementConstraint {
    /// The constraint kind (e.g., "maxLength", "minimum", "format").
    pub kind: Arc<str>,
    /// The constraint value as a string.
    pub value: Arc<str>,
}

/// Error when a value fails refinement.
#[derive(Debug, Clone, thiserror::Error)]
pub enum RefinementError {
    /// A numeric constraint was violated.
    #[error("constraint {kind} violated: value {value} not in range")]
    NumericViolation {
        /// The constraint kind that was violated.
        kind: String,
        /// The value that violated the constraint.
        value: String,
    },
    /// A pattern/format constraint was violated.
    #[error("format constraint {kind} violated")]
    FormatViolation {
        /// The format constraint kind that was violated.
        kind: String,
    },
    /// A numeric-kind constraint carries a value that does not parse as a
    /// number, so its subsort relationship cannot be decided.
    #[error("numeric constraint {kind} has unparseable value {value:?}")]
    MalformedConstraint {
        /// The constraint kind whose value failed to parse.
        kind: String,
        /// The value that could not be parsed as a number.
        value: String,
    },
}

impl RefinedSort {
    /// Build a refined sort from a base sort name and constraint pairs.
    #[must_use]
    pub fn from_constraints(base: &str, constraints: &[(String, String)]) -> Self {
        Self {
            base: Arc::from(base),
            constraints: constraints
                .iter()
                .map(|(k, v)| RefinementConstraint {
                    kind: Arc::from(k.as_str()),
                    value: Arc::from(v.as_str()),
                })
                .collect(),
        }
    }

    /// Returns true if `self`'s constraints are strictly tighter than `other`'s.
    ///
    /// This is the infallible convenience wrapper over [`Self::try_subsort_of`].
    /// A malformed numeric constraint makes the relationship undecidable, and
    /// this method reports it conservatively as *not* a subsort (returning
    /// `false`). Callers that need to distinguish a malformed constraint from a
    /// genuine non-refinement — for instance, to keep breaking-change detection
    /// from silently passing on a garbled bound — should call
    /// [`Self::try_subsort_of`] and inspect the [`RefinementError`].
    #[must_use]
    pub fn subsort_of(&self, other: &Self) -> bool {
        self.try_subsort_of(other).unwrap_or(false)
    }

    /// Returns `Ok(true)` if `self`'s constraints are strictly tighter than
    /// `other`'s, `Ok(false)` if they are not, and `Err` if a numeric
    /// constraint carries an unparseable value.
    ///
    /// For numeric constraints (`maxLength`, `minLength`, `maximum`, `minimum`),
    /// this checks interval containment: every value satisfying `self` must
    /// also satisfy `other`. Same base sort is required.
    ///
    /// # Errors
    ///
    /// Returns [`RefinementError::MalformedConstraint`] naming the constraint
    /// kind and offending value when a numeric-kind constraint that is needed
    /// to decide the relationship does not parse as a number. Surfacing this
    /// rather than swallowing it keeps a malformed bound from being reported as
    /// a definitive non-refinement with no signal.
    pub fn try_subsort_of(&self, other: &Self) -> Result<bool, RefinementError> {
        if self.base != other.base {
            return Ok(false);
        }

        // Self is a subsort of other if for every constraint in other,
        // self has a constraint of the same kind that is at least as tight.
        // A same-kind comparison whose values are malformed is surfaced only
        // when no well-formed constraint of that kind already dominates.
        for other_c in &other.constraints {
            let mut dominated = false;
            let mut pending_err: Option<RefinementError> = None;
            for self_c in &self.constraints {
                if self_c.kind != other_c.kind {
                    continue;
                }
                match constraint_tighter(&self_c.kind, &self_c.value, &other_c.value) {
                    Ok(true) => {
                        dominated = true;
                        break;
                    }
                    Ok(false) => {}
                    Err(e) => pending_err = Some(e),
                }
            }
            if dominated {
                continue;
            }
            if let Some(e) = pending_err {
                return Err(e);
            }
            return Ok(false);
        }

        // Also, self must actually be *strictly* tighter; it must have at
        // least one constraint that is tighter or an additional constraint.
        if self.constraints.len() == other.constraints.len()
            && self.constraints.iter().all(|sc| {
                other
                    .constraints
                    .iter()
                    .any(|oc| sc.kind == oc.kind && sc.value == oc.value)
            })
        {
            return Ok(false);
        }

        Ok(true)
    }
}

/// Check whether `self_val` is at least as tight as `other_val` for the
/// given constraint kind.
///
/// # Errors
///
/// Returns [`RefinementError::MalformedConstraint`] when `kind` is numeric and
/// either value fails to parse as a number.
fn constraint_tighter(
    kind: &str,
    self_val: &str,
    other_val: &str,
) -> Result<bool, RefinementError> {
    let parse_numeric = |value: &str| -> Result<f64, RefinementError> {
        value
            .parse::<f64>()
            .map_err(|_| RefinementError::MalformedConstraint {
                kind: kind.to_string(),
                value: value.to_string(),
            })
    };

    match kind {
        // Upper-bound constraints: tighter means smaller or equal value.
        "maxLength" | "maximum" | "exclusiveMaximum" | "maxItems" | "maxProperties" => {
            Ok(parse_numeric(self_val)? <= parse_numeric(other_val)?)
        }
        // Lower-bound constraints: tighter means larger or equal value.
        "minLength" | "minimum" | "exclusiveMinimum" | "minItems" | "minProperties" => {
            Ok(parse_numeric(self_val)? >= parse_numeric(other_val)?)
        }
        // Non-numeric constraints: equal values are considered matching.
        _ => Ok(self_val == other_val),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn subsort_same_base_tighter_max() {
        let narrow = RefinedSort::from_constraints("string", &[("maxLength".into(), "100".into())]);
        let wide = RefinedSort::from_constraints("string", &[("maxLength".into(), "300".into())]);
        assert!(narrow.subsort_of(&wide));
        assert!(!wide.subsort_of(&narrow));
    }

    #[test]
    fn subsort_same_base_tighter_min() {
        let narrow = RefinedSort::from_constraints("int", &[("minimum".into(), "10".into())]);
        let wide = RefinedSort::from_constraints("int", &[("minimum".into(), "0".into())]);
        assert!(narrow.subsort_of(&wide));
        assert!(!wide.subsort_of(&narrow));
    }

    #[test]
    fn subsort_different_base_returns_false() {
        let a = RefinedSort::from_constraints("string", &[("maxLength".into(), "100".into())]);
        let b = RefinedSort::from_constraints("int", &[("maxLength".into(), "200".into())]);
        assert!(!a.subsort_of(&b));
    }

    #[test]
    fn identical_constraints_not_strict_subsort() {
        let a = RefinedSort::from_constraints("string", &[("maxLength".into(), "100".into())]);
        let b = RefinedSort::from_constraints("string", &[("maxLength".into(), "100".into())]);
        assert!(!a.subsort_of(&b));
    }

    #[test]
    fn additional_constraint_makes_subsort() {
        let narrow = RefinedSort::from_constraints(
            "string",
            &[
                ("maxLength".into(), "100".into()),
                ("minLength".into(), "5".into()),
            ],
        );
        let wide = RefinedSort::from_constraints("string", &[("maxLength".into(), "100".into())]);
        assert!(narrow.subsort_of(&wide));
    }

    #[test]
    fn malformed_numeric_constraint_is_explicit_error() {
        let bad = RefinedSort::from_constraints("string", &[("maxLength".into(), "abc".into())]);
        let good = RefinedSort::from_constraints("string", &[("maxLength".into(), "300".into())]);

        let err = bad
            .try_subsort_of(&good)
            .expect_err("a malformed maxLength must surface as an error, not a silent false");
        match err {
            RefinementError::MalformedConstraint { kind, value } => {
                assert_eq!(kind, "maxLength");
                assert_eq!(value, "abc");
            }
            other => panic!("expected MalformedConstraint, got {other:?}"),
        }

        // The malformed value is symmetric: a garbled target is surfaced too.
        assert!(matches!(
            good.try_subsort_of(&bad),
            Err(RefinementError::MalformedConstraint { .. })
        ));

        // The infallible wrapper is conservative: an undecidable comparison is
        // reported as not-a-subsort rather than panicking.
        assert!(!bad.subsort_of(&good));
    }

    #[test]
    fn try_subsort_of_matches_subsort_of_on_well_formed() {
        let narrow = RefinedSort::from_constraints("string", &[("maxLength".into(), "100".into())]);
        let wide = RefinedSort::from_constraints("string", &[("maxLength".into(), "300".into())]);
        assert!(
            narrow
                .try_subsort_of(&wide)
                .expect("well-formed values decide")
        );
        assert!(
            !wide
                .try_subsort_of(&narrow)
                .expect("well-formed values decide")
        );
        assert_eq!(
            narrow.try_subsort_of(&wide).ok(),
            Some(narrow.subsort_of(&wide))
        );
    }

    #[test]
    fn from_constraints_round_trip() {
        let sort = RefinedSort::from_constraints(
            "string",
            &[
                ("maxLength".into(), "300".into()),
                ("format".into(), "uri".into()),
            ],
        );
        assert_eq!(&*sort.base, "string");
        assert_eq!(sort.constraints.len(), 2);
        assert_eq!(&*sort.constraints[0].kind, "maxLength");
        assert_eq!(&*sort.constraints[0].value, "300");
    }
}
