use std::collections::HashMap;
use std::sync::Arc;

use crate::compose::{
    ClashPolicy, ComposeOptions, CompositionMode, CompositionReport, DEFAULT_VERIFY_BUDGET, Naming,
    Provenance, TheoryDiagram, compose_with_naming,
};
use crate::error::GatError;
use crate::morphism::{TheoryMorphism, check_morphism};
use crate::theory::Theory;

/// Result of a categorical pushout (colimit) computation.
///
/// Contains the pushout theory along with inclusion morphisms from
/// both input theories into the pushout, the provenance of every source
/// element, and the report of every check the composition ran.
#[derive(Debug, Clone)]
pub struct ColimitResult {
    /// The pushout theory.
    pub theory: Theory,
    /// Inclusion morphism from the first theory into the pushout: j1: T1 → P.
    pub inclusion1: TheoryMorphism,
    /// Inclusion morphism from the second theory into the pushout: j2: T2 → P.
    pub inclusion2: TheoryMorphism,
    /// Where every element of the apex and of both sides went; the apex
    /// is the node labeled `apex`, the sides `left` and `right`.
    pub provenance: Provenance,
    /// Every check the composition ran.
    pub report: CompositionReport,
}

impl ColimitResult {
    /// Verify the cocone (commutativity) condition: `j1 ∘ i1 = j2 ∘ i2`.
    ///
    /// For every sort and operation in the shared theory, the two paths
    /// through the pushout must agree. `t1` and `t2` are the span's two
    /// legs' codomains, i.e. the theories the inclusions map out of; they
    /// supply the operation signatures each composition needs.
    ///
    /// # Errors
    ///
    /// Returns [`GatError`] if any composition fails or the cocone condition
    /// is violated.
    pub fn verify_cocone(
        &self,
        i1: &TheoryMorphism,
        i2: &TheoryMorphism,
        shared: &Theory,
        t1: &Theory,
        t2: &Theory,
    ) -> Result<(), GatError> {
        let lhs = i1.compose(&self.inclusion1, t1)?;
        let rhs = i2.compose(&self.inclusion2, t2)?;

        for sort in &shared.sorts {
            let l = lhs.sort_map.get(&sort.name);
            let r = rhs.sort_map.get(&sort.name);
            if l != r {
                return Err(GatError::EquationNotPreserved {
                    equation: format!("cocone sort {}", sort.name),
                    detail: format!(
                        "j1∘i1 maps to {}, j2∘i2 maps to {}",
                        l.map_or("(none)", |s| s.as_ref()),
                        r.map_or("(none)", |s| s.as_ref()),
                    ),
                });
            }
        }

        for op in &shared.ops {
            let l = lhs.op_map.get(&op.name);
            let r = rhs.op_map.get(&op.name);
            if l != r {
                return Err(GatError::EquationNotPreserved {
                    equation: format!("cocone op {}", op.name),
                    detail: format!(
                        "j1∘i1 maps to {}, j2∘i2 maps to {}",
                        l.map_or_else(|| "(none)".to_owned(), ToString::to_string),
                        r.map_or_else(|| "(none)".to_owned(), ToString::to_string),
                    ),
                });
            }
        }

        Ok(())
    }
}

/// The label of the apex in the span the wrappers build.
const APEX: &str = "apex";
/// The label of the first side.
const LEFT: &str = "left";
/// The label of the second side.
const RIGHT: &str = "right";

/// Compute the pushout (colimit) of a span `t1 <- shared -> t2`.
///
/// This is the amalgamation compatibility mode: it builds the span
/// [`TheoryDiagram::span`] and runs [`crate::compose()`] with
/// [`CompositionMode::Amalgamate`], so beyond what `i1` and `i2` identify,
/// same-name sorts and operations (and same-name equations, directed
/// equations and policies of equal content) of the two sides are
/// identified. The result is named `"{t1}_{t2}_colimit"`, and each class is
/// named after its member in `t1` if it has one, else its member in `t2`,
/// as this function always has. Non-injective legs build the quotient in
/// either argument order; a leg that sends an operation to a derived term
/// adds the equation identifying the operation with the term.
///
/// # Errors
///
/// Returns [`GatError::Composition`] when the span is invalid (a leg that
/// is not a theory morphism), when identified symbols disagree on their
/// signatures, or when same-name elements of the two sides are
/// incompatible ([`crate::compose::CompositionError::NameConflict`]).
pub fn colimit(
    shared: &Theory,
    t1: &Theory,
    t2: &Theory,
    i1: &TheoryMorphism,
    i2: &TheoryMorphism,
) -> Result<ColimitResult, GatError> {
    let diagram = TheoryDiagram::span(
        (APEX, shared.clone()),
        (LEFT, t1.clone()),
        (RIGHT, t2.clone()),
        i1.clone(),
        i2.clone(),
    );
    amalgamate(&diagram, t1, t2)
}

/// Compose a span labeled `apex`, `left`, `right` in the amalgamation mode,
/// naming classes after their `left` member, else their `right` member.
fn amalgamate(
    diagram: &TheoryDiagram,
    t1: &Theory,
    t2: &Theory,
) -> Result<ColimitResult, GatError> {
    let opts = ComposeOptions {
        mode: CompositionMode::Amalgamate,
        result_name: Arc::from(format!("{}_{}_colimit", t1.name, t2.name)),
        clash: ClashPolicy::Reject,
        rewrite_budget: DEFAULT_VERIFY_BUDGET,
    };
    let preferred: [Arc<str>; 2] = [Arc::from(LEFT), Arc::from(RIGHT)];
    let composition = compose_with_naming(diagram, &opts, Naming::PreferNodes(&preferred))?;
    let mut inclusions = composition.inclusions.into_iter().skip(1);
    let (Some(left), Some(right)) = (inclusions.next(), inclusions.next()) else {
        return Err(GatError::Composition(Box::new(
            crate::compose::CompositionError::NotACocone { class: Vec::new() },
        )));
    };
    Ok(ColimitResult {
        theory: composition.theory,
        inclusion1: left,
        inclusion2: right,
        provenance: composition.provenance,
        report: composition.report,
    })
}

/// Compute the pushout of two theories that both contain `shared` by name.
///
/// The legs `shared -> t1` and `shared -> t2` send every sort and
/// operation of `shared` to the same-name one of each side
/// ([`TheoryDiagram::by_name`]); the pushout is then [`colimit`]'s.
///
/// Returns the full [`ColimitResult`] including pushout inclusions,
/// so callers can verify the universal property via
/// [`ColimitResult::verify_universal`].
///
/// # Errors
///
/// Returns [`GatError::MissingSortMapping`] or [`GatError::MissingOpMapping`]
/// when a side lacks a sort or operation of `shared`, and otherwise the
/// errors of [`colimit`].
pub fn pushout_by_name(
    t1: &Theory,
    t2: &Theory,
    shared: &Theory,
) -> Result<ColimitResult, GatError> {
    let diagram = TheoryDiagram::by_name(
        (APEX, shared.clone()),
        (LEFT, t1.clone()),
        (RIGHT, t2.clone()),
    )?;
    amalgamate(&diagram, t1, t2)
}

/// Compute the pushout of two theories that both contain a `shared`
/// theory by name, returning only the resulting [`Theory`].
///
/// This is [`pushout_by_name`] without the inclusions, provenance and
/// report.
///
/// # Errors
///
/// The errors of [`pushout_by_name`].
pub fn colimit_by_name(t1: &Theory, t2: &Theory, shared: &Theory) -> Result<Theory, GatError> {
    pushout_by_name(t1, t2, shared).map(|result| result.theory)
}

