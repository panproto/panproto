use std::collections::HashMap;
use std::sync::Arc;

use crate::eq::{DirectedEquation, Equation, alpha_equivalent_equation};
use crate::error::GatError;
use crate::morphism::TheoryMorphism;
use crate::op::Operation;
use crate::sort::Sort;
use crate::theory::Theory;

/// Result of a pullback computation.
///
/// Contains the pullback theory together with projection morphisms
/// to the two source theories.
#[derive(Debug, Clone)]
pub struct PullbackResult {
    /// The pullback theory.
    pub theory: Theory,
    /// Projection morphism from the pullback to the first source theory.
    pub proj1: TheoryMorphism,
    /// Projection morphism from the pullback to the second source theory.
    pub proj2: TheoryMorphism,
}

/// A paired sort/op/eq entry: `(name_in_t1, name_in_t2, name_in_pullback)`.
type Triple = (Arc<str>, Arc<str>, Arc<str>);

/// Maps a `(t1_name, t2_name)` pair to the corresponding pullback name.
type PairMap = HashMap<(Arc<str>, Arc<str>), Arc<str>>;

/// Build a reverse index from codomain names to lists of domain names.
fn reverse_index(forward: &HashMap<Arc<str>, Arc<str>>) -> HashMap<Arc<str>, Vec<Arc<str>>> {
    let mut rev: HashMap<Arc<str>, Vec<Arc<str>>> = HashMap::new();
    for (dom, cod) in forward {
        rev.entry(Arc::clone(cod))
            .or_default()
            .push(Arc::clone(dom));
    }
    rev
}

/// Choose a pullback name: use the shared name when both sides agree,
/// otherwise join with `=`.
fn paired_name(name_a: &Arc<str>, name_b: &Arc<str>) -> Arc<str> {
    if name_a == name_b {
        Arc::clone(name_a)
    } else {
        Arc::from(format!("{name_a}={name_b}"))
    }
}

/// Pair sorts from `t1` and `t2` that agree in the codomain under `m1`/`m2`.
fn pair_sorts(
    t1: &Theory,
    t2: &Theory,
    m1: &TheoryMorphism,
    m2_rev: &HashMap<Arc<str>, Vec<Arc<str>>>,
) -> (Vec<Triple>, PairMap) {
    let mut triples: Vec<Triple> = Vec::new();
    let mut pair_map: PairMap = HashMap::new();

    for s1 in &t1.sorts {
        let Some(cod) = m1.sort_map.get(&s1.name) else {
            continue;
        };
        let Some(candidates) = m2_rev.get(cod) else {
            continue;
        };
        for s2_name in candidates {
            let Some(s2) = t2.find_sort(s2_name) else {
                continue;
            };
            if s1.arity() != s2.arity() {
                continue;
            }
            let pb = paired_name(&s1.name, s2_name);
            triples.push((Arc::clone(&s1.name), Arc::clone(s2_name), Arc::clone(&pb)));
            pair_map.insert((Arc::clone(&s1.name), Arc::clone(s2_name)), pb);
        }
    }

    (triples, pair_map)
}

/// Build pullback `Sort` declarations from the paired sort triples.
fn build_sorts(t1: &Theory, sort_triples: &[Triple]) -> Vec<Sort> {
    sort_triples
        .iter()
        .filter_map(|(s1_name, _s2_name, pb_name)| {
            let s1 = t1.find_sort(s1_name)?;
            if s1.params.is_empty() {
                Some(Sort::simple(Arc::clone(pb_name)))
            } else {
                let pb_params: Vec<_> = s1
                    .params
                    .iter()
                    .filter_map(|p| {
                        sort_triples.iter().find_map(|(sn, _s2n, pbn)| {
                            if *sn == p.sort {
                                Some(crate::sort::SortParam::new(
                                    Arc::clone(&p.name),
                                    Arc::clone(pbn),
                                ))
                            } else {
                                None
                            }
                        })
                    })
                    .collect();
                Some(Sort::dependent(Arc::clone(pb_name), pb_params))
            }
        })
        .collect()
}

