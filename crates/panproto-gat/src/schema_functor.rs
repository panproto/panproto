use std::sync::Arc;

use rustc_hash::FxHashSet;

use crate::eq::{DirectedEquation, Equation, Term};
use crate::error::GatError;
use crate::morphism::TheoryMorphism;
use crate::op::Operation;
use crate::sort::{CoercionClass, Sort, SortKind, SortParam, ValueKind};
use crate::theory::Theory;

/// A predicate on theories: the precondition for applying a transform.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TheoryConstraint {
    /// Any theory satisfies this.
    Unconstrained,
    /// Theory must have a sort with this name.
    HasSort(Arc<str>),
    /// Theory must have an operation with this name.
    HasOp(Arc<str>),
    /// Theory must have an equation with this name.
    HasEquation(Arc<str>),
    /// Theory must have a directed equation with this name.
    HasDirectedEq(Arc<str>),
    /// Theory must have a value sort of the given kind.
    HasValSort(ValueKind),
    /// Theory must have a coercion sort between the given kinds.
    HasCoercion {
        /// The source value kind.
        from: ValueKind,
        /// The target value kind.
        to: ValueKind,
    },
    /// Theory must have a merger sort for the given kind.
    HasMerger(ValueKind),
    /// Theory must have a conflict policy with this name.
    HasPolicy(Arc<str>),
    /// Conjunction.
    All(Vec<Self>),
    /// Disjunction.
    Any(Vec<Self>),
    /// Negation.
    Not(Box<Self>),
}

/// How a theory endofunctor transforms a theory.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TheoryTransform {
    /// Identity: T ↦ T
    Identity,
    /// Add a sort: T ↦ T + {sort}
    ///
    /// The optional `vertex_kind` specifies the schema-level vertex kind
    /// for the Grothendieck fibration. When `None`, the vertex kind is
    /// derived from the sort: `Val(vk)` → canonical value kind name,
    /// `Structural` → sort name.
    AddSort {
        /// The sort to add.
        sort: Sort,
        /// Schema-level vertex kind override.
        vertex_kind: Option<Arc<str>>,
    },
    /// Drop a sort and all dependent ops/equations.
    DropSort(Arc<str>),
    /// Rename a sort.
    RenameSort {
        /// The old sort name.
        old: Arc<str>,
        /// The new sort name.
        new: Arc<str>,
    },
    /// Add an operation.
    AddOp(Operation),
    /// Drop an operation and dependent equations.
    DropOp(Arc<str>),
    /// Rename an operation.
    RenameOp {
        /// The old operation name.
        old: Arc<str>,
        /// The new operation name.
        new: Arc<str>,
    },
    /// Add an equation.
    AddEquation(Equation),
    /// Drop an equation.
    DropEquation(Arc<str>),
    /// Coerce a sort to a different value kind.
    CoerceSort {
        /// The sort to coerce.
        sort_name: Arc<str>,
        /// The target value kind.
        target_kind: ValueKind,
        /// The coercion expression.
        coercion_expr: panproto_expr::Expr,
        /// Optional inverse expression for round-tripping.
        inverse_expr: Option<panproto_expr::Expr>,
        /// Round-trip classification of this coercion.
        coercion_class: CoercionClass,
    },
    /// Merge two sorts into one.
    MergeSorts {
        /// The first sort to merge.
        sort_a: Arc<str>,
        /// The second sort to merge.
        sort_b: Arc<str>,
        /// The name for the merged sort.
        merged_name: Arc<str>,
        /// The merger expression.
        merger_expr: panproto_expr::Expr,
    },
    /// Add a sort with a default expression for backward compatibility.
    AddSortWithDefault {
        /// The sort to add.
        sort: Sort,
        /// Schema-level vertex kind override (see `AddSort`).
        vertex_kind: Option<Arc<str>>,
        /// The default expression for existing data.
        default_expr: panproto_expr::Expr,
    },
    /// Add a directed equation.
    AddDirectedEquation(DirectedEquation),
    /// Drop a directed equation by name.
    DropDirectedEquation(Arc<str>),
    /// Pullback along a theory morphism.
    Pullback(TheoryMorphism),
    /// Rename an edge label (JSON property key) without changing sort/op structure.
    ///
    /// This is a fiber-level natural isomorphism in the Grothendieck fibration:
    /// the theory is unchanged, but the schema-level edge metadata is relabeled.
    /// Always classified as `Iso` (empty complement, bijective relabeling).
    RenameEdgeName {
        /// The source sort of the edge whose label to rename.
        src_sort: Arc<str>,
        /// The target sort of the edge.
        tgt_sort: Arc<str>,
        /// The old edge label (JSON property key).
        old_name: Arc<str>,
        /// The new edge label.
        new_name: Arc<str>,
    },
    /// Apply a transform to the sub-theory reachable from a focus sort.
    ///
    /// Categorically, this is the left Kan extension along the inclusion
    /// `ι : Sub(T, focus) ↪ T` of the sub-theory at the focus sort.
    /// The inner transform is applied only to the sub-theory; the rest
    /// of `T` is unchanged. The result is the pushout of `T` and
    /// `inner(Sub(T, focus))` over `Sub(T, focus)`.
    ///
    /// At the instance level, the optic class depends on the edge kind
    /// connecting the parent to the focus sort:
    ///   - `prop` edge → Lens (apply once)
    ///   - `item` edge → Traversal (apply per element)
    ///   - `variant` edge → Prism (apply if present)
    ScopedTransform {
        /// The sort to focus on (root of the sub-theory).
        focus: Arc<str>,
        /// The inner transform applied within the focus.
        inner: Box<Self>,
    },
    /// Sequential composition: T ↦ G(F(T))
    Compose(Box<Self>, Box<Self>),
}

