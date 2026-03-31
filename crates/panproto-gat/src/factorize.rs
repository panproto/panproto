use std::sync::Arc;

use rustc_hash::FxHashSet;

use crate::error::GatError;
use crate::morphism::TheoryMorphism;
use crate::schema_functor::{TheoryConstraint, TheoryEndofunctor, TheoryTransform};
use crate::theory::Theory;

/// The result of factorizing a theory morphism.
#[derive(Debug, Clone)]
pub struct Factorization {
    /// Ordered sequence of elementary endofunctors.
    pub steps: Vec<TheoryEndofunctor>,
    /// Domain theory name.
    pub domain: Arc<str>,
    /// Codomain theory name.
    pub codomain: Arc<str>,
}

/// Emit drop steps for elements in domain not present in codomain.
fn emit_drops(
    steps: &mut Vec<TheoryEndofunctor>,
    morphism: &TheoryMorphism,
    domain: &Theory,
    codomain: &Theory,
) {
    let codomain_sort_names: FxHashSet<&str> = codomain.sorts.iter().map(|s| &*s.name).collect();
    let codomain_op_names: FxHashSet<&str> = codomain.ops.iter().map(|o| &*o.name).collect();
    let codomain_eq_names: FxHashSet<&str> = codomain.eqs.iter().map(|e| &*e.name).collect();

    // Equations first (they depend on ops/sorts)
    for eq in &domain.eqs {
        if !codomain_eq_names.contains(&*eq.name) {
            steps.push(TheoryEndofunctor {
                name: Arc::from(format!("drop_eq_{}", eq.name)),
                precondition: TheoryConstraint::HasEquation(Arc::clone(&eq.name)),
                transform: TheoryTransform::DropEquation(Arc::clone(&eq.name)),
            });
        }
    }

    // Ops
    for op in &domain.ops {
        let effective_name = morphism.op_map.get(&op.name).unwrap_or(&op.name);
        if !codomain_op_names.contains(&**effective_name) {
            steps.push(TheoryEndofunctor {
                name: Arc::from(format!("drop_op_{}", op.name)),
                precondition: TheoryConstraint::HasOp(Arc::clone(&op.name)),
                transform: TheoryTransform::DropOp(Arc::clone(&op.name)),
            });
        }
    }

    // Sorts
    for sort in &domain.sorts {
        let effective_name = morphism.sort_map.get(&sort.name).unwrap_or(&sort.name);
        if !codomain_sort_names.contains(&**effective_name) {
            steps.push(TheoryEndofunctor {
                name: Arc::from(format!("drop_sort_{}", sort.name)),
                precondition: TheoryConstraint::HasSort(Arc::clone(&sort.name)),
                transform: TheoryTransform::DropSort(Arc::clone(&sort.name)),
            });
        }
    }
}

/// Emit rename steps from the identified renames.
fn emit_renames(
    steps: &mut Vec<TheoryEndofunctor>,
    sort_renames: &[(Arc<str>, Arc<str>)],
    op_renames: &[(Arc<str>, Arc<str>)],
) {
    for (old, new) in sort_renames {
        steps.push(TheoryEndofunctor {
            name: Arc::from(format!("rename_sort_{old}_{new}")),
            precondition: TheoryConstraint::HasSort(Arc::clone(old)),
            transform: TheoryTransform::RenameSort {
                old: Arc::clone(old),
                new: Arc::clone(new),
            },
        });
    }

    for (old, new) in op_renames {
        steps.push(TheoryEndofunctor {
            name: Arc::from(format!("rename_op_{old}_{new}")),
            precondition: TheoryConstraint::HasOp(Arc::clone(old)),
            transform: TheoryTransform::RenameOp {
                old: Arc::clone(old),
                new: Arc::clone(new),
            },
        });
    }
}