/// Pair operations from `t1` and `t2` that agree in the codomain.
fn pair_ops(
    t1: &Theory,
    t2: &Theory,
    m1: &TheoryMorphism,
    m2_op_rev: &HashMap<Arc<str>, Vec<Arc<str>>>,
    sort_pair_map: &PairMap,
) -> (Vec<Operation>, Vec<Triple>) {
    let mut ops = Vec::new();
    let mut triples: Vec<Triple> = Vec::new();

    for op1 in &t1.ops {
        let Some(cod) = m1.op_map.get(&op1.name) else {
            continue;
        };
        let Some(candidates) = m2_op_rev.get(cod) else {
            continue;
        };
        for op2_name in candidates {
            let Some(op2) = t2.find_op(op2_name) else {
                continue;
            };
            if op1.inputs.len() != op2.inputs.len() {
                continue;
            }

            // Check all input sort pairs exist in the pullback.
            let input_pb: Option<Vec<(Arc<str>, Arc<str>)>> = op1
                .inputs
                .iter()
                .zip(&op2.inputs)
                .map(|((param, s1_sort), (_, s2_sort))| {
                    sort_pair_map
                        .get(&(Arc::clone(s1_sort), Arc::clone(s2_sort)))
                        .map(|pb| (Arc::clone(param), Arc::clone(pb)))
                })
                .collect();

            let Some(input_pb_sorts) = input_pb else {
                continue;
            };

            // Check output sort pair exists.
            let Some(output_pb) =
                sort_pair_map.get(&(Arc::clone(&op1.output), Arc::clone(&op2.output)))
            else {
                continue;
            };

            let pb_name = paired_name(&op1.name, op2_name);
            ops.push(Operation::new(
                Arc::clone(&pb_name),
                input_pb_sorts,
                Arc::clone(output_pb),
            ));
            triples.push((Arc::clone(&op1.name), Arc::clone(op2_name), pb_name));
        }
    }

    (ops, triples)
}

/// Pair equations that agree when mapped into the codomain.
fn pair_eqs(
    t1: &Theory,
    t2: &Theory,
    m1: &TheoryMorphism,
    m2: &TheoryMorphism,
    op_triples: &[Triple],
) -> Vec<Equation> {
    let pb_op_rename: HashMap<Arc<str>, Arc<str>> = op_triples
        .iter()
        .map(|(o1, _o2, pb)| (Arc::clone(o1), Arc::clone(pb)))
        .collect();

    let mut eqs = Vec::new();

    for eq1 in &t1.eqs {
        let lhs_via_m1 = m1.apply_to_term(&eq1.lhs);
        let rhs_via_m1 = m1.apply_to_term(&eq1.rhs);

        for eq2 in &t2.eqs {
            let lhs_via_m2 = m2.apply_to_term(&eq2.lhs);
            let rhs_via_m2 = m2.apply_to_term(&eq2.rhs);

            // Compare mapped equations up to α-equivalence, since equations
            // are universally quantified and variable names are bound.
            if !alpha_equivalent_equation(&lhs_via_m1, &rhs_via_m1, &lhs_via_m2, &rhs_via_m2) {
                continue;
            }

            let pb_lhs = eq1.lhs.rename_ops(&pb_op_rename);
            let pb_rhs = eq1.rhs.rename_ops(&pb_op_rename);
            eqs.push(Equation::new(
                paired_name(&eq1.name, &eq2.name),
                pb_lhs,
                pb_rhs,
            ));
        }
    }

    eqs
}

