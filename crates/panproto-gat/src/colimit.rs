use std::collections::HashMap;
use std::sync::Arc;

use crate::eq::alpha_equivalent_equation;
use crate::error::GatError;
use crate::morphism::TheoryMorphism;
use crate::theory::Theory;

/// Result of a categorical pushout (colimit) computation.
///
/// Contains the pushout theory along with inclusion morphisms from
/// both input theories into the pushout. The cocone condition
/// `j1 ∘ i1 = j2 ∘ i2` is verified at construction time.
#[derive(Debug, Clone)]
pub struct ColimitResult {
    /// The pushout theory.
    pub theory: Theory,
    /// Inclusion morphism from the first theory into the pushout: j1: T1 → P.
    pub inclusion1: TheoryMorphism,
    /// Inclusion morphism from the second theory into the pushout: j2: T2 → P.
    pub inclusion2: TheoryMorphism,
}

impl ColimitResult {
    /// Verify the cocone (commutativity) condition: `j1 ∘ i1 = j2 ∘ i2`.
    ///
    /// For every sort and operation in the shared theory, the two paths
    /// through the pushout must agree.
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
    ) -> Result<(), GatError> {
        let lhs = i1.compose(&self.inclusion1)?;
        let rhs = i2.compose(&self.inclusion2)?;

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
                        l.map_or("(none)", |s| s.as_ref()),
                        r.map_or("(none)", |s| s.as_ref()),
                    ),
                });
            }
        }

        Ok(())
    }
}