/// A theory endofunctor: maps theories to theories.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TheoryEndofunctor {
    /// Human-readable name.
    pub name: Arc<str>,
    /// Precondition.
    pub precondition: TheoryConstraint,
    /// The transformation.
    pub transform: TheoryTransform,
}

impl TheoryConstraint {
    /// Check if a theory satisfies this constraint.
    #[must_use]
    pub fn satisfied_by(&self, theory: &Theory) -> bool {
        match self {
            Self::Unconstrained => true,
            Self::HasSort(name) => theory.has_sort(name),
            Self::HasOp(name) => theory.has_op(name),
            Self::HasEquation(name) => theory.find_eq(name).is_some(),
            Self::HasDirectedEq(name) => theory.has_directed_eq(name),
            Self::HasValSort(vk) => theory.sorts.iter().any(|s| s.kind == SortKind::Val(*vk)),
            Self::HasCoercion { from, to } => theory.sorts.iter().any(|s| {
                matches!(s.kind, SortKind::Coercion { from: f, to: t, .. } if f == *from && t == *to)
            }),
            Self::HasMerger(vk) => theory.sorts.iter().any(|s| s.kind == SortKind::Merger(*vk)),
            Self::HasPolicy(name) => theory.has_policy(name),
            Self::All(cs) => cs.iter().all(|c| c.satisfied_by(theory)),
            Self::Any(cs) => cs.iter().any(|c| c.satisfied_by(theory)),
            Self::Not(c) => !c.satisfied_by(theory),
        }
    }
}

/// Drop a sort from a theory, cascading to dependent ops and equations.
fn apply_drop_sort(theory: &Theory, name: &Arc<str>) -> Theory {
    let sorts: Vec<_> = theory
        .sorts
        .iter()
        .filter(|s| s.name != *name)
        .cloned()
        .collect();
    let ops: Vec<_> = theory
        .ops
        .iter()
        .filter(|o| o.output != *name && o.inputs.iter().all(|(_, s)| s != name))
        .cloned()
        .collect();
    let eqs = filter_eqs_by_remaining_ops(&theory.eqs, &ops);
    Theory::new(Arc::clone(&theory.name), sorts, ops, eqs)
}