/// Pair directed equations that agree when mapped into the codomain.
fn pair_directed_eqs(
    t1: &Theory,
    t2: &Theory,
    m1: &TheoryMorphism,
    m2: &TheoryMorphism,
    op_triples: &[Triple],
) -> Vec<DirectedEquation> {
    let pb_op_rename: HashMap<Arc<str>, Arc<str>> = op_triples
        .iter()
        .map(|(o1, _o2, pb)| (Arc::clone(o1), Arc::clone(pb)))
        .collect();

    let mut directed_eqs = Vec::new();

    for de1 in &t1.directed_eqs {
        let lhs_via_m1 = m1.apply_to_term(&de1.lhs);
        let rhs_via_m1 = m1.apply_to_term(&de1.rhs);

        for de2 in &t2.directed_eqs {
            let lhs_via_m2 = m2.apply_to_term(&de2.lhs);
            let rhs_via_m2 = m2.apply_to_term(&de2.rhs);

            if !alpha_equivalent_equation(&lhs_via_m1, &rhs_via_m1, &lhs_via_m2, &rhs_via_m2) {
                continue;
            }

            let pb_lhs = de1.lhs.rename_ops(&pb_op_rename);
            let pb_rhs = de1.rhs.rename_ops(&pb_op_rename);

            directed_eqs.push(DirectedEquation {
                name: paired_name(&de1.name, &de2.name),
                lhs: pb_lhs,
                rhs: pb_rhs,
                impl_term: de1.impl_term.clone(),
                inverse: de1.inverse.clone(),
            });
        }
    }

    directed_eqs
}

