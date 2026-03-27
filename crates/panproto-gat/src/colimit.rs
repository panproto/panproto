use std::sync::Arc;

use crate::eq::alpha_equivalent_equation;
use crate::error::GatError;
use crate::theory::Theory;

/// Compute the pushout (colimit) of two theories over a shared base theory.
///
/// Given theories `t1` and `t2` that both extend a `shared` theory, this
/// produces a combined theory containing all sorts, operations, and equations
/// from both, with the shared components identified (not duplicated).
///
/// The resulting theory is named `"{t1.name}_{t2.name}_colimit"`.
///
/// # Errors
///
/// Returns [`GatError::SortConflict`] if `t1` and `t2` both declare a sort with
/// the same name but incompatible definitions (different parameter lists).
///
/// Returns [`GatError::OpConflict`] if `t1` and `t2` both declare an operation
/// with the same name but incompatible signatures.
///
/// Returns [`GatError::EqConflict`] if `t1` and `t2` both declare an equation
/// with the same name but different content.
pub fn colimit(t1: &Theory, t2: &Theory, shared: &Theory) -> Result<Theory, GatError> {
    // Start with all sorts from t1.
    let mut sorts = t1.sorts.clone();

    // Add sorts from t2, checking for conflicts.
    // Use the theory's O(1) index for lookups instead of building separate HashSets.
    for sort in &t2.sorts {
        if t1.has_sort(&sort.name) {
            // Present in both — must be identical or shared.
            if shared.has_sort(&sort.name) {
                // Shared sort: already included via t1, skip.
                continue;
            }
            // Both define it independently — check compatibility.
            let t1_sort = t1
                .find_sort(&sort.name)
                .ok_or_else(|| GatError::SortConflict {
                    name: sort.name.to_string(),
                })?;
            if t1_sort.params != sort.params {
                return Err(GatError::SortConflict {
                    name: sort.name.to_string(),
                });
            }
            // Compatible duplicate; already included.
        } else {
            sorts.push(sort.clone());
        }
    }

    // Same for operations.
    let mut ops = t1.ops.clone();

    for op in &t2.ops {
        if t1.has_op(&op.name) {
            if shared.has_op(&op.name) {
                continue;
            }
            let t1_op = t1.find_op(&op.name).ok_or_else(|| GatError::OpConflict {
                name: op.name.to_string(),
            })?;
            if t1_op.inputs != op.inputs || t1_op.output != op.output {
                return Err(GatError::OpConflict {
                    name: op.name.to_string(),
                });
            }
        } else {
            ops.push(op.clone());
        }
    }

    // Same for equations.
    let mut eqs = t1.eqs.clone();

    for eq in &t2.eqs {
        if let Some(t1_eq) = t1.find_eq(&eq.name) {
            if shared.find_eq(&eq.name).is_some() {
                continue;
            }
            if t1_eq.lhs != eq.lhs || t1_eq.rhs != eq.rhs {
                return Err(GatError::EqConflict {
                    name: eq.name.to_string(),
                });
            }
        } else {
            eqs.push(eq.clone());
        }
    }

    // Same for directed equations.
    let mut directed_eqs = t1.directed_eqs.clone();

    for de in &t2.directed_eqs {
        if let Some(t1_de) = t1.find_directed_eq(&de.name) {
            if shared.find_directed_eq(&de.name).is_some() {
                continue;
            }
            if !alpha_equivalent_equation(&t1_de.lhs, &t1_de.rhs, &de.lhs, &de.rhs) {
                return Err(GatError::DirectedEqConflict {
                    name: de.name.to_string(),
                });
            }
        } else {
            directed_eqs.push(de.clone());
        }
    }

    // Same for conflict policies.
    let mut policies = t1.policies.clone();

    for pol in &t2.policies {
        if let Some(t1_pol) = t1.find_policy(&pol.name) {
            if shared.find_policy(&pol.name).is_some() {
                continue;
            }
            if t1_pol.value_kind != pol.value_kind || t1_pol.strategy != pol.strategy {
                return Err(GatError::PolicyConflict {
                    name: pol.name.to_string(),
                });
            }
        } else {
            policies.push(pol.clone());
        }
    }

    let name: Arc<str> = format!("{}_{}_colimit", t1.name, t2.name).into();
    Ok(Theory::full(
        name,
        Vec::new(),
        sorts,
        ops,
        eqs,
        directed_eqs,
        policies,
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::eq::{Equation, Term};
    use crate::op::Operation;
    use crate::sort::{Sort, SortParam};

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

        let result = colimit(&th_graph, &th_constraint, &shared).unwrap();

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

        let result = colimit(&t1, &t2, &shared);
        assert!(matches!(result, Err(GatError::SortConflict { .. })));
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

        let result = colimit(&t1, &t2, &shared);
        assert!(matches!(result, Err(GatError::OpConflict { .. })));
    }

    #[test]
    fn eq_conflict_detected() {
        let shared = Theory::new("Empty", Vec::new(), Vec::new(), Vec::new());

        let t1 = Theory::new(
            "T1",
            Vec::new(),
            Vec::new(),
            vec![Equation::new("ax", Term::var("x"), Term::var("y"))],
        );
        let t2 = Theory::new(
            "T2",
            Vec::new(),
            Vec::new(),
            vec![Equation::new("ax", Term::var("a"), Term::var("b"))], // same name, different terms
        );

        let result = colimit(&t1, &t2, &shared);
        assert!(matches!(result, Err(GatError::EqConflict { .. })));
    }

    #[test]
    fn compatible_non_shared_duplicates_allowed() {
        let shared = Theory::new("Empty", Vec::new(), Vec::new(), Vec::new());

        // Both define identical sort X.
        let t1 = Theory::new("T1", vec![Sort::simple("X")], Vec::new(), Vec::new());
        let t2 = Theory::new("T2", vec![Sort::simple("X")], Vec::new(), Vec::new());

        let result = colimit(&t1, &t2, &shared).unwrap();
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

        let result = colimit(&t1, &t2, &shared).unwrap();
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

        let result = colimit(&t1, &t2, &shared).unwrap();
        assert_eq!(result.directed_eqs.len(), 1);
    }

    #[test]
    fn colimit_directed_eq_conflict() {
        use crate::eq::{DirectedEquation, Term};

        let shared = Theory::new("Empty", Vec::new(), Vec::new(), Vec::new());

        let t1 = Theory::full(
            "T1",
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![DirectedEquation::new(
                "rule",
                Term::var("x"),
                Term::var("y"),
                panproto_expr::Expr::Var("_".into()),
            )],
            Vec::new(),
        );

        let t2 = Theory::full(
            "T2",
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![DirectedEquation::new(
                "rule",
                Term::constant("a"),
                Term::constant("b"),
                panproto_expr::Expr::Var("_".into()),
            )],
            Vec::new(),
        );

        let result = colimit(&t1, &t2, &shared);
        assert!(matches!(result, Err(GatError::DirectedEqConflict { .. })));
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

        let result = colimit(&t1, &t2, &shared).unwrap();
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

        let result = colimit(&t1, &t2, &shared);
        assert!(matches!(result, Err(GatError::PolicyConflict { .. })));
    }

    #[test]
    fn shared_declarations_not_duplicated() {
        let shared = Theory::new(
            "Shared",
            vec![Sort::simple("S")],
            vec![Operation::nullary("c", "S")],
            vec![Equation::new("e", Term::var("x"), Term::var("x"))],
        );

        let t1 = Theory::new(
            "T1",
            vec![Sort::simple("S"), Sort::simple("A")],
            vec![Operation::nullary("c", "S")],
            vec![Equation::new("e", Term::var("x"), Term::var("x"))],
        );
        let t2 = Theory::new(
            "T2",
            vec![Sort::simple("S"), Sort::simple("B")],
            vec![Operation::nullary("c", "S")],
            vec![Equation::new("e", Term::var("x"), Term::var("x"))],
        );

        let result = colimit(&t1, &t2, &shared).unwrap();
        assert_eq!(result.sorts.len(), 3); // S, A, B
        assert_eq!(result.ops.len(), 1); // c
        assert_eq!(result.eqs.len(), 1); // e
    }
}
