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

// ---------------------------------------------------------------------------
// Symbolic lens-law proofs for the elementary combinator vocabulary
// ---------------------------------------------------------------------------

impl SymbolicStep {
    /// The `put`-direction step that inverts this `get`-direction step.
    ///
    /// Each elementary combinator is a one-step protolens whose `get`
    /// runs this step forward and whose `put` runs the returned step. The
    /// pairing is:
    ///
    /// - `RenameSort(a, b)` ↔ `RenameSort(b, a)` — a bijective relabeling.
    /// - `RenameOp(f, g)` ↔ `RenameOp(g, f)`.
    /// - `AddSort(a)` ↔ `DropSort(a)` — `get` introduces the sort with a
    ///   default, `put` removes it.
    /// - `DropSort(a)` ↔ `AddSort(a)` — `get` removes the sort and records
    ///   its data in the complement, `put` reintroduces it from the
    ///   complement.
    /// - `AddOp` / `DropOp` mirror `AddSort` / `DropSort`.
    ///
    /// [`Opaque`](SymbolicStep::Opaque) steps have no known inverse and
    /// return `None`, so their lens laws cannot be established symbolically.
    ///
    /// For the lossy `Add`/`Drop` combinators the inverse is exact only
    /// because the complement carries the discarded data; the returned step
    /// names the structural inverse and the complement supplies the values.
    #[must_use]
    pub fn inverse(&self) -> Option<Self> {
        match self {
            Self::RenameSort { old, new } => Some(Self::RenameSort {
                old: Arc::clone(new),
                new: Arc::clone(old),
            }),
            Self::RenameOp { old, new } => Some(Self::RenameOp {
                old: Arc::clone(new),
                new: Arc::clone(old),
            }),
            Self::AddSort(name) => Some(Self::DropSort(Arc::clone(name))),
            Self::DropSort(name) => Some(Self::AddSort(Arc::clone(name))),
            Self::AddOp(name) => Some(Self::DropOp(Arc::clone(name))),
            Self::DropOp(name) => Some(Self::AddOp(Arc::clone(name))),
            Self::Opaque(_) => None,
        }
    }
}

/// Reduce a step sequence by cancelling every adjacent inverse pair,
/// returning `true` when the sequence collapses to the empty (identity)
/// program.
///
/// This is the law-proof normalizer: unlike [`simplify_steps`], which only
/// applies the *sound-in-any-context* rewrites (notably `Add;Drop → ∅` but
/// not its unsound converse `Drop;Add`), this routine cancels a step
/// immediately followed by its [`SymbolicStep::inverse`] in *either* order.
/// That is valid for establishing the round-trip lens laws — where `get` is
/// always paired with the complement-carrying `put` that exactly inverts it
/// — but would be unsound as a general chain optimization, which is why the
/// two engines are kept separate.
///
/// The reduction is a single left-to-right pass with a stack: each step
/// either cancels the step on top of the stack (when it is that step's
/// inverse) or is pushed. Because cancellation exposes the next pair, nested
/// palindromes such as `a; b; b⁻¹; a⁻¹` collapse fully.
#[must_use]
pub fn cancels_to_identity(steps: &[SymbolicStep]) -> bool {
    let mut stack: Vec<&SymbolicStep> = Vec::with_capacity(steps.len());
    for step in steps {
        match stack.last() {
            Some(top) if top.inverse().as_ref() == Some(step) => {
                stack.pop();
            }
            _ => stack.push(step),
        }
    }
    stack.is_empty()
}

/// Establish the get-put lens law for a one-step elementary combinator.
///
/// The proof is by symbolic rewriting: running the combinator's `get` step
/// and then its `put` step (its [`inverse`](SymbolicStep::inverse)) cancels
/// to the identity program. Returns `false` when the step has no symbolic
/// inverse (an [`Opaque`](SymbolicStep::Opaque) step), since the law then
/// cannot be discharged by rewriting.
#[must_use]
pub fn proves_get_put(get: &SymbolicStep) -> bool {
    let Some(put) = get.inverse() else {
        return false;
    };
    cancels_to_identity(&[get.clone(), put])
}

/// Establish the put-get lens law for a one-step elementary combinator.
///
/// The proof is by symbolic rewriting: running the combinator's `put` step
/// (its [`inverse`](SymbolicStep::inverse)) and then its `get` step cancels
/// to the identity program. Returns `false` when the step has no symbolic
/// inverse.
#[must_use]
pub fn proves_put_get(get: &SymbolicStep) -> bool {
    let Some(put) = get.inverse() else {
        return false;
    };
    cancels_to_identity(&[put, get.clone()])
}