impl ColimitResult {
    /// Verify the universal property of the pushout against an
    /// alternative cocone `(q, k1, k2)` and return the unique mediating
    /// morphism `m : self.theory → q` satisfying
    /// `m ∘ self.inclusion1 = k1` and `m ∘ self.inclusion2 = k2`.
    ///
    /// The mediating morphism is constructed by case-analysis: every
    /// element of the pushout is either an image of T1 (and is sent
    /// to its image under k1) or an image of T2 not already covered
    /// (and is sent to its image under k2). Cocone commutativity
    /// guarantees the two assignments agree on the shared image.
    ///
    /// The constructed mediator is validated with [`check_morphism`]
    /// against `q` before the factorization equations are compared, so a
    /// cocone whose name maps commute but map an operation to one with an
    /// incompatible signature in `q` is rejected rather than accepted on
    /// the strength of the name maps alone.
    ///
    /// # Errors
    ///
    /// Returns [`GatError::EquationNotPreserved`] when `(q, k1, k2)`
    /// is not a valid cocone (i.e. `k1` and `k2` disagree on a name
    /// that the pushout identifies), or when the mediator fails to
    /// factor `k1`/`k2` through the inclusions. Returns the relevant
    /// signature or preservation error from [`check_morphism`] when the
    /// mediator is not a well-formed morphism into `q`.
    pub fn verify_universal(
        &self,
        q: &Theory,
        k1: &TheoryMorphism,
        k2: &TheoryMorphism,
    ) -> Result<TheoryMorphism, GatError> {
        if k1.codomain != q.name || k2.codomain != q.name {
            return Err(GatError::EquationNotPreserved {
                equation: "universal property".to_string(),
                detail: format!(
                    "alternative cocone codomain mismatch: k1={}, k2={}, q={}",
                    k1.codomain, k2.codomain, q.name,
                ),
            });
        }

        let mut sort_map: HashMap<Arc<str>, Arc<str>> = HashMap::new();
        merge_mediator_assignments(
            &mut sort_map,
            &self.inclusion1.sort_map,
            &k1.sort_map,
            "sort",
            "k1",
        )?;
        merge_mediator_assignments(
            &mut sort_map,
            &self.inclusion2.sort_map,
            &k2.sort_map,
            "sort",
            "k2",
        )?;

        // Colimit inclusions and cocone legs rename operations; the
        // mediator is assembled from their operation-rename views.
        let mut op_map: HashMap<Arc<str>, Arc<str>> = HashMap::new();
        let incl1_ops = self.inclusion1.op_rename_map();
        let incl2_ops = self.inclusion2.op_rename_map();
        let k1_ops = k1.op_rename_map();
        let k2_ops = k2.op_rename_map();
        merge_mediator_assignments(&mut op_map, &incl1_ops, &k1_ops, "op", "k1")?;
        merge_mediator_assignments(&mut op_map, &incl2_ops, &k2_ops, "op", "k2")?;

        // Defensive coverage check: every sort/op present in the
        // pushout theory must have a mediator entry. Construction of
        // the pushout from `t1 ⊔ t2` quotient guarantees this in
        // principle (every name comes from one of the two inclusions),
        // but a future refactor that adds free generators to the
        // pushout would break the universal-property contract; we
        // detect that here rather than relying on the downstream
        // `compose` call's `ComposeUnmapped` error message.
        for sort in &self.theory.sorts {
            if !sort_map.contains_key(&sort.name) {
                return Err(GatError::EquationNotPreserved {
                    equation: format!("universal property sort {}", sort.name),
                    detail: format!(
                        "pushout sort `{}` is not the image of any T1 or T2 sort under the inclusions; \
                         no mediator can be defined on it",
                        sort.name,
                    ),
                });
            }
        }
        for op in &self.theory.ops {
            if !op_map.contains_key(&op.name) {
                return Err(GatError::EquationNotPreserved {
                    equation: format!("universal property op {}", op.name),
                    detail: format!(
                        "pushout op `{}` is not the image of any T1 or T2 op under the inclusions; \
                         no mediator can be defined on it",
                        op.name,
                    ),
                });
            }
        }

        let mediator = TheoryMorphism::new(
            format!("mediator_{}_to_{}", self.theory.name, q.name),
            Arc::clone(&self.theory.name),
            Arc::clone(&q.name),
            sort_map,
            op_map,
        );

        // Validate the mediator as a genuine morphism into q before the
        // factorization comparisons: the name maps may commute while still
        // sending an operation to one whose signature q does not preserve.
        check_morphism(&mediator, &self.theory, q)?;

        let m_j1 = self.inclusion1.compose(&mediator, &self.theory)?;
        let m_j2 = self.inclusion2.compose(&mediator, &self.theory)?;
        if m_j1.sort_map != k1.sort_map || m_j1.op_map != k1.op_map {
            return Err(GatError::EquationNotPreserved {
                equation: "universal property: m ∘ j1 = k1".to_string(),
                detail: "mediating morphism does not factor k1 through j1".to_string(),
            });
        }
        if m_j2.sort_map != k2.sort_map || m_j2.op_map != k2.op_map {
            return Err(GatError::EquationNotPreserved {
                equation: "universal property: m ∘ j2 = k2".to_string(),
                detail: "mediating morphism does not factor k2 through j2".to_string(),
            });
        }
        Ok(mediator)
    }

    /// Verify the universal property against the pushout's own canonical
    /// cocone `(self.theory, self.inclusion1, self.inclusion2)` and confirm
    /// the mediating morphism is the identity on the pushout.
    ///
    /// This is the universal property applied to the pushout itself: the
    /// canonical cocone must factor through the pushout via the identity, so
    /// any deviation signals that an inclusion image escapes the pushout's
    /// own generators or that the two legs disagree on a name the pushout
    /// identifies. The check delegates to [`ColimitResult::verify_universal`],
    /// which validates the mediator with [`check_morphism`]; it therefore
    /// requires the pushout theory to be a total signature, with every
    /// operation's input and output sorts declared locally.
    ///
    /// # Errors
    ///
    /// Returns whatever [`ColimitResult::verify_universal`] returns when the
    /// canonical cocone fails to factor, or [`GatError::EquationNotPreserved`]
    /// if the mediator sends any sort or operation to a name other than
    /// itself.
    pub fn verify_universal_identity(&self) -> Result<(), GatError> {
        let mediator = self.verify_universal(&self.theory, &self.inclusion1, &self.inclusion2)?;
        for (from, to) in &mediator.sort_map {
            if from != to {
                return Err(GatError::EquationNotPreserved {
                    equation: format!("universal identity sort {from}"),
                    detail: format!(
                        "mediator into the pushout maps sort `{from}` to `{to}`, not to itself",
                    ),
                });
            }
        }
        for (from, to) in &mediator.op_map {
            if to.as_op() != Some(from) {
                return Err(GatError::EquationNotPreserved {
                    equation: format!("universal identity op {from}"),
                    detail: format!(
                        "mediator into the pushout maps op `{from}` to `{to}`, not to itself",
                    ),
                });
            }
        }
        Ok(())
    }
}