/// Rename a sort throughout a theory (sort defs, sort param refs, op signatures).
fn apply_rename_sort(theory: &Theory, old: &Arc<str>, new: &Arc<str>) -> Theory {
    let sorts: Vec<_> = theory
        .sorts
        .iter()
        .map(|s| {
            if s.name == *old {
                Sort {
                    name: Arc::clone(new),
                    params: s.params.clone(),
                    kind: s.kind.clone(),
                }
            } else {
                let params = s
                    .params
                    .iter()
                    .map(|p| {
                        if p.sort == *old {
                            SortParam {
                                name: Arc::clone(&p.name),
                                sort: Arc::clone(new),
                            }
                        } else {
                            p.clone()
                        }
                    })
                    .collect();
                Sort {
                    name: Arc::clone(&s.name),
                    params,
                    kind: s.kind.clone(),
                }
            }
        })
        .collect();
    let ops: Vec<_> = theory
        .ops
        .iter()
        .map(|o| {
            let inputs = o
                .inputs
                .iter()
                .map(|(n, s)| {
                    (
                        Arc::clone(n),
                        if s == old {
                            Arc::clone(new)
                        } else {
                            Arc::clone(s)
                        },
                    )
                })
                .collect();
            let output = if o.output == *old {
                Arc::clone(new)
            } else {
                Arc::clone(&o.output)
            };
            Operation {
                name: Arc::clone(&o.name),
                inputs,
                output,
            }
        })
        .collect();
    Theory::new(Arc::clone(&theory.name), sorts, ops, theory.eqs.clone())
}

/// Drop an operation from a theory, cascading to dependent equations.
fn apply_drop_op(theory: &Theory, name: &Arc<str>) -> Theory {
    let ops: Vec<_> = theory
        .ops
        .iter()
        .filter(|o| o.name != *name)
        .cloned()
        .collect();
    let eqs = filter_eqs_by_remaining_ops(&theory.eqs, &ops);
    Theory::new(Arc::clone(&theory.name), theory.sorts.clone(), ops, eqs)
}

/// Rename an operation throughout a theory (op defs and equation terms).
fn apply_rename_op(theory: &Theory, old: &Arc<str>, new: &Arc<str>) -> Theory {
    let ops: Vec<_> = theory
        .ops
        .iter()
        .map(|o| {
            if o.name == *old {
                Operation {
                    name: Arc::clone(new),
                    inputs: o.inputs.clone(),
                    output: Arc::clone(&o.output),
                }
            } else {
                o.clone()
            }
        })
        .collect();
    let mut op_map = std::collections::HashMap::new();
    op_map.insert(Arc::clone(old), Arc::clone(new));
    let eqs: Vec<_> = theory.eqs.iter().map(|eq| eq.rename_ops(&op_map)).collect();
    Theory::new(Arc::clone(&theory.name), theory.sorts.clone(), ops, eqs)
}

/// Apply a pullback (sort/op renaming) from a theory morphism.
fn apply_pullback(theory: &Theory, morphism: &TheoryMorphism) -> Theory {
    let mut result = theory.clone();
    for (old, new) in &morphism.sort_map {
        if old != new {
            result = apply_rename_sort(&result, old, new);
        }
    }
    for (old, new) in &morphism.op_map {
        if old != new {
            result = apply_rename_op(&result, old, new);
        }
    }
    result
}

