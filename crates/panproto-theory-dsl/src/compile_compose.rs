//! Compile a [`CompositionBody`] into a composed [`Theory`].
//!
//! Each DSL step is composed as [`panproto_gat::compose()`] composes it, and
//! recorded as the [`CompositionStep`] that
//! [`panproto_gat::recompose`] replays to the same theory:
//!
//! - a step with no `identify` entries in the default `amalgamate` mode is a
//!   pushout by name ([`CompositionStep::Colimit`] in
//!   [`CompositionMode::Amalgamate`]), as every step was before modes;
//! - a step with no `identify` entries in `explicit_identity` mode is the
//!   same span composed with explicit identity, refusing any other name the
//!   two sides share;
//! - a step with `identify` entries is a [`CompositionStep::Diagram`] over
//!   the two sides and a shared theory built for the step, whose legs send
//!   each shared or identified left symbol to its right counterpart; a side
//!   whose name cannot label a node, or a theory composed with itself, is
//!   carried inline in the recipe under a label of the step's own.
//!
//! [`compile_composition_with_report`] also returns the
//! [`CompositionReport`] of every step.

use std::collections::HashMap;
use std::hash::BuildHasher;
use std::sync::Arc;

use panproto_gat::{
    ClashPolicy, ComposeOptions, CompositionMode, CompositionReport, CompositionSpec,
    CompositionStep, DEFAULT_VERIFY_BUDGET, DiagramEdge, DiagramNode, OpAssignment, RecipeEdge,
    RecipeOpImage, Sort, Theory, TheoryDiagram, TheoryMorphism, compose, pushout_by_name,
};

use crate::document::{ColimitStepSpec, CompositionBody, CompositionSpec_, IdentifyKind};
use crate::error::TheoryDslError;

/// A compiled composition: the theory, the recipe that replays it, and the
/// report of each step in step order.
#[derive(Debug, Clone)]
pub struct ComposedTheory {
    /// The composed theory, named by the composition's `result`.
    pub theory: Theory,
    /// The recipe [`panproto_gat::recompose`] replays to `theory`.
    pub spec: CompositionSpec,
    /// The report of each step's composition, in step order.
    pub reports: Vec<CompositionReport>,
}

/// Compile a [`CompositionBody`] into a composed [`Theory`] and its
/// [`CompositionSpec`] recipe.
///
/// Builds a local registry from resolved base theories and any
/// theories already compiled in the same document (passed via
/// `local_theories`), then composes the steps.
///
/// # Errors
///
/// Returns [`TheoryDslError::TheoryNotFound`] if a base theory
/// cannot be resolved, or [`TheoryDslError::ColimitFailed`] if
/// a colimit step fails.
pub fn compile_composition<S: BuildHasher>(
    body: &CompositionBody,
    resolver: &dyn Fn(&str) -> Option<Theory>,
    local_theories: &HashMap<String, Theory, S>,
) -> Result<(Theory, CompositionSpec), TheoryDslError> {
    compile_composition_with_report(body, resolver, local_theories)
        .map(|composed| (composed.theory, composed.spec))
}

/// [`compile_composition`], also returning the report of every step.
///
/// # Errors
///
/// As [`compile_composition`].
pub fn compile_composition_with_report<S: BuildHasher>(
    body: &CompositionBody,
    resolver: &dyn Fn(&str) -> Option<Theory>,
    local_theories: &HashMap<String, Theory, S>,
) -> Result<ComposedTheory, TheoryDslError> {
    compile_composition_spec_with_report(&body.compose, resolver, local_theories)
}

/// Compile a [`CompositionSpec_`] (the inner spec, usable from both
/// `CompositionBody` and inline `TheoryRef::Composed`).
///
/// # Errors
///
/// Returns [`TheoryDslError::TheoryNotFound`] if a referenced theory
/// cannot be resolved, or [`TheoryDslError::ColimitFailed`] if
/// a step fails.
pub fn compile_composition_spec<S: BuildHasher>(
    spec: &CompositionSpec_,
    resolver: &dyn Fn(&str) -> Option<Theory>,
    local_theories: &HashMap<String, Theory, S>,
) -> Result<(Theory, CompositionSpec), TheoryDslError> {
    compile_composition_spec_with_report(spec, resolver, local_theories)
        .map(|composed| (composed.theory, composed.spec))
}