/// Compute the pushout (colimit) of two theories over explicit morphisms.
///
/// Given morphisms `i1: S → T1` and `i2: S → T2` from a shared theory S,
/// this produces the pushout P with inclusion morphisms `j1: T1 → P` and
/// `j2: T2 → P` satisfying the universal property: `j1 ∘ i1 = j2 ∘ i2`.
///
/// The pushout identifies `i1(x)` with `i2(x)` for every sort and operation
/// x in the shared theory.
///
/// # Errors
///
/// Returns [`GatError::SortConflict`] if T1 and T2 both declare a sort with
/// the same name but incompatible definitions (different parameter lists) and
/// the sort is not identified via the morphisms.
///
/// Returns [`GatError::OpConflict`] if T1 and T2 both declare an operation
/// with the same name but incompatible signatures and the operation is not
/// identified via the morphisms.
///
/// Returns [`GatError::EqConflict`] if T1 and T2 both declare an equation
/// with the same name but different content and the equation is not identified
/// via the morphisms.
#[allow(clippy::too_many_lines)]
pub fn colimit(
    t1: &Theory,
    t2: &Theory,
    i1: &TheoryMorphism,
    i2: &TheoryMorphism,
) -> Result<ColimitResult, GatError> {
    // Build a renaming map for T2: for each sort/op s in the shared theory,
    // map i2(s) → i1(s) so T2's names align with T1's naming convention.
    let mut sort_rename: HashMap<Arc<str>, Arc<str>> = HashMap::new();
    for (shared_sort, t1_sort) in &i1.sort_map {
        if let Some(t2_sort) = i2.sort_map.get(shared_sort) {
            sort_rename.insert(Arc::clone(t2_sort), Arc::clone(t1_sort));
        }
    }

    let mut op_rename: HashMap<Arc<str>, Arc<str>> = HashMap::new();
    for (shared_op, t1_op) in &i1.op_map {
        if let Some(t2_op) = i2.op_map.get(shared_op) {
            op_rename.insert(Arc::clone(t2_op), Arc::clone(t1_op));
        }
    }

    // Start with all sorts from T1.
    let mut sorts = t1.sorts.clone();

    for sort in &t2.sorts {
        let effective_name = sort_rename
            .get(&sort.name)
            .cloned()
            .unwrap_or_else(|| Arc::clone(&sort.name));
        if t1.has_sort(&effective_name) {
            // This sort is identified with a T1 sort via the morphisms; skip.
            if sort_rename.contains_key(&sort.name) {
                continue;
            }
            // Both define it independently; check compatibility.
            let t1_sort = t1
                .find_sort(&effective_name)
                .ok_or_else(|| GatError::SortConflict {
                    name: effective_name.to_string(),
                })?;
            if t1_sort.params != sort.params || t1_sort.kind != sort.kind {
                return Err(GatError::SortConflict {
                    name: effective_name.to_string(),
                });
            }
            // Compatible duplicate; already included.
        } else {
            // Rename sort references in dependent sort params to use pushout names.
            let renamed_sort = rename_sort_refs(sort, &sort_rename);
            sorts.push(renamed_sort);
        }
    }

    // Same for operations.
    let mut ops = t1.ops.clone();

    for op in &t2.ops {
        let effective_name = op_rename
            .get(&op.name)
            .cloned()
            .unwrap_or_else(|| Arc::clone(&op.name));
        if t1.has_op(&effective_name) {
            if op_rename.contains_key(&op.name) {
                continue;
            }
            let t1_op = t1
                .find_op(&effective_name)
                .ok_or_else(|| GatError::OpConflict {
                    name: effective_name.to_string(),
                })?;
            // Compare with renamed sort references for compatibility.
            let renamed_op = rename_op_sort_refs(op, &sort_rename);
            if t1_op.inputs != renamed_op.inputs || t1_op.output != renamed_op.output {
                return Err(GatError::OpConflict {
                    name: effective_name.to_string(),
                });
            }
        } else {
            // Rename sort references in operation signature to use pushout names.
            ops.push(rename_op_sort_refs(op, &sort_rename));
        }
    }

    let eqs = merge_equations(t1, t2, &op_rename)?;
    let directed_eqs = merge_directed_equations(t1, t2, &op_rename)?;
    let policies = merge_policies(t1, t2)?;

    let pushout_name: Arc<str> = format!("{}_{}_colimit", t1.name, t2.name).into();
    let theory = Theory::full(
        Arc::clone(&pushout_name),
        Vec::new(),
        sorts,
        ops,
        eqs,
        directed_eqs,
        policies,
    );

    let j1 = build_inclusion(t1, &pushout_name, &HashMap::new(), &HashMap::new());

    let j2 = build_inclusion(t2, &pushout_name, &sort_rename, &op_rename);

    let result = ColimitResult {
        theory,
        inclusion1: j1,
        inclusion2: j2,
    };

    // Verify the cocone condition: j1 ∘ i1 = j2 ∘ i2 on all shared
    // sorts and ops (the domain of i1 and i2).
    let lhs = i1.compose(&result.inclusion1)?;
    let rhs = i2.compose(&result.inclusion2)?;
    for shared_sort in i1.sort_map.keys() {
        let l = lhs.sort_map.get(shared_sort);
        let r = rhs.sort_map.get(shared_sort);
        if l != r {
            return Err(GatError::EquationNotPreserved {
                equation: format!("cocone sort {shared_sort}"),
                detail: format!(
                    "j1∘i1 maps to {}, j2∘i2 maps to {}",
                    l.map_or("(none)", |s| s.as_ref()),
                    r.map_or("(none)", |s| s.as_ref()),
                ),
            });
        }
    }
    for shared_op in i1.op_map.keys() {
        let l = lhs.op_map.get(shared_op);
        let r = rhs.op_map.get(shared_op);
        if l != r {
            return Err(GatError::EquationNotPreserved {
                equation: format!("cocone op {shared_op}"),
                detail: format!(
                    "j1∘i1 maps to {}, j2∘i2 maps to {}",
                    l.map_or("(none)", |s| s.as_ref()),
                    r.map_or("(none)", |s| s.as_ref()),
                ),
            });
        }
    }

    Ok(result)
}