/// Compute the set of sort names reachable from `start` via directed
/// operation edges.
///
/// An operation `op(a₁: S₁, …, aₙ: Sₙ) → T` creates directed edges
/// from each input sort Sᵢ to the output sort T. Starting from `start`,
/// we follow these directed edges to find all transitively reachable sorts.
///
/// This mirrors the schema-level BFS over outgoing edges: operations in
/// the theory correspond to edges in the schema, and the input→output
/// direction corresponds to the src→tgt direction.
fn reachable_sorts_from(theory: &Theory, start: &str) -> FxHashSet<Arc<str>> {
    let start_arc: Arc<str> = Arc::from(start);
    let mut reachable: FxHashSet<Arc<str>> = FxHashSet::default();
    reachable.insert(Arc::clone(&start_arc));
    let mut queue: std::collections::VecDeque<Arc<str>> = std::collections::VecDeque::new();
    queue.push_back(start_arc);
    while let Some(current) = queue.pop_front() {
        for op in &theory.ops {
            // If any input sort is the current sort, the output sort is reachable.
            let has_current_as_input = op.inputs.iter().any(|(_, s)| **s == *current);
            if has_current_as_input && reachable.insert(Arc::clone(&op.output)) {
                queue.push_back(Arc::clone(&op.output));
            }
        }
    }
    reachable
}

/// Collect all operation names referenced in a directed equation's terms.
fn collect_ops_in_directed_eq(deq: &DirectedEquation) -> Vec<Arc<str>> {
    let mut ops = Vec::new();
    collect_ops_in_term(&deq.lhs, &mut ops);
    collect_ops_in_term(&deq.rhs, &mut ops);
    ops
}

/// Filter equations, keeping only those whose ops are all in the remaining ops list.
fn filter_eqs_by_remaining_ops(eqs: &[Equation], ops: &[Operation]) -> Vec<Equation> {
    let remaining_op_names: FxHashSet<Arc<str>> = ops.iter().map(|o| Arc::clone(&o.name)).collect();
    eqs.iter()
        .filter(|eq| {
            let ops_used = collect_ops_in_equation(eq);
            ops_used.iter().all(|op| remaining_op_names.contains(op))
        })
        .cloned()
        .collect()
}

