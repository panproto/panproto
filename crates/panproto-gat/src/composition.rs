//! Declarative composition specs for theory colimit recipes.
//!
//! A [`CompositionSpec`] records the exact sequence of steps used to build
//! a composed theory. This serves two purposes:
//!
//! 1. **Reproducibility**: given a spec and a theory registry, [`recompose`]
//!    replays the recipe to reconstruct the theory.
//! 2. **Serialization**: specs are plain data (`Serialize`/`Deserialize`)
//!    and can be stored alongside protocols so that downstream tools know
//!    how a theory was assembled without re-running the registration logic.
//!
//! A step is either a binary [`CompositionStep::Colimit`] over shared names,
//! or a [`CompositionStep::Diagram`] that records a whole
//! [`crate::TheoryDiagram`] and every option of the [`crate::compose()`] call
//! but the result name.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::colimit::colimit_by_name;
use crate::compose::{
    ClashPolicy, ComposeOptions, CompositionMode, DEFAULT_VERIFY_BUDGET, DiagramEdge, DiagramNode,
    TheoryDiagram, compose,
};
use crate::eq::Term;
use crate::error::GatError;
use crate::morphism::{OpAssignment, TheoryMorphism};
use crate::op::Operation;
use crate::sort::Sort;
use crate::theory::Theory;

/// Declarative recipe for composing theories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionSpec {
    /// Name of the resulting composed theory.
    pub result_name: String,
    /// Ordered sequence of composition steps.
    pub steps: Vec<CompositionStep>,
    /// The recipe format: 0 for a recipe every reader since 0.74 replays
    /// (absent when decoding such a recipe), 1 for one that uses a
    /// [`CompositionStep::Diagram`] or a [`CompositionStep::Colimit`] in a
    /// mode other than [`CompositionMode::Amalgamate`].
    #[serde(default)]
    pub version: u16,
}

impl CompositionSpec {
    /// A spec whose `version` is the least that expresses `steps`.
    #[must_use]
    pub fn new(result_name: impl Into<String>, steps: Vec<CompositionStep>) -> Self {
        let version = required_version(&steps);
        Self {
            result_name: result_name.into(),
            steps,
            version,
        }
    }

    /// The least recipe format that expresses this spec's steps.
    #[must_use]
    pub fn required_version(&self) -> u16 {
        required_version(&self.steps)
    }
}

/// 1 when a step needs the format that records diagrams and modes, else 0.
fn required_version(steps: &[CompositionStep]) -> u16 {
    let needs_one = steps.iter().any(|step| match step {
        CompositionStep::Base(_) => false,
        CompositionStep::Colimit { mode, .. } => *mode != CompositionMode::Amalgamate,
        CompositionStep::Diagram { .. } => true,
    });
    u16::from(needs_one)
}

/// The mode of a recipe step that does not state one.
const fn amalgamate() -> CompositionMode {
    CompositionMode::Amalgamate
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
        /// How the colimit identifies symbols; recipes that do not state it
        /// are [`CompositionMode::Amalgamate`], which is what they always
        /// meant.
        #[serde(default = "amalgamate")]
        mode: CompositionMode,
    },
    /// The colimit of a whole diagram, as [`crate::compose()`] computes it.
    Diagram {
        /// The node names, each resolved first against `inline`, then
        /// against the registry and earlier steps; each is also the node's
        /// label.
        nodes: Vec<String>,
        /// The edges between them.
        edges: Vec<RecipeEdge>,
        /// The composition mode.
        mode: CompositionMode,
        /// The clash policy.
        clash: ClashPolicy,
        /// The rewrite budget.
        rewrite_budget: u32,
        /// Theories the diagram builds for itself (apex theories, say),
        /// which are not in the registry.
        inline: Vec<Theory>,
    },
}

/// One edge of a [`CompositionStep::Diagram`]: a theory morphism between
/// two of its nodes, by name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeEdge {
    /// The domain node's name.
    pub src: String,
    /// The codomain node's name.
    pub tgt: String,
    /// The sort map, as `(domain sort, codomain sort)` pairs.
    pub sorts: Vec<(String, String)>,
    /// The operation map, as `(domain operation, image)` pairs.
    pub ops: Vec<(String, RecipeOpImage)>,
}

