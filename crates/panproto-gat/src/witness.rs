//! Equality witnesses: propositional equality proofs.
//!
//! An `EqWitness` certifies that two terms are equal, carrying
//! a justification that can be verified against a theory.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::eq::Term;
use crate::error::GatError;
use crate::theory::Theory;

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

    /// Verify that this witness genuinely justifies `lhs = rhs` in `theory`.
    ///
    /// Each justification is checked structurally:
    ///
    /// - `Reflexivity` requires `lhs == rhs`.
    /// - `Axiom(name)` requires `(lhs, rhs)` to be a substitution instance of
    ///   the named theory equation (in either orientation, since equality is
    ///   symmetric).
    /// - `Symmetry(w)` requires `w` to verify with swapped endpoints.
    /// - `Transitivity(a, b)` requires both to verify and `a.rhs == b.lhs`,
    ///   with the endpoints of `self` matching `a.lhs` and `b.rhs`.
    /// - `Congruence { op, args }` requires each argument witness to verify and
    ///   the endpoints to be `op` applied to the argument endpoints.
    /// - `RuntimeChecked` is a trusted fallback and is accepted as given.
    ///
    /// # Errors
    ///
    /// Returns [`GatError::WitnessInvalid`] describing the first check that
    /// fails.
    pub fn verify(&self, theory: &Theory) -> Result<(), GatError> {
        match &self.justification {
            WitnessJustification::Reflexivity => {
                if self.lhs == self.rhs {
                    Ok(())
                } else {
                    Err(GatError::WitnessInvalid {
                        reason: "reflexivity witness whose sides are not identical".to_owned(),
                    })
                }
            }
            WitnessJustification::Axiom(name) => {
                let axiom = theory
                    .find_eq(name)
                    .map(|e| (&e.lhs, &e.rhs))
                    .or_else(|| theory.find_directed_eq(name).map(|d| (&d.lhs, &d.rhs)));
                let Some((ax_lhs, ax_rhs)) = axiom else {
                    return Err(GatError::WitnessInvalid {
                        reason: format!(
                            "axiom `{name}` is not an equation of theory `{}`",
                            theory.name
                        ),
                    });
                };
                if is_axiom_instance(ax_lhs, ax_rhs, &self.lhs, &self.rhs) {
                    Ok(())
                } else {
                    Err(GatError::WitnessInvalid {
                        reason: format!("witness endpoints are not an instance of axiom `{name}`"),
                    })
                }
            }
            WitnessJustification::Symmetry(inner) => {
                inner.verify(theory)?;
                if self.lhs == inner.rhs && self.rhs == inner.lhs {
                    Ok(())
                } else {
                    Err(GatError::WitnessInvalid {
                        reason: "symmetry witness endpoints do not swap the inner witness"
                            .to_owned(),
                    })
                }
            }
            WitnessJustification::Transitivity(ab, bc) => {
                ab.verify(theory)?;
                bc.verify(theory)?;
                if ab.rhs != bc.lhs {
                    return Err(GatError::WitnessInvalid {
                        reason: "transitivity witnesses do not meet (a.rhs != b.lhs)".to_owned(),
                    });
                }
                if self.lhs == ab.lhs && self.rhs == bc.rhs {
                    Ok(())
                } else {
                    Err(GatError::WitnessInvalid {
                        reason: "transitivity witness endpoints do not match the chain".to_owned(),
                    })
                }
            }
            WitnessJustification::Congruence { op, arg_witnesses } => {
                for w in arg_witnesses {
                    w.verify(theory)?;
                }
                let lhs_args: Vec<Term> = arg_witnesses.iter().map(|w| w.lhs.clone()).collect();
                let rhs_args: Vec<Term> = arg_witnesses.iter().map(|w| w.rhs.clone()).collect();
                let expected_lhs = Term::app(Arc::clone(op), lhs_args);
                let expected_rhs = Term::app(Arc::clone(op), rhs_args);
                if self.lhs == expected_lhs && self.rhs == expected_rhs {
                    Ok(())
                } else {
                    Err(GatError::WitnessInvalid {
                        reason: format!(
                            "congruence witness endpoints are not `{op}` applied to the argument endpoints"
                        ),
                    })
                }
            }
            // A runtime-checked witness is a trusted fallback: it records that
            // the equality was decided by evaluation rather than static proof.
            WitnessJustification::RuntimeChecked { .. } => Ok(()),
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

/// Returns `true` if `(w_lhs, w_rhs)` is a substitution instance of the axiom
/// `ax_lhs = ax_rhs`, trying both orientations (equality is symmetric).
fn is_axiom_instance(ax_lhs: &Term, ax_rhs: &Term, w_lhs: &Term, w_rhs: &Term) -> bool {
    matches_pair(ax_lhs, ax_rhs, w_lhs, w_rhs) || matches_pair(ax_rhs, ax_lhs, w_lhs, w_rhs)
}

/// Match a pair of axiom patterns against a pair of witness terms under a
/// single, consistent substitution of the axiom's variables.
fn matches_pair(pat_l: &Term, pat_r: &Term, tgt_l: &Term, tgt_r: &Term) -> bool {
    let mut subst: HashMap<Arc<str>, Term> = HashMap::new();
    match_term(pat_l, tgt_l, &mut subst) && match_term(pat_r, tgt_r, &mut subst)
}

/// First-order pattern match: variables of `pat` are metavariables bound to
/// sub-terms of `target`; every other constructor must agree structurally.
/// Bindings must be consistent across the whole match. `Case`, `Hole`, and
/// `Let` patterns are matched by exact equality (a conservative, sound choice
/// since equational axioms are first-order applications of operations).
fn match_term(pat: &Term, target: &Term, subst: &mut HashMap<Arc<str>, Term>) -> bool {
    match pat {
        Term::Var(x) => match subst.entry(Arc::clone(x)) {
            std::collections::hash_map::Entry::Occupied(e) => e.get() == target,
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(target.clone());
                true
            }
        },
        Term::App { op, args } => match target {
            Term::App {
                op: t_op,
                args: t_args,
            } => {
                op == t_op
                    && args.len() == t_args.len()
                    && args
                        .iter()
                        .zip(t_args)
                        .all(|(p, t)| match_term(p, t, subst))
            }
            _ => false,
        },
        other => other == target,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::eq::Equation;
    use crate::theory::Theory;

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

    /// A theory with one axiom `right_identity: add(x, zero) = x`.
    fn right_identity_theory() -> Theory {
        let eq = Equation::new(
            "right_identity",
            Term::app("add", vec![Term::var("x"), Term::constant("zero")]),
            Term::var("x"),
        );
        Theory::full("T", vec![], vec![], vec![], vec![eq], vec![], vec![])
    }

    #[test]
    fn verify_accepts_reflexivity() {
        let theory = right_identity_theory();
        let w = EqWitness::reflexivity(Term::var("q"));
        assert!(w.verify(&theory).is_ok());
    }

    #[test]
    fn verify_rejects_bogus_reflexivity() {
        let theory = right_identity_theory();
        // A reflexivity witness whose sides are not identical is forged.
        let w = EqWitness {
            lhs: Term::var("a"),
            rhs: Term::var("b"),
            justification: WitnessJustification::Reflexivity,
        };
        assert!(matches!(
            w.verify(&theory),
            Err(GatError::WitnessInvalid { .. })
        ));
    }

    #[test]
    fn verify_accepts_axiom_instance() {
        let theory = right_identity_theory();
        // add(foo, zero) = foo is an instance of right_identity (x := foo).
        let w = EqWitness::axiom(
            "right_identity",
            Term::app("add", vec![Term::constant("foo"), Term::constant("zero")]),
            Term::constant("foo"),
        );
        assert!(w.verify(&theory).is_ok());
    }

    #[test]
    fn verify_rejects_forged_axiom() {
        let theory = right_identity_theory();
        // Claims right_identity but the endpoints are not an instance of it.
        let w = EqWitness::axiom(
            "right_identity",
            Term::app("add", vec![Term::constant("a"), Term::constant("b")]),
            Term::constant("c"),
        );
        assert!(matches!(
            w.verify(&theory),
            Err(GatError::WitnessInvalid { .. })
        ));
    }

    #[test]
    fn verify_rejects_unknown_axiom() {
        let theory = right_identity_theory();
        let w = EqWitness::axiom("no_such_axiom", Term::var("a"), Term::var("a"));
        assert!(matches!(
            w.verify(&theory),
            Err(GatError::WitnessInvalid { .. })
        ));
    }

    #[test]
    fn verify_accepts_symmetry_and_transitivity() {
        let theory = right_identity_theory();
        let lhs = Term::app("add", vec![Term::constant("foo"), Term::constant("zero")]);
        let foo = Term::constant("foo");
        let ax = EqWitness::axiom("right_identity", lhs, foo);
        let sym = EqWitness::symmetry(ax.clone());
        assert!(sym.verify(&theory).is_ok());
        // add(foo,zero) = foo (axiom) then foo = add(foo,zero) (symmetry) gives
        // add(foo,zero) = add(foo,zero) by transitivity.
        let trans = EqWitness::transitivity(ax, sym);
        assert!(trans.verify(&theory).is_ok());
    }

    #[test]
    fn verify_rejects_mismatched_transitivity() {
        let theory = right_identity_theory();
        // a = b and c = d do not meet (b != c), so the chain is invalid.
        let ab = EqWitness::axiom("right_identity", Term::var("a"), Term::var("b"));
        let cd = EqWitness::axiom("right_identity", Term::var("c"), Term::var("d"));
        let bad = EqWitness {
            lhs: Term::var("a"),
            rhs: Term::var("d"),
            justification: WitnessJustification::Transitivity(Box::new(ab), Box::new(cd)),
        };
        assert!(matches!(
            bad.verify(&theory),
            Err(GatError::WitnessInvalid { .. })
        ));
    }

    #[test]
    fn verify_accepts_congruence_and_runtime() {
        let theory = right_identity_theory();
        let arg = EqWitness::reflexivity(Term::var("x"));
        let cong = EqWitness::congruence("f", vec![arg]);
        assert!(cong.verify(&theory).is_ok());

        let rt = EqWitness {
            lhs: Term::var("a"),
            rhs: Term::var("b"),
            justification: WitnessJustification::RuntimeChecked {
                description: "decided by evaluation".to_owned(),
            },
        };
        assert!(rt.verify(&theory).is_ok());
    }
}