impl TheoryTransform {
    /// Apply this transform to a theory.
    ///
    /// # Errors
    ///
    /// Returns [`GatError::FactorizationError`] if the transform cannot be applied.
    #[allow(clippy::too_many_lines)]
    pub fn apply(&self, theory: &Theory) -> Result<Theory, GatError> {
        match self {
            Self::Identity => Ok(theory.clone()),
            Self::AddSort { sort, .. }
            | Self::AddSortWithDefault {
                sort,
                default_expr: _,
                vertex_kind: _,
            } => {
                let mut sorts = theory.sorts.clone();
                sorts.push(sort.clone());
                Ok(Theory::full(
                    Arc::clone(&theory.name),
                    theory.extends.clone(),
                    sorts,
                    theory.ops.clone(),
                    theory.eqs.clone(),
                    theory.directed_eqs.clone(),
                    theory.policies.clone(),
                ))
            }
            Self::DropSort(name) => Ok(apply_drop_sort(theory, name)),
            Self::RenameSort { old, new } => Ok(apply_rename_sort(theory, old, new)),
            Self::AddOp(op) => {
                let mut ops = theory.ops.clone();
                ops.push(op.clone());
                Ok(Theory::full(
                    Arc::clone(&theory.name),
                    theory.extends.clone(),
                    theory.sorts.clone(),
                    ops,
                    theory.eqs.clone(),
                    theory.directed_eqs.clone(),
                    theory.policies.clone(),
                ))
            }
            Self::DropOp(name) => Ok(apply_drop_op(theory, name)),
            Self::RenameOp { old, new } => Ok(apply_rename_op(theory, old, new)),
            Self::AddEquation(eq) => {
                let mut eqs = theory.eqs.clone();
                eqs.push(eq.clone());
                Ok(Theory::full(
                    Arc::clone(&theory.name),
                    theory.extends.clone(),
                    theory.sorts.clone(),
                    theory.ops.clone(),
                    eqs,
                    theory.directed_eqs.clone(),
                    theory.policies.clone(),
                ))
            }
            Self::DropEquation(name) => {
                let eqs: Vec<_> = theory
                    .eqs
                    .iter()
                    .filter(|eq| eq.name != *name)
                    .cloned()
                    .collect();
                Ok(Theory::full(
                    Arc::clone(&theory.name),
                    theory.extends.clone(),
                    theory.sorts.clone(),
                    theory.ops.clone(),
                    eqs,
                    theory.directed_eqs.clone(),
                    theory.policies.clone(),
                ))
            }
            Self::CoerceSort {
                sort_name,
                target_kind,
                coercion_expr: _,
                inverse_expr: _,
                coercion_class: _,
            } => {
                let sorts: Vec<_> = theory
                    .sorts
                    .iter()
                    .map(|s| {
                        if s.name == *sort_name {
                            Sort {
                                name: Arc::clone(&s.name),
                                params: s.params.clone(),
                                kind: SortKind::Val(*target_kind),
                            }
                        } else {
                            s.clone()
                        }
                    })
                    .collect();
                Ok(Theory::full(
                    Arc::clone(&theory.name),
                    theory.extends.clone(),
                    sorts,
                    theory.ops.clone(),
                    theory.eqs.clone(),
                    theory.directed_eqs.clone(),
                    theory.policies.clone(),
                ))
            }
            Self::MergeSorts {
                sort_a,
                sort_b,
                merged_name,
                merger_expr: _,
            } => {
                let sorts: Vec<_> = theory
                    .sorts
                    .iter()
                    .filter_map(|s| {
                        if s.name == *sort_a {
                            Some(Sort {
                                name: Arc::clone(merged_name),
                                params: s.params.clone(),
                                kind: s.kind.clone(),
                            })
                        } else if s.name == *sort_b {
                            None
                        } else {
                            Some(s.clone())
                        }
                    })
                    .collect();
                // Rename references in ops
                let ops: Vec<_> = theory
                    .ops
                    .iter()
                    .map(|o| {
                        let inputs: Vec<_> = o
                            .inputs
                            .iter()
                            .map(|(n, s)| {
                                let mapped = if s == sort_a || s == sort_b {
                                    Arc::clone(merged_name)
                                } else {
                                    Arc::clone(s)
                                };
                                (Arc::clone(n), mapped)
                            })
                            .collect();
                        let output = if o.output == *sort_a || o.output == *sort_b {
                            Arc::clone(merged_name)
                        } else {
                            Arc::clone(&o.output)
                        };
                        Operation {
                            name: Arc::clone(&o.name),
                            inputs,
                            output,
                        }
                    })
                    .collect();
                Ok(Theory::full(
                    Arc::clone(&theory.name),
                    theory.extends.clone(),
                    sorts,
                    ops,
                    theory.eqs.clone(),
                    theory.directed_eqs.clone(),
                    theory.policies.clone(),
                ))
            }
            Self::AddDirectedEquation(de) => {
                let mut directed_eqs = theory.directed_eqs.clone();
                directed_eqs.push(de.clone());
                Ok(Theory::full(
                    Arc::clone(&theory.name),
                    theory.extends.clone(),
                    theory.sorts.clone(),
                    theory.ops.clone(),
                    theory.eqs.clone(),
                    directed_eqs,
                    theory.policies.clone(),
                ))
            }
            Self::DropDirectedEquation(name) => {
                let directed_eqs: Vec<_> = theory
                    .directed_eqs
                    .iter()
                    .filter(|de| de.name != *name)
                    .cloned()
                    .collect();
                Ok(Theory::full(
                    Arc::clone(&theory.name),
                    theory.extends.clone(),
                    theory.sorts.clone(),
                    theory.ops.clone(),
                    theory.eqs.clone(),
                    directed_eqs,
                    theory.policies.clone(),
                ))
            }
            Self::Pullback(morphism) => Ok(apply_pullback(theory, morphism)),
            Self::RenameEdgeName { .. } => {
                // Fiber-level operation: the theory is unchanged.
                // The actual relabeling happens at the schema level
                // in apply_theory_transform_to_schema.
                Ok(theory.clone())
            }
            Self::ScopedTransform { focus, inner } => {
                // Extract the sub-theory reachable from the focus sort,
                // apply the inner transform, and merge back.
                if !theory.has_sort(focus) {
                    return Err(GatError::FactorizationError(format!(
                        "scoped transform focus sort '{focus}' not found in theory"
                    )));
                }
                // Find sorts reachable from focus via operations.
                let reachable = reachable_sorts_from(theory, focus);
                // Build the sub-theory from reachable sorts.
                let sub_sorts: Vec<_> = theory
                    .sorts
                    .iter()
                    .filter(|s| reachable.contains(&s.name))
                    .cloned()
                    .collect();
                let sub_ops: Vec<_> = theory
                    .ops
                    .iter()
                    .filter(|op| {
                        op.inputs.iter().all(|i| reachable.contains(&i.1))
                            && reachable.contains(&op.output)
                    })
                    .cloned()
                    .collect();
                let sub_eqs: Vec<_> = filter_eqs_by_remaining_ops(&theory.eqs, &sub_ops);
                let sub_directed_eqs: Vec<_> = theory
                    .directed_eqs
                    .iter()
                    .filter(|de| {
                        let ops_used = collect_ops_in_directed_eq(de);
                        let sub_op_names: FxHashSet<Arc<str>> =
                            sub_ops.iter().map(|o| Arc::clone(&o.name)).collect();
                        ops_used.iter().all(|op| sub_op_names.contains(op))
                    })
                    .cloned()
                    .collect();
                let sub_theory = Theory::full(
                    Arc::from(format!("{}_sub_{focus}", theory.name)),
                    Vec::new(),
                    sub_sorts,
                    sub_ops,
                    sub_eqs,
                    sub_directed_eqs,
                    Vec::new(),
                );
                // Apply inner transform to sub-theory.
                let transformed_sub = inner.apply(&sub_theory)?;
                // Merge: replace sub-theory sorts/ops with transformed versions,
                // keep everything outside the focus unchanged.
                let mut merged_sorts: Vec<_> = theory
                    .sorts
                    .iter()
                    .filter(|s| !reachable.contains(&s.name))
                    .cloned()
                    .collect();
                merged_sorts.extend(transformed_sub.sorts);
                let sub_op_names: FxHashSet<Arc<str>> = theory
                    .ops
                    .iter()
                    .filter(|op| {
                        op.inputs.iter().all(|i| reachable.contains(&i.1))
                            && reachable.contains(&op.output)
                    })
                    .map(|o| Arc::clone(&o.name))
                    .collect();
                let mut merged_ops: Vec<_> = theory
                    .ops
                    .iter()
                    .filter(|o| !sub_op_names.contains(&o.name))
                    .cloned()
                    .collect();
                merged_ops.extend(transformed_sub.ops);
                Ok(Theory::full(
                    Arc::clone(&theory.name),
                    theory.extends.clone(),
                    merged_sorts,
                    merged_ops,
                    theory.eqs.clone(),
                    theory.directed_eqs.clone(),
                    theory.policies.clone(),
                ))
            }
            Self::Compose(first, second) => {
                let intermediate = first.apply(theory)?;
                second.apply(&intermediate)
            }
        }
    }
}