/// The image of an operation under a [`RecipeEdge`].
///
/// Serialized externally tagged in `snake_case`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecipeOpImage {
    /// A rename to a codomain operation.
    Op(String),
    /// A derived term over the codomain's operations.
    Term(Term),
}

/// Replay a composition spec against a theory registry.
///
/// Each step looks up a base theory, computes a colimit, or composes a
/// diagram. Intermediate results are stored under generated names
/// (`step_0`, `step_1`, ...). The final step's result is renamed to
/// `spec.result_name`. A [`CompositionStep::Colimit`] uses
/// [`colimit_by_name`] unless its mode is
/// [`CompositionMode::ExplicitIdentity`], in which case it composes the
/// same by-name span in that mode, rejecting name clashes.
///
/// # Errors
///
/// Returns [`GatError::TheoryNotFound`] if a referenced theory is
/// missing from the inline theories, the registry and intermediate
/// results. Propagates any [`GatError`] from [`colimit_by_name`], and a
/// refused composition as [`GatError::Composition`].
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
                mode,
            } => {
                let result = replay_colimit(
                    &intermediates,
                    (left, right),
                    shared_sorts,
                    shared_ops,
                    *mode,
                    &step_name,
                )?;
                intermediates.insert(step_name.clone(), result);
                last_name = Some(step_name);
            }
            CompositionStep::Diagram {
                nodes,
                edges,
                mode,
                clash,
                rewrite_budget,
                inline,
            } => {
                let opts = ComposeOptions {
                    mode: *mode,
                    result_name: Arc::from(step_name.as_str()),
                    clash: *clash,
                    rewrite_budget: *rewrite_budget,
                };
                let diagram = replay_diagram(&intermediates, nodes, edges, inline)?;
                let result = compose(&diagram, &opts)?.theory;
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

/// Replay one binary colimit step.
fn replay_colimit(
    intermediates: &HashMap<String, Theory>,
    (left, right): (&String, &String),
    shared_sorts: &[String],
    shared_ops: &[String],
    mode: CompositionMode,
    step_name: &str,
) -> Result<Theory, GatError> {
    let left_theory = intermediates
        .get(left)
        .ok_or_else(|| GatError::TheoryNotFound(left.clone()))?;
    let right_theory = intermediates
        .get(right)
        .ok_or_else(|| GatError::TheoryNotFound(right.clone()))?;

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
        step_name,
        shared_sorts
            .iter()
            .map(|s| Sort::simple(s.as_str()))
            .collect(),
        shared_op_defs,
        vec![],
    );

    match mode {
        CompositionMode::Amalgamate => colimit_by_name(left_theory, right_theory, &shared),
        CompositionMode::ExplicitIdentity => {
            let diagram = TheoryDiagram::by_name(
                ("apex", shared),
                ("left", left_theory.clone()),
                ("right", right_theory.clone()),
            )?;
            let opts = ComposeOptions {
                mode,
                result_name: Arc::from(step_name),
                clash: ClashPolicy::Reject,
                rewrite_budget: DEFAULT_VERIFY_BUDGET,
            };
            Ok(compose(&diagram, &opts)?.theory)
        }
    }
}