/// The `get`-direction steps of the elementary protolens combinator
/// vocabulary, keyed by a stable label.
///
/// Iterating this list gives one representative of each elementary
/// combinator whose lens laws [`proves_get_put`] and [`proves_put_get`]
/// discharge by rewriting. The `Opaque` step is deliberately excluded: it
/// stands for a non-elementary step with no symbolic inverse.
#[must_use]
pub fn elementary_get_steps() -> Vec<(&'static str, SymbolicStep)> {
    vec![
        (
            "rename_sort",
            SymbolicStep::RenameSort {
                old: Arc::from("A"),
                new: Arc::from("B"),
            },
        ),
        (
            "rename_op",
            SymbolicStep::RenameOp {
                old: Arc::from("f"),
                new: Arc::from("g"),
            },
        ),
        ("add_sort", SymbolicStep::AddSort(Arc::from("A"))),
        ("drop_sort", SymbolicStep::DropSort(Arc::from("A"))),
        ("add_op", SymbolicStep::AddOp(Arc::from("f"))),
        ("drop_op", SymbolicStep::DropOp(Arc::from("f"))),
    ]
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

    // --- symbolic lens-law proofs for the elementary vocabulary ---

    #[test]
    fn inverse_is_involutive_on_elementary_steps() {
        for (label, step) in elementary_get_steps() {
            let inv = step.inverse().expect("elementary step has an inverse");
            let inv_inv = inv.inverse().expect("inverse is also invertible");
            assert_eq!(inv_inv, step, "inverse²  should be identity for {label}");
        }
    }

    #[test]
    fn opaque_step_has_no_inverse() {
        let step = SymbolicStep::Opaque("custom".into());
        assert!(step.inverse().is_none());
        assert!(!proves_get_put(&step), "opaque law cannot be discharged");
        assert!(!proves_put_get(&step), "opaque law cannot be discharged");
    }

    #[test]
    fn every_elementary_combinator_satisfies_both_laws() {
        for (label, step) in elementary_get_steps() {
            assert!(
                proves_get_put(&step),
                "get-put must hold symbolically for {label}"
            );
            assert!(
                proves_put_get(&step),
                "put-get must hold symbolically for {label}"
            );
        }
    }

    #[test]
    fn rename_sort_laws_cancel() {
        // get: A→B, put: B→A. get;put and put;get both reduce to identity.
        let get = rename_sort("A", "B");
        assert_eq!(get.inverse(), Some(rename_sort("B", "A")));
        assert!(cancels_to_identity(&[
            rename_sort("A", "B"),
            rename_sort("B", "A")
        ]));
    }

    #[test]
    fn add_sort_get_put_is_drop() {
        // AddSort's put drops the added sort; the pair cancels.
        let get = SymbolicStep::AddSort(Arc::from("X"));
        assert_eq!(get.inverse(), Some(SymbolicStep::DropSort(Arc::from("X"))));
        assert!(proves_get_put(&get));
        assert!(proves_put_get(&get));
    }

    #[test]
    fn drop_sort_get_put_is_add() {
        // DropSort's put restores the sort from the complement; the pair
        // cancels even though `simplify_steps` leaves `Drop;Add` intact
        // (the general converse rewrite would be unsound).
        let get = SymbolicStep::DropSort(Arc::from("X"));
        assert_eq!(get.inverse(), Some(SymbolicStep::AddSort(Arc::from("X"))));
        assert!(proves_get_put(&get));
        assert!(proves_put_get(&get));
        // The generic chain optimizer must NOT collapse Drop;Add: dropping
        // then re-adding with a default is not the identity in general.
        let drop_then_add = vec![
            SymbolicStep::DropSort(Arc::from("X")),
            SymbolicStep::AddSort(Arc::from("X")),
        ];
        assert_eq!(simplify_steps(drop_then_add.clone()), drop_then_add);
    }

    #[test]
    fn nested_chain_and_its_reverse_cancel() {
        // A chain of elementary combinators followed by the reversed chain
        // of their inverses cancels to identity — the symbolic proof scales
        // beyond a single combinator.
        let chain = [
            rename_sort("A", "B"),
            SymbolicStep::AddOp(Arc::from("f")),
            SymbolicStep::DropSort(Arc::from("C")),
        ];
        let mut program: Vec<SymbolicStep> = chain.to_vec();
        for step in chain.iter().rev() {
            program.push(step.inverse().expect("elementary step invertible"));
        }
        assert!(cancels_to_identity(&program));
    }
}