/// [`compile_composition_spec`], also returning the report of every step.
///
/// # Errors
///
/// As [`compile_composition_spec`].
pub fn compile_composition_spec_with_report<S: BuildHasher>(
    spec: &CompositionSpec_,
    resolver: &dyn Fn(&str) -> Option<Theory>,
    local_theories: &HashMap<String, Theory, S>,
) -> Result<ComposedTheory, TheoryDslError> {
    let mut registry: HashMap<String, Theory> = HashMap::new();

    // Seed registry with local theories.
    for (name, theory) in local_theories {
        registry.insert(name.clone(), theory.clone());
    }

    // Resolve and register base theories.
    for base_name in &spec.bases {
        if !registry.contains_key(base_name) {
            let theory = resolver(base_name).ok_or_else(|| TheoryDslError::TheoryNotFound {
                name: base_name.clone(),
                context: format!("composition '{}' bases", spec.result),
            })?;
            registry.insert(base_name.clone(), theory);
        }
    }

    // Also resolve any theories referenced in steps but not in bases.
    for step in &spec.steps {
        for name in [&step.left, &step.right] {
            if !registry.contains_key(name) && !name.starts_with("step_") {
                if let Some(theory) = resolver(name) {
                    registry.insert(name.clone(), theory);
                }
            }
        }
    }

    let mut steps = Vec::with_capacity(spec.steps.len());
    let mut reports = Vec::with_capacity(spec.steps.len());
    let mut last: Option<Theory> = None;
    for (index, step) in spec.steps.iter().enumerate() {
        let lookup = |name: &String| {
            registry
                .get(name)
                .cloned()
                .ok_or_else(|| TheoryDslError::TheoryNotFound {
                    name: name.clone(),
                    context: format!("composition '{}' step {index}", spec.result),
                })
        };
        let left = lookup(&step.left)?;
        let right = lookup(&step.right)?;
        let composed = compose_step(index, step, &left, &right)?;
        registry.insert(format!("step_{index}"), composed.theory.clone());
        steps.push(composed.step);
        reports.push(composed.report);
        last = Some(composed.theory);
    }

    let mut theory = last.ok_or_else(|| TheoryDslError::ColimitFailed {
        step: 0,
        message: format!("composition '{}' has no steps", spec.result),
    })?;
    theory.name = spec.result.clone().into();
    Ok(ComposedTheory {
        theory,
        spec: CompositionSpec::new(spec.result.clone(), steps),
        reports,
    })
}

/// One composed step: its result, the recipe step that replays it, and
/// its report.
struct ComposedStep {
    theory: Theory,
    step: CompositionStep,
    report: CompositionReport,
}

/// Compose one DSL step of index `index` over its two resolved sides.
fn compose_step(
    index: usize,
    step: &ColimitStepSpec,
    left: &Theory,
    right: &Theory,
) -> Result<ComposedStep, TheoryDslError> {
    let failed = |message: String| TheoryDslError::ColimitFailed {
        step: index,
        message,
    };
    let step_name = format!("step_{index}");
    let shared =
        shared_theory(&format!("{step_name}_shared"), step, left, right).map_err(failed)?;

    if step.identify.is_empty() {
        let recipe = CompositionStep::Colimit {
            left: step.left.clone(),
            right: step.right.clone(),
            shared_sorts: step.shared_sorts.clone(),
            shared_ops: step.shared_ops.clone(),
            mode: step.mode,
        };
        let (theory, report) = match step.mode {
            CompositionMode::Amalgamate => {
                let result =
                    pushout_by_name(left, right, &shared).map_err(|e| failed(e.to_string()))?;
                (result.theory, result.report)
            }
            CompositionMode::ExplicitIdentity => {
                let diagram = TheoryDiagram::by_name(
                    ("apex", shared),
                    ("left", left.clone()),
                    ("right", right.clone()),
                )
                .map_err(|e| failed(e.to_string()))?;
                let composition = compose(&diagram, &options(step.mode, &step_name))
                    .map_err(|e| failed(e.to_string()))?;
                (composition.theory, composition.report)
            }
        };
        return Ok(ComposedStep {
            theory,
            step: recipe,
            report,
        });
    }

    compose_identified(step, left, right, shared, &step_name).map_err(failed)
}