/// Emit add steps for elements in codomain not present in domain (after renames).
/// Sorts are topologically sorted by parameter dependencies.
fn emit_adds(
    steps: &mut Vec<TheoryEndofunctor>,
    morphism: &TheoryMorphism,
    domain: &Theory,
    codomain: &Theory,
) {
    let domain_sort_names_after_renames: FxHashSet<Arc<str>> = domain
        .sorts
        .iter()
        .map(|s| morphism.sort_map.get(&s.name).unwrap_or(&s.name).clone())
        .collect();
    let domain_op_names_after_renames: FxHashSet<Arc<str>> = domain
        .ops
        .iter()
        .map(|o| morphism.op_map.get(&o.name).unwrap_or(&o.name).clone())
        .collect();

    // Sorts to add (topologically sorted by parameter deps)
    let sorts_to_add: Vec<_> = codomain
        .sorts
        .iter()
        .filter(|s| !domain_sort_names_after_renames.contains(&s.name))
        .collect();
    let mut added_sorts = domain_sort_names_after_renames;
    let mut sorted_adds = Vec::new();
    let mut remaining = sorts_to_add;
    let max_iterations = remaining.len() + 1;
    for _ in 0..max_iterations {
        if remaining.is_empty() {
            break;
        }
        let (ready, not_ready): (Vec<_>, Vec<_>) = remaining
            .into_iter()
            .partition(|s| s.params.iter().all(|p| added_sorts.contains(&p.sort)));
        for sort in &ready {
            added_sorts.insert(Arc::clone(&sort.name));
            sorted_adds.push((*sort).clone());
        }
        remaining = not_ready;
    }
    for sort in sorted_adds {
        steps.push(TheoryEndofunctor {
            name: Arc::from(format!("add_sort_{}", sort.name)),
            precondition: TheoryConstraint::Unconstrained,
            transform: TheoryTransform::AddSort {
                sort,
                vertex_kind: None,
            },
        });
    }

    // Ops to add
    for op in &codomain.ops {
        if !domain_op_names_after_renames.contains(&op.name) {
            steps.push(TheoryEndofunctor {
                name: Arc::from(format!("add_op_{}", op.name)),
                precondition: TheoryConstraint::Unconstrained,
                transform: TheoryTransform::AddOp(op.clone()),
            });
        }
    }

    // Equations to add
    for eq in &codomain.eqs {
        let domain_has_eq = domain.eqs.iter().any(|deq| {
            let mapped = deq.rename_ops(
                &morphism
                    .op_map
                    .iter()
                    .map(|(k, v)| (Arc::clone(k), Arc::clone(v)))
                    .collect(),
            );
            mapped.lhs == eq.lhs && mapped.rhs == eq.rhs
        });
        if !domain_has_eq && domain.find_eq(&eq.name).is_none() {
            steps.push(TheoryEndofunctor {
                name: Arc::from(format!("add_eq_{}", eq.name)),
                precondition: TheoryConstraint::Unconstrained,
                transform: TheoryTransform::AddEquation(eq.clone()),
            });
        }
    }
}

/// Factorize a theory morphism into elementary endofunctors.
///
/// Given a morphism F: T1 → T2, decompose it into a sequence of
/// elementary endofunctors (add/drop/rename sort/op/equation) such
/// that applying them in order to T1 produces T2.
///
/// Ordering: drops first (equations → ops → sorts), then renames,
/// then adds (sorts → ops → equations). Within adds, sorts are
/// topologically sorted by parameter dependencies.
///
/// # Errors
///
/// Returns [`GatError::FactorizationError`] if the morphism cannot be
/// factorized into a valid sequence of transforms.
pub fn factorize(
    morphism: &TheoryMorphism,
    domain: &Theory,
    codomain: &Theory,
) -> Result<Factorization, GatError> {
    let mut steps = Vec::new();

    // Phase 1: Identify renames
    let mut sort_renames: Vec<(Arc<str>, Arc<str>)> = Vec::new();
    let mut op_renames: Vec<(Arc<str>, Arc<str>)> = Vec::new();

    for (old, new) in &morphism.sort_map {
        if old != new && domain.has_sort(old) && codomain.has_sort(new) {
            sort_renames.push((Arc::clone(old), Arc::clone(new)));
        }
    }
    for (old, new) in &morphism.op_map {
        if old != new && domain.has_op(old) && codomain.has_op(new) {
            op_renames.push((Arc::clone(old), Arc::clone(new)));
        }
    }

    // Phase 2: Drops
    emit_drops(&mut steps, morphism, domain, codomain);

    // Phase 3: Renames
    emit_renames(&mut steps, &sort_renames, &op_renames);

    // Phase 4: Adds
    emit_adds(&mut steps, morphism, domain, codomain);

    Ok(Factorization {
        steps,
        domain: Arc::clone(&morphism.domain),
        codomain: Arc::clone(&morphism.codomain),
    })
}