/// Helper: thread the mediator assignment for one leg of the cocone.
/// `inclusion` maps T-names to P-names (here-codomain); `k` maps
/// T-names to Q-names. We assemble `mediator: P → Q` by composing.
fn merge_mediator_assignments(
    mediator: &mut HashMap<Arc<str>, Arc<str>>,
    inclusion: &HashMap<Arc<str>, Arc<str>>,
    k: &HashMap<Arc<str>, Arc<str>>,
    kind: &str,
    leg: &str,
) -> Result<(), GatError> {
    for (t_name, p_name) in inclusion {
        let q_name = k
            .get(t_name)
            .ok_or_else(|| GatError::EquationNotPreserved {
                equation: format!("universal property {kind} {p_name}"),
                detail: format!("{leg} has no mapping for {kind} `{t_name}`"),
            })?;
        match mediator.get(p_name) {
            None => {
                mediator.insert(Arc::clone(p_name), Arc::clone(q_name));
            }
            Some(existing) if existing == q_name => {}
            Some(existing) => {
                return Err(GatError::EquationNotPreserved {
                    equation: format!("universal property {kind} {p_name}"),
                    detail: format!(
                        "two T-preimages of `{p_name}` map to distinct Q-images under {leg}: \
                         `{existing}` vs `{q_name}`; the alternative cocone is not commutative",
                    ),
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::eq::{Equation, Term};
    use crate::op::Operation;
    use crate::sort::{Sort, SortExpr, SortParam};

    #[test]
    fn graph_constraint_colimit() {
        // Shared: just Vertex.
        let shared = Theory::new(
            "ThVertex",
            vec![Sort::simple("Vertex")],
            Vec::new(),
            Vec::new(),
        );

        // ThGraph: Vertex + Edge, ops src/tgt.
        let th_graph = Theory::new(
            "ThGraph",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![
                Operation::unary("src", "e", "Edge", "Vertex"),
                Operation::unary("tgt", "e", "Edge", "Vertex"),
            ],
            Vec::new(),
        );

        // ThConstraint: Vertex + Constraint, op target.
        let th_constraint = Theory::new(
            "ThConstraint",
            vec![Sort::simple("Vertex"), Sort::simple("Constraint")],
            vec![Operation::unary("target", "c", "Constraint", "Vertex")],
            Vec::new(),
        );

        let result = colimit_by_name(&th_graph, &th_constraint, &shared).unwrap();

        assert_eq!(&*result.name, "ThGraph_ThConstraint_colimit");
        assert_eq!(result.sorts.len(), 3); // Vertex, Edge, Constraint
        assert_eq!(result.ops.len(), 3); // src, tgt, target

        assert!(result.find_sort("Vertex").is_some());
        assert!(result.find_sort("Edge").is_some());
        assert!(result.find_sort("Constraint").is_some());
        assert!(result.find_op("src").is_some());
        assert!(result.find_op("tgt").is_some());
        assert!(result.find_op("target").is_some());
    }

    #[test]
    fn sort_conflict_detected() {
        let shared = Theory::new("Empty", Vec::new(), Vec::new(), Vec::new());

        let t1 = Theory::new("T1", vec![Sort::simple("X")], Vec::new(), Vec::new());
        let t2 = Theory::new(
            "T2",
            vec![Sort::dependent("X", vec![SortParam::new("a", "S")])],
            Vec::new(),
            Vec::new(),
        );

        let result = colimit_by_name(&t1, &t2, &shared);
        assert!(
            matches!(&result, Err(GatError::Composition(e)) if matches!(&**e, crate::compose::CompositionError::NameConflict { kind: crate::compose::SymKind::Sort, name, .. } if name.as_ref() == "X")),
            "{result:?}"
        );
    }

    #[test]
    fn op_conflict_detected() {
        let shared = Theory::new("Empty", Vec::new(), Vec::new(), Vec::new());

        let t1 = Theory::new(
            "T1",
            vec![Sort::simple("A"), Sort::simple("B")],
            vec![Operation::unary("f", "x", "A", "B")],
            Vec::new(),
        );
        let t2 = Theory::new(
            "T2",
            vec![Sort::simple("A"), Sort::simple("B")],
            vec![Operation::unary("f", "x", "B", "A")], // reversed
            Vec::new(),
        );

        let result = colimit_by_name(&t1, &t2, &shared);
        assert!(
            matches!(&result, Err(GatError::Composition(e)) if matches!(&**e, crate::compose::CompositionError::NameConflict { kind: crate::compose::SymKind::Op, name, .. } if name.as_ref() == "f")),
            "{result:?}"
        );
    }

    #[test]
    fn eq_conflict_detected() {
        let shared = Theory::new("Empty", Vec::new(), Vec::new(), Vec::new());

        // t1: ax says g(x, y) = x (two distinct variables).
        // t2: ax says g(a, a) = a (one variable, used twice).
        // These are NOT alpha-equivalent since the variable multiplicity differs.
        let g = || {
            Operation::new(
                "g",
                vec![
                    (Arc::from("l"), SortExpr::from("A")),
                    (Arc::from("r"), SortExpr::from("A")),
                ],
                "A",
            )
        };
        let t1 = Theory::new(
            "T1",
            vec![Sort::simple("A")],
            vec![g()],
            vec![Equation::new(
                "ax",
                Term::app("g", vec![Term::var("x"), Term::var("y")]),
                Term::var("x"),
            )],
        );
        let t2 = Theory::new(
            "T2",
            vec![Sort::simple("A")],
            vec![g()],
            vec![Equation::new(
                "ax",
                Term::app("g", vec![Term::var("a"), Term::var("a")]),
                Term::var("a"),
            )],
        );

        let result = colimit_by_name(&t1, &t2, &shared);
        assert!(
            matches!(&result, Err(GatError::Composition(e)) if matches!(&**e, crate::compose::CompositionError::NameConflict { kind: crate::compose::SymKind::Eq, name, .. } if name.as_ref() == "ax")),
            "{result:?}"
        );
    }

    #[test]
    fn alpha_equivalent_eqs_not_conflicted() {
        let shared = Theory::new("Empty", Vec::new(), Vec::new(), Vec::new());

        // Same equation with different variable names: should NOT conflict.
        let t1 = Theory::new(
            "T1",
            vec![Sort::simple("A")],
            vec![Operation::unary("g", "x", "A", "A")],
            vec![Equation::new(
                "ax",
                Term::app("g", vec![Term::var("x")]),
                Term::var("x"),
            )],
        );
        let t2 = Theory::new(
            "T2",
            vec![Sort::simple("A")],
            vec![Operation::unary("g", "x", "A", "A")],
            vec![Equation::new(
                "ax",
                Term::app("g", vec![Term::var("a")]),
                Term::var("a"),
            )],
        );

        let result = colimit_by_name(&t1, &t2, &shared).unwrap();
        assert_eq!(result.eqs.len(), 1);
    }

    /// Pins the amalgamation convention: two theories that each declare a
    /// same-name sort outside the shared theory have that sort identified
    /// (not disjointly duplicated) when their signatures are compatible.
    #[test]
    fn compatible_non_shared_duplicates_allowed() {
        let shared = Theory::new("Empty", Vec::new(), Vec::new(), Vec::new());

        // Both define identical sort X.
        let t1 = Theory::new("T1", vec![Sort::simple("X")], Vec::new(), Vec::new());
        let t2 = Theory::new("T2", vec![Sort::simple("X")], Vec::new(), Vec::new());

        let result = colimit_by_name(&t1, &t2, &shared).unwrap();
        assert_eq!(result.sorts.len(), 1);
    }

    #[test]
    fn colimit_merges_directed_eqs() {
        use crate::eq::{DirectedEquation, Term};

        let shared = Theory::new("Empty", Vec::new(), Vec::new(), Vec::new());

        let t1 = Theory::full(
            "T1",
            Vec::new(),
            vec![Sort::simple("A")],
            vec![Operation::unary("f", "x", "A", "A")],
            Vec::new(),
            vec![DirectedEquation::new(
                "rule1",
                Term::app("f", vec![Term::app("f", vec![Term::var("x")])]),
                Term::app("f", vec![Term::var("x")]),
                panproto_expr::Expr::Var("_".into()),
            )],
            Vec::new(),
        );

        let t2 = Theory::full(
            "T2",
            Vec::new(),
            vec![Sort::simple("A")],
            vec![
                Operation::unary("f", "x", "A", "A"),
                Operation::nullary("c", "A"),
            ],
            Vec::new(),
            vec![DirectedEquation::new(
                "rule2",
                Term::app("f", vec![Term::constant("c")]),
                Term::constant("c"),
                panproto_expr::Expr::Var("_".into()),
            )],
            Vec::new(),
        );

        let result = colimit_by_name(&t1, &t2, &shared).unwrap();
        assert_eq!(result.directed_eqs.len(), 2);
        assert!(result.find_directed_eq("rule1").is_some());
        assert!(result.find_directed_eq("rule2").is_some());
    }

    #[test]
    fn colimit_shared_directed_eq_not_duplicated() {
        use crate::eq::{DirectedEquation, Term};

        let de = DirectedEquation::new(
            "shared_rule",
            Term::app("f", vec![Term::var("x")]),
            Term::var("x"),
            panproto_expr::Expr::Var("_".into()),
        );

        let shared = Theory::full(
            "Shared",
            Vec::new(),
            vec![Sort::simple("A")],
            vec![Operation::unary("f", "x", "A", "A")],
            Vec::new(),
            vec![de.clone()],
            Vec::new(),
        );

        let t1 = Theory::full(
            "T1",
            Vec::new(),
            vec![Sort::simple("A")],
            vec![Operation::unary("f", "x", "A", "A")],
            Vec::new(),
            vec![de.clone()],
            Vec::new(),
        );
        let t2 = Theory::full(
            "T2",
            Vec::new(),
            vec![Sort::simple("A")],
            vec![Operation::unary("f", "x", "A", "A")],
            Vec::new(),
            vec![de],
            Vec::new(),
        );

        let result = colimit_by_name(&t1, &t2, &shared).unwrap();
        assert_eq!(result.directed_eqs.len(), 1);
    }

    #[test]
    fn colimit_directed_eq_conflict() {
        use crate::eq::{DirectedEquation, Term};

        let shared = Theory::new("Empty", Vec::new(), Vec::new(), Vec::new());

        // Both sides declare `A`, `f : A -> A` and constants `a, b : A`;
        // the two `rule`s differ in content.
        let side = |name: &str, rule: DirectedEquation| {
            Theory::full(
                name,
                Vec::new(),
                vec![Sort::simple("A")],
                vec![
                    Operation::unary("f", "x", "A", "A"),
                    Operation::nullary("a", "A"),
                    Operation::nullary("b", "A"),
                ],
                Vec::new(),
                vec![rule],
                Vec::new(),
            )
        };
        let t1 = side(
            "T1",
            DirectedEquation::new(
                "rule",
                Term::app("f", vec![Term::var("x")]),
                Term::var("x"),
                panproto_expr::Expr::Var("_".into()),
            ),
        );
        let t2 = side(
            "T2",
            DirectedEquation::new(
                "rule",
                Term::constant("a"),
                Term::constant("b"),
                panproto_expr::Expr::Var("_".into()),
            ),
        );

        let result = colimit_by_name(&t1, &t2, &shared);
        assert!(
            matches!(&result, Err(GatError::Composition(e)) if matches!(&**e, crate::compose::CompositionError::NameConflict { kind: crate::compose::SymKind::DirectedEq, name, .. } if name.as_ref() == "rule")),
            "{result:?}"
        );
    }

    #[test]
    fn colimit_merges_policies() {
        use crate::sort::ValueKind;
        use crate::theory::{ConflictPolicy, ConflictStrategy};

        let shared = Theory::new("Empty", Vec::new(), Vec::new(), Vec::new());

        let t1 = Theory::full(
            "T1",
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![ConflictPolicy {
                name: "p1".into(),
                value_kind: ValueKind::Str,
                strategy: ConflictStrategy::KeepLeft,
            }],
        );

        let t2 = Theory::full(
            "T2",
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![ConflictPolicy {
                name: "p2".into(),
                value_kind: ValueKind::Int,
                strategy: ConflictStrategy::Fail,
            }],
        );

        let result = colimit_by_name(&t1, &t2, &shared).unwrap();
        assert_eq!(result.policies.len(), 2);
        assert!(result.find_policy("p1").is_some());
        assert!(result.find_policy("p2").is_some());
    }

    #[test]
    fn colimit_policy_conflict() {
        use crate::sort::ValueKind;
        use crate::theory::{ConflictPolicy, ConflictStrategy};

        let shared = Theory::new("Empty", Vec::new(), Vec::new(), Vec::new());

        let t1 = Theory::full(
            "T1",
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![ConflictPolicy {
                name: "p".into(),
                value_kind: ValueKind::Str,
                strategy: ConflictStrategy::KeepLeft,
            }],
        );

        let t2 = Theory::full(
            "T2",
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![ConflictPolicy {
                name: "p".into(),
                value_kind: ValueKind::Str,
                strategy: ConflictStrategy::KeepRight, // Different strategy
            }],
        );

        let result = colimit_by_name(&t1, &t2, &shared);
        assert!(
            matches!(&result, Err(GatError::Composition(e)) if matches!(&**e, crate::compose::CompositionError::NameConflict { kind: crate::compose::SymKind::Policy, name, .. } if name.as_ref() == "p")),
            "{result:?}"
        );
    }

    #[test]
    fn colimit_equations_with_renamed_ops() {
        // Shared theory S has sort A and op f: A → A with equation e: f(f(x)) = f(x).
        // T1 keeps the names as-is.
        // T2 renames f → g but has the same equation.
        // Morphisms: i1 maps f→f; i2 maps f→g.
        // The colimit should identify them and the equation should NOT conflict.
        let shared = Theory::new(
            "Shared",
            vec![Sort::simple("A")],
            vec![Operation::unary("f", "x", "A", "A")],
            vec![Equation::new(
                "idem",
                Term::app("f", vec![Term::app("f", vec![Term::var("x")])]),
                Term::app("f", vec![Term::var("x")]),
            )],
        );

        let t1 = Theory::new(
            "T1",
            vec![Sort::simple("A")],
            vec![Operation::unary("f", "x", "A", "A")],
            vec![Equation::new(
                "idem",
                Term::app("f", vec![Term::app("f", vec![Term::var("x")])]),
                Term::app("f", vec![Term::var("x")]),
            )],
        );

        // T2 renames f → g
        let t2 = Theory::new(
            "T2",
            vec![Sort::simple("A")],
            vec![Operation::unary("g", "x", "A", "A")],
            vec![Equation::new(
                "idem",
                Term::app("g", vec![Term::app("g", vec![Term::var("x")])]),
                Term::app("g", vec![Term::var("x")]),
            )],
        );

        // Morphisms from Shared into T1 and T2.
        let i1 = TheoryMorphism::new(
            "i1",
            "Shared",
            "T1",
            HashMap::from([(Arc::from("A"), Arc::from("A"))]),
            HashMap::from([(Arc::from("f"), Arc::from("f"))]),
        );
        let i2 = TheoryMorphism::new(
            "i2",
            "Shared",
            "T2",
            HashMap::from([(Arc::from("A"), Arc::from("A"))]),
            HashMap::from([(Arc::from("f"), Arc::from("g"))]),
        );

        let result = colimit(&shared, &t1, &t2, &i1, &i2).unwrap();
        // The equation should be included exactly once (g renamed to f).
        assert_eq!(result.theory.eqs.len(), 1);
        assert!(result.theory.find_eq("idem").is_some());
        // The pushout should have op f (from T1's naming convention).
        assert!(result.theory.find_op("f").is_some());
        // g should NOT appear as a separate op (it was renamed to f).
        assert!(result.theory.find_op("g").is_none());
    }

    #[test]
    fn shared_declarations_not_duplicated() {
        let shared = Theory::new(
            "Shared",
            vec![Sort::simple("S")],
            vec![Operation::nullary("c", "S")],
            vec![Equation::new("e", Term::constant("c"), Term::constant("c"))],
        );

        let t1 = Theory::new(
            "T1",
            vec![Sort::simple("S"), Sort::simple("A")],
            vec![Operation::nullary("c", "S")],
            vec![Equation::new("e", Term::constant("c"), Term::constant("c"))],
        );
        let t2 = Theory::new(
            "T2",
            vec![Sort::simple("S"), Sort::simple("B")],
            vec![Operation::nullary("c", "S")],
            vec![Equation::new("e", Term::constant("c"), Term::constant("c"))],
        );

        let result = colimit_by_name(&t1, &t2, &shared).unwrap();
        assert_eq!(result.sorts.len(), 3); // S, A, B
        assert_eq!(result.ops.len(), 1); // c
        assert_eq!(result.eqs.len(), 1); // e
    }

    // --- proptest strategies and property tests ---

    mod property {
        use super::*;
        use proptest::prelude::*;

        /// Generate a colimit input: shared theory with 1-2 sorts, extended
        /// independently to T1 and T2 with 1-2 additional sorts/ops each.
        fn arb_colimit_input() -> impl Strategy<Value = (Theory, Theory, Theory)> {
            // Shared: 1-2 sorts, no ops
            let shared_sort_count = 1..=2usize;
            shared_sort_count
                .prop_flat_map(|n| {
                    let shared_sorts: Vec<Sort> =
                        (0..n).map(|i| Sort::simple(format!("Shared{i}"))).collect();
                    let shared = Theory::new("Shared", shared_sorts, Vec::new(), Vec::new());

                    // T1: shared sorts + 1-2 extra sorts + 0-2 ops
                    let extra1_count = 1..=2usize;
                    let extra2_count = 1..=2usize;
                    let op1_count = 0..=2usize;
                    let op2_count = 0..=2usize;
                    (
                        Just(shared),
                        extra1_count,
                        extra2_count,
                        op1_count,
                        op2_count,
                    )
                })
                .prop_map(|(shared, extra1, extra2, ops1, ops2)| {
                    let mut sorts1 = shared.sorts.clone();
                    for i in 0..extra1 {
                        sorts1.push(Sort::simple(format!("T1Extra{i}")));
                    }
                    let mut t1_ops = Vec::new();
                    for i in 0..std::cmp::min(ops1, sorts1.len()) {
                        t1_ops.push(Operation::unary(
                            format!("t1op{i}"),
                            "x",
                            &*sorts1[i % sorts1.len()].name,
                            &*sorts1[0].name,
                        ));
                    }
                    let t1 = Theory::new("T1", sorts1, t1_ops, Vec::new());

                    let mut sorts2 = shared.sorts.clone();
                    for i in 0..extra2 {
                        sorts2.push(Sort::simple(format!("T2Extra{i}")));
                    }
                    let mut t2_ops = Vec::new();
                    for i in 0..std::cmp::min(ops2, sorts2.len()) {
                        t2_ops.push(Operation::unary(
                            format!("t2op{i}"),
                            "x",
                            &*sorts2[i % sorts2.len()].name,
                            &*sorts2[0].name,
                        ));
                    }
                    let t2 = Theory::new("T2", sorts2, t2_ops, Vec::new());

                    (shared, t1, t2)
                })
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(256))]

            #[test]
            fn colimit_contains_all_sorts((shared, t1, t2) in arb_colimit_input()) {
                let result = colimit_by_name(&t1, &t2, &shared).unwrap();
                // All sorts from T1 should be in the colimit.
                for sort in &t1.sorts {
                    prop_assert!(
                        result.find_sort(&sort.name).is_some(),
                        "T1 sort {:?} missing from colimit",
                        sort.name,
                    );
                }
                // All sorts from T2 should be in the colimit.
                for sort in &t2.sorts {
                    prop_assert!(
                        result.find_sort(&sort.name).is_some(),
                        "T2 sort {:?} missing from colimit",
                        sort.name,
                    );
                }
            }

            #[test]
            fn colimit_contains_all_ops((shared, t1, t2) in arb_colimit_input()) {
                let result = colimit_by_name(&t1, &t2, &shared).unwrap();
                for op in &t1.ops {
                    prop_assert!(
                        result.find_op(&op.name).is_some(),
                        "T1 op {:?} missing from colimit",
                        op.name,
                    );
                }
                for op in &t2.ops {
                    prop_assert!(
                        result.find_op(&op.name).is_some(),
                        "T2 op {:?} missing from colimit",
                        op.name,
                    );
                }
            }

            #[test]
            fn colimit_shared_not_duplicated((shared, t1, t2) in arb_colimit_input()) {
                let result = colimit_by_name(&t1, &t2, &shared).unwrap();
                // Each shared sort appears exactly once.
                for sort in &shared.sorts {
                    let count = result.sorts.iter().filter(|s| s.name == sort.name).count();
                    prop_assert_eq!(count, 1, "shared sort {:?} duplicated", sort.name);
                }
            }

            #[test]
            fn colimit_is_commutative((shared, t1, t2) in arb_colimit_input()) {
                let result_12 = colimit_by_name(&t1, &t2, &shared).unwrap();
                let result_21 = colimit_by_name(&t2, &t1, &shared).unwrap();
                prop_assert_eq!(
                    result_12.sorts.len(),
                    result_21.sorts.len(),
                    "commutative: same sort count",
                );
                prop_assert_eq!(
                    result_12.ops.len(),
                    result_21.ops.len(),
                    "commutative: same op count",
                );
            }

            /// Universal property: identity inclusions of `shared`
            /// give the trivial cocone `(shared, shared)`, and the
            /// identity-on-pushout cocone `(P, j1, j2)` factors through
            /// the pushout itself via the identity mediator.
            #[test]
            fn pushout_universal_property_identity_cocone(
                (shared, t1, t2) in arb_colimit_input()
            ) {
                let result = pushout_by_name(&t1, &t2, &shared).unwrap();
                // The pushout itself with its inclusions is the canonical cocone.
                // The mediator into the pushout from itself must be the identity.
                let m = result
                    .verify_universal(&result.theory, &result.inclusion1, &result.inclusion2)
                    .expect("universal property: identity cocone factors");
                // The identity mediator sends every sort/op to itself.
                for (k, v) in &m.sort_map {
                    prop_assert_eq!(k, v, "identity mediator must be identity on sorts");
                }
                for (k, v) in &m.op_map {
                    prop_assert_eq!(v.as_op(), Some(k), "identity mediator must be identity on ops");
                }
            }
        }
    }

    /// Verify the universal property end-to-end on a concrete graph
    /// example: an alternative cocone landing in a larger theory must
    /// factor uniquely through the pushout.
    #[test]
    fn pushout_universal_property_graph_constraint() {
        use crate::sort::Sort;

        let shared = Theory::new(
            "ThVertex",
            vec![Sort::simple("Vertex")],
            Vec::new(),
            Vec::new(),
        );
        let th_graph = Theory::new(
            "ThGraph",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![Operation::unary("src", "e", "Edge", "Vertex")],
            Vec::new(),
        );
        let th_constraint = Theory::new(
            "ThConstraint",
            vec![Sort::simple("Vertex"), Sort::simple("Constraint")],
            vec![Operation::unary("target", "c", "Constraint", "Vertex")],
            Vec::new(),
        );
        let pushout = pushout_by_name(&th_graph, &th_constraint, &shared).unwrap();

        // Build an alternative cocone into a larger theory `Q` that
        // contains every sort/op of the pushout plus some extra
        // material.
        let q = Theory::new(
            "Q",
            vec![
                Sort::simple("Vertex"),
                Sort::simple("Edge"),
                Sort::simple("Constraint"),
                Sort::simple("Extra"),
            ],
            vec![
                Operation::unary("src", "e", "Edge", "Vertex"),
                Operation::unary("target", "c", "Constraint", "Vertex"),
            ],
            Vec::new(),
        );
        let mut k1_sort_map = HashMap::new();
        k1_sort_map.insert(Arc::from("Vertex"), Arc::from("Vertex"));
        k1_sort_map.insert(Arc::from("Edge"), Arc::from("Edge"));
        let mut k1_op_map = HashMap::new();
        k1_op_map.insert(Arc::from("src"), Arc::from("src"));
        let k1 = TheoryMorphism::new("k1", "ThGraph", "Q", k1_sort_map, k1_op_map);

        let mut k2_sort_map = HashMap::new();
        k2_sort_map.insert(Arc::from("Vertex"), Arc::from("Vertex"));
        k2_sort_map.insert(Arc::from("Constraint"), Arc::from("Constraint"));
        let mut k2_op_map = HashMap::new();
        k2_op_map.insert(Arc::from("target"), Arc::from("target"));
        let k2 = TheoryMorphism::new("k2", "ThConstraint", "Q", k2_sort_map, k2_op_map);

        let mediator = pushout
            .verify_universal(&q, &k1, &k2)
            .expect("alternative cocone factors through pushout");
        // Mediator must send every shared name to its corresponding Q name.
        assert_eq!(
            mediator.sort_map.get("Vertex").map(AsRef::as_ref),
            Some("Vertex")
        );
        assert_eq!(
            mediator.sort_map.get("Edge").map(AsRef::as_ref),
            Some("Edge")
        );
        assert_eq!(
            mediator.sort_map.get("Constraint").map(AsRef::as_ref),
            Some("Constraint")
        );
    }

    /// A cocone whose name maps commute but whose target theory declares
    /// an operation with an incompatible signature must be rejected when
    /// the mediator is validated as a morphism, not accepted on the
    /// strength of the name maps alone.
    #[test]
    fn verify_universal_rejects_signature_incompatible_mediator() {
        let shared = Theory::new(
            "ThVertex",
            vec![Sort::simple("Vertex")],
            Vec::new(),
            Vec::new(),
        );
        let th_graph = Theory::new(
            "ThGraph",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![Operation::unary("src", "e", "Edge", "Vertex")],
            Vec::new(),
        );
        let th_constraint = Theory::new(
            "ThConstraint",
            vec![Sort::simple("Vertex"), Sort::simple("Constraint")],
            vec![Operation::unary("target", "c", "Constraint", "Vertex")],
            Vec::new(),
        );
        let pushout = pushout_by_name(&th_graph, &th_constraint, &shared).unwrap();

        // Q declares `src` with a reversed signature (Vertex -> Edge). The
        // name maps below still commute over the shared Vertex, so the
        // mediator is constructed, but it is not a valid morphism into Q.
        let q = Theory::new(
            "Q",
            vec![
                Sort::simple("Vertex"),
                Sort::simple("Edge"),
                Sort::simple("Constraint"),
            ],
            vec![
                Operation::unary("src", "v", "Vertex", "Edge"),
                Operation::unary("target", "c", "Constraint", "Vertex"),
            ],
            Vec::new(),
        );

        let mut k1_sort_map = HashMap::new();
        k1_sort_map.insert(Arc::from("Vertex"), Arc::from("Vertex"));
        k1_sort_map.insert(Arc::from("Edge"), Arc::from("Edge"));
        let mut k1_op_map = HashMap::new();
        k1_op_map.insert(Arc::from("src"), Arc::from("src"));
        let k1 = TheoryMorphism::new("k1", "ThGraph", "Q", k1_sort_map, k1_op_map);

        let mut k2_sort_map = HashMap::new();
        k2_sort_map.insert(Arc::from("Vertex"), Arc::from("Vertex"));
        k2_sort_map.insert(Arc::from("Constraint"), Arc::from("Constraint"));
        let mut k2_op_map = HashMap::new();
        k2_op_map.insert(Arc::from("target"), Arc::from("target"));
        let k2 = TheoryMorphism::new("k2", "ThConstraint", "Q", k2_sort_map, k2_op_map);

        let result = pushout.verify_universal(&q, &k1, &k2);
        assert!(
            matches!(result, Err(GatError::OpTypeMismatch { .. })),
            "expected the mediator's signature violation to be rejected, got {result:?}"
        );
    }

    /// A colimit leg that identifies two shared elements with a single
    /// element of the other theory builds the quotient: every element the
    /// span relates lands in one class, named after its first-side member,
    /// identically across runs regardless of map iteration order.
    #[test]
    fn non_injective_colimit_leg_builds_the_quotient_deterministically() {
        let shared = Theory::new(
            "S",
            vec![Sort::simple("A"), Sort::simple("B")],
            Vec::new(),
            Vec::new(),
        );
        let t1 = Theory::new(
            "T1",
            vec![Sort::simple("X"), Sort::simple("Y")],
            Vec::new(),
            Vec::new(),
        );
        let t2 = Theory::new("T2", vec![Sort::simple("Z")], Vec::new(), Vec::new());

        // i1 sends A -> X and B -> Y; i2 sends both A and B to Z.
        let i1 = sort_leg("i1", "S", "T1", &[("A", "X"), ("B", "Y")]);
        let i2 = sort_leg("i2", "S", "T2", &[("A", "Z"), ("B", "Z")]);

        let mut results = std::collections::HashSet::new();
        for _ in 0..10 {
            let result = colimit(&shared, &t1, &t2, &i1, &i2).expect("the quotient exists");
            let names: Vec<String> = result
                .theory
                .sorts
                .iter()
                .map(|s| s.name.to_string())
                .collect();
            let mut j2: Vec<(String, String)> = result
                .inclusion2
                .sort_map
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect();
            j2.sort();
            results.insert((names, j2));
        }
        assert_eq!(
            results.len(),
            1,
            "result must be identical across runs: {results:?}"
        );
        let (names, j2) = results.into_iter().next().unwrap();
        assert_eq!(names, vec!["X".to_owned()]);
        assert_eq!(j2, vec![("Z".to_owned(), "X".to_owned())]);
    }

    /// Two theories carrying the same axiom under different names collapse
    /// to a single equation in the colimit: dedup is by content, not name.
    #[test]
    fn colimit_by_name_dedups_alpha_equivalent_eqs_under_different_names() {
        let shared = Theory::new("Empty", Vec::new(), Vec::new(), Vec::new());
        let t1 = Theory::new(
            "T1",
            vec![Sort::simple("A")],
            vec![Operation::unary("f", "x", "A", "A")],
            vec![Equation::new(
                "t1_ax",
                Term::app("f", vec![Term::var("x")]),
                Term::var("x"),
            )],
        );
        let t2 = Theory::new(
            "T2",
            vec![Sort::simple("A")],
            vec![Operation::unary("f", "x", "A", "A")],
            vec![Equation::new(
                "t2_ax",
                Term::app("f", vec![Term::var("y")]),
                Term::var("y"),
            )],
        );
        let result = colimit_by_name(&t1, &t2, &shared).unwrap();
        assert_eq!(
            result.eqs.len(),
            1,
            "alpha-equivalent axioms under different names must dedup to one, got {:?}",
            result.eqs
        );
    }

    /// The universal-identity check rejects a hand-built [`ColimitResult`]
    /// whose pushout theory declares a sort that neither inclusion covers:
    /// no mediator can be defined on such an orphan generator, so the
    /// canonical cocone cannot factor through the identity.
    #[test]
    fn colimit_result_rejects_uncovered_pushout_sort() {
        let theory = Theory::new(
            "P",
            vec![Sort::simple("Covered"), Sort::simple("Orphan")],
            Vec::new(),
            Vec::new(),
        );
        let inclusion1 = TheoryMorphism::new(
            "j1",
            "T1",
            "P",
            HashMap::from([(Arc::from("Covered"), Arc::from("Covered"))]),
            HashMap::<Arc<str>, Arc<str>>::new(),
        );
        let inclusion2 = TheoryMorphism::new(
            "j2",
            "T2",
            "P",
            HashMap::new(),
            HashMap::<Arc<str>, Arc<str>>::new(),
        );
        let result = ColimitResult {
            theory,
            inclusion1,
            inclusion2,
            provenance: crate::compose::Provenance::default(),
            report: CompositionReport::new(
                CompositionMode::Amalgamate,
                true,
                Vec::new(),
                Vec::new(),
            ),
        };
        let err = result
            .verify_universal_identity()
            .expect_err("orphan pushout sort must be rejected");
        assert!(
            matches!(err, GatError::EquationNotPreserved { .. }),
            "expected EquationNotPreserved for the uncovered sort, got {err:?}",
        );
    }

    /// A colimit of two standalone-total theories passes the
    /// universal-identity check: the canonical cocone factors through the
    /// pushout via the identity mediator, and every operation's sorts are
    /// declared locally so the mediator validates as a total morphism.
    #[test]
    fn colimit_runs_universal_identity_check() {
        // Base carrier shared by both extensions.
        let shared = Theory::new("Base", vec![Sort::simple("M")], Vec::new(), Vec::new());
        // A monoid-like extension: mul and unit over the shared carrier.
        let t1 = Theory::new(
            "T1",
            vec![Sort::simple("M")],
            vec![
                Operation::new(
                    "mul",
                    vec![("a".into(), "M".into()), ("b".into(), "M".into())],
                    "M",
                ),
                Operation::nullary("unit", "M"),
            ],
            Vec::new(),
        );
        // A pointed extension over the same carrier.
        let t2 = Theory::new(
            "T2",
            vec![Sort::simple("M")],
            vec![Operation::nullary("point", "M")],
            Vec::new(),
        );
        let result = pushout_by_name(&t1, &t2, &shared).expect("standalone-total colimit succeeds");
        result
            .verify_universal_identity()
            .expect("canonical cocone factors via the identity mediator");
    }

    // --- operation renaming inside dependent-sort argument terms ---

    /// A theory carrying one point and one dependent sort whose argument
    /// term mentions that point.
    fn pointed_loop_theory(name: &str, point_sort: &str, point: &str, loop_op: &str) -> Theory {
        Theory::new(
            name,
            vec![
                Sort::simple(point_sort),
                Sort::dependent("Loop", vec![SortParam::new("p", point_sort)]),
            ],
            vec![
                Operation::new(point, vec![], point_sort),
                Operation::new(
                    loop_op,
                    vec![],
                    crate::sort::SortExpr::app("Loop", vec![Term::constant(point)]),
                ),
            ],
            vec![],
        )
    }

    #[test]
    fn pushout_merges_same_name_sorts_compatible_after_renaming() {
        // T1 and T2 both declare `Loop`, over point sorts the span
        // identifies. The two declarations agree once the identification is
        // applied, so the pushout carries a single `Loop`.
        let shared = Theory::new(
            "S",
            vec![Sort::simple("Pt")],
            vec![Operation::new("pt", vec![], "Pt")],
            vec![],
        );
        let t1 = pointed_loop_theory("T1", "Pt", "pt", "loop1");
        let t2 = pointed_loop_theory("T2", "Pt2", "pt2", "loop2");
        let i1 = TheoryMorphism::new(
            "i1",
            "S",
            "T1",
            HashMap::from([(Arc::from("Pt"), Arc::from("Pt"))]),
            HashMap::from([(Arc::from("pt"), Arc::from("pt"))]),
        );
        let i2 = TheoryMorphism::new(
            "i2",
            "S",
            "T2",
            HashMap::from([(Arc::from("Pt"), Arc::from("Pt2"))]),
            HashMap::from([(Arc::from("pt"), Arc::from("pt2"))]),
        );
        let result = colimit(&shared, &t1, &t2, &i1, &i2).expect("pushout exists");
        assert_eq!(
            result
                .theory
                .sorts
                .iter()
                .filter(|s| &*s.name == "Loop")
                .count(),
            1,
            "the two Loop declarations agree after renaming: {:?}",
            result.theory.sorts,
        );
    }

    #[test]
    fn pushout_renames_ops_inside_dependent_sort_arguments() {
        // The span identifies T1's point with T2's point. T2's `loop2` has
        // output sort `Loop(pt2())`, so the pushout must rewrite the
        // argument term too, not just the sort head.
        let shared = Theory::new(
            "S",
            vec![Sort::simple("Pt")],
            vec![Operation::new("pt", vec![], "Pt")],
            vec![],
        );
        let t1 = pointed_loop_theory("T1", "Pt", "pt", "loop1");
        let t2 = pointed_loop_theory("T2", "Pt2", "pt2", "loop2");

        let i1 = TheoryMorphism::new(
            "i1",
            "S",
            "T1",
            HashMap::from([(Arc::from("Pt"), Arc::from("Pt"))]),
            HashMap::from([(Arc::from("pt"), Arc::from("pt"))]),
        );
        let i2 = TheoryMorphism::new(
            "i2",
            "S",
            "T2",
            HashMap::from([(Arc::from("Pt"), Arc::from("Pt2"))]),
            HashMap::from([(Arc::from("pt"), Arc::from("pt2"))]),
        );

        let result = colimit(&shared, &t1, &t2, &i1, &i2).expect("pushout exists");

        // No surviving signature may mention a name the pushout quotiented
        // away.
        for op in &result.theory.ops {
            for arg in op.output.args() {
                for name in referenced_ops(arg) {
                    assert!(
                        result.theory.has_op(&name),
                        "op `{}` output mentions `{name}`, absent from the pushout: {:?}",
                        op.name,
                        op.output,
                    );
                }
            }
        }

        // The T2 inclusion must be a genuine morphism into the pushout.
        check_morphism(&result.inclusion2, &t2, &result.theory)
            .expect("T2 inclusion is a morphism into the pushout");
        check_morphism(&result.inclusion1, &t1, &result.theory)
            .expect("T1 inclusion is a morphism into the pushout");
        result
            .verify_cocone(&i1, &i2, &shared, &t1, &t2)
            .expect("the pushout cocone commutes");
    }

    // --- failing-first cases for the composition defects ---

    /// A morphism that only maps sorts.
    fn sort_leg(name: &str, dom: &str, cod: &str, sorts: &[(&str, &str)]) -> TheoryMorphism {
        TheoryMorphism::new(
            name,
            dom,
            cod,
            sorts
                .iter()
                .map(|(a, b)| (Arc::from(*a), Arc::from(*b)))
                .collect(),
            HashMap::<Arc<str>, Arc<str>>::new(),
        )
    }

    /// A span whose legs identify `X` and `Y` in one leg and keep them apart
    /// in the other is a quotient: the pushout identifies `A` with `B`. The
    /// result must not depend on which leg is passed first.
    #[test]
    fn non_injective_leg_builds_the_quotient_in_either_argument_order() {
        let u1 = Theory::new("U1", vec![Sort::simple("V")], vec![], vec![]);
        let u2 = Theory::new(
            "U2",
            vec![Sort::simple("A"), Sort::simple("B")],
            vec![],
            vec![],
        );
        let s2 = Theory::new(
            "S2",
            vec![Sort::simple("X"), Sort::simple("Y")],
            vec![],
            vec![],
        );
        let j1 = sort_leg("j1", "S2", "U1", &[("X", "V"), ("Y", "V")]);
        let j2 = sort_leg("j2", "S2", "U2", &[("X", "A"), ("Y", "B")]);
        let forward = colimit(&s2, &u1, &u2, &j1, &j2).expect("non-injective first leg");
        let backward = colimit(&s2, &u2, &u1, &j2, &j1).expect("non-injective second leg");
        assert_eq!(forward.theory.sorts.len(), 1, "{:?}", forward.theory.sorts);
        assert_eq!(
            backward.theory.sorts.len(),
            1,
            "{:?}",
            backward.theory.sorts
        );
    }

    /// Identifying a simple sort with a dependent one collapses sorts of
    /// different arity; the pushout must refuse it rather than return a
    /// theory whose inclusion is not a morphism.
    #[test]
    fn collapsing_sorts_of_different_arity_is_a_typed_obstruction() {
        // The span that sends a simple sort to a dependent one has an ill
        // typed leg, rejected before anything is identified, whichever side
        // is passed first.
        let (s3, w1, w2, k1, k2) = arity_collapsing_span();
        for result in [
            colimit(&s3, &w1, &w2, &k1, &k2),
            colimit(&s3, &w2, &w1, &k2, &k1),
        ] {
            match result {
                Err(GatError::Composition(err)) => match *err {
                    crate::compose::CompositionError::InvalidLeg { ref detail, .. } => {
                        assert!(detail.contains("arity"), "{detail}");
                    }
                    other => panic!("expected an ill-typed leg, got {other:?}"),
                },
                other => panic!("collapse accepted: {other:?}"),
            }
        }
        // Same-name sorts of different arity that only the name rule
        // relates conflict by name.
        let simple = Theory::new(
            "T1",
            vec![Sort::simple("Ob"), Sort::simple("B")],
            vec![],
            vec![],
        );
        let dependent = Theory::new(
            "T2",
            vec![
                Sort::simple("Ob"),
                Sort::dependent("B", vec![SortParam::new("o", "Ob")]),
            ],
            vec![],
            vec![],
        );
        let apex = Theory::new("S", vec![Sort::simple("Ob")], vec![], vec![]);
        let result = pushout_by_name(&simple, &dependent, &apex);
        assert!(
            matches!(&result, Err(GatError::Composition(e)) if matches!(**e, crate::compose::CompositionError::NameConflict { kind: crate::compose::SymKind::Sort, .. })),
            "{result:?}"
        );
    }

    /// The span of the arity-collapse case: `X` and `Y` both go to `V` on the
    /// left, and to the simple `A` and the dependent `B(o : Ob)` on the right.
    fn arity_collapsing_span() -> (Theory, Theory, Theory, TheoryMorphism, TheoryMorphism) {
        let w2 = Theory::new(
            "W2",
            vec![
                Sort::simple("Ob"),
                Sort::simple("A"),
                Sort::dependent("B", vec![SortParam::new("o", "Ob")]),
            ],
            vec![Operation::new(
                "mk",
                vec![(Arc::from("o"), crate::sort::SortExpr::from("Ob"))],
                crate::sort::SortExpr::app("B", vec![Term::var("o")]),
            )],
            vec![],
        );
        let w1 = Theory::new(
            "W1",
            vec![Sort::simple("Ob"), Sort::simple("V")],
            vec![],
            vec![],
        );
        let s3 = Theory::new(
            "S3",
            vec![Sort::simple("Ob"), Sort::simple("X"), Sort::simple("Y")],
            vec![],
            vec![],
        );
        let k1 = sort_leg("k1", "S3", "W1", &[("Ob", "Ob"), ("X", "V"), ("Y", "V")]);
        let k2 = sort_leg("k2", "S3", "W2", &[("Ob", "Ob"), ("X", "A"), ("Y", "B")]);
        (s3, w1, w2, k1, k2)
    }

    /// Whatever a colimit returns must be well typed: its inclusions are
    /// morphisms and its signature passes the typechecker.
    #[test]
    fn colimit_output_signatures_typecheck() {
        let (s3, w1, w2, k1, k2) = arity_collapsing_span();
        if let Ok(result) = colimit(&s3, &w1, &w2, &k1, &k2) {
            crate::typecheck::typecheck_theory(&result.theory).expect("pushout typechecks");
            check_morphism(&result.inclusion1, &w1, &result.theory).expect("j1 is a morphism");
            check_morphism(&result.inclusion2, &w2, &result.theory).expect("j2 is a morphism");
        }
        // A well-typed dependent span: its pushout's signature and both
        // inclusions check.
        let shared = Theory::new(
            "P",
            vec![Sort::simple("Pt")],
            vec![Operation::new("pt", vec![], "Pt")],
            vec![],
        );
        let t1 = pointed_loop_theory("T1", "Pt", "pt", "loop1");
        let t2 = pointed_loop_theory("T2", "Pt2", "pt2", "loop2");
        let i1 = TheoryMorphism::new(
            "i1",
            "P",
            "T1",
            HashMap::from([(Arc::from("Pt"), Arc::from("Pt"))]),
            HashMap::from([(Arc::from("pt"), Arc::from("pt"))]),
        );
        let i2 = TheoryMorphism::new(
            "i2",
            "P",
            "T2",
            HashMap::from([(Arc::from("Pt"), Arc::from("Pt2"))]),
            HashMap::from([(Arc::from("pt"), Arc::from("pt2"))]),
        );
        let result = colimit(&shared, &t1, &t2, &i1, &i2).expect("pushout exists");
        crate::typecheck::typecheck_signature(&result.theory).expect("pushout signature checks");
        crate::typecheck::typecheck_theory(&result.theory).expect("pushout typechecks");
        check_morphism(&result.inclusion1, &t1, &result.theory).expect("j1 is a morphism");
        check_morphism(&result.inclusion2, &t2, &result.theory).expect("j2 is a morphism");
    }

    /// A leg that sends a shared operation to a derived term is a valid
    /// morphism; the pushout identifies the operation with the term by
    /// adding the equation `e() = inv(u())`.
    #[test]
    fn derived_term_leg_adds_the_identifying_equation() {
        let sp = Theory::new(
            "SP",
            vec![Sort::simple("A")],
            vec![Operation::nullary("e", "A")],
            vec![],
        );
        let tp1 = Theory::new(
            "TP1",
            vec![Sort::simple("A")],
            vec![Operation::nullary("e", "A")],
            vec![],
        );
        let tp2 = Theory::new(
            "TP2",
            vec![Sort::simple("A")],
            vec![
                Operation::nullary("u", "A"),
                Operation::unary("inv", "x", "A", "A"),
            ],
            vec![],
        );
        let ip1 = TheoryMorphism::new(
            "ip1",
            "SP",
            "TP1",
            HashMap::from([(Arc::from("A"), Arc::from("A"))]),
            HashMap::from([(Arc::from("e"), Arc::from("e"))]),
        );
        let ip2 = TheoryMorphism::new(
            "ip2",
            "SP",
            "TP2",
            HashMap::from([(Arc::from("A"), Arc::from("A"))]),
            HashMap::from([(
                Arc::from("e"),
                crate::morphism::OpAssignment::Term(Term::app("inv", vec![Term::constant("u")])),
            )]),
        );
        check_morphism(&ip2, &sp, &tp2).expect("the derived-term leg is a morphism");
        let result = colimit(&sp, &tp1, &tp2, &ip1, &ip2).expect("derived-term span composes");
        let expected = (
            Term::constant("e"),
            Term::app("inv", vec![Term::constant("u")]),
        );
        assert!(
            result
                .theory
                .eqs
                .iter()
                .any(|eq| eq.lhs == expected.0 && eq.rhs == expected.1),
            "missing e() = inv(u()): {:?}",
            result.theory.eqs,
        );
        assert_eq!(
            result
                .theory
                .eqs
                .iter()
                .map(|e| e.name.to_string())
                .collect::<Vec<_>>(),
            vec!["apex.e~1".to_owned()]
        );
        assert!(result.provenance.added_eqs.contains_key("apex.e~1"));
        let cocone: Vec<_> = result
            .report
            .records(crate::compose::CompositionCheckKind::CoconeCommutes)
            .map(|r| (r.subject.clone(), r.outcome.clone()))
            .collect();
        assert_eq!(
            cocone,
            vec![
                (
                    crate::compose::CheckSubject::Edge(0),
                    crate::compose::CheckOutcome::Passed
                ),
                (
                    crate::compose::CheckSubject::Edge(1),
                    crate::compose::CheckOutcome::PassedByEquation {
                        equation: "apex.e~1".into()
                    }
                ),
            ]
        );
        check_morphism(&result.inclusion2, &tp2, &result.theory).expect("j2 is a morphism");
    }

    /// A shared operation typed `A -> B` in the apex and `B -> A` in one
    /// side makes that leg ill typed; the pushout must reject the span.
    #[test]
    fn ill_typed_leg_is_rejected_before_composition() {
        let l = Theory::new(
            "L",
            vec![Sort::simple("A"), Sort::simple("B")],
            vec![Operation::unary("f", "x", "A", "B")],
            vec![],
        );
        let rt = Theory::new(
            "Rt",
            vec![Sort::simple("A"), Sort::simple("B")],
            vec![Operation::unary("f", "x", "B", "A")],
            vec![],
        );
        let sh = Theory::new(
            "ShF",
            vec![Sort::simple("A"), Sort::simple("B")],
            vec![Operation::unary("f", "x", "A", "B")],
            vec![],
        );
        let result = pushout_by_name(&l, &rt, &sh);
        match &result {
            Err(GatError::Composition(err)) => assert!(
                matches!(
                    **err,
                    crate::compose::CompositionError::InvalidLeg { edge: 1, .. }
                ),
                "{err:?}"
            ),
            other => panic!("ill-typed leg accepted: {other:?}"),
        }
        let by_name = colimit_by_name(&l, &rt, &sh);
        assert!(by_name.is_err(), "ill-typed leg accepted: {by_name:?}");
    }

    /// The wrappers keep the names their pushouts have always had: a class
    /// is named after its member in the first side, else the second, even
    /// where the majority rule of `compose` would choose another member.
    #[test]
    fn amalgamate_wrappers_name_classes_by_their_t1_member() {
        let shared = Theory::new(
            "S",
            vec![Sort::simple("Pt")],
            vec![Operation::nullary("x", "Pt")],
            Vec::new(),
        );
        let t1 = Theory::new(
            "T1",
            vec![Sort::simple("Point")],
            vec![Operation::nullary("a", "Point")],
            Vec::new(),
        );
        let t2 = Theory::new(
            "T2",
            vec![Sort::simple("P2")],
            vec![Operation::nullary("b", "P2")],
            Vec::new(),
        );
        let i1 = TheoryMorphism::new(
            "i1",
            "S",
            "T1",
            HashMap::from([(Arc::from("Pt"), Arc::from("Point"))]),
            HashMap::from([(Arc::from("x"), Arc::from("a"))]),
        );
        let i2 = TheoryMorphism::new(
            "i2",
            "S",
            "T2",
            HashMap::from([(Arc::from("Pt"), Arc::from("P2"))]),
            HashMap::from([(Arc::from("x"), Arc::from("b"))]),
        );
        let result = colimit(&shared, &t1, &t2, &i1, &i2).expect("pushout exists");
        assert_eq!(result.theory.sorts[0].name.as_ref(), "Point");
        assert_eq!(result.theory.ops[0].name.as_ref(), "a");
        assert_eq!(
            result.inclusion2.sort_map.get("P2").map(AsRef::as_ref),
            Some("Point")
        );
        assert_eq!(
            result
                .inclusion2
                .op_map
                .get("b")
                .and_then(|a| a.as_op())
                .map(AsRef::as_ref),
            Some("a")
        );
        assert_eq!(
            result
                .provenance
                .members(crate::compose::SymKind::Sort, "Point")
                .len(),
            3
        );
        // The same span through `compose` takes the majority name, `P2`.
        let diagram =
            crate::compose::TheoryDiagram::span(("S", shared), ("T1", t1), ("T2", t2), i1, i2);
        let composed = crate::compose::compose(
            &diagram,
            &ComposeOptions {
                mode: CompositionMode::Amalgamate,
                result_name: "R".into(),
                clash: ClashPolicy::Reject,
                rewrite_budget: DEFAULT_VERIFY_BUDGET,
            },
        )
        .expect("composes");
        assert_eq!(composed.theory.sorts[0].name.as_ref(), "P2");
    }

    /// Every operation name applied anywhere inside a term.
    fn referenced_ops(term: &Term) -> Vec<Arc<str>> {
        let mut names = Vec::new();
        collect_referenced_ops(term, &mut names);
        names
    }

    fn collect_referenced_ops(term: &Term, names: &mut Vec<Arc<str>>) {
        match term {
            Term::Var(_) | Term::Hole { .. } => {}
            Term::App { op, args } => {
                names.push(Arc::clone(op));
                for arg in args {
                    collect_referenced_ops(arg, names);
                }
            }
            Term::Case {
                scrutinee,
                branches,
            } => {
                collect_referenced_ops(scrutinee, names);
                for branch in branches {
                    names.push(Arc::clone(&branch.constructor));
                    collect_referenced_ops(&branch.body, names);
                }
            }
            Term::Let { bound, body, .. } => {
                collect_referenced_ops(bound, names);
                collect_referenced_ops(body, names);
            }
        }
    }

    /// The pushout of `ThGraph` (`Vertex`, `Edge`, `src`) and
    /// `ThConstraint` (`Vertex`, `Constraint`, `target`) over `Vertex`,
    /// a target theory `Q` with every pushout symbol plus `Extra`, and
    /// the identity-named legs into `Q`.
    fn graph_constraint_cocone() -> (ColimitResult, Theory, TheoryMorphism, TheoryMorphism) {
        let shared = Theory::new(
            "ThVertex",
            vec![Sort::simple("Vertex")],
            Vec::new(),
            Vec::new(),
        );
        let th_graph = Theory::new(
            "ThGraph",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![Operation::unary("src", "e", "Edge", "Vertex")],
            Vec::new(),
        );
        let th_constraint = Theory::new(
            "ThConstraint",
            vec![Sort::simple("Vertex"), Sort::simple("Constraint")],
            vec![Operation::unary("target", "c", "Constraint", "Vertex")],
            Vec::new(),
        );
        let pushout = pushout_by_name(&th_graph, &th_constraint, &shared).unwrap();
        let q = Theory::new(
            "Q",
            vec![
                Sort::simple("Vertex"),
                Sort::simple("Edge"),
                Sort::simple("Constraint"),
                Sort::simple("Extra"),
            ],
            vec![
                Operation::unary("src", "e", "Edge", "Vertex"),
                Operation::unary("target", "c", "Constraint", "Vertex"),
            ],
            Vec::new(),
        );
        let k1 = cocone_leg(
            "k1",
            "ThGraph",
            &[("Vertex", "Vertex"), ("Edge", "Edge")],
            &[("src", "src")],
        );
        let k2 = cocone_leg(
            "k2",
            "ThConstraint",
            &[("Vertex", "Vertex"), ("Constraint", "Constraint")],
            &[("target", "target")],
        );
        (pushout, q, k1, k2)
    }

    fn cocone_leg(
        name: &str,
        domain: &str,
        sorts: &[(&str, &str)],
        ops: &[(&str, &str)],
    ) -> TheoryMorphism {
        TheoryMorphism::new(
            name,
            domain,
            "Q",
            sorts
                .iter()
                .map(|(a, b)| (Arc::from(*a), Arc::from(*b)))
                .collect(),
            ops.iter()
                .map(|(a, b)| (Arc::from(*a), Arc::from(*b)))
                .collect::<HashMap<Arc<str>, Arc<str>>>(),
        )
    }

    fn not_preserved(result: Result<TheoryMorphism, GatError>) -> (String, String) {
        match result {
            Err(GatError::EquationNotPreserved { equation, detail }) => (equation, detail),
            other => panic!("expected EquationNotPreserved, got {other:?}"),
        }
    }

    #[test]
    fn verify_cocone_rejects_legs_that_disagree_on_a_shared_sort() {
        let shared = Theory::new("S", vec![Sort::simple("V")], Vec::new(), Vec::new());
        let t1 = Theory::new(
            "T1",
            vec![Sort::simple("V"), Sort::simple("E")],
            Vec::new(),
            Vec::new(),
        );
        let t2 = Theory::new(
            "T2",
            vec![Sort::simple("V"), Sort::simple("C")],
            Vec::new(),
            Vec::new(),
        );
        let leg = |cod: &str, image: &str| {
            TheoryMorphism::new(
                format!("S_to_{cod}"),
                "S",
                cod,
                HashMap::from([(Arc::from("V"), Arc::from(image))]),
                HashMap::<Arc<str>, Arc<str>>::new(),
            )
        };
        let (i1, i2) = (leg("T1", "V"), leg("T2", "V"));
        let result = colimit(&shared, &t1, &t2, &i1, &i2).unwrap();
        result.verify_cocone(&i1, &i2, &shared, &t1, &t2).unwrap();
        let skewed = leg("T2", "C");
        match result.verify_cocone(&i1, &skewed, &shared, &t1, &t2) {
            Err(GatError::EquationNotPreserved { equation, detail }) => {
                assert_eq!(equation, "cocone sort V");
                assert_eq!(detail, "j1∘i1 maps to V, j2∘i2 maps to C");
            }
            other => panic!("expected EquationNotPreserved, got {other:?}"),
        }
    }

    #[test]
    fn verify_universal_requires_both_legs_into_the_target() {
        let (pushout, q, k1, mut k2) = graph_constraint_cocone();
        k2.codomain = Arc::from("Elsewhere");
        let (equation, detail) = not_preserved(pushout.verify_universal(&q, &k1, &k2));
        assert_eq!(equation, "universal property");
        assert_eq!(
            detail,
            "alternative cocone codomain mismatch: k1=Q, k2=Elsewhere, q=Q"
        );
    }

    #[test]
    fn verify_universal_requires_the_mediator_to_reproduce_each_leg_op_map() {
        let (pushout, q, k1, k2) = graph_constraint_cocone();
        pushout.verify_universal(&q, &k1, &k2).unwrap();

        let mut k1_extra = k1.clone();
        k1_extra
            .op_map
            .insert(Arc::from("ghost"), crate::morphism::OpAssignment::op("src"));
        let (equation, _) = not_preserved(pushout.verify_universal(&q, &k1_extra, &k2));
        assert_eq!(equation, "universal property: m ∘ j1 = k1");

        let mut k2_extra = k2;
        k2_extra.op_map.insert(
            Arc::from("ghost"),
            crate::morphism::OpAssignment::op("target"),
        );
        let (equation, _) = not_preserved(pushout.verify_universal(&q, &k1, &k2_extra));
        assert_eq!(equation, "universal property: m ∘ j2 = k2");
    }

    #[test]
    fn verify_universal_names_the_identified_sort_the_legs_split() {
        let (pushout, q, k1, mut k2) = graph_constraint_cocone();
        k2.sort_map.insert(Arc::from("Vertex"), Arc::from("Extra"));
        let (equation, detail) = not_preserved(pushout.verify_universal(&q, &k1, &k2));
        assert_eq!(equation, "universal property sort Vertex");
        assert_eq!(
            detail,
            "two T-preimages of `Vertex` map to distinct Q-images under k2: `Vertex` vs \
             `Extra`; the alternative cocone is not commutative"
        );
    }
}