/// Compose a step that has identifications: one diagram over the two sides
/// and the step's shared theory.
///
/// Each side is a node of the diagram, labelled as [`side_node`] says, so a
/// side whose name cannot label a node, or a theory composed with itself,
/// composes as it does in a step without identifications.
fn compose_identified(
    step: &ColimitStepSpec,
    left: &Theory,
    right: &Theory,
    shared: Theory,
    step_name: &str,
) -> Result<ComposedStep, String> {
    let shared_label = shared.name.to_string();
    let left_node = side_node(&step.left, left, &[&shared_label], step_name, "left");
    let right_node = side_node(
        &step.right,
        right,
        &[&shared_label, &left_node.label],
        step_name,
        "right",
    );
    let (mut to_left, mut to_right) = legs(step, &shared);
    // Each leg ends at its node's theory, whose name is the side's theory
    // name (for an earlier step's result, not its label) or, for a side
    // carried inline, its label; recompose names the legs the same way.
    for (leg, node) in [(&mut to_left, &left_node), (&mut to_right, &right_node)] {
        leg.name = Arc::from(format!("{shared_label}_to_{}", node.label));
        leg.codomain = Arc::clone(&node.theory.name);
    }
    let mut inline = vec![shared.clone()];
    inline.extend(
        [&left_node, &right_node]
            .into_iter()
            .filter(|node| node.inline)
            .map(|node| node.theory.clone()),
    );
    let recipe = CompositionStep::Diagram {
        nodes: vec![
            left_node.label.clone(),
            right_node.label.clone(),
            shared_label.clone(),
        ],
        edges: vec![
            recipe_edge(&shared_label, &left_node.label, &to_left),
            recipe_edge(&shared_label, &right_node.label, &to_right),
        ],
        mode: step.mode,
        clash: ClashPolicy::Reject,
        rewrite_budget: DEFAULT_VERIFY_BUDGET,
        inline,
    };
    let edge = |tgt: &str, morphism: TheoryMorphism| DiagramEdge {
        src: Arc::clone(&shared.name),
        tgt: Arc::from(tgt),
        morphism,
    };
    let diagram = TheoryDiagram {
        edges: vec![
            edge(&left_node.label, to_left),
            edge(&right_node.label, to_right),
        ],
        nodes: vec![
            DiagramNode {
                label: Arc::from(left_node.label.as_str()),
                theory: left_node.theory,
            },
            DiagramNode {
                label: Arc::from(right_node.label.as_str()),
                theory: right_node.theory,
            },
            DiagramNode {
                label: Arc::clone(&shared.name),
                theory: shared,
            },
        ],
    };
    let composition =
        compose(&diagram, &options(step.mode, step_name)).map_err(|e| e.to_string())?;
    Ok(ComposedStep {
        theory: composition.theory,
        step: recipe,
        report: composition.report,
    })
}

/// A side of an identify step as a node of the step's diagram.
struct SideNode {
    /// The node's label, which the recipe also names the node by.
    label: String,
    /// The node's theory.
    theory: Theory,
    /// Whether the recipe carries `theory` inline rather than naming a
    /// registry entry.
    inline: bool,
}

/// The node of the side named `name`, whose theory is `theory`, in the
/// diagram of the step `step_name`; `taken` are the labels of the step's
/// other nodes.
///
/// The node is labelled by the side's name, which the recipe resolves in
/// the registry, when that name is a valid label (non-empty, with neither
/// `.` nor `#`) not in `taken`. Otherwise (a name such as `my.Graph`, or a
/// theory composed with itself) the side is carried inline under the first
/// of `{step_name}_{role}`, `{step_name}_{role}_`, ... not in `taken`, and
/// its theory is renamed to that label, since `recompose` finds an inline
/// theory by its name.
fn side_node(name: &str, theory: &Theory, taken: &[&str], step_name: &str, role: &str) -> SideNode {
    let valid = !name.is_empty() && !name.contains(['.', '#']);
    if valid && !taken.contains(&name) {
        return SideNode {
            label: name.to_owned(),
            theory: theory.clone(),
            inline: false,
        };
    }
    let mut label = format!("{step_name}_{role}");
    while taken.contains(&label.as_str()) {
        label.push('_');
    }
    let mut theory = theory.clone();
    theory.name = Arc::from(label.as_str());
    SideNode {
        label,
        theory,
        inline: true,
    }
}

