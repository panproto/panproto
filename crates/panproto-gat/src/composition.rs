//! Declarative composition specs for theory colimit recipes.
//!
//! A [`CompositionSpec`] records the exact sequence of colimit steps used
//! to build a composed theory. This serves two purposes:
//!
//! 1. **Reproducibility**: given a spec and a theory registry, [`recompose`]
//!    replays the recipe to reconstruct the theory.
//! 2. **Serialization**: specs are plain data (`Serialize`/`Deserialize`)
//!    and can be stored alongside protocols so that downstream tools know
//!    how a theory was assembled without re-running the registration logic.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::colimit::colimit_by_name;
use crate::error::GatError;
use crate::op::Operation;
use crate::sort::Sort;
use crate::theory::Theory;

/// Declarative recipe for composing theories via colimit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionSpec {
    /// Name of the resulting composed theory.
    pub result_name: String,
    /// Ordered sequence of composition steps.
    pub steps: Vec<CompositionStep>,
}

/// A single step in a composition recipe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompositionStep {
    /// Reference a named theory from the registry.
    Base(String),
    /// Colimit of two theories over a shared sub-theory.
    Colimit {
        /// Name of the left theory (from registry or a previous step).
        left: String,
        /// Name of the right theory (from registry or a previous step).
        right: String,
        /// Sort names shared between left and right (identified in the colimit).
        shared_sorts: Vec<String>,
        /// Operation names shared between left and right (identified in the colimit).
        /// Operations listed here must exist in both theories with compatible signatures.
        #[serde(default)]
        shared_ops: Vec<String>,
    },
}