/// Merge equations from t2 into t1's equations, checking alpha-equivalence for conflicts.
///
/// Applies `op_rename` to T2's equation terms before comparison so that
/// operations identified via the morphisms are properly aligned with T1's
/// naming convention.
/// Rename sort references in a sort's dependent parameters using the rename map.
fn rename_sort_refs(
    sort: &crate::sort::Sort,
    sort_rename: &HashMap<Arc<str>, Arc<str>>,
) -> crate::sort::Sort {
    let params = sort
        .params
        .iter()
        .map(|p| {
            let renamed_sort = sort_rename
                .get(&p.sort)
                .cloned()
                .unwrap_or_else(|| Arc::clone(&p.sort));
            crate::sort::SortParam::new(Arc::clone(&p.name), renamed_sort)
        })
        .collect();
    crate::sort::Sort {
        name: Arc::clone(&sort.name),
        params,
        kind: sort.kind.clone(),
    }
}

/// Rename sort references in an operation's input/output sorts using the rename map.
fn rename_op_sort_refs(
    op: &crate::op::Operation,
    sort_rename: &HashMap<Arc<str>, Arc<str>>,
) -> crate::op::Operation {
    let inputs = op
        .inputs
        .iter()
        .map(|(name, sort)| {
            let renamed = sort_rename
                .get(sort)
                .cloned()
                .unwrap_or_else(|| Arc::clone(sort));
            (Arc::clone(name), renamed)
        })
        .collect();
    let output = sort_rename
        .get(&op.output)
        .cloned()
        .unwrap_or_else(|| Arc::clone(&op.output));
    crate::op::Operation::new(Arc::clone(&op.name), inputs, output)
}

fn merge_equations(
    t1: &Theory,
    t2: &Theory,
    op_rename: &HashMap<Arc<str>, Arc<str>>,
) -> Result<Vec<crate::eq::Equation>, GatError> {
    let mut eqs = t1.eqs.clone();
    for eq in &t2.eqs {
        let renamed = eq.rename_ops(op_rename);
        if let Some(t1_eq) = t1.find_eq(&eq.name) {
            if !alpha_equivalent_equation(&t1_eq.lhs, &t1_eq.rhs, &renamed.lhs, &renamed.rhs) {
                return Err(GatError::EqConflict {
                    name: eq.name.to_string(),
                });
            }
        } else {
            eqs.push(renamed);
        }
    }
    Ok(eqs)
}

/// Merge directed equations from t2 into t1's directed equations.
///
/// Applies `op_rename` to T2's directed equation terms before comparison.
fn merge_directed_equations(
    t1: &Theory,
    t2: &Theory,
    op_rename: &HashMap<Arc<str>, Arc<str>>,
) -> Result<Vec<crate::eq::DirectedEquation>, GatError> {
    let mut directed_eqs = t1.directed_eqs.clone();
    for de in &t2.directed_eqs {
        let renamed = de.rename_ops(op_rename);
        if let Some(t1_de) = t1.find_directed_eq(&de.name) {
            if !alpha_equivalent_equation(&t1_de.lhs, &t1_de.rhs, &renamed.lhs, &renamed.rhs) {
                return Err(GatError::DirectedEqConflict {
                    name: de.name.to_string(),
                });
            }
        } else {
            directed_eqs.push(renamed);
        }
    }
    Ok(directed_eqs)
}

/// Merge conflict policies from t2 into t1's policies.
fn merge_policies(
    t1: &Theory,
    t2: &Theory,
) -> Result<Vec<crate::theory::ConflictPolicy>, GatError> {
    let mut policies = t1.policies.clone();
    for pol in &t2.policies {
        if let Some(t1_pol) = t1.find_policy(&pol.name) {
            if t1_pol.value_kind != pol.value_kind || t1_pol.strategy != pol.strategy {
                return Err(GatError::PolicyConflict {
                    name: pol.name.to_string(),
                });
            }
        } else {
            policies.push(pol.clone());
        }
    }
    Ok(policies)
}

