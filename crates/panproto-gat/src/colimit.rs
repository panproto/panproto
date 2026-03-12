use rustc_hash::FxHashSet;

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
    let shared_sort_names: FxHashSet<String> =
        shared.sorts.iter().map(|s| s.name.clone()).collect();
    let shared_op_names: FxHashSet<String> = shared.ops.iter().map(|o| o.name.clone()).collect();
    let shared_eq_names: FxHashSet<String> = shared.eqs.iter().map(|e| e.name.clone()).collect();

    // Start with all sorts from t1.
    let mut sorts = t1.sorts.clone();
    let t1_sort_names: FxHashSet<String> = t1.sorts.iter().map(|s| s.name.clone()).collect();

    // Add sorts from t2, checking for conflicts.
    for sort in &t2.sorts {
        if t1_sort_names.contains(&sort.name) {
            // Present in both — must be identical or shared.
            if shared_sort_names.contains(&sort.name) {
                // Shared sort: already included via t1, skip.
                continue;
            }
            // Both define it independently — check compatibility.
            let t1_sort = t1
                .find_sort(&sort.name)
                .ok_or_else(|| GatError::SortConflict {
                    name: sort.name.clone(),
                })?;
            if t1_sort.params != sort.params {
                return Err(GatError::SortConflict {
                    name: sort.name.clone(),
                });
            }
            // Compatible duplicate; already included.
        } else {
            sorts.push(sort.clone());
        }
    }

    // Same for operations.
    let mut ops = t1.ops.clone();
    let t1_op_names: FxHashSet<String> = t1.ops.iter().map(|o| o.name.clone()).collect();

    for op in &t2.ops {
        if t1_op_names.contains(&op.name) {
            if shared_op_names.contains(&op.name) {
                continue;
            }
            let t1_op = t1.find_op(&op.name).ok_or_else(|| GatError::OpConflict {
                name: op.name.clone(),
            })?;
            if t1_op.inputs != op.inputs || t1_op.output != op.output {
                return Err(GatError::OpConflict {
                    name: op.name.clone(),
                });
            }
        } else {
            ops.push(op.clone());
        }
    }

    // Same for equations.
    let mut eqs = t1.eqs.clone();
    let t1_eq_names: FxHashSet<String> = t1.eqs.iter().map(|e| e.name.clone()).collect();

    for eq in &t2.eqs {
        if t1_eq_names.contains(&eq.name) {
            if shared_eq_names.contains(&eq.name) {
                continue;
            }
            let t1_eq = t1.find_eq(&eq.name).ok_or_else(|| GatError::EqConflict {
                name: eq.name.clone(),
            })?;
            if t1_eq.lhs != eq.lhs || t1_eq.rhs != eq.rhs {
                return Err(GatError::EqConflict {
                    name: eq.name.clone(),
                });
            }
        } else {
            eqs.push(eq.clone());
        }
    }

    let name = format!("{}_{}_colimit", t1.name, t2.name);
    Ok(Theory::new(name, sorts, ops, eqs))
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

        assert_eq!(result.name, "ThGraph_ThConstraint_colimit");
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