impl TheoryEndofunctor {
    /// Check if this functor can act on the given theory.
    #[must_use]
    pub fn applicable_to(&self, theory: &Theory) -> bool {
        self.precondition.satisfied_by(theory)
    }

    /// Apply this functor to a theory.
    ///
    /// # Errors
    ///
    /// Returns [`GatError::FactorizationError`] if the precondition is not
    /// satisfied or if the transform fails.
    pub fn apply(&self, theory: &Theory) -> Result<Theory, GatError> {
        if !self.applicable_to(theory) {
            return Err(GatError::FactorizationError(format!(
                "endofunctor '{}' precondition not satisfied",
                self.name
            )));
        }
        self.transform.apply(theory)
    }

    /// Compose with another endofunctor: self then other (G ∘ F).
    #[must_use]
    pub fn compose(&self, other: &Self) -> Self {
        Self {
            name: Arc::from(format!("{}.{}", other.name, self.name)),
            // The second functor's precondition applies to the transformed
            // theory, which we check dynamically at apply time.
            precondition: self.precondition.clone(),
            transform: TheoryTransform::Compose(
                Box::new(self.transform.clone()),
                Box::new(other.transform.clone()),
            ),
        }
    }
}

/// Collect all operation names used in an equation.
fn collect_ops_in_equation(eq: &Equation) -> Vec<Arc<str>> {
    let mut ops = Vec::new();
    collect_ops_in_term(&eq.lhs, &mut ops);
    collect_ops_in_term(&eq.rhs, &mut ops);
    ops
}

