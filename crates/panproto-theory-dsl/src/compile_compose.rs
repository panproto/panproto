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
//!   the two sides and the shared theories built for the step, whose legs
//!   send each shared or identified left symbol to its right counterpart; a
//!   side whose name cannot label a node, or a theory composed with itself,
//!   is carried inline in the recipe under a label of the step's own.
//!
//! A step may name one left symbol more than once, as a shared sort that is
//! also identified, or identified with several right symbols: every right
//! symbol it is named with becomes one symbol with it. Every sort that a
//! shared operation, or an identified sort or operation, mentions in its
//! declaration must itself be shared or identified, so the legs can map it;
//! a step that leaves one out is refused, naming the symbol and the sort.
//!
//! [`compile_composition_with_report`] also returns the
//! [`CompositionReport`] of every step.

use std::collections::HashMap;
use std::hash::BuildHasher;
use std::sync::Arc;

use panproto_gat::{
    ClashPolicy, ComposeOptions, CompositionMode, CompositionReport, CompositionSpec,
    CompositionStep, DEFAULT_VERIFY_BUDGET, DiagramEdge, DiagramNode, OpAssignment, Operation,
    RecipeEdge, RecipeOpImage, Sort, Theory, TheoryDiagram, TheoryMorphism, compose,
    pushout_by_name,
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
    let shared_name = format!("{step_name}_shared");
    let entries = shared_entries(step, left, right).map_err(failed)?;

    if step.identify.is_empty() {
        let recipe = CompositionStep::Colimit {
            left: step.left.clone(),
            right: step.right.clone(),
            shared_sorts: step.shared_sorts.clone(),
            shared_ops: step.shared_ops.clone(),
            mode: step.mode,
        };
        let shared = shared_theory(&shared_name, entries);
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

    check_mentioned_sorts(&entries).map_err(failed)?;
    compose_identified(step, left, right, apexes(&shared_name, entries), &step_name).map_err(failed)
}

/// Compose a step that has identifications: one diagram over the two sides
/// and the step's shared theories (see [`apexes`]), each with a leg into
/// either side.
///
/// Each side is a node of the diagram, labelled as [`side_node`] says, so a
/// side whose name cannot label a node, or a theory composed with itself,
/// composes as it does in a step without identifications.
fn compose_identified(
    step: &ColimitStepSpec,
    left: &Theory,
    right: &Theory,
    apexes: Vec<Apex>,
    step_name: &str,
) -> Result<ComposedStep, String> {
    let apex_labels: Vec<String> = apexes.iter().map(|a| a.theory.name.to_string()).collect();
    let mut taken: Vec<&str> = apex_labels.iter().map(String::as_str).collect();
    let left_node = side_node(&step.left, left, &taken, step_name, "left");
    taken.push(&left_node.label);
    let right_node = side_node(&step.right, right, &taken, step_name, "right");

    let mut nodes = vec![left_node.label.clone(), right_node.label.clone()];
    let mut recipe_edges = Vec::with_capacity(2 * apexes.len());
    let mut edges = Vec::with_capacity(2 * apexes.len());
    // The shared theories first, then any side carried inline.
    let mut inline: Vec<Theory> = apexes.iter().map(|a| a.theory.clone()).collect();
    let mut diagram_nodes = Vec::with_capacity(apexes.len() + 2);
    for apex in &apexes {
        let (to_left, to_right) = apex.legs(&left_node, &right_node);
        let label = &apex.theory.name;
        nodes.push(label.to_string());
        recipe_edges.push(recipe_edge(label, &left_node.label, &to_left));
        recipe_edges.push(recipe_edge(label, &right_node.label, &to_right));
        for (node, morphism) in [(&left_node, to_left), (&right_node, to_right)] {
            edges.push(DiagramEdge {
                src: Arc::clone(label),
                tgt: Arc::from(node.label.as_str()),
                morphism,
            });
        }
    }
    for node in [&left_node, &right_node] {
        if node.inline {
            inline.push(node.theory.clone());
        }
        diagram_nodes.push(DiagramNode {
            label: Arc::from(node.label.as_str()),
            theory: node.theory.clone(),
        });
    }
    for apex in apexes {
        diagram_nodes.push(DiagramNode {
            label: Arc::clone(&apex.theory.name),
            theory: apex.theory,
        });
    }
    let recipe = CompositionStep::Diagram {
        nodes,
        edges: recipe_edges,
        mode: step.mode,
        clash: ClashPolicy::Reject,
        rewrite_budget: DEFAULT_VERIFY_BUDGET,
        inline,
    };
    let diagram = TheoryDiagram {
        nodes: diagram_nodes,
        edges,
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

/// One symbol a step shares between its sides: the left declaration, which
/// the shared theory copies, and the name of its right counterpart.
#[derive(Clone)]
enum SharedEntry {
    /// A shared or identified sort.
    Sort {
        /// The sort as the shared theory declares it.
        decl: Sort,
        /// The right sort it is one symbol with.
        right: Arc<str>,
    },
    /// A shared or identified operation.
    Op {
        /// The operation as the shared theory declares it.
        decl: Operation,
        /// The right operation it is one symbol with.
        right: Arc<str>,
    },
}

impl SharedEntry {
    /// The entry's kind and left name, which a shared theory declares once.
    fn key(&self) -> (IdentifyKind, &str) {
        match self {
            Self::Sort { decl, .. } => (IdentifyKind::Sort, &decl.name),
            Self::Op { decl, .. } => (IdentifyKind::Op, &decl.name),
        }
    }

    /// The sort names the entry's declaration mentions: an operation's
    /// input and output sorts, or a sort's parameter sorts.
    fn mentioned_sorts(&self) -> Vec<&Arc<str>> {
        match self {
            Self::Sort { decl, .. } => decl.params.iter().map(|p| p.sort.head()).collect(),
            Self::Op { decl, .. } => decl
                .inputs
                .iter()
                .map(|(_, sort, _)| sort.head())
                .chain(std::iter::once(decl.output.head()))
                .collect(),
        }
    }
}

/// The symbols a step shares between its sides, in step order: its shared
/// sorts (simple, as a pushout by name has always built them), its shared
/// operations as the left theory declares them, then its identified sorts
/// and operations, each as the left theory declares it.
fn shared_entries(
    step: &ColimitStepSpec,
    left: &Theory,
    right: &Theory,
) -> Result<Vec<SharedEntry>, String> {
    let mut entries: Vec<SharedEntry> = step
        .shared_sorts
        .iter()
        .map(|s| SharedEntry::Sort {
            decl: Sort::simple(s.as_str()),
            right: Arc::from(s.as_str()),
        })
        .collect();
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
        entries.push(SharedEntry::Op {
            decl: op.clone(),
            right: Arc::from(op_name.as_str()),
        });
    }
    for pair in &step.identify {
        let entry = match pair.kind {
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
                SharedEntry::Sort {
                    decl: sort.clone(),
                    right: Arc::from(pair.right.as_str()),
                }
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
                SharedEntry::Op {
                    decl: op.clone(),
                    right: Arc::from(pair.right.as_str()),
                }
            }
        };
        entries.push(entry);
    }
    Ok(entries)
}

/// The theory a step without identifications shares between its sides,
/// named `name`: its shared sorts, then its shared operations.
fn shared_theory(name: &str, entries: Vec<SharedEntry>) -> Theory {
    let mut sorts = Vec::new();
    let mut ops = Vec::new();
    for entry in entries {
        match entry {
            SharedEntry::Sort { decl, .. } => sorts.push(decl),
            SharedEntry::Op { decl, .. } => ops.push(decl),
        }
    }
    Theory::new(name, sorts, ops, vec![])
}

/// One shared theory of a step with identifications, with the right
/// counterpart of each of its symbols.
struct Apex {
    /// The shared theory, named by its node label.
    theory: Theory,
    /// The right sort each of its sorts is one symbol with, by sort name.
    sort_images: HashMap<Arc<str>, Arc<str>>,
    /// The right operation each of its operations is one symbol with, by
    /// operation name.
    op_images: HashMap<Arc<str>, Arc<str>>,
}

impl Apex {
    /// The shared theory labelled `label` over `entries`, which name each
    /// left symbol once.
    fn new(label: &str, entries: Vec<SharedEntry>) -> Self {
        let mut sorts = Vec::new();
        let mut ops = Vec::new();
        let mut sort_images = HashMap::new();
        let mut op_images = HashMap::new();
        for entry in entries {
            match entry {
                SharedEntry::Sort { decl, right } => {
                    sort_images.insert(Arc::clone(&decl.name), right);
                    sorts.push(decl);
                }
                SharedEntry::Op { decl, right } => {
                    op_images.insert(Arc::clone(&decl.name), right);
                    ops.push(decl);
                }
            }
        }
        Self {
            theory: Theory::new(label, sorts, ops, vec![]),
            sort_images,
            op_images,
        }
    }

    /// The legs into the two side nodes: into the left one, every symbol
    /// maps to itself; into the right one, to its right counterpart. Each
    /// leg ends at its node's theory, whose name is the side's theory name
    /// (for an earlier step's result, not its label) or, for a side carried
    /// inline, its label; recompose names the legs the same way.
    fn legs(&self, left: &SideNode, right: &SideNode) -> (TheoryMorphism, TheoryMorphism) {
        let leg =
            |node: &SideNode,
             image: &dyn Fn(&HashMap<Arc<str>, Arc<str>>, &Arc<str>) -> Arc<str>| {
                TheoryMorphism::new(
                    format!("{}_to_{}", self.theory.name, node.label),
                    Arc::clone(&self.theory.name),
                    Arc::clone(&node.theory.name),
                    self.theory
                        .sorts
                        .iter()
                        .map(|s| (Arc::clone(&s.name), image(&self.sort_images, &s.name)))
                        .collect::<HashMap<_, _>>(),
                    self.theory
                        .ops
                        .iter()
                        .map(|o| {
                            (
                                Arc::clone(&o.name),
                                OpAssignment::Op(image(&self.op_images, &o.name)),
                            )
                        })
                        .collect::<HashMap<_, _>>(),
                )
            };
        (
            leg(left, &|_, name| Arc::clone(name)),
            leg(right, &|images, name| {
                images
                    .get(name)
                    .map_or_else(|| Arc::clone(name), Arc::clone)
            }),
        )
    }
}

/// Check that every sort a shared or identified symbol's declaration
/// mentions is itself shared or identified, so the legs of the step can
/// map it.
fn check_mentioned_sorts(entries: &[SharedEntry]) -> Result<(), String> {
    let sorts: Vec<&str> = entries
        .iter()
        .filter_map(|e| match e.key() {
            (IdentifyKind::Sort, name) => Some(name),
            (IdentifyKind::Op, _) => None,
        })
        .collect();
    for entry in entries {
        for sort in entry.mentioned_sorts() {
            if !sorts.contains(&&**sort) {
                let (what, name) = match entry.key() {
                    (IdentifyKind::Sort, name) => ("sort", name),
                    (IdentifyKind::Op, name) => ("operation", name),
                };
                return Err(format!(
                    "{what} '{name}' is shared or identified, but the sort '{sort}' its \
                     declaration mentions is neither shared nor identified"
                ));
            }
        }
    }
    Ok(())
}

/// The shared theories of a step with identifications, labelled `base`,
/// `{base}_1`, `{base}_2`, ...
///
/// A theory declares each name once, so the entries go into layers: an
/// entry joins the first layer that does not yet declare its kind and left
/// name. One left symbol named by several entries (shared and also
/// identified, or identified with several right symbols) is then one
/// symbol of each of several shared theories, and every right symbol it is
/// named with becomes one symbol with it in the result. A step that names
/// each left symbol once has the one shared theory `base`.
fn apexes(base: &str, entries: Vec<SharedEntry>) -> Vec<Apex> {
    let mut layers: Vec<Vec<SharedEntry>> = Vec::new();
    for entry in entries {
        let free = layers
            .iter()
            .position(|layer| layer.iter().all(|e| e.key() != entry.key()));
        match free {
            Some(index) => layers[index].push(entry),
            None => layers.push(vec![entry]),
        }
    }
    // A later layer declares the sorts its symbols mention, as the first
    // layer that declares each, so its legs map them as that layer's do.
    for index in 1..layers.len() {
        let (earlier, rest) = layers.split_at_mut(index);
        let layer = &mut rest[0];
        let mut pending: Vec<Arc<str>> = layer
            .iter()
            .flat_map(SharedEntry::mentioned_sorts)
            .cloned()
            .collect();
        while let Some(sort) = pending.pop() {
            let key = (IdentifyKind::Sort, &*sort);
            if layer.iter().any(|e| e.key() == key) {
                continue;
            }
            let Some(entry) = earlier.iter().flatten().find(|e| e.key() == key) else {
                continue;
            };
            pending.extend(entry.mentioned_sorts().into_iter().cloned());
            layer.push(entry.clone());
        }
    }
    layers
        .into_iter()
        .enumerate()
        .map(|(index, layer)| {
            let label = if index == 0 {
                base.to_owned()
            } else {
                format!("{base}_{index}")
            };
            Apex::new(&label, layer)
        })
        .collect()
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

    /// The sorted sort names of `theory`.
    fn sort_names(theory: &Theory) -> Vec<String> {
        let mut sorts: Vec<String> = theory.sorts.iter().map(|s| s.name.to_string()).collect();
        sorts.sort();
        sorts
    }

    #[test]
    fn dsl_identify_step_collapses_two_right_sorts_onto_one_left_sort() {
        let registry = registry(&["ThGraph", "ThHypergraph"]);
        for mode in ["amalgamate", "explicit_identity"] {
            let theory = compile_and_replay(
                &one_step(
                    "ThGraph",
                    "ThHypergraph",
                    &[],
                    mode,
                    &[("Edge", "HyperEdge"), ("Edge", "Label")],
                ),
                &HashMap::new(),
                &registry,
            )
            .unwrap_or_else(|e| panic!("{mode}: Edge identifies HyperEdge and Label: {e}"));
            assert_eq!(
                sort_names(&theory),
                vec!["Edge".to_owned(), "Vertex".to_owned()],
                "{mode}"
            );
            let parent_label = theory
                .find_op("parent_label")
                .unwrap_or_else(|| panic!("{mode}: parent_label is kept"));
            assert_eq!(&*parent_label.inputs[0].1.head().to_string(), "Edge");
            assert_eq!(&*parent_label.output.head().to_string(), "Edge");
        }
    }

    #[test]
    fn dsl_identify_step_identifies_a_shared_sort_with_another_right_sort() {
        let registry = registry(&["ThGraph", "ThHypergraph"]);
        for mode in ["amalgamate", "explicit_identity"] {
            // `Vertex` is shared, so it is one sort with the right `Vertex`,
            // and identified with `HyperEdge` too.
            let theory = compile_and_replay(
                &one_step(
                    "ThGraph",
                    "ThHypergraph",
                    &[],
                    mode,
                    &[("Vertex", "HyperEdge")],
                ),
                &HashMap::new(),
                &registry,
            )
            .unwrap_or_else(|e| panic!("{mode}: Vertex is shared and identified: {e}"));
            assert_eq!(
                sort_names(&theory),
                vec!["Edge".to_owned(), "Label".to_owned(), "Vertex".to_owned()],
                "{mode}"
            );
            let incident = theory
                .find_op("incident")
                .unwrap_or_else(|| panic!("{mode}: incident is kept"));
            assert_eq!(&*incident.inputs[0].1.head().to_string(), "Vertex");
        }
    }

    #[test]
    fn dsl_identify_step_collapses_two_right_operations_onto_one_left_operation() {
        let body = body(
            r#"{
                "id": "test.collapse-ops",
                "description": "src identifies both right endpoints",
                "compose": {
                    "result": "R",
                    "steps": [{
                        "left": "ThGraph",
                        "right": "ThGraph",
                        "shared_sorts": ["Vertex", "Edge"],
                        "shared_ops": ["src"],
                        "mode": "explicit_identity",
                        "identify": [{"kind": "op", "left": "src", "right": "tgt"}]
                    }]
                }
            }"#,
        );
        let theory = compile_and_replay(&body, &HashMap::new(), &registry(&["ThGraph"]))
            .unwrap_or_else(|e| panic!("src identifies src and tgt: {e}"));
        let mut ops: Vec<String> = theory.ops.iter().map(|o| o.name.to_string()).collect();
        ops.sort();
        // The class of `src` holds the left `src` and both right
        // endpoints; the left `tgt` stays alone.
        assert_eq!(ops, vec!["src".to_owned(), "tgt".to_owned()]);
    }

    #[test]
    fn dsl_identify_step_refuses_an_operation_whose_sorts_it_does_not_share() {
        let body = body(
            r#"{
                "id": "test.op-sorts",
                "description": "src is identified but Edge is not shared",
                "compose": {
                    "result": "R",
                    "steps": [{
                        "left": "ThGraph",
                        "right": "ThGraph",
                        "shared_sorts": ["Vertex"],
                        "mode": "explicit_identity",
                        "identify": [{"kind": "op", "left": "src", "right": "tgt"}]
                    }]
                }
            }"#,
        );
        let Err(error) =
            compile_composition_with_report(&body, &builtin_resolver(), &HashMap::new())
        else {
            panic!("src needs Edge shared");
        };
        let TheoryDslError::ColimitFailed { step, message } = error else {
            panic!("expected ColimitFailed, got {error:?}");
        };
        assert_eq!(step, 0);
        // The message names what the user wrote, not a node or edge of the
        // step's diagram.
        assert!(
            message.contains("'src'") && message.contains("'Edge'"),
            "{message}"
        );
        assert!(
            !message.contains("step_0") && !message.contains("edge 0"),
            "{message}"
        );
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
