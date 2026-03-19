//! Symbolic simplification of protolens chains.
//!
//! Applies algebraic rewrite rules to normalize protolens chains,
//! eliminating redundant steps before instantiation.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// A symbolic representation of a protolens step for simplification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymbolicStep {
    /// Rename a sort.
    RenameSort {
        /// The original sort name.
        old: Arc<str>,
        /// The new sort name.
        new: Arc<str>,
    },
    /// Rename an operation.
    RenameOp {
        /// The original operation name.
        old: Arc<str>,
        /// The new operation name.
        new: Arc<str>,
    },
    /// Add a sort.
    AddSort(Arc<str>),
    /// Drop a sort.
    DropSort(Arc<str>),
    /// Add an operation.
    AddOp(Arc<str>),
    /// Drop an operation.
    DropOp(Arc<str>),
    /// Any other step (not simplifiable).
    Opaque(String),
}

/// Simplify a sequence of symbolic steps by applying rewrite rules.
///
/// Iterates the rewrite rules until a fixed point is reached or a maximum
/// iteration count is hit (100 iterations).
#[must_use]
pub fn simplify_steps(steps: Vec<SymbolicStep>) -> Vec<SymbolicStep> {
    let mut current = steps;
    for _ in 0..100 {
        let next = apply_rules(&current);
        if next == current {
            break;
        }
        current = next;
    }
    current
}

/// Apply a single pass of rewrite rules over the step sequence.
fn apply_rules(steps: &[SymbolicStep]) -> Vec<SymbolicStep> {
    let mut result = Vec::with_capacity(steps.len());
    let mut i = 0;

    while i < steps.len() {
        // Try pairwise rules when a next step exists.
        if i + 1 < steps.len() {
            if let Some(replacement) = try_pairwise_rule(&steps[i], &steps[i + 1]) {
                result.extend(replacement);
                i += 2;
                continue;
            }
        }
        result.push(steps[i].clone());
        i += 1;
    }

    result
}

/// Try to apply a pairwise rewrite rule to two adjacent steps.
///
/// Returns `Some(replacement)` if a rule fired, `None` otherwise.
/// The replacement may be empty (cancellation), one step (fusion), etc.
fn try_pairwise_rule(a: &SymbolicStep, b: &SymbolicStep) -> Option<Vec<SymbolicStep>> {
    match (a, b) {
        // Rule 1: Inverse cancellation for sort renames.
        // RenameSort(A, B) then RenameSort(B, A) → cancel both.
        (
            SymbolicStep::RenameSort {
                old: a_old,
                new: a_new,
            },
            SymbolicStep::RenameSort {
                old: b_old,
                new: b_new,
            },
        ) if a_new == b_old && b_new == a_old => Some(vec![]),

        // Rule 2: Rename fusion for sorts.
        // RenameSort(A, B) then RenameSort(B, C) → RenameSort(A, C).
        (
            SymbolicStep::RenameSort {
                old: a_old,
                new: a_new,
            },
            SymbolicStep::RenameSort {
                old: b_old,
                new: b_new,
            },
        ) if a_new == b_old => Some(vec![SymbolicStep::RenameSort {
            old: Arc::clone(a_old),
            new: Arc::clone(b_new),
        }]),

        // Rule 3: Inverse cancellation for op renames.
        (
            SymbolicStep::RenameOp {
                old: a_old,
                new: a_new,
            },
            SymbolicStep::RenameOp {
                old: b_old,
                new: b_new,
            },
        ) if a_new == b_old && b_new == a_old => Some(vec![]),

        // Rule 4: Rename fusion for ops.
        (
            SymbolicStep::RenameOp {
                old: a_old,
                new: a_new,
            },
            SymbolicStep::RenameOp {
                old: b_old,
                new: b_new,
            },
        ) if a_new == b_old => Some(vec![SymbolicStep::RenameOp {
            old: Arc::clone(a_old),
            new: Arc::clone(b_new),
        }]),

        // Rule 5: Add-drop cancellation for sorts.
        (SymbolicStep::AddSort(added), SymbolicStep::DropSort(dropped)) if added == dropped => {
            Some(vec![])
        }

        // Rule 6: Add-drop cancellation for ops.
        (SymbolicStep::AddOp(added), SymbolicStep::DropOp(dropped)) if added == dropped => {
            Some(vec![])
        }

        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn rename_sort(old: &str, new: &str) -> SymbolicStep {
        SymbolicStep::RenameSort {
            old: Arc::from(old),
            new: Arc::from(new),
        }
    }

    fn rename_op(old: &str, new: &str) -> SymbolicStep {
        SymbolicStep::RenameOp {
            old: Arc::from(old),
            new: Arc::from(new),
        }
    }

    #[test]
    fn inverse_cancellation_sorts() {
        let steps = vec![rename_sort("A", "B"), rename_sort("B", "A")];
        let simplified = simplify_steps(steps);
        assert!(simplified.is_empty());
    }

    #[test]
    fn rename_fusion_sorts() {
        let steps = vec![rename_sort("A", "B"), rename_sort("B", "C")];
        let simplified = simplify_steps(steps);
        assert_eq!(simplified, vec![rename_sort("A", "C")]);
    }

    #[test]
    fn inverse_cancellation_ops() {
        let steps = vec![rename_op("f", "g"), rename_op("g", "f")];
        let simplified = simplify_steps(steps);
        assert!(simplified.is_empty());
    }

    #[test]
    fn rename_fusion_ops() {
        let steps = vec![rename_op("f", "g"), rename_op("g", "h")];
        let simplified = simplify_steps(steps);
        assert_eq!(simplified, vec![rename_op("f", "h")]);
    }

    #[test]
    fn add_drop_cancellation_sort() {
        let steps = vec![
            SymbolicStep::AddSort(Arc::from("X")),
            SymbolicStep::DropSort(Arc::from("X")),
        ];
        let simplified = simplify_steps(steps);
        assert!(simplified.is_empty());
    }

    #[test]
    fn add_drop_cancellation_op() {
        let steps = vec![
            SymbolicStep::AddOp(Arc::from("f")),
            SymbolicStep::DropOp(Arc::from("f")),
        ];
        let simplified = simplify_steps(steps);
        assert!(simplified.is_empty());
    }

    #[test]
    fn opaque_steps_preserved() {
        let steps = vec![SymbolicStep::Opaque("custom".into()), rename_sort("A", "B")];
        let simplified = simplify_steps(steps.clone());
        assert_eq!(simplified, steps);
    }

    #[test]
    fn multi_step_fusion_chain() {
        // A→B, B→C, C→D should fuse to A→D over multiple iterations.
        let steps = vec![
            rename_sort("A", "B"),
            rename_sort("B", "C"),
            rename_sort("C", "D"),
        ];
        let simplified = simplify_steps(steps);
        assert_eq!(simplified, vec![rename_sort("A", "D")]);
    }

    #[test]
    fn non_adjacent_steps_not_cancelled() {
        let steps = vec![
            rename_sort("A", "B"),
            SymbolicStep::Opaque("barrier".into()),
            rename_sort("B", "A"),
        ];
        let simplified = simplify_steps(steps.clone());
        assert_eq!(simplified, steps);
    }
}