/// A leg as a recipe edge from `src` to `tgt`, its maps in key order.
fn recipe_edge(src: &str, tgt: &str, morphism: &TheoryMorphism) -> RecipeEdge {
    let mut sorts: Vec<(String, String)> = morphism
        .sort_map
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
    sorts.sort();
    let mut ops: Vec<(String, RecipeOpImage)> = morphism
        .op_map
        .iter()
        .map(|(a, image)| {
            let image = match image {
                OpAssignment::Op(b) => RecipeOpImage::Op(b.to_string()),
                OpAssignment::Term(t) => RecipeOpImage::Term(t.clone()),
            };
            (a.to_string(), image)
        })
        .collect();
    ops.sort_by(|a, b| a.0.cmp(&b.0));
    RecipeEdge {
        src: src.to_owned(),
        tgt: tgt.to_owned(),
        sorts,
        ops,
    }
}

/// The options a DSL step composes with: its mode, clashes refused.
fn options(mode: CompositionMode, result_name: &str) -> ComposeOptions {
    ComposeOptions {
        mode,
        result_name: Arc::from(result_name),
        clash: ClashPolicy::Reject,
        rewrite_budget: DEFAULT_VERIFY_BUDGET,
    }
}

/// The theory a step shares between its sides, named `name`: its shared
/// sorts (simple, as a pushout by name has always built them), then the
/// left sides of its identified sorts, then its shared operations and the
/// left sides of its identified operations, as the left theory declares
/// them.
fn shared_theory(
    name: &str,
    step: &ColimitStepSpec,
    left: &Theory,
    right: &Theory,
) -> Result<Theory, String> {
    let mut sorts: Vec<Sort> = step
        .shared_sorts
        .iter()
        .map(|s| Sort::simple(s.as_str()))
        .collect();
    let mut ops = Vec::new();
    for op_name in &step.shared_ops {
        let op = left.find_op(op_name).ok_or_else(|| {
            format!(
                "shared operation '{op_name}' not found in left theory '{}'",
                step.left
            )
        })?;
        if !right.has_op(op_name) {
            return Err(format!(
                "shared operation '{op_name}' not found in right theory '{}'",
                step.right
            ));
        }
        ops.push(op.clone());
    }
    for pair in &step.identify {
        match pair.kind {
            IdentifyKind::Sort => {
                let sort = left.find_sort(&pair.left).ok_or_else(|| {
                    format!(
                        "identified sort '{}' not found in left theory '{}'",
                        pair.left, step.left
                    )
                })?;
                if !right.has_sort(&pair.right) {
                    return Err(format!(
                        "identified sort '{}' not found in right theory '{}'",
                        pair.right, step.right
                    ));
                }
                sorts.push(sort.clone());
            }
            IdentifyKind::Op => {
                let op = left.find_op(&pair.left).ok_or_else(|| {
                    format!(
                        "identified operation '{}' not found in left theory '{}'",
                        pair.left, step.left
                    )
                })?;
                if !right.has_op(&pair.right) {
                    return Err(format!(
                        "identified operation '{}' not found in right theory '{}'",
                        pair.right, step.right
                    ));
                }
                ops.push(op.clone());
            }
        }
    }
    Ok(Theory::new(name, sorts, ops, vec![]))
}

/// The legs of a step with identifications: into the left side, every
/// shared symbol maps to itself; into the right side, an identified symbol
/// maps to its right name and a shared one to itself.
fn legs(step: &ColimitStepSpec, shared: &Theory) -> (TheoryMorphism, TheoryMorphism) {
    let right_name = |kind: IdentifyKind, name: &str| -> Arc<str> {
        step.identify
            .iter()
            .find(|pair| pair.kind == kind && pair.left == name)
            .map_or_else(|| Arc::from(name), |pair| Arc::from(pair.right.as_str()))
    };
    let leg = |tgt: &str, rename: &dyn Fn(IdentifyKind, &str) -> Arc<str>| {
        TheoryMorphism::new(
            format!("{}_to_{tgt}", shared.name),
            Arc::clone(&shared.name),
            tgt,
            shared
                .sorts
                .iter()
                .map(|s| (Arc::clone(&s.name), rename(IdentifyKind::Sort, &s.name)))
                .collect::<HashMap<_, _>>(),
            shared
                .ops
                .iter()
                .map(|o| {
                    (
                        Arc::clone(&o.name),
                        OpAssignment::Op(rename(IdentifyKind::Op, &o.name)),
                    )
                })
                .collect::<HashMap<_, _>>(),
        )
    };
    (
        leg(&step.left, &|_, name| Arc::from(name)),
        leg(&step.right, &right_name),
    )
}