/// Replay a composition spec against a theory registry.
///
/// Each step either looks up a base theory or computes a colimit.
/// Intermediate results are stored under generated names (`step_0`,
/// `step_1`, ...). The final step's result is renamed to
/// `spec.result_name`.
///
/// # Errors
///
/// Returns [`GatError::TheoryNotFound`] if a referenced theory is
/// missing from both the registry and intermediate results.
/// Propagates any [`GatError`] from [`colimit_by_name`].
pub fn recompose<S: ::std::hash::BuildHasher>(
    spec: &CompositionSpec,
    registry: &HashMap<String, Theory, S>,
) -> Result<Theory, GatError> {
    let mut intermediates: HashMap<String, Theory> = registry
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let mut last_name: Option<String> = None;

    for (i, step) in spec.steps.iter().enumerate() {
        let step_name = format!("step_{i}");
        match step {
            CompositionStep::Base(name) => {
                if !intermediates.contains_key(name) {
                    return Err(GatError::TheoryNotFound(name.clone()));
                }
                last_name = Some(name.clone());
            }
            CompositionStep::Colimit {
                left,
                right,
                shared_sorts,
                shared_ops,
            } => {
                let left_theory = intermediates
                    .get(left)
                    .ok_or_else(|| GatError::TheoryNotFound(left.clone()))?
                    .clone();
                let right_theory = intermediates
                    .get(right)
                    .ok_or_else(|| GatError::TheoryNotFound(right.clone()))?
                    .clone();

                // Build the shared sub-theory from named sorts and operations.
                // Operations must exist in both theories to form a valid span.
                let mut shared_op_defs: Vec<Operation> = Vec::with_capacity(shared_ops.len());
                for op_name in shared_ops {
                    let left_op = left_theory.find_op(op_name).ok_or_else(|| {
                        GatError::FactorizationError(format!(
                            "shared operation '{op_name}' not found in left theory '{left}'"
                        ))
                    })?;
                    if !right_theory.has_op(op_name) {
                        return Err(GatError::FactorizationError(format!(
                            "shared operation '{op_name}' not found in right theory '{right}'"
                        )));
                    }
                    shared_op_defs.push(left_op.clone());
                }

                let shared = Theory::new(
                    &*step_name,
                    shared_sorts
                        .iter()
                        .map(|s| Sort::simple(s.as_str()))
                        .collect(),
                    shared_op_defs,
                    vec![],
                );

                let result = colimit_by_name(&left_theory, &right_theory, &shared)?;
                intermediates.insert(step_name.clone(), result);
                last_name = Some(step_name);
            }
        }
    }

    let final_name = last_name.ok_or_else(|| {
        GatError::TheoryNotFound(format!("empty composition spec for '{}'", spec.result_name))
    })?;

    let mut theory = intermediates
        .get(&final_name)
        .ok_or_else(|| GatError::TheoryNotFound(final_name.clone()))?
        .clone();
    theory.name = spec.result_name.clone().into();
    Ok(theory)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::op::Operation;
    use crate::sort::{Sort, SortParam};

    fn base_registry() -> HashMap<String, Theory> {
        let mut reg = HashMap::new();
        reg.insert(
            "ThGraph".into(),
            Theory::new(
                "ThGraph",
                vec![Sort::simple("Vertex"), Sort::simple("Edge")],
                vec![
                    Operation::unary("src", "e", "Edge", "Vertex"),
                    Operation::unary("tgt", "e", "Edge", "Vertex"),
                ],
                vec![],
            ),
        );
        reg.insert(
            "ThConstraint".into(),
            Theory::new(
                "ThConstraint",
                vec![
                    Sort::simple("Vertex"),
                    Sort::dependent("Constraint", vec![SortParam::new("v", "Vertex")]),
                ],
                vec![Operation::unary("target", "c", "Constraint", "Vertex")],
                vec![],
            ),
        );
        reg.insert(
            "ThMulti".into(),
            Theory::new(
                "ThMulti",
                vec![
                    Sort::simple("Vertex"),
                    Sort::simple("Edge"),
                    Sort::simple("EdgeLabel"),
                ],
                vec![Operation::unary("edge_label", "e", "Edge", "EdgeLabel")],
                vec![],
            ),
        );
        reg.insert(
            "ThWType".into(),
            Theory::new(
                "ThWType",
                vec![
                    Sort::simple("Node"),
                    Sort::simple("Arc"),
                    Sort::simple("Value"),
                ],
                vec![
                    Operation::unary("anchor", "n", "Node", "Vertex"),
                    Operation::unary("arc_src", "a", "Arc", "Node"),
                    Operation::unary("arc_tgt", "a", "Arc", "Node"),
                    Operation::unary("arc_edge", "a", "Arc", "Edge"),
                    Operation::unary("node_value", "n", "Node", "Value"),
                ],
                vec![],
            ),
        );
        reg
    }

    #[test]
    fn base_step_verifies_existence() {
        let reg = base_registry();
        let spec = CompositionSpec {
            result_name: "Result".into(),
            steps: vec![CompositionStep::Base("ThWType".into())],
        };
        let theory = recompose(&spec, &reg).unwrap();
        assert_eq!(&*theory.name, "Result");
        assert_eq!(theory.sorts.len(), 3);
    }

    #[test]
    fn base_step_missing_theory_errors() {
        let reg = base_registry();
        let spec = CompositionSpec {
            result_name: "Result".into(),
            steps: vec![CompositionStep::Base("ThNonexistent".into())],
        };
        let result = recompose(&spec, &reg);
        assert!(matches!(result, Err(GatError::TheoryNotFound(_))));
    }

    #[test]
    fn single_colimit_step() {
        let reg = base_registry();
        let spec = CompositionSpec {
            result_name: "GraphConstraint".into(),
            steps: vec![CompositionStep::Colimit {
                left: "ThGraph".into(),
                right: "ThConstraint".into(),
                shared_sorts: vec!["Vertex".into()],
                shared_ops: vec![],
            }],
        };
        let theory = recompose(&spec, &reg).unwrap();
        assert_eq!(&*theory.name, "GraphConstraint");
        // Vertex, Edge, Constraint
        assert_eq!(theory.sorts.len(), 3);
        // src, tgt, target
        assert_eq!(theory.ops.len(), 3);
    }

    #[test]
    fn chained_colimit_matches_direct() {
        let reg = base_registry();

        // Build via spec (the declarative way).
        let spec = CompositionSpec {
            result_name: "SchemaA".into(),
            steps: vec![
                CompositionStep::Colimit {
                    left: "ThGraph".into(),
                    right: "ThConstraint".into(),
                    shared_sorts: vec!["Vertex".into()],
                    shared_ops: vec![],
                },
                CompositionStep::Colimit {
                    left: "step_0".into(),
                    right: "ThMulti".into(),
                    shared_sorts: vec!["Vertex".into(), "Edge".into()],
                    shared_ops: vec![],
                },
            ],
        };
        let from_spec = recompose(&spec, &reg).unwrap();

        // Build directly (the imperative way).
        let g = reg.get("ThGraph").unwrap();
        let c = reg.get("ThConstraint").unwrap();
        let m = reg.get("ThMulti").unwrap();

        let shared_v = Theory::new("sv", vec![Sort::simple("Vertex")], vec![], vec![]);
        let gc = colimit_by_name(g, c, &shared_v).unwrap();

        let shared_ve = Theory::new(
            "sve",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![],
            vec![],
        );
        let mut direct = colimit_by_name(&gc, m, &shared_ve).unwrap();
        direct.name = "SchemaA".into();

        // Same sorts and ops.
        assert_eq!(from_spec.sorts.len(), direct.sorts.len());
        assert_eq!(from_spec.ops.len(), direct.ops.len());
        for sort in &direct.sorts {
            assert!(
                from_spec.find_sort(&sort.name).is_some(),
                "missing sort: {}",
                sort.name
            );
        }
        for op in &direct.ops {
            assert!(
                from_spec.find_op(&op.name).is_some(),
                "missing op: {}",
                op.name
            );
        }
    }

    #[test]
    fn empty_spec_errors() {
        let reg = base_registry();
        let spec = CompositionSpec {
            result_name: "Empty".into(),
            steps: vec![],
        };
        let result = recompose(&spec, &reg);
        assert!(matches!(result, Err(GatError::TheoryNotFound(_))));
    }

    #[test]
    fn serde_round_trip() {
        let spec = CompositionSpec {
            result_name: "SchemaA".into(),
            steps: vec![
                CompositionStep::Colimit {
                    left: "ThGraph".into(),
                    right: "ThConstraint".into(),
                    shared_sorts: vec!["Vertex".into()],
                    shared_ops: vec![],
                },
                CompositionStep::Base("ThWType".into()),
            ],
        };
        let json = serde_json::to_string(&spec).unwrap();
        let roundtripped: CompositionSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(spec, roundtripped);
    }
}