/// Rebuild the diagram of a [`CompositionStep::Diagram`].
fn replay_diagram(
    intermediates: &HashMap<String, Theory>,
    nodes: &[String],
    edges: &[RecipeEdge],
    inline: &[Theory],
) -> Result<TheoryDiagram, GatError> {
    let resolve = |name: &String| -> Result<Theory, GatError> {
        inline
            .iter()
            .find(|t| t.name.as_ref() == name.as_str())
            .or_else(|| intermediates.get(name))
            .cloned()
            .ok_or_else(|| GatError::TheoryNotFound(name.clone()))
    };
    let mut theories: HashMap<&str, Theory> = HashMap::new();
    let mut diagram_nodes = Vec::with_capacity(nodes.len());
    for name in nodes {
        let theory = resolve(name)?;
        theories.insert(name.as_str(), theory.clone());
        diagram_nodes.push(DiagramNode {
            label: Arc::from(name.as_str()),
            theory,
        });
    }
    let theory_name = |label: &String| -> Arc<str> {
        theories
            .get(label.as_str())
            .map_or_else(|| Arc::from(label.as_str()), |t| Arc::clone(&t.name))
    };
    let diagram_edges = edges
        .iter()
        .map(|edge| DiagramEdge {
            src: Arc::from(edge.src.as_str()),
            tgt: Arc::from(edge.tgt.as_str()),
            morphism: TheoryMorphism::new(
                format!("{}_to_{}", edge.src, edge.tgt),
                theory_name(&edge.src),
                theory_name(&edge.tgt),
                edge.sorts
                    .iter()
                    .map(|(a, b)| (Arc::from(a.as_str()), Arc::from(b.as_str())))
                    .collect(),
                edge.ops
                    .iter()
                    .map(|(a, image)| {
                        let image = match image {
                            RecipeOpImage::Op(name) => OpAssignment::Op(Arc::from(name.as_str())),
                            RecipeOpImage::Term(term) => OpAssignment::Term(term.clone()),
                        };
                        (Arc::from(a.as_str()), image)
                    })
                    .collect::<HashMap<Arc<str>, OpAssignment>>(),
            ),
        })
        .collect();
    Ok(TheoryDiagram {
        nodes: diagram_nodes,
        edges: diagram_edges,
    })
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
            version: 0,
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
            version: 0,
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
                mode: CompositionMode::Amalgamate,
            }],
            version: 0,
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
                    mode: CompositionMode::Amalgamate,
                },
                CompositionStep::Colimit {
                    left: "step_0".into(),
                    right: "ThMulti".into(),
                    shared_sorts: vec!["Vertex".into(), "Edge".into()],
                    shared_ops: vec![],
                    mode: CompositionMode::Amalgamate,
                },
            ],
            version: 0,
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
            version: 0,
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
                    mode: CompositionMode::Amalgamate,
                },
                CompositionStep::Base("ThWType".into()),
            ],
            version: 0,
        };
        let json = serde_json::to_string(&spec).unwrap();
        let roundtripped: CompositionSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(spec, roundtripped);
    }

    /// The graph + constraint diagram over an inline apex `ThVertex`.
    fn graph_constraint_step(mode: CompositionMode) -> CompositionStep {
        let to = |tgt: &str| RecipeEdge {
            src: "ThVertex".into(),
            tgt: tgt.into(),
            sorts: vec![("Vertex".into(), "Vertex".into())],
            ops: vec![],
        };
        CompositionStep::Diagram {
            nodes: vec!["ThVertex".into(), "ThGraph".into(), "ThConstraint".into()],
            edges: vec![to("ThGraph"), to("ThConstraint")],
            mode,
            clash: ClashPolicy::Reject,
            rewrite_budget: DEFAULT_VERIFY_BUDGET,
            inline: vec![Theory::new(
                "ThVertex",
                vec![Sort::simple("Vertex")],
                vec![],
                vec![],
            )],
        }
    }

    #[test]
    fn a_diagram_recipe_recomposes_to_the_same_theory() {
        let reg = base_registry();
        let spec = CompositionSpec::new(
            "GraphConstraint",
            vec![graph_constraint_step(CompositionMode::ExplicitIdentity)],
        );
        assert_eq!(spec.version, 1);
        let json = serde_json::to_string(&spec).unwrap();
        let stored: CompositionSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(stored, spec);
        let replayed = recompose(&stored, &reg).unwrap();

        let diagram = crate::compose::TheoryDiagram::by_name(
            (
                "ThVertex",
                Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![]),
            ),
            ("ThGraph", reg["ThGraph"].clone()),
            ("ThConstraint", reg["ThConstraint"].clone()),
        )
        .unwrap();
        let opts = ComposeOptions {
            mode: CompositionMode::ExplicitIdentity,
            result_name: "GraphConstraint".into(),
            clash: ClashPolicy::Reject,
            rewrite_budget: DEFAULT_VERIFY_BUDGET,
        };
        let direct = compose(&diagram, &opts).unwrap().theory;
        assert_eq!(replayed, direct);
        // A node the recipe names but neither inlines nor registers.
        let mut missing = spec;
        if let CompositionStep::Diagram { inline, .. } = &mut missing.steps[0] {
            inline.clear();
        }
        assert!(
            matches!(recompose(&missing, &reg), Err(GatError::TheoryNotFound(n)) if n == "ThVertex")
        );
    }

    #[test]
    fn a_stored_0_74_recipe_replays_unchanged() {
        let stored = r#"{"result_name":"SchemaA","steps":[
            {"Colimit":{"left":"ThGraph","right":"ThConstraint","shared_sorts":["Vertex"],"shared_ops":[]}},
            {"Colimit":{"left":"step_0","right":"ThMulti","shared_sorts":["Vertex","Edge"]}}]}"#;
        let spec: CompositionSpec = serde_json::from_str(stored).unwrap();
        assert_eq!(spec.version, 0);
        assert!(spec.steps.iter().all(|s| matches!(
            s,
            CompositionStep::Colimit {
                mode: CompositionMode::Amalgamate,
                ..
            }
        )));
        let reg = base_registry();
        let replayed = recompose(&spec, &reg).unwrap();
        let shared_v = Theory::new("sv", vec![Sort::simple("Vertex")], vec![], vec![]);
        let gc = colimit_by_name(&reg["ThGraph"], &reg["ThConstraint"], &shared_v).unwrap();
        let shared_ve = Theory::new(
            "sve",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![],
            vec![],
        );
        let mut direct = colimit_by_name(&gc, &reg["ThMulti"], &shared_ve).unwrap();
        direct.name = "SchemaA".into();
        assert_eq!(replayed, direct);
    }

    #[test]
    fn recipe_version_is_one_iff_diagram_or_mode_is_used() {
        let colimit = |mode| CompositionStep::Colimit {
            left: "ThGraph".into(),
            right: "ThConstraint".into(),
            shared_sorts: vec!["Vertex".into()],
            shared_ops: vec![],
            mode,
        };
        let cases = [
            (vec![CompositionStep::Base("ThGraph".into())], 0),
            (vec![colimit(CompositionMode::Amalgamate)], 0),
            (vec![colimit(CompositionMode::ExplicitIdentity)], 1),
            (vec![graph_constraint_step(CompositionMode::Amalgamate)], 1),
            (
                vec![
                    colimit(CompositionMode::Amalgamate),
                    graph_constraint_step(CompositionMode::ExplicitIdentity),
                ],
                1,
            ),
        ];
        for (steps, version) in cases {
            let spec = CompositionSpec::new("R", steps);
            assert_eq!(spec.version, version, "{spec:?}");
            assert_eq!(spec.required_version(), version);
        }
    }

    #[test]
    fn recompose_uses_the_step_mode() {
        // Both sides declare an `X` the shared theory does not mention.
        let mut reg = base_registry();
        for name in ["L", "R"] {
            reg.insert(
                name.into(),
                Theory::new(
                    name,
                    vec![Sort::simple("V"), Sort::simple("X")],
                    vec![],
                    vec![],
                ),
            );
        }
        let spec = |mode| {
            CompositionSpec::new(
                "Out",
                vec![CompositionStep::Colimit {
                    left: "L".into(),
                    right: "R".into(),
                    shared_sorts: vec!["V".into()],
                    shared_ops: vec![],
                    mode,
                }],
            )
        };
        let amalgamated = recompose(&spec(CompositionMode::Amalgamate), &reg).unwrap();
        assert_eq!(amalgamated.sorts.len(), 2);
        let explicit = recompose(&spec(CompositionMode::ExplicitIdentity), &reg);
        assert!(
            matches!(&explicit, Err(GatError::Composition(e)) if matches!(**e, crate::compose::CompositionError::NameClash { .. })),
            "{explicit:?}"
        );
    }
}