#[cfg(test)]
mod tests {
    use panproto_gat::{CheckStatus, GatError, recompose};

    use super::*;
    use crate::compile::builtin_resolver;
    use crate::document::{TheoryBody, TheoryDocument};

    fn body(json: &str) -> CompositionBody {
        let document: TheoryDocument =
            serde_json::from_str(json).unwrap_or_else(|e| panic!("the document parses: {e}"));
        match document.body {
            TheoryBody::Composition(body) => body,
            other => panic!("expected a composition, got {other:?}"),
        }
    }

    fn registry(names: &[&str]) -> HashMap<String, Theory> {
        let resolve = builtin_resolver();
        names
            .iter()
            .map(|n| {
                (
                    (*n).to_owned(),
                    resolve(n).unwrap_or_else(|| panic!("{n} is a built-in theory")),
                )
            })
            .collect()
    }

    #[test]
    fn dsl_compose_with_identify_returns_the_report() {
        let body = body(
            r#"{
                "id": "test.identify",
                "description": "graph edges are hyper-edges",
                "compose": {
                    "result": "GraphHyper",
                    "bases": ["ThGraph", "ThHypergraph"],
                    "steps": [{
                        "left": "ThGraph",
                        "right": "ThHypergraph",
                        "shared_sorts": ["Vertex"],
                        "mode": "explicit_identity",
                        "identify": [{"kind": "sort", "left": "Edge", "right": "HyperEdge"}]
                    }]
                }
            }"#,
        );
        let local: HashMap<String, Theory> = HashMap::new();
        let composed = compile_composition_with_report(&body, &builtin_resolver(), &local)
            .unwrap_or_else(|e| panic!("the composition compiles: {e}"));

        // Edge and HyperEdge are one sort, named by the majority of its
        // members (the left sort and the shared theory's copy).
        let theory = &composed.theory;
        assert_eq!(&*theory.name, "GraphHyper");
        assert!(theory.has_sort("Edge"));
        assert!(!theory.has_sort("HyperEdge"));
        let incident = theory
            .find_op("incident")
            .unwrap_or_else(|| panic!("incident is kept"));
        assert_eq!(&*incident.inputs[0].1.head().to_string(), "Edge");

        assert_eq!(composed.reports.len(), 1);
        let report = &composed.reports[0];
        assert_eq!(report.mode, CompositionMode::ExplicitIdentity);
        assert_eq!(report.status, CheckStatus::Passed);
        assert!(report.name_clash_free);

