//! Equality witnesses: propositional equality proofs.
//!
//! An `EqWitness` certifies that two terms are equal, carrying
//! a justification that can be verified against a theory.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::eq::Term;

/// A witness that two terms are equal.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EqWitness {
    /// The left-hand side.
    pub lhs: Term,
    /// The right-hand side.
    pub rhs: Term,
    /// How the equality was established.
    pub justification: WitnessJustification,
}

/// How an equality was established.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WitnessJustification {
    /// Both sides are identical (`a = a`).
    Reflexivity,
    /// The equality is an axiom of the theory.
    Axiom(Arc<str>),
    /// Derived by symmetry from another witness.
    Symmetry(Box<EqWitness>),
    /// Derived by transitivity from two witnesses.
    Transitivity(Box<EqWitness>, Box<EqWitness>),
    /// Derived by congruence: applying the same operation to equal arguments.
    Congruence {
        /// The operation applied.
        op: Arc<str>,
        /// Witnesses for equality of each argument.
        arg_witnesses: Vec<EqWitness>,
    },
    /// Verified at runtime (fallback when static proof unavailable).
    RuntimeChecked {
        /// Human-readable description of the runtime check.
        description: String,
    },
}

impl EqWitness {
    /// Create a reflexivity witness (`term = term`).
    #[must_use]
    pub fn reflexivity(term: Term) -> Self {
        Self {
            lhs: term.clone(),
            rhs: term,
            justification: WitnessJustification::Reflexivity,
        }
    }

    /// Create an axiom witness.
    #[must_use]
    pub fn axiom(name: impl Into<Arc<str>>, lhs: Term, rhs: Term) -> Self {
        Self {
            lhs,
            rhs,
            justification: WitnessJustification::Axiom(name.into()),
        }
    }

    /// Compose two witnesses by transitivity: if `a=b` and `b=c` then `a=c`.
    #[must_use]
    pub fn transitivity(ab: Self, bc: Self) -> Self {
        Self {
            lhs: ab.lhs.clone(),
            rhs: bc.rhs.clone(),
            justification: WitnessJustification::Transitivity(Box::new(ab), Box::new(bc)),
        }
    }

    /// Derive a symmetry witness: if `a=b` then `b=a`.
    #[must_use]
    pub fn symmetry(witness: Self) -> Self {
        Self {
            lhs: witness.rhs.clone(),
            rhs: witness.lhs.clone(),
            justification: WitnessJustification::Symmetry(Box::new(witness)),
        }
    }

    /// Derive a congruence witness: if `a_i = b_i` for all `i`, then
    /// `op(a_1, ..., a_n) = op(b_1, ..., b_n)`.
    #[must_use]
    pub fn congruence(op: impl Into<Arc<str>>, arg_witnesses: Vec<Self>) -> Self {
        let op = op.into();
        let lhs_args: Vec<Term> = arg_witnesses.iter().map(|w| w.lhs.clone()).collect();
        let rhs_args: Vec<Term> = arg_witnesses.iter().map(|w| w.rhs.clone()).collect();
        Self {
            lhs: Term::app(Arc::clone(&op), lhs_args),
            rhs: Term::app(Arc::clone(&op), rhs_args),
            justification: WitnessJustification::Congruence { op, arg_witnesses },
        }
    }

    /// The depth of the proof tree (number of nested justification layers).
    #[must_use]
    pub fn depth(&self) -> usize {
        match &self.justification {
            WitnessJustification::Reflexivity
            | WitnessJustification::Axiom(_)
            | WitnessJustification::RuntimeChecked { .. } => 1,
            WitnessJustification::Symmetry(w) => 1 + w.depth(),
            WitnessJustification::Transitivity(a, b) => 1 + a.depth().max(b.depth()),
            WitnessJustification::Congruence { arg_witnesses, .. } => {
                1 + arg_witnesses.iter().map(Self::depth).max().unwrap_or(0)
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn reflexivity_witness() {
        let t = Term::var("x");
        let w = EqWitness::reflexivity(t.clone());
        assert_eq!(w.lhs, t);
        assert_eq!(w.rhs, t);
        assert_eq!(w.depth(), 1);
    }

    #[test]
    fn axiom_witness() {
        let lhs = Term::app("add", vec![Term::var("x"), Term::constant("zero")]);
        let rhs = Term::var("x");
        let w = EqWitness::axiom("right_identity", lhs.clone(), rhs.clone());
        assert_eq!(w.lhs, lhs);
        assert_eq!(w.rhs, rhs);
        assert_eq!(w.depth(), 1);
    }

    #[test]
    fn transitivity_chain() {
        let a = Term::var("a");
        let b = Term::var("b");
        let c = Term::var("c");

        let ab = EqWitness::axiom("ax1", a.clone(), b.clone());
        let bc = EqWitness::axiom("ax2", b, c.clone());
        let ac = EqWitness::transitivity(ab, bc);

        assert_eq!(ac.lhs, a);
        assert_eq!(ac.rhs, c);
        assert_eq!(ac.depth(), 2);
    }

    #[test]
    fn symmetry_witness() {
        let a = Term::var("a");
        let b = Term::var("b");
        let ab = EqWitness::axiom("ax", a.clone(), b.clone());
        let ba = EqWitness::symmetry(ab);

        assert_eq!(ba.lhs, b);
        assert_eq!(ba.rhs, a);
        assert_eq!(ba.depth(), 2);
    }

    #[test]
    fn congruence_witness() {
        let x = Term::var("x");
        let _y = Term::var("y");
        let w = EqWitness::reflexivity(x.clone());
        let cong = EqWitness::congruence("f", vec![w]);

        assert_eq!(cong.lhs, Term::app("f", vec![x.clone()]));
        assert_eq!(cong.rhs, Term::app("f", vec![x]));
        assert_eq!(cong.depth(), 2);
    }

    #[test]
    fn serialization_round_trip() {
        let w = EqWitness::axiom("ax", Term::var("a"), Term::var("b"));
        let json = serde_json::to_string(&w).expect("serialize");
        let deserialized: EqWitness = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(w, deserialized);
    }
}