/// Validate that applying a factorization to the domain yields a theory
/// compatible with the codomain.
///
/// # Errors
///
/// Returns [`GatError::FactorizationError`] if the factorized theory is
/// missing sorts or operations from the codomain.
pub fn validate_factorization(
    factorization: &Factorization,
    domain: &Theory,
    codomain: &Theory,
) -> Result<(), GatError> {
    let mut current = domain.clone();
    for step in &factorization.steps {
        current = step.transform.apply(&current)?;
    }
    for sort in &codomain.sorts {
        if !current.has_sort(&sort.name) {
            return Err(GatError::FactorizationError(format!(
                "factorized theory missing sort '{}' from codomain",
                sort.name
            )));
        }
    }
    for op in &codomain.ops {
        if !current.has_op(&op.name) {
            return Err(GatError::FactorizationError(format!(
                "factorized theory missing op '{}' from codomain",
                op.name
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::eq::{Equation, Term};
    use crate::op::Operation;
    use crate::sort::{Sort, SortParam};

    fn graph_theory() -> Theory {
        Theory::new(
            "ThGraph",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![
                Operation::unary("src", "e", "Edge", "Vertex"),
                Operation::unary("tgt", "e", "Edge", "Vertex"),
            ],
            Vec::new(),
        )
    }

    fn renamed_graph_theory() -> Theory {
        Theory::new(
            "ThRenamedGraph",
            vec![Sort::simple("Node"), Sort::simple("Arrow")],
            vec![
                Operation::unary("source", "e", "Arrow", "Node"),
                Operation::unary("target", "e", "Arrow", "Node"),
            ],
            Vec::new(),
        )
    }

    #[test]
    fn identity_morphism_empty_factorization() {
        let t = graph_theory();
        let morph = TheoryMorphism::new(
            "id",
            "ThGraph",
            "ThGraph",
            HashMap::from([
                (Arc::from("Vertex"), Arc::from("Vertex")),
                (Arc::from("Edge"), Arc::from("Edge")),
            ]),
            HashMap::from([
                (Arc::from("src"), Arc::from("src")),
                (Arc::from("tgt"), Arc::from("tgt")),
            ]),
        );
        let result = factorize(&morph, &t, &t).unwrap();
        assert!(
            result.steps.is_empty(),
            "identity morphism should produce no steps"
        );
    }

    #[test]
    fn pure_rename_morphism() {
        let domain = graph_theory();
        let codomain = renamed_graph_theory();
        let morph = TheoryMorphism::new(
            "rename",
            "ThGraph",
            "ThRenamedGraph",
            HashMap::from([
                (Arc::from("Vertex"), Arc::from("Node")),
                (Arc::from("Edge"), Arc::from("Arrow")),
            ]),
            HashMap::from([
                (Arc::from("src"), Arc::from("source")),
                (Arc::from("tgt"), Arc::from("target")),
            ]),
        );
        let result = factorize(&morph, &domain, &codomain).unwrap();
        // Should have 4 renames (2 sorts + 2 ops)
        assert_eq!(result.steps.len(), 4);
        // Validate
        validate_factorization(&result, &domain, &codomain).unwrap();
    }

    #[test]
    fn adding_sort_produces_add_step() {
        let domain = Theory::new("T1", vec![Sort::simple("A")], Vec::new(), Vec::new());
        let codomain = Theory::new(
            "T2",
            vec![Sort::simple("A"), Sort::simple("B")],
            Vec::new(),
            Vec::new(),
        );
        let morph = TheoryMorphism::new(
            "add_b",
            "T1",
            "T2",
            HashMap::from([(Arc::from("A"), Arc::from("A"))]),
            HashMap::new(),
        );
        let result = factorize(&morph, &domain, &codomain).unwrap();
        assert_eq!(result.steps.len(), 1);
        assert!(matches!(
            result.steps[0].transform,
            TheoryTransform::AddSort { .. }
        ));
        validate_factorization(&result, &domain, &codomain).unwrap();
    }

    #[test]
    fn dropping_sort_produces_correct_sequence() {
        let domain = Theory::new(
            "T1",
            vec![Sort::simple("A"), Sort::simple("B")],
            vec![Operation::unary("f", "x", "A", "B")],
            Vec::new(),
        );
        let codomain = Theory::new("T2", vec![Sort::simple("A")], Vec::new(), Vec::new());
        let morph = TheoryMorphism::new(
            "drop_b",
            "T1",
            "T2",
            HashMap::from([(Arc::from("A"), Arc::from("A"))]),
            HashMap::new(),
        );
        let result = factorize(&morph, &domain, &codomain).unwrap();
        // Should drop op f first (depends on B), then sort B
        assert!(result.steps.len() >= 2);
        validate_factorization(&result, &domain, &codomain).unwrap();
    }

    #[test]
    fn dependent_sort_ordering() {
        let domain = Theory::new("T1", vec![Sort::simple("A")], Vec::new(), Vec::new());
        let codomain = Theory::new(
            "T2",
            vec![
                Sort::simple("A"),
                Sort::simple("B"),
                Sort::dependent("C", vec![SortParam::new("x", "B")]),
            ],
            Vec::new(),
            Vec::new(),
        );
        let morph = TheoryMorphism::new(
            "add",
            "T1",
            "T2",
            HashMap::from([(Arc::from("A"), Arc::from("A"))]),
            HashMap::new(),
        );
        let result = factorize(&morph, &domain, &codomain).unwrap();
        // B must be added before C (since C depends on B)
        let b_idx = result.steps.iter().position(
            |s| matches!(&s.transform, TheoryTransform::AddSort { sort, .. } if &*sort.name == "B"),
        );
        let c_idx = result.steps.iter().position(
            |s| matches!(&s.transform, TheoryTransform::AddSort { sort, .. } if &*sort.name == "C"),
        );
        assert!(b_idx.is_some() && c_idx.is_some());
        assert!(b_idx.unwrap() < c_idx.unwrap(), "B must come before C");
        validate_factorization(&result, &domain, &codomain).unwrap();
    }

    #[test]
    fn mixed_add_drop_rename() {
        let domain = Theory::new(
            "T1",
            vec![Sort::simple("A"), Sort::simple("B")],
            vec![Operation::unary("f", "x", "A", "B")],
            Vec::new(),
        );
        let codomain = Theory::new(
            "T2",
            vec![Sort::simple("Alpha"), Sort::simple("C")],
            vec![Operation::unary("g", "x", "Alpha", "C")],
            Vec::new(),
        );
        let morph = TheoryMorphism::new(
            "mixed",
            "T1",
            "T2",
            HashMap::from([
                (Arc::from("A"), Arc::from("Alpha")),
                // B is dropped, not mapped
            ]),
            HashMap::from([
                // f is dropped, not mapped
            ]),
        );
        let result = factorize(&morph, &domain, &codomain).unwrap();
        // Should have: drop f, drop B, rename A→Alpha, add C, add g
        assert!(!result.steps.is_empty());
        validate_factorization(&result, &domain, &codomain).unwrap();
    }

    #[test]
    fn equation_changes() {
        let domain = Theory::new(
            "T1",
            vec![Sort::simple("A")],
            vec![Operation::nullary("a", "A"), Operation::nullary("b", "A")],
            vec![Equation::new(
                "old_eq",
                Term::constant("a"),
                Term::constant("b"),
            )],
        );
        let codomain = Theory::new(
            "T2",
            vec![Sort::simple("A")],
            vec![Operation::nullary("a", "A"), Operation::nullary("b", "A")],
            vec![Equation::new(
                "new_eq",
                Term::constant("b"),
                Term::constant("a"),
            )],
        );
        let morph = TheoryMorphism::new(
            "eq_change",
            "T1",
            "T2",
            HashMap::from([(Arc::from("A"), Arc::from("A"))]),
            HashMap::from([
                (Arc::from("a"), Arc::from("a")),
                (Arc::from("b"), Arc::from("b")),
            ]),
        );
        let result = factorize(&morph, &domain, &codomain).unwrap();
        // Should drop old_eq and add new_eq
        let has_drop = result
            .steps
            .iter()
            .any(|s| matches!(&s.transform, TheoryTransform::DropEquation(n) if &**n == "old_eq"));
        let has_add = result.steps.iter().any(
            |s| matches!(&s.transform, TheoryTransform::AddEquation(eq) if &*eq.name == "new_eq"),
        );
        assert!(has_drop, "should drop old equation");
        assert!(has_add, "should add new equation");
        validate_factorization(&result, &domain, &codomain).unwrap();
    }

    #[test]
    fn validate_factorization_catches_missing_sort() {
        let domain = graph_theory();
        let bad_factorization = Factorization {
            steps: vec![],
            domain: Arc::from("ThGraph"),
            codomain: Arc::from("ThRenamedGraph"),
        };
        let codomain = renamed_graph_theory();
        let result = validate_factorization(&bad_factorization, &domain, &codomain);
        assert!(result.is_err());
    }
}