        assert_eq!(composed.spec.version, 1);
        assert!(matches!(
            composed.spec.steps.as_slice(),
            [CompositionStep::Diagram {
                mode: CompositionMode::ExplicitIdentity,
                ..
            }]
        ));
        let replayed = recompose(&composed.spec, &registry(&["ThGraph", "ThHypergraph"]))
            .unwrap_or_else(|e| panic!("the recipe replays: {e}"));
        assert_eq!(&replayed, theory);
    }

    #[test]
    fn dsl_compose_without_a_mode_amalgamates_as_before() {
        let body = body(
            r#"{
                "id": "test.amalgamate",
                "description": "graph with multigraph labels",
                "compose": {
                    "result": "GM",
                    "bases": ["ThGraph", "ThMulti"],
                    "steps": [{"left": "ThGraph", "right": "ThMulti", "shared_sorts": ["Vertex"]}]
                }
            }"#,
        );
        let local: HashMap<String, Theory> = HashMap::new();
        let composed = compile_composition_with_report(&body, &builtin_resolver(), &local)
            .unwrap_or_else(|e| panic!("the composition compiles: {e}"));
        assert_eq!(composed.spec.version, 0);
        assert!(matches!(
            composed.spec.steps.as_slice(),
            [CompositionStep::Colimit {
                mode: CompositionMode::Amalgamate,
                ..
            }]
        ));
        assert_eq!(composed.reports[0].mode, CompositionMode::Amalgamate);
        // Edge is glued by name, as a step without a mode always did.
        assert_eq!(
            composed
                .theory
                .sorts
                .iter()
                .filter(|s| &*s.name == "Edge")
                .count(),
            1
        );
        let replayed = recompose(&composed.spec, &registry(&["ThGraph", "ThMulti"]))
            .unwrap_or_else(|e| panic!("the recipe replays: {e}"));
        assert_eq!(replayed, composed.theory);
        let (theory, spec) = compile_composition(&body, &builtin_resolver(), &local)
            .unwrap_or_else(|e| panic!("the composition compiles: {e}"));
        assert_eq!((theory, spec), (composed.theory, composed.spec));
    }

    /// A one-step composition of `left` and `right` sharing `Vertex` and the
    /// given operations, in `mode`, with the given sort identifications.
    fn one_step(
        left: &str,
        right: &str,
        shared_ops: &[&str],
        mode: &str,
        identify: &[(&str, &str)],
    ) -> CompositionBody {
        let identify: Vec<String> = identify
            .iter()
            .map(|(l, r)| format!(r#"{{"kind": "sort", "left": "{l}", "right": "{r}"}}"#))
            .collect();
        let shared_ops: Vec<String> = shared_ops.iter().map(|o| format!("\"{o}\"")).collect();
        body(&format!(
            r#"{{
                "id": "test.sides",
                "description": "a step over the given sides",
                "compose": {{
                    "result": "R",
                    "steps": [{{
                        "left": "{left}",
                        "right": "{right}",
                        "shared_sorts": ["Vertex"],
                        "shared_ops": [{}],
                        "mode": "{mode}",
                        "identify": [{}]
                    }}]
                }}
            }}"#,
            shared_ops.join(", "),
            identify.join(", "),
        ))
    }

    /// Compile `body` over `local` and check that its recipe replays to its
    /// theory against `registry`.
    fn compile_and_replay(
        body: &CompositionBody,
        local: &HashMap<String, Theory>,
        registry: &HashMap<String, Theory>,
    ) -> Result<Theory, TheoryDslError> {
        let composed = compile_composition_with_report(body, &builtin_resolver(), local)?;
        let replayed = recompose(&composed.spec, registry)
            .unwrap_or_else(|e| panic!("the recipe replays: {e}"));
        assert_eq!(replayed, composed.theory);
        Ok(composed.theory)
    }

    #[test]
    fn dsl_identify_step_accepts_a_side_that_is_not_a_node_label() {
        let mut graph = builtin_resolver()("ThGraph").unwrap_or_else(|| panic!("ThGraph"));
        graph.name = "my.Graph".into();
        let local: HashMap<String, Theory> = [("my.Graph".to_owned(), graph.clone())].into();
        let mut with_local = registry(&["ThHypergraph"]);
        with_local.insert("my.Graph".into(), graph);
        for mode in ["amalgamate", "explicit_identity"] {
            let reference = compile_and_replay(
                &one_step(
                    "ThGraph",
                    "ThHypergraph",
                    &[],
                    mode,
                    &[("Edge", "HyperEdge")],
                ),
                &HashMap::new(),
                &registry(&["ThGraph", "ThHypergraph"]),
            )
            .unwrap_or_else(|e| panic!("{mode}: ThGraph composes: {e}"));
            let theory = compile_and_replay(
                &one_step(
                    "my.Graph",
                    "ThHypergraph",
                    &[],
                    mode,
                    &[("Edge", "HyperEdge")],
                ),
                &local,
                &with_local,
            )
            .unwrap_or_else(|e| panic!("{mode}: my.Graph composes: {e}"));
            assert_eq!(theory, reference, "{mode}");
        }
    }

    #[test]
    fn dsl_identify_step_composes_a_theory_with_itself() {
        let graph = registry(&["ThGraph"]);
        for (mode, shared_ops) in [
            ("amalgamate", &[][..]),
            ("explicit_identity", &["src", "tgt"][..]),
        ] {
            let without = compile_and_replay(
                &one_step("ThGraph", "ThGraph", shared_ops, mode, &[]),
                &HashMap::new(),
                &graph,
            )
            .unwrap_or_else(|e| panic!("{mode}: the step without identify composes: {e}"));
            let with = compile_and_replay(
                &one_step("ThGraph", "ThGraph", shared_ops, mode, &[("Edge", "Edge")]),
                &HashMap::new(),
                &graph,
            )
            .unwrap_or_else(|e| panic!("{mode}: the step with identify composes: {e}"));
            let names = |t: &Theory| {
                let mut sorts: Vec<String> = t.sorts.iter().map(|s| s.name.to_string()).collect();
                let mut ops: Vec<String> = t.ops.iter().map(|o| o.name.to_string()).collect();
                sorts.sort();
                ops.sort();
                (sorts, ops)
            };
            assert_eq!(names(&with), names(&without), "{mode}");
            assert_eq!(
                names(&with),
                (
                    vec!["Edge".to_owned(), "Vertex".to_owned()],
                    vec!["src".to_owned(), "tgt".to_owned()]
                ),
                "{mode}"
            );
        }
    }

    #[test]
    fn dsl_compose_in_explicit_mode_refuses_an_unshared_name() {
        let body = body(
            r#"{
                "id": "test.explicit",
                "description": "Edge is not shared",
                "compose": {
                    "result": "GM",
                    "bases": ["ThGraph", "ThMulti"],
                    "steps": [{
                        "left": "ThGraph",
                        "right": "ThMulti",
                        "shared_sorts": ["Vertex"],
                        "mode": "explicit_identity"
                    }]
                }
            }"#,
        );
        let local: HashMap<String, Theory> = HashMap::new();
        let Err(error) = compile_composition_with_report(&body, &builtin_resolver(), &local) else {
            panic!("Edge clashes");
        };
        let TheoryDslError::ColimitFailed { step, message } = error else {
            panic!("expected ColimitFailed, got {error:?}");
        };
        assert_eq!(step, 0);
        assert!(message.contains("Edge"), "{message}");
        // The recorded recipe step says the same.
        let spec = CompositionSpec::new(
            "GM",
            vec![CompositionStep::Colimit {
                left: "ThGraph".into(),
                right: "ThMulti".into(),
                shared_sorts: vec!["Vertex".into()],
                shared_ops: vec![],
                mode: CompositionMode::ExplicitIdentity,
            }],
        );
        assert!(matches!(
            recompose(&spec, &registry(&["ThGraph", "ThMulti"])),
            Err(GatError::Composition(_))
        ));
    }
}

