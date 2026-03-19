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
    /// For numeric constraints (`maxLength`, `minLength`, `maximum`, `minimum`),
    /// this checks interval containment: every value satisfying `self` must
    /// also satisfy `other`. Same base sort is required.
    #[must_use]
    pub fn subsort_of(&self, other: &Self) -> bool {
        if self.base != other.base {
            return false;
        }

        // Self is a subsort of other if for every constraint in other,
        // self has a constraint of the same kind that is at least as tight.
        for other_c in &other.constraints {
            let dominated = self.constraints.iter().any(|self_c| {
                self_c.kind == other_c.kind
                    && constraint_tighter(&self_c.kind, &self_c.value, &other_c.value)
            });
            if !dominated {
                return false;
            }
        }

        // Also, self must actually be *strictly* tighter — it must have at
        // least one constraint that is tighter or an additional constraint.
        if self.constraints.len() == other.constraints.len()
            && self.constraints.iter().all(|sc| {
                other
                    .constraints
                    .iter()
                    .any(|oc| sc.kind == oc.kind && sc.value == oc.value)
            })
        {
            return false;
        }

        true
    }
}

/// Check whether `self_val` is at least as tight as `other_val` for the
/// given constraint kind. Returns true if self's constraint dominates other's.
fn constraint_tighter(kind: &str, self_val: &str, other_val: &str) -> bool {
    let parse_both = || -> Option<(f64, f64)> {
        let s = self_val.parse::<f64>().ok()?;
        let o = other_val.parse::<f64>().ok()?;
        Some((s, o))
    };

    match kind {
        // Upper-bound constraints: tighter means smaller or equal value.
        "maxLength" | "maximum" | "exclusiveMaximum" | "maxItems" | "maxProperties" => {
            parse_both().is_some_and(|(s, o)| s <= o)
        }
        // Lower-bound constraints: tighter means larger or equal value.
        "minLength" | "minimum" | "exclusiveMinimum" | "minItems" | "minProperties" => {
            parse_both().is_some_and(|(s, o)| s >= o)
        }
        // Non-numeric constraints: equal values are considered matching.
        _ => self_val == other_val,
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