fn collect_ops_in_term(term: &Term, ops: &mut Vec<Arc<str>>) {
    match term {
        Term::Var(_) => {}
        Term::App { op, args } => {
            ops.push(Arc::clone(op));
            for arg in args {
                collect_ops_in_term(arg, ops);
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

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

    #[test]
    fn constraint_unconstrained() {
        assert!(TheoryConstraint::Unconstrained.satisfied_by(&graph_theory()));
    }

    #[test]
    fn constraint_has_sort() {
        let t = graph_theory();
        assert!(TheoryConstraint::HasSort(Arc::from("Vertex")).satisfied_by(&t));
        assert!(!TheoryConstraint::HasSort(Arc::from("Foo")).satisfied_by(&t));
    }

    #[test]
    fn constraint_has_op() {
        let t = graph_theory();
        assert!(TheoryConstraint::HasOp(Arc::from("src")).satisfied_by(&t));
        assert!(!TheoryConstraint::HasOp(Arc::from("bar")).satisfied_by(&t));
    }

    #[test]
    fn constraint_all() {
        let t = graph_theory();
        let c = TheoryConstraint::All(vec![
            TheoryConstraint::HasSort(Arc::from("Vertex")),
            TheoryConstraint::HasSort(Arc::from("Edge")),
        ]);
        assert!(c.satisfied_by(&t));
        let c2 = TheoryConstraint::All(vec![
            TheoryConstraint::HasSort(Arc::from("Vertex")),
            TheoryConstraint::HasSort(Arc::from("Missing")),
        ]);
        assert!(!c2.satisfied_by(&t));
    }

    #[test]
    fn constraint_not() {
        let t = graph_theory();
        assert!(
            TheoryConstraint::Not(Box::new(TheoryConstraint::HasSort(Arc::from("Foo"))))
                .satisfied_by(&t)
        );
    }

    #[test]
    fn transform_identity() {
        let t = graph_theory();
        let result = TheoryTransform::Identity.apply(&t).unwrap();
        assert_eq!(result.sorts.len(), t.sorts.len());
        assert_eq!(result.ops.len(), t.ops.len());
    }

    #[test]
    fn transform_add_sort() {
        let t = graph_theory();
        let result = TheoryTransform::AddSort {
            sort: Sort::simple("Label"),
            vertex_kind: None,
        }
        .apply(&t)
        .unwrap();
        assert_eq!(result.sorts.len(), 3);
        assert!(result.has_sort("Label"));
    }

    #[test]
    fn transform_drop_sort() {
        let t = graph_theory();
        let result = TheoryTransform::DropSort(Arc::from("Edge"))
            .apply(&t)
            .unwrap();
        assert_eq!(result.sorts.len(), 1);
        assert!(result.has_sort("Vertex"));
        // Ops referencing Edge should be dropped too
        assert_eq!(result.ops.len(), 0);
    }

    #[test]
    fn transform_rename_sort() {
        let t = graph_theory();
        let result = TheoryTransform::RenameSort {
            old: Arc::from("Vertex"),
            new: Arc::from("Node"),
        }
        .apply(&t)
        .unwrap();
        assert!(result.has_sort("Node"));
        assert!(!result.has_sort("Vertex"));
        // Ops should reference the renamed sort
        let src = result.find_op("src").unwrap();
        assert_eq!(&*src.output, "Node");
    }

    #[test]
    fn transform_add_op() {
        let t = graph_theory();
        let result = TheoryTransform::AddOp(Operation::unary("label", "e", "Edge", "Vertex"))
            .apply(&t)
            .unwrap();
        assert_eq!(result.ops.len(), 3);
        assert!(result.has_op("label"));
    }

    #[test]
    fn transform_drop_op() {
        let t = graph_theory();
        let result = TheoryTransform::DropOp(Arc::from("src")).apply(&t).unwrap();
        assert_eq!(result.ops.len(), 1);
        assert!(!result.has_op("src"));
        assert!(result.has_op("tgt"));
    }

    #[test]
    fn transform_rename_op() {
        let t = graph_theory();
        let result = TheoryTransform::RenameOp {
            old: Arc::from("src"),
            new: Arc::from("source"),
        }
        .apply(&t)
        .unwrap();
        assert!(result.has_op("source"));
        assert!(!result.has_op("src"));
    }

    #[test]
    fn transform_compose() {
        let t = graph_theory();
        let composed = TheoryTransform::Compose(
            Box::new(TheoryTransform::AddSort {
                sort: Sort::simple("Label"),
                vertex_kind: None,
            }),
            Box::new(TheoryTransform::RenameSort {
                old: Arc::from("Vertex"),
                new: Arc::from("Node"),
            }),
        );
        let result = composed.apply(&t).unwrap();
        assert!(result.has_sort("Label"));
        assert!(result.has_sort("Node"));
        assert!(!result.has_sort("Vertex"));
    }

    #[test]
    fn endofunctor_applicable() {
        let t = graph_theory();
        let f = TheoryEndofunctor {
            name: Arc::from("add_label"),
            precondition: TheoryConstraint::HasSort(Arc::from("Vertex")),
            transform: TheoryTransform::AddSort {
                sort: Sort::simple("Label"),
                vertex_kind: None,
            },
        };
        assert!(f.applicable_to(&t));
        let result = f.apply(&t).unwrap();
        assert!(result.has_sort("Label"));
    }

    #[test]
    fn endofunctor_not_applicable() {
        let t = graph_theory();
        let f = TheoryEndofunctor {
            name: Arc::from("needs_foo"),
            precondition: TheoryConstraint::HasSort(Arc::from("Foo")),
            transform: TheoryTransform::Identity,
        };
        assert!(!f.applicable_to(&t));
        assert!(f.apply(&t).is_err());
    }

    #[test]
    fn endofunctor_compose() {
        let f1 = TheoryEndofunctor {
            name: Arc::from("add_label"),
            precondition: TheoryConstraint::Unconstrained,
            transform: TheoryTransform::AddSort {
                sort: Sort::simple("Label"),
                vertex_kind: None,
            },
        };
        let f2 = TheoryEndofunctor {
            name: Arc::from("rename_vertex"),
            precondition: TheoryConstraint::HasSort(Arc::from("Vertex")),
            transform: TheoryTransform::RenameSort {
                old: Arc::from("Vertex"),
                new: Arc::from("Node"),
            },
        };
        let composed = f1.compose(&f2);
        let t = graph_theory();
        let result = composed.apply(&t).unwrap();
        assert!(result.has_sort("Label"));
        assert!(result.has_sort("Node"));
    }

    #[test]
    fn drop_sort_cascades_to_equations() {
        let t = Theory::new(
            "T",
            vec![Sort::simple("A"), Sort::simple("B")],
            vec![
                Operation::unary("f", "x", "A", "B"),
                Operation::nullary("a0", "A"),
            ],
            vec![Equation::new(
                "law",
                Term::app("f", vec![Term::constant("a0")]),
                Term::app("f", vec![Term::constant("a0")]),
            )],
        );
        let result = TheoryTransform::DropSort(Arc::from("A")).apply(&t).unwrap();
        assert_eq!(result.sorts.len(), 1);
        assert_eq!(result.ops.len(), 0); // f and a0 reference A
        assert_eq!(result.eqs.len(), 0); // law uses f which was dropped
    }
}