#[cfg(test)]
mod nickel_tests {
    use std::collections::HashMap;

    use panproto_gat::{CompositionMode, CompositionStep, Theory};

    use super::compile_composition_with_report;
    use crate::compile::builtin_resolver;
    use crate::document::TheoryBody;
    use crate::error::TheoryDslError;
    use crate::eval::eval_nickel;

    const SOURCE: &str = r#"
        let { Composition, colimit, explicit, identifying, identify_sort, .. } =
          import "panproto/theory.ncl" in
        {
          id = "test.nickel-identify",
          description = "graph edges are hyper-edges",
          compose = {
            result = "GraphHyper",
            bases = ["ThGraph", "ThHypergraph"],
            steps = [
              colimit "ThGraph" "ThHypergraph" ["Vertex"]
              |> explicit
              |> identifying [identify_sort "Edge" "HyperEdge"]
            ],
          },
        } | Composition
    "#;

    #[test]
    fn nickel_compose_steps_carry_mode_and_identify() {
        let document =
            eval_nickel(SOURCE, &[]).unwrap_or_else(|e| panic!("the source evaluates: {e}"));
        let TheoryBody::Composition(body) = document.body else {
            panic!("expected a composition");
        };
        assert_eq!(
            body.compose.steps[0].mode,
            CompositionMode::ExplicitIdentity
        );
        assert_eq!(body.compose.steps[0].identify.len(), 1);
        let local: HashMap<String, Theory> = HashMap::new();
        let composed = compile_composition_with_report(&body, &builtin_resolver(), &local)
            .unwrap_or_else(|e| panic!("the composition compiles: {e}"));
        assert!(composed.theory.has_sort("Edge"));
        assert!(!composed.theory.has_sort("HyperEdge"));
        assert!(matches!(
            composed.spec.steps.as_slice(),
            [CompositionStep::Diagram { .. }]
        ));
    }

    #[test]
    fn nickel_rejects_an_unknown_mode() {
        let source = SOURCE.replace("|> explicit", "& { mode = \"glue\" }");
        let Err(error) = eval_nickel(&source, &[]) else {
            panic!("glue is not a mode");
        };
        assert!(
            matches!(
                error,
                TheoryDslError::NickelEval { .. } | TheoryDslError::NickelEvalSpanned { .. }
            ),
            "{error:?}"
        );
    }
}