/// Build an inclusion morphism from `source` into the pushout theory named `pushout_name`.
///
/// Shared sorts/ops are renamed according to the given maps; non-shared sorts/ops
/// map to themselves.
fn build_inclusion(
    source: &Theory,
    pushout_name: &Arc<str>,
    sort_rename: &HashMap<Arc<str>, Arc<str>>,
    op_rename: &HashMap<Arc<str>, Arc<str>>,
) -> TheoryMorphism {
    let sort_map: HashMap<Arc<str>, Arc<str>> = source
        .sorts
        .iter()
        .map(|s| {
            let target = sort_rename
                .get(&s.name)
                .cloned()
                .unwrap_or_else(|| Arc::clone(&s.name));
            (Arc::clone(&s.name), target)
        })
        .collect();
    let op_map: HashMap<Arc<str>, Arc<str>> = source
        .ops
        .iter()
        .map(|o| {
            let target = op_rename
                .get(&o.name)
                .cloned()
                .unwrap_or_else(|| Arc::clone(&o.name));
            (Arc::clone(&o.name), target)
        })
        .collect();
    TheoryMorphism::new(
        format!("incl_{}_{pushout_name}", source.name),
        &*source.name,
        &**pushout_name,
        sort_map,
        op_map,
    )
}

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
pub fn colimit_by_name(t1: &Theory, t2: &Theory, shared: &Theory) -> Result<Theory, GatError> {
    // Start with all sorts from t1.
    let mut sorts = t1.sorts.clone();

    // Add sorts from t2, checking for conflicts.
    // Use the theory's O(1) index for lookups instead of building separate HashSets.
    for sort in &t2.sorts {
        if t1.has_sort(&sort.name) {
            // Present in both; must be identical or shared.
            if shared.has_sort(&sort.name) {
                // Shared sort: already included via t1, skip.
                continue;
            }
            // Both define it independently; check compatibility.
            let t1_sort = t1
                .find_sort(&sort.name)
                .ok_or_else(|| GatError::SortConflict {
                    name: sort.name.to_string(),
                })?;
            if t1_sort.params != sort.params || t1_sort.kind != sort.kind {
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
            if !alpha_equivalent_equation(&t1_eq.lhs, &t1_eq.rhs, &eq.lhs, &eq.rhs) {
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

        let result = colimit_by_name(&t1, &t2, &shared);
        assert!(matches!(result, Err(GatError::OpConflict { .. })));
    }

    #[test]
    fn eq_conflict_detected() {
        let shared = Theory::new("Empty", Vec::new(), Vec::new(), Vec::new());

        // t1: ax says x = y (two distinct variables).
        // t2: ax says x = x (one variable, used twice).
        // These are NOT alpha-equivalent since the variable multiplicity differs.
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
            vec![Equation::new("ax", Term::var("a"), Term::var("a"))],
        );

        let result = colimit_by_name(&t1, &t2, &shared);
        assert!(matches!(result, Err(GatError::EqConflict { .. })));
    }

    #[test]
    fn alpha_equivalent_eqs_not_conflicted() {
        let shared = Theory::new("Empty", Vec::new(), Vec::new(), Vec::new());

        // Same equation with different variable names: should NOT conflict.
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
            vec![Equation::new("ax", Term::var("a"), Term::var("b"))],
        );

        let result = colimit_by_name(&t1, &t2, &shared).unwrap();
        assert_eq!(result.eqs.len(), 1);
    }

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

        let result = colimit_by_name(&t1, &t2, &shared);
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
        assert!(matches!(result, Err(GatError::PolicyConflict { .. })));
    }

    #[test]
    fn colimit_equations_with_renamed_ops() {
        // Shared theory S has sort A and op f: A → A with equation e: f(f(x)) = f(x).
        // T1 keeps the names as-is.
        // T2 renames f → g but has the same equation.
        // Morphisms: i1 maps f→f; i2 maps f→g.
        // The colimit should identify them and the equation should NOT conflict.
        let _shared = Theory::new(
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

        let result = colimit(&t1, &t2, &i1, &i2).unwrap();
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
        }
    }
}