/// Compute the pullback of two theories over a common codomain.
///
/// Given morphisms `m1: t1 -> target` and `m2: t2 -> target`,
/// computes the pullback theory `P` with projections `p1: P -> t1` and
/// `p2: P -> t2` such that `m1 . p1 = m2 . p2`.
///
/// The pullback is the categorical dual of the pushout (colimit). Where
/// pushouts merge theories together, pullbacks find the common structure
/// that two theories share when mapped into a common target.
///
/// # Errors
///
/// Returns [`GatError`] if the morphisms reference sorts or operations
/// that cannot be found.
pub fn pullback(
    t1: &Theory,
    t2: &Theory,
    m1: &TheoryMorphism,
    m2: &TheoryMorphism,
) -> Result<PullbackResult, GatError> {
    let m2_sort_rev = reverse_index(&m2.sort_map);
    let m2_op_rev = reverse_index(&m2.op_map);

    let (sort_triples, sort_pair_map) = pair_sorts(t1, t2, m1, &m2_sort_rev);
    let pb_sorts = build_sorts(t1, &sort_triples);
    let (pb_ops, op_triples) = pair_ops(t1, t2, m1, &m2_op_rev, &sort_pair_map);
    let pb_eqs = pair_eqs(t1, t2, m1, m2, &op_triples);
    let pb_directed_eqs = pair_directed_eqs(t1, t2, m1, m2, &op_triples);

    let pb_name: Arc<str> = format!("{}_{}_pullback", t1.name, t2.name).into();
    let pb_theory = Theory::full(
        pb_name,
        Vec::new(),
        pb_sorts,
        pb_ops,
        pb_eqs,
        pb_directed_eqs,
        Vec::new(),
    );

    // Build projection morphisms.
    let mut proj1_sort = HashMap::new();
    let mut proj2_sort = HashMap::new();
    for (s1, s2, pb) in &sort_triples {
        proj1_sort.insert(Arc::clone(pb), Arc::clone(s1));
        proj2_sort.insert(Arc::clone(pb), Arc::clone(s2));
    }

    let mut proj1_ops = HashMap::new();
    let mut proj2_ops = HashMap::new();
    for (o1, o2, pb) in &op_triples {
        proj1_ops.insert(Arc::clone(pb), Arc::clone(o1));
        proj2_ops.insert(Arc::clone(pb), Arc::clone(o2));
    }

    let proj1 = TheoryMorphism::new(
        format!("{}_proj1", pb_theory.name),
        Arc::clone(&pb_theory.name),
        Arc::clone(&t1.name),
        proj1_sort,
        proj1_ops,
    );

    let proj2 = TheoryMorphism::new(
        format!("{}_proj2", pb_theory.name),
        Arc::clone(&pb_theory.name),
        Arc::clone(&t2.name),
        proj2_sort,
        proj2_ops,
    );

    Ok(PullbackResult {
        theory: pb_theory,
        proj1,
        proj2,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eq::Term;
    use crate::morphism::check_morphism;

    /// Test 1: Pullback of identical identity morphisms yields a theory
    /// isomorphic to the original.
    #[test]
    fn pullback_identity_morphisms() -> Result<(), Box<dyn std::error::Error>> {
        let theory = Theory::new(
            "ThGraph",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![
                Operation::unary("src", "e", "Edge", "Vertex"),
                Operation::unary("tgt", "e", "Edge", "Vertex"),
            ],
            Vec::new(),
        );

        let id_sort_map = HashMap::from([
            (Arc::from("Vertex"), Arc::from("Vertex")),
            (Arc::from("Edge"), Arc::from("Edge")),
        ]);
        let id_op_map = HashMap::from([
            (Arc::from("src"), Arc::from("src")),
            (Arc::from("tgt"), Arc::from("tgt")),
        ]);

        let id1 = TheoryMorphism::new(
            "id1",
            "ThGraph",
            "ThGraph",
            id_sort_map.clone(),
            id_op_map.clone(),
        );
        let id2 = TheoryMorphism::new("id2", "ThGraph", "ThGraph", id_sort_map, id_op_map);

        let result = pullback(&theory, &theory, &id1, &id2)?;

        // The pullback should have the same number of sorts and ops.
        assert_eq!(result.theory.sorts.len(), 2);
        assert_eq!(result.theory.ops.len(), 2);

        // Sort names should be preserved (since both sides have the same names).
        assert!(result.theory.find_sort("Vertex").is_some());
        assert!(result.theory.find_sort("Edge").is_some());
        assert!(result.theory.find_op("src").is_some());
        assert!(result.theory.find_op("tgt").is_some());

        // Projections should validate.
        assert!(check_morphism(&result.proj1, &result.theory, &theory).is_ok());
        assert!(check_morphism(&result.proj2, &result.theory, &theory).is_ok());
        Ok(())
    }

    /// Test 2: Pullback of disjoint images yields an empty theory.
    #[test]
    fn pullback_disjoint_images() -> Result<(), Box<dyn std::error::Error>> {
        let t1 = Theory::new("T1", vec![Sort::simple("A")], Vec::new(), Vec::new());
        let t2 = Theory::new("T2", vec![Sort::simple("B")], Vec::new(), Vec::new());

        // m1 maps A -> X, m2 maps B -> Y. Disjoint images.
        let m1 = TheoryMorphism::new(
            "m1",
            "T1",
            "Target",
            HashMap::from([(Arc::from("A"), Arc::from("X"))]),
            HashMap::new(),
        );
        let m2 = TheoryMorphism::new(
            "m2",
            "T2",
            "Target",
            HashMap::from([(Arc::from("B"), Arc::from("Y"))]),
            HashMap::new(),
        );

        let result = pullback(&t1, &t2, &m1, &m2)?;

        assert_eq!(result.theory.sorts.len(), 0);
        assert_eq!(result.theory.ops.len(), 0);
        assert_eq!(result.theory.eqs.len(), 0);
        Ok(())
    }

    /// Test 3: Pullback recovering a shared Vertex sort.
    ///
    /// Two graph-like theories both map their Vertex sort to the same
    /// codomain sort, so the pullback contains a Vertex sort.
    #[test]
    fn pullback_shared_vertex_sort() -> Result<(), Box<dyn std::error::Error>> {
        let t1 = Theory::new(
            "T1",
            vec![Sort::simple("V1"), Sort::simple("E1")],
            vec![Operation::unary("src1", "e", "E1", "V1")],
            Vec::new(),
        );
        let t2 = Theory::new(
            "T2",
            vec![Sort::simple("V2"), Sort::simple("F2")],
            vec![Operation::unary("src2", "f", "F2", "V2")],
            Vec::new(),
        );

        let m1 = TheoryMorphism::new(
            "m1",
            "T1",
            "Target",
            HashMap::from([
                (Arc::from("V1"), Arc::from("Vertex")),
                (Arc::from("E1"), Arc::from("Edge")),
            ]),
            HashMap::from([(Arc::from("src1"), Arc::from("src"))]),
        );
        let m2 = TheoryMorphism::new(
            "m2",
            "T2",
            "Target",
            HashMap::from([
                (Arc::from("V2"), Arc::from("Vertex")),
                (Arc::from("F2"), Arc::from("Edge")),
            ]),
            HashMap::from([(Arc::from("src2"), Arc::from("src"))]),
        );

        let result = pullback(&t1, &t2, &m1, &m2)?;

        // Both V1 and V2 map to Vertex, E1 and F2 map to Edge.
        // So pullback has sorts: V1=V2, E1=F2.
        assert_eq!(result.theory.sorts.len(), 2);
        assert!(result.theory.find_sort("V1=V2").is_some());
        assert!(result.theory.find_sort("E1=F2").is_some());

        // The src1=src2 operation should exist.
        assert_eq!(result.theory.ops.len(), 1);
        assert!(result.theory.find_op("src1=src2").is_some());

        // Projections validate.
        assert!(check_morphism(&result.proj1, &result.theory, &t1).is_ok());
        assert!(check_morphism(&result.proj2, &result.theory, &t2).is_ok());
        Ok(())
    }

    /// Test 4: Projection morphisms validate via `check_morphism`.
    ///
    /// Uses a richer theory (monoid with equation) to ensure projections
    /// are well-formed.
    #[test]
    fn projection_morphisms_validate() -> Result<(), Box<dyn std::error::Error>> {
        let theory = Theory::new(
            "Monoid",
            vec![Sort::simple("Carrier")],
            vec![
                Operation::new(
                    "mul",
                    vec![
                        ("a".into(), "Carrier".into()),
                        ("b".into(), "Carrier".into()),
                    ],
                    "Carrier",
                ),
                Operation::nullary("unit", "Carrier"),
            ],
            vec![Equation::new(
                "left_id",
                Term::app("mul", vec![Term::constant("unit"), Term::var("a")]),
                Term::var("a"),
            )],
        );

        let sort_map = HashMap::from([(Arc::from("Carrier"), Arc::from("Carrier"))]);
        let op_map = HashMap::from([
            (Arc::from("mul"), Arc::from("mul")),
            (Arc::from("unit"), Arc::from("unit")),
        ]);

        let id1 = TheoryMorphism::new("id1", "Monoid", "Monoid", sort_map.clone(), op_map.clone());
        let id2 = TheoryMorphism::new("id2", "Monoid", "Monoid", sort_map, op_map);

        let result = pullback(&theory, &theory, &id1, &id2)?;

        assert_eq!(result.theory.sorts.len(), 1);
        assert_eq!(result.theory.ops.len(), 2);
        assert_eq!(result.theory.eqs.len(), 1);

        // Both projections must pass validation.
        check_morphism(&result.proj1, &result.theory, &theory)?;
        check_morphism(&result.proj2, &result.theory, &theory)?;
        Ok(())
    }

    /// Test 5: Equation pairing.
    ///
    /// Two theories with equations that map to the same codomain equation
    /// should produce a paired equation in the pullback.
    #[test]
    fn equation_pairing() -> Result<(), Box<dyn std::error::Error>> {
        let t1 = Theory::new(
            "T1",
            vec![Sort::simple("S")],
            vec![Operation::unary("f", "x", "S", "S")],
            vec![Equation::new(
                "idem1",
                Term::app("f", vec![Term::app("f", vec![Term::var("x")])]),
                Term::app("f", vec![Term::var("x")]),
            )],
        );

        let t2 = Theory::new(
            "T2",
            vec![Sort::simple("T")],
            vec![Operation::unary("g", "y", "T", "T")],
            vec![Equation::new(
                "idem2",
                Term::app("g", vec![Term::app("g", vec![Term::var("x")])]),
                Term::app("g", vec![Term::var("x")]),
            )],
        );

        let m1 = TheoryMorphism::new(
            "m1",
            "T1",
            "Target",
            HashMap::from([(Arc::from("S"), Arc::from("U"))]),
            HashMap::from([(Arc::from("f"), Arc::from("h"))]),
        );
        let m2 = TheoryMorphism::new(
            "m2",
            "T2",
            "Target",
            HashMap::from([(Arc::from("T"), Arc::from("U"))]),
            HashMap::from([(Arc::from("g"), Arc::from("h"))]),
        );

        let result = pullback(&t1, &t2, &m1, &m2)?;

        // Should have one paired sort, one paired op, one paired equation.
        assert_eq!(result.theory.sorts.len(), 1);
        assert_eq!(result.theory.ops.len(), 1);
        assert_eq!(result.theory.eqs.len(), 1);

        // The equation name should be "idem1=idem2" since the names differ.
        assert!(result.theory.find_eq("idem1=idem2").is_some());

        // Projections validate.
        check_morphism(&result.proj1, &result.theory, &t1)?;
        check_morphism(&result.proj2, &result.theory, &t2)?;
        Ok(())
    }

    /// Equations that differ only in variable names should still be paired
    /// in the pullback, thanks to α-equivalence.
    #[test]
    fn equation_pairing_with_renamed_vars() -> Result<(), Box<dyn std::error::Error>> {
        let t1 = Theory::new(
            "T1",
            vec![Sort::simple("S")],
            vec![Operation::new(
                "f",
                vec![("a".into(), "S".into()), ("b".into(), "S".into())],
                "S",
            )],
            vec![Equation::new(
                "comm1",
                Term::app("f", vec![Term::var("a"), Term::var("b")]),
                Term::app("f", vec![Term::var("b"), Term::var("a")]),
            )],
        );

        // t2 has the same equation but with variables x, y.
        let t2 = Theory::new(
            "T2",
            vec![Sort::simple("T")],
            vec![Operation::new(
                "g",
                vec![("x".into(), "T".into()), ("y".into(), "T".into())],
                "T",
            )],
            vec![Equation::new(
                "comm2",
                Term::app("g", vec![Term::var("x"), Term::var("y")]),
                Term::app("g", vec![Term::var("y"), Term::var("x")]),
            )],
        );

        let m1 = TheoryMorphism::new(
            "m1",
            "T1",
            "Target",
            HashMap::from([(Arc::from("S"), Arc::from("U"))]),
            HashMap::from([(Arc::from("f"), Arc::from("h"))]),
        );
        let m2 = TheoryMorphism::new(
            "m2",
            "T2",
            "Target",
            HashMap::from([(Arc::from("T"), Arc::from("U"))]),
            HashMap::from([(Arc::from("g"), Arc::from("h"))]),
        );

        let result = pullback(&t1, &t2, &m1, &m2)?;

        // The equations should be paired despite different variable names.
        assert_eq!(result.theory.eqs.len(), 1);
        assert!(result.theory.find_eq("comm1=comm2").is_some());

        check_morphism(&result.proj1, &result.theory, &t1)?;
        check_morphism(&result.proj2, &result.theory, &t2)?;
        Ok(())
    }

    /// Directed equations that agree in the codomain should be paired.
    #[test]
    fn pullback_pairs_directed_eqs() -> Result<(), Box<dyn std::error::Error>> {
        use crate::eq::DirectedEquation;

        let t1 = Theory::full(
            "T1",
            Vec::new(),
            vec![Sort::simple("S")],
            vec![Operation::unary("f", "x", "S", "S")],
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
            vec![Sort::simple("T")],
            vec![Operation::unary("g", "y", "T", "T")],
            Vec::new(),
            vec![DirectedEquation::new(
                "rule2",
                Term::app("g", vec![Term::app("g", vec![Term::var("y")])]),
                Term::app("g", vec![Term::var("y")]),
                panproto_expr::Expr::Var("_".into()),
            )],
            Vec::new(),
        );

        let m1 = TheoryMorphism::new(
            "m1",
            "T1",
            "Target",
            HashMap::from([(Arc::from("S"), Arc::from("U"))]),
            HashMap::from([(Arc::from("f"), Arc::from("h"))]),
        );
        let m2 = TheoryMorphism::new(
            "m2",
            "T2",
            "Target",
            HashMap::from([(Arc::from("T"), Arc::from("U"))]),
            HashMap::from([(Arc::from("g"), Arc::from("h"))]),
        );

        let result = pullback(&t1, &t2, &m1, &m2)?;

        assert_eq!(result.theory.directed_eqs.len(), 1);
        assert!(result.theory.find_directed_eq("rule1=rule2").is_some());
        Ok(())
    }

    /// Directed equations that don't agree in the codomain produce no pairs.
    #[test]
    fn pullback_no_directed_eq_match() -> Result<(), Box<dyn std::error::Error>> {
        use crate::eq::DirectedEquation;

        let t1 = Theory::full(
            "T1",
            Vec::new(),
            vec![Sort::simple("S")],
            vec![Operation::unary("f", "x", "S", "S")],
            Vec::new(),
            vec![DirectedEquation::new(
                "rule1",
                Term::app("f", vec![Term::var("x")]),
                Term::var("x"),
                panproto_expr::Expr::Var("_".into()),
            )],
            Vec::new(),
        );

        let t2 = Theory::full(
            "T2",
            Vec::new(),
            vec![Sort::simple("T")],
            vec![Operation::unary("g", "y", "T", "T")],
            Vec::new(),
            vec![DirectedEquation::new(
                "rule2",
                Term::var("y"),
                Term::app("g", vec![Term::var("y")]),
                panproto_expr::Expr::Var("_".into()),
            )],
            Vec::new(),
        );

        let m1 = TheoryMorphism::new(
            "m1",
            "T1",
            "Target",
            HashMap::from([(Arc::from("S"), Arc::from("U"))]),
            HashMap::from([(Arc::from("f"), Arc::from("h"))]),
        );
        let m2 = TheoryMorphism::new(
            "m2",
            "T2",
            "Target",
            HashMap::from([(Arc::from("T"), Arc::from("U"))]),
            HashMap::from([(Arc::from("g"), Arc::from("h"))]),
        );

        let result = pullback(&t1, &t2, &m1, &m2)?;
        assert_eq!(result.theory.directed_eqs.len(), 0);
        Ok(())
    }

    /// When sorts have the same name on both sides, the pullback sort
    /// keeps the original name (not "X=X").
    #[test]
    fn same_name_sorts_not_duplicated() -> Result<(), Box<dyn std::error::Error>> {
        let t1 = Theory::new("T1", vec![Sort::simple("Vertex")], Vec::new(), Vec::new());
        let t2 = Theory::new("T2", vec![Sort::simple("Vertex")], Vec::new(), Vec::new());

        let m1 = TheoryMorphism::new(
            "m1",
            "T1",
            "Target",
            HashMap::from([(Arc::from("Vertex"), Arc::from("V"))]),
            HashMap::new(),
        );
        let m2 = TheoryMorphism::new(
            "m2",
            "T2",
            "Target",
            HashMap::from([(Arc::from("Vertex"), Arc::from("V"))]),
            HashMap::new(),
        );

        let result = pullback(&t1, &t2, &m1, &m2)?;

        assert_eq!(result.theory.sorts.len(), 1);
        assert!(result.theory.find_sort("Vertex").is_some());
        // Should NOT be named "Vertex=Vertex".
        assert!(result.theory.find_sort("Vertex=Vertex").is_none());
        Ok(())
    }
}
