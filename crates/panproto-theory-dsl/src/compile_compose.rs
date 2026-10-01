//! Compile a [`CompositionBody`] into a composed [`Theory`].
//!
//! Each DSL step is composed as [`panproto_gat::compose()`] composes it, and
//! recorded as the [`CompositionStep`] that
//! [`panproto_gat::recompose`] replays to the same theory:
//!
//! - a step with no `identify` entries whose shared sorts are all simple is,
//!   in the default `amalgamate` mode, a pushout by name
//!   ([`CompositionStep::Colimit`] in [`CompositionMode::Amalgamate`]), as
//!   every step was before modes, and in `explicit_identity` mode the same
//!   span composed with explicit identity, refusing any other name the two
//!   sides share;
//! - a step with `identify` entries, or one that shares a dependent sort
//!   (which a `Colimit` recipe step would replay as a simple sort), is a
//!   [`CompositionStep::Diagram`] over the two sides and the shared theories
//!   built for the step, whose legs send each shared or identified left
//!   symbol to its right counterpart; a side whose name cannot label a node,
//!   or a theory composed with itself, is carried inline in the recipe under
//!   a label of the step's own.
//!
//! The shared theories copy each shared or identified symbol's left
//! declaration, a shared sort's parameters included. A step may name one
//! left symbol more than once, as a shared sort that is also identified, or
//! identified with several right symbols: every right symbol it is named
//! with becomes one symbol with it. In a step composed as a diagram, every
//! sort that a shared or identified symbol mentions in its declaration (a
//! sort's parameter sorts, an operation's input and output sorts) must be
//! shared or identified with the sort in the same place of the right
//! declaration, so the legs can map it; a step that leaves one out is
//! refused, naming the symbol and the sort. Every step refuses a shared or
//! identified name that a side does not declare, and a shared or identified
//! pair whose arities differ (an operation's inputs, a sort's parameters),
//! naming the symbols, their two arities and the theories.
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
    // A `Colimit` recipe step replays every shared sort as a simple sort, so
    // a step sharing a dependent sort is recorded, and composed, as a
    // diagram whose legs carry its declaration.
    let shares_a_dependent_sort = entries
        .iter()
        .any(|e| matches!(e, SharedEntry::Sort { decl, .. } if !decl.params.is_empty()));

    if step.identify.is_empty() && !shares_a_dependent_sort {
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

    let apexes = apexes(&shared_name, entries).map_err(failed)?;
    compose_diagram(step, left, right, apexes, &step_name).map_err(failed)
}

/// Compose a step that has identifications, or that shares a dependent
/// sort: one diagram over the two sides and the step's shared theories (see
/// [`apexes`]), each with a leg into either side.
///
/// Each side is a node of the diagram, labelled as [`side_node`] says, so a
/// side whose name cannot label a node, or a theory composed with itself,
/// composes as it does in a step without identifications.
fn compose_diagram(
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
/// the shared theory copies, the name of its right counterpart, and the
/// right sort each sort of its declaration must be one symbol with.
#[derive(Clone)]
enum SharedEntry {
    /// A shared or identified sort.
    Sort {
        /// The sort as the shared theory declares it.
        decl: Sort,
        /// The right sort it is one symbol with.
        right: Arc<str>,
        /// The sort itself and each of its parameter sorts, with the right
        /// sort it must be one symbol with.
        images: Vec<(Arc<str>, Arc<str>)>,
    },
    /// A shared or identified operation.
    Op {
        /// The operation as the shared theory declares it.
        decl: Operation,
        /// The right operation it is one symbol with.
        right: Arc<str>,
        /// Each input sort and the output sort, with the sort in the same
        /// place of the right operation.
        images: Vec<(Arc<str>, Arc<str>)>,
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

    /// The name of the entry's right counterpart.
    const fn right(&self) -> &Arc<str> {
        match self {
            Self::Sort { right, .. } | Self::Op { right, .. } => right,
        }
    }

    /// The sort images a leg carrying the entry must have.
    fn images(&self) -> &[(Arc<str>, Arc<str>)] {
        match self {
            Self::Sort { images, .. } | Self::Op { images, .. } => images,
        }
    }

    /// Whether `other` names the same left and right symbols.
    fn same(&self, other: &Self) -> bool {
        self.key() == other.key() && self.right() == other.right()
    }

    /// The entry as the user wrote it: `sort 'A'` or `operation 'f'`, with
    /// its right counterpart.
    fn describe(&self) -> String {
        let (what, name) = match self.key() {
            (IdentifyKind::Sort, name) => ("sort", name),
            (IdentifyKind::Op, name) => ("operation", name),
        };
        format!(
            "{what} '{name}', shared or identified with '{}',",
            self.right()
        )
    }
}

/// The symbols a step shares between its sides, in step order: its shared
/// sorts, its shared operations, then its identified sorts and operations,
/// each as the left theory declares it (a sort with its parameters).
///
/// Every named symbol must be declared on its side, and a shared or
/// identified pair must agree in arity (an operation's inputs, a sort's
/// parameters); a step that breaks either is refused, naming the symbols
/// and the theories as the step does.
fn shared_entries(
    step: &ColimitStepSpec,
    left: &Theory,
    right: &Theory,
) -> Result<Vec<SharedEntry>, String> {
    let sides = Sides { step, left, right };
    let mut entries = Vec::new();
    for name in &step.shared_sorts {
        entries.push(sides.sort_entry("shared", name, name)?);
    }
    for op_name in &step.shared_ops {
        entries.push(sides.op_entry("shared", op_name, op_name)?);
    }
    for pair in &step.identify {
        entries.push(match pair.kind {
            IdentifyKind::Sort => sides.sort_entry("identified", &pair.left, &pair.right)?,
            IdentifyKind::Op => sides.op_entry("identified", &pair.left, &pair.right)?,
        });
    }
    Ok(entries)
}

/// The two sides of a step, with the names the step gives them.
struct Sides<'a> {
    step: &'a ColimitStepSpec,
    left: &'a Theory,
    right: &'a Theory,
}

impl Sides<'_> {
    /// The entry for the sorts `left_name` and `right_name`, shared or
    /// identified as `what` says: the left declaration, with the sort itself
    /// and each of its parameter sorts paired with the right sort in the
    /// same place.
    fn sort_entry(
        &self,
        what: &str,
        left_name: &str,
        right_name: &str,
    ) -> Result<SharedEntry, String> {
        let sort = self.left.find_sort(left_name).ok_or_else(|| {
            format!(
                "{what} sort '{left_name}' not found in left theory '{}'",
                self.step.left
            )
        })?;
        let counterpart = self.right.find_sort(right_name).ok_or_else(|| {
            format!(
                "{what} sort '{right_name}' not found in right theory '{}'",
                self.step.right
            )
        })?;
        if sort.params.len() != counterpart.params.len() {
            return Err(format!(
                "{what} sort '{left_name}' has arity {} in left theory '{}', but \
                 '{right_name}' has arity {} in right theory '{}'",
                sort.params.len(),
                self.step.left,
                counterpart.params.len(),
                self.step.right
            ));
        }
        let right: Arc<str> = Arc::from(right_name);
        let images = std::iter::once((Arc::clone(&sort.name), Arc::clone(&right)))
            .chain(
                sort.params
                    .iter()
                    .zip(&counterpart.params)
                    .map(|(a, b)| (Arc::clone(a.sort.head()), Arc::clone(b.sort.head()))),
            )
            .collect();
        Ok(SharedEntry::Sort {
            decl: sort.clone(),
            right,
            images,
        })
    }

    /// The entry for the operations `left_name` and `right_name`, shared
    /// or identified as `what` says.
    fn op_entry(
        &self,
        what: &str,
        left_name: &str,
        right_name: &str,
    ) -> Result<SharedEntry, String> {
        let op = self.left.find_op(left_name).ok_or_else(|| {
            format!(
                "{what} operation '{left_name}' not found in left theory '{}'",
                self.step.left
            )
        })?;
        let counterpart = self.right.find_op(right_name).ok_or_else(|| {
            format!(
                "{what} operation '{right_name}' not found in right theory '{}'",
                self.step.right
            )
        })?;
        if op.inputs.len() != counterpart.inputs.len() {
            return Err(format!(
                "{what} operation '{left_name}' has arity {} in left theory '{}', but \
                 '{right_name}' has arity {} in right theory '{}'",
                op.inputs.len(),
                self.step.left,
                counterpart.inputs.len(),
                self.step.right
            ));
        }
        let images = op
            .inputs
            .iter()
            .zip(&counterpart.inputs)
            .map(|((_, a, _), (_, b, _))| (Arc::clone(a.head()), Arc::clone(b.head())))
            .chain(std::iter::once((
                Arc::clone(op.output.head()),
                Arc::clone(counterpart.output.head()),
            )))
            .collect();
        Ok(SharedEntry::Op {
            decl: op.clone(),
            right: Arc::from(right_name),
            images,
        })
    }
}

/// The theory a step composed as a pushout by name (no identifications, no
/// dependent shared sort) shares between its sides, named `name`: its
/// shared sorts, then its shared operations.
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

/// One shared theory of a step composed as a diagram, with the right
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
                SharedEntry::Sort { decl, right, .. } => {
                    sort_images.insert(Arc::clone(&decl.name), right);
                    sorts.push(decl);
                }
                SharedEntry::Op { decl, right, .. } => {
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
        (self.leg(left, false), self.leg(right, true))
    }

    /// The leg into `node`, sending each symbol to its right counterpart
    /// when `to_right`, else to itself.
    fn leg(&self, node: &SideNode, to_right: bool) -> TheoryMorphism {
        let image = |images: &HashMap<Arc<str>, Arc<str>>, name: &Arc<str>| -> Arc<str> {
            if to_right {
                images
                    .get(name)
                    .map_or_else(|| Arc::clone(name), Arc::clone)
            } else {
                Arc::clone(name)
            }
        };
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
    }
}

/// `entry` with the sort entries its images need, transitively: for each
/// image `(a, b)`, the entry sharing or identifying the left sort `a` with
/// the right sort `b`. `entry` comes first.
///
/// # Errors
///
/// A message naming `entry` and the sort, in the step's terms, when an
/// image has no such entry (the sort is neither shared nor identified, or
/// is not one symbol with the right sort the declaration needs), or when
/// one left sort would need two right sorts.
fn closure<'a>(
    entry: &'a SharedEntry,
    entries: &'a [SharedEntry],
) -> Result<Vec<&'a SharedEntry>, String> {
    let mut members = vec![entry];
    let mut pending: Vec<&(Arc<str>, Arc<str>)> = entry.images().iter().rev().collect();
    while let Some((sort, image)) = pending.pop() {
        let key = (IdentifyKind::Sort, &**sort);
        if let Some(member) = members.iter().find(|m| m.key() == key) {
            if member.right() != image {
                return Err(format!(
                    "{} needs the sort '{sort}' to be one symbol with both '{}' and \
                     '{image}', which one shared symbol cannot be",
                    entry.describe(),
                    member.right()
                ));
            }
            continue;
        }
        let mut candidates = entries.iter().filter(|e| e.key() == key).peekable();
        if candidates.peek().is_none() {
            return Err(format!(
                "{} mentions the sort '{sort}' in its declaration, but '{sort}' is neither \
                 shared nor identified",
                entry.describe()
            ));
        }
        let Some(found) = candidates.find(|e| e.right() == image) else {
            return Err(format!(
                "{} needs the sort '{sort}' to be one symbol with the right sort '{image}', \
                 but the step neither shares '{sort}' nor identifies it with '{image}'",
                entry.describe()
            ));
        };
        members.push(found);
        pending.extend(found.images().iter().rev());
    }
    Ok(members)
}

/// The shared theories of a step composed as a diagram, labelled `base`,
/// `{base}_1`, `{base}_2`, ...
///
/// Each shared theory has legs into the two sides, so it declares each
/// name once and maps each of its sorts to one right sort. The entries go
/// into layers in step order: an entry, together with the sort entries its
/// declaration needs (see [`closure`]), joins the first layer in which none
/// of them names a left symbol that the layer already sends to another
/// right symbol. One left symbol named by several entries (shared and also
/// identified, or identified with several right symbols) is then one
/// symbol of each of several shared theories, and every right symbol it is
/// named with becomes one symbol with it in the result, whatever the order
/// of the entries and whether a redundant entry repeats one. A step that
/// names each left symbol once has the one shared theory `base`.
///
/// # Errors
///
/// The message of [`closure`] for the first entry whose declaration needs
/// a sort the step does not share or identify as it must.
fn apexes(base: &str, entries: Vec<SharedEntry>) -> Result<Vec<Apex>, String> {
    let mut unique: Vec<SharedEntry> = Vec::with_capacity(entries.len());
    for entry in entries {
        if !unique.iter().any(|u| u.same(&entry)) {
            unique.push(entry);
        }
    }
    let mut layers: Vec<Vec<SharedEntry>> = Vec::new();
    for entry in &unique {
        let members = closure(entry, &unique)?;
        if layers.iter().flatten().any(|e| e.same(entry)) {
            continue;
        }
        let fits = |layer: &Vec<SharedEntry>| {
            members
                .iter()
                .all(|m| layer.iter().all(|e| e.key() != m.key() || e.same(m)))
        };
        let index = layers.iter().position(fits).unwrap_or_else(|| {
            layers.push(Vec::new());
            layers.len() - 1
        });
        let layer = &mut layers[index];
        for member in members {
            if !layer.iter().any(|e| e.same(member)) {
                layer.push(member.clone());
            }
        }
    }
    Ok(layers
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
        .collect())
}

#[cfg(test)]
mod tests {
    use panproto_gat::{CheckStatus, GatError, recompose};

    use super::*;
    use crate::compile::builtin_resolver;
    use crate::document::{CompiledTheorySet, TheoryBody, TheoryDocument};

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

    /// The composition of [`dsl_compose_with_identify_returns_the_report`]
    /// as a JSON object, result `GraphHyper`.
    const GRAPH_HYPER: &str = r#"{
        "result": "GraphHyper",
        "bases": ["ThGraph", "ThHypergraph"],
        "steps": [{
            "left": "ThGraph",
            "right": "ThHypergraph",
            "shared_sorts": ["Vertex"],
            "mode": "explicit_identity",
            "identify": [{"kind": "sort", "left": "Edge", "right": "HyperEdge"}]
        }]
    }"#;

    /// `GraphHyper` built otherwise: `ThGraph` and `ThHypergraph` amalgamated
    /// over `Vertex`, without identifying the edges.
    const GRAPH_HYPER_AMALGAMATED: &str = r#"{
        "result": "GraphHyper",
        "bases": ["ThGraph", "ThHypergraph"],
        "steps": [{"left": "ThGraph", "right": "ThHypergraph", "shared_sorts": ["Vertex"]}]
    }"#;

    /// The document whose body is the JSON object `body` with the `id` and
    /// `description` keys added.
    fn document(body: &str) -> TheoryDocument {
        let body = body.trim();
        let json = format!(
            r#"{{"id": "test.doc", "description": "a document", {}"#,
            &body[1..]
        );
        serde_json::from_str(&json).unwrap_or_else(|e| panic!("the document parses: {e}\n{json}"))
    }

    /// The protocol `P` whose schema and instance theories are `schema` and
    /// `instance` (JSON values).
    fn protocol(schema: &str, instance: &str) -> String {
        format!(r#"{{"protocol": "P", "schema_theory": {schema}, "instance_theory": {instance}}}"#)
    }

    /// The theory, recipe and step reports that compiling [`GRAPH_HYPER`]
    /// on its own gives.
    fn graph_hyper() -> (Theory, CompositionSpec, Vec<CompositionReport>) {
        let body = body(&format!(
            r#"{{"id": "test.gh", "description": "", "compose": {GRAPH_HYPER}}}"#
        ));
        let composed = compile_composition_with_report(&body, &builtin_resolver(), &HashMap::new())
            .unwrap_or_else(|e| panic!("GraphHyper compiles: {e}"));
        assert_eq!(composed.reports.len(), 1);
        (composed.theory, composed.spec, composed.reports)
    }

    /// Compile `doc` with the built-in resolver.
    fn compile_document(doc: &TheoryDocument) -> Result<CompiledTheorySet, TheoryDslError> {
        crate::compile::compile(doc, &builtin_resolver())
    }

    /// `set` records `GraphHyper` with its theory, its recipe and its one
    /// step report.
    fn assert_records_graph_hyper(set: &CompiledTheorySet, what: &str) {
        let (theory, spec, reports) = graph_hyper();
        assert_eq!(
            set.composition_specs.get("GraphHyper"),
            Some(&spec),
            "{what}: {set:?}"
        );
        assert_eq!(
            set.composition_reports.get("GraphHyper"),
            Some(&reports),
            "{what}: {set:?}"
        );
        assert_eq!(reports[0].mode, CompositionMode::ExplicitIdentity, "{what}");
        assert_eq!(
            set.theories.get("GraphHyper"),
            Some(&theory),
            "{what}: {set:?}"
        );
    }

    #[test]
    fn document_compile_returns_the_report_of_a_single_composition() {
        let set = compile_document(&document(&format!(r#"{{"compose": {GRAPH_HYPER}}}"#)))
            .unwrap_or_else(|e| panic!("the document compiles: {e}"));
        assert_records_graph_hyper(&set, "single composition");
        assert_eq!(set.composition_reports.len(), 1);
    }

    #[test]
    fn document_compile_returns_the_report_of_each_bundle_composition() {
        let set = compile_document(&document(&format!(
            r#"{{"bundle": "b", "compositions": [{GRAPH_HYPER}]}}"#
        )))
        .unwrap_or_else(|e| panic!("the bundle compiles: {e}"));
        assert_records_graph_hyper(&set, "bundle composition");
        assert_eq!(set.composition_reports.len(), 1);
    }

    #[test]
    fn document_compile_records_an_inline_composed_protocol_theory_and_its_report() {
        let spec = protocol(GRAPH_HYPER, r#""ThGraph""#);
        let single = compile_document(&document(&spec))
            .unwrap_or_else(|e| panic!("the protocol compiles: {e}"));
        let bundle = compile_document(&document(&format!(
            r#"{{"bundle": "b", "protocols": [{spec}]}}"#
        )))
        .unwrap_or_else(|e| panic!("the bundle compiles: {e}"));
        for (what, set) in [("protocol", &single), ("bundle protocol", &bundle)] {
            assert_records_graph_hyper(set, what);
            assert_eq!(set.protocols["P"].schema_theory, "GraphHyper", "{what}");
            assert_eq!(set.composition_reports.len(), 1, "{what}");
        }
    }

    #[test]
    fn document_compile_records_one_inline_composition_named_twice_alike() {
        // The schema and instance theories are the same composition.
        let spec = protocol(GRAPH_HYPER, GRAPH_HYPER);
        let single = compile_document(&document(&spec))
            .unwrap_or_else(|e| panic!("the protocol compiles: {e}"));
        // A bundle composition and an inline one of the same theory.
        let bundle = compile_document(&document(&format!(
            r#"{{"bundle": "b", "compositions": [{GRAPH_HYPER}], "protocols": [{spec}]}}"#
        )))
        .unwrap_or_else(|e| panic!("the bundle compiles: {e}"));
        for (what, set) in [("protocol", &single), ("bundle", &bundle)] {
            assert_records_graph_hyper(set, what);
            assert_eq!(set.composition_reports.len(), 1, "{what}");
        }
    }

    #[test]
    fn document_compile_refuses_two_different_compositions_of_one_name() {
        let duplicate = |result: Result<CompiledTheorySet, TheoryDslError>, what: &str| {
            let Err(TheoryDslError::Duplicate { kind, name }) = result else {
                panic!("{what}: expected Duplicate, got {result:?}");
            };
            assert_eq!(kind, "theory (from composition)", "{what}");
            assert_eq!(name, "GraphHyper", "{what}");
        };
        // Two inline compositions of one name with different theories.
        let spec = protocol(GRAPH_HYPER, GRAPH_HYPER_AMALGAMATED);
        duplicate(compile_document(&document(&spec)), "protocol");
        duplicate(
            compile_document(&document(&format!(
                r#"{{"bundle": "b", "protocols": [{spec}]}}"#
            ))),
            "bundle protocol",
        );
        // An inline composition named as a different bundle composition.
        let inline = protocol(GRAPH_HYPER, r#""ThGraph""#);
        duplicate(
            compile_document(&document(&format!(
                r#"{{"bundle": "b", "compositions": [{GRAPH_HYPER_AMALGAMATED}], "protocols": [{inline}]}}"#
            ))),
            "bundle composition and inline composition",
        );
        // An inline composition named as a bundle theory.
        duplicate(
            compile_document(&document(&format!(
                r#"{{"bundle": "b",
                     "theories": [{{"theory": "GraphHyper", "sorts": [{{"name": "Vertex"}}]}}],
                     "protocols": [{inline}]}}"#
            ))),
            "bundle theory and inline composition",
        );
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

    /// A one-step composition of `left` and `right` in `mode` with the given
    /// shared sorts, shared operations and identifications, each a kind
    /// (`sort` or `op`), a left name and a right name.
    fn step_with(
        (left, right): (&str, &str),
        shared_sorts: &[&str],
        shared_ops: &[&str],
        mode: &str,
        identify: &[(&str, &str, &str)],
    ) -> CompositionBody {
        let quoted = |names: &[&str]| -> String {
            names
                .iter()
                .map(|n| format!("\"{n}\""))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let identify: Vec<String> = identify
            .iter()
            .map(|(kind, l, r)| format!(r#"{{"kind": "{kind}", "left": "{l}", "right": "{r}"}}"#))
            .collect();
        body(&format!(
            r#"{{
                "id": "test.step",
                "description": "one step",
                "compose": {{
                    "result": "R",
                    "steps": [{{
                        "left": "{left}",
                        "right": "{right}",
                        "shared_sorts": [{}],
                        "shared_ops": [{}],
                        "mode": "{mode}",
                        "identify": [{}]
                    }}]
                }}
            }}"#,
            quoted(shared_sorts),
            quoted(shared_ops),
            identify.join(", "),
        ))
    }

    /// The sorted sort and operation names of `theory`.
    fn symbol_names(theory: &Theory) -> (Vec<String>, Vec<String>) {
        let mut ops: Vec<String> = theory.ops.iter().map(|o| o.name.to_string()).collect();
        ops.sort();
        (sort_names(theory), ops)
    }

    /// The message of a step-0 `ColimitFailed` error, checked to name no
    /// node or edge of the step's diagram.
    fn user_message(error: TheoryDslError) -> String {
        let TheoryDslError::ColimitFailed { step, message } = error else {
            panic!("expected ColimitFailed, got {error:?}");
        };
        assert_eq!(step, 0);
        assert!(
            !message.contains("step_0") && !message.contains("edge "),
            "{message}"
        );
        message
    }

    #[test]
    fn dsl_identify_step_composes_the_same_with_a_redundant_shared_operation() {
        // Vertex and Edge are shared, and the left `src` and `Vertex` are
        // identified with the right `tgt` and `Edge`. Sharing `src` (and
        // `tgt`) as well names a left symbol a second time, which must not
        // change whether or how the step composes.
        let graph = registry(&["ThGraph"]);
        let identify_orders: [&[(&str, &str, &str)]; 2] = [
            &[("op", "src", "tgt"), ("sort", "Vertex", "Edge")],
            &[("sort", "Vertex", "Edge"), ("op", "src", "tgt")],
        ];
        let compile = |mode: &str, shared_ops: &[&str], identify: &[(&str, &str, &str)]| {
            compile_and_replay(
                &step_with(
                    ("ThGraph", "ThGraph"),
                    &["Vertex", "Edge"],
                    shared_ops,
                    mode,
                    identify,
                ),
                &HashMap::new(),
                &graph,
            )
            .map(|theory| symbol_names(&theory))
        };
        // Every sort is one class, and so is every operation.
        let one_class = (vec!["Vertex".to_owned()], vec!["src".to_owned()]);
        for identify in identify_orders {
            // Amalgamation glues the redundant entries by name anyway.
            for shared_ops in [&[][..], &["src"][..], &["src", "tgt"][..]] {
                let names = compile("amalgamate", shared_ops, identify)
                    .unwrap_or_else(|e| panic!("{identify:?}, shared ops {shared_ops:?}: {e}"));
                assert_eq!(names, one_class, "{identify:?}, shared ops {shared_ops:?}");
            }
            // With explicit identity, sharing both operations makes every
            // operation one class.
            let names = compile("explicit_identity", &["src", "tgt"], identify)
                .unwrap_or_else(|e| panic!("explicit, {identify:?}: {e}"));
            assert_eq!(names, one_class, "explicit, {identify:?}");
            // Sharing `src` only leaves the left `tgt` alone, while the
            // right `tgt` joins the class of `src`.
            let names = compile("explicit_identity", &["src"], identify)
                .unwrap_or_else(|e| panic!("explicit, {identify:?}, src shared: {e}"));
            assert_eq!(
                names,
                (
                    vec!["Vertex".to_owned()],
                    vec!["src".to_owned(), "tgt".to_owned()]
                ),
                "explicit, {identify:?}, src shared"
            );
            // Sharing neither leaves the left `tgt` and the right `src`
            // outside the class, and their names clash: refused in the
            // step's terms.
            let Err(error) = compile("explicit_identity", &[], identify) else {
                panic!("explicit, {identify:?}: the unshared operations clash");
            };
            let message = user_message(error);
            assert!(message.contains("src"), "explicit, {identify:?}: {message}");
        }
    }

    #[test]
    fn dsl_identify_step_refuses_an_operation_whose_sorts_are_identified_otherwise() {
        // `src` is identified with `tgt`, which needs the left `Vertex` to be
        // one symbol with the right `Vertex`; it is only identified with the
        // right `Edge`.
        for mode in ["amalgamate", "explicit_identity"] {
            let Err(error) = compile_composition_with_report(
                &step_with(
                    ("ThGraph", "ThGraph"),
                    &["Edge"],
                    &[],
                    mode,
                    &[("sort", "Vertex", "Edge"), ("op", "src", "tgt")],
                ),
                &builtin_resolver(),
                &HashMap::new(),
            ) else {
                panic!("{mode}: src~tgt needs Vertex~Vertex");
            };
            let message = user_message(error);
            assert_eq!(
                message,
                "operation 'src', shared or identified with 'tgt', needs the sort 'Vertex' to be \
                 one symbol with the right sort 'Vertex', but the step neither shares 'Vertex' \
                 nor identifies it with 'Vertex'",
                "{mode}"
            );
        }
    }

    #[test]
    fn dsl_identify_step_refuses_operations_of_different_arity() {
        // `src` takes one input and ThHypergraph's `incident` two.
        let other = "incident";
        for mode in ["amalgamate", "explicit_identity"] {
            let Err(error) = compile_composition_with_report(
                &step_with(
                    ("ThGraph", "ThHypergraph"),
                    &["Vertex"],
                    &[],
                    mode,
                    &[("sort", "Edge", "HyperEdge"), ("op", "src", other)],
                ),
                &builtin_resolver(),
                &HashMap::new(),
            ) else {
                panic!("{mode}: src and {other} differ in arity");
            };
            let message = user_message(error);
            assert!(
                message
                    == format!(
                        "identified operation 'src' has arity 1 in left theory 'ThGraph', but \
                         '{other}' has arity 2 in right theory 'ThHypergraph'"
                    ),
                "{mode}: {message}"
            );
        }
    }

    #[test]
    fn dsl_step_names_a_shared_sort_missing_from_a_side() {
        for mode in ["amalgamate", "explicit_identity"] {
            for identify in [&[][..], &[("sort", "Edge", "HyperEdge")][..]] {
                let Err(error) = compile_composition_with_report(
                    &step_with(
                        ("ThGraph", "ThHypergraph"),
                        &["Vertex", "Nope"],
                        &[],
                        mode,
                        identify,
                    ),
                    &builtin_resolver(),
                    &HashMap::new(),
                ) else {
                    panic!("{mode}, {identify:?}: Nope is in neither theory");
                };
                let message = user_message(error);
                assert_eq!(
                    message, "shared sort 'Nope' not found in left theory 'ThGraph'",
                    "{mode}, {identify:?}"
                );
            }
            // Present on the left only: the right theory is named.
            let Err(error) = compile_composition_with_report(
                &step_with(
                    ("ThGraph", "ThHypergraph"),
                    &["Vertex", "Edge"],
                    &[],
                    mode,
                    &[("sort", "Edge", "HyperEdge")],
                ),
                &builtin_resolver(),
                &HashMap::new(),
            ) else {
                panic!("{mode}: Edge is not in ThHypergraph");
            };
            let message = user_message(error);
            assert_eq!(
                message, "shared sort 'Edge' not found in right theory 'ThHypergraph'",
                "{mode}"
            );
        }
    }

    /// The theory `name` with the simple sort `vertex` and the sort `E`,
    /// simple or `E(a: vertex, b: vertex)` as `dependent` says.
    fn vertex_edge(name: &str, vertex: &str, dependent: bool) -> Theory {
        use panproto_gat::SortParam;
        let edge = if dependent {
            Sort::dependent(
                "E",
                vec![SortParam::new("a", vertex), SortParam::new("b", vertex)],
            )
        } else {
            Sort::simple("E")
        };
        Theory::new(name, vec![Sort::simple(vertex), edge], vec![], vec![])
    }

    /// The local theories `L` and `R` of [`vertex_edge`] over `V`.
    fn vertex_edge_sides(left_dependent: bool, right_dependent: bool) -> HashMap<String, Theory> {
        [
            ("L".to_owned(), vertex_edge("L", "V", left_dependent)),
            ("R".to_owned(), vertex_edge("R", "V", right_dependent)),
        ]
        .into()
    }

    /// The parameters of the sort `name` of `theory`, as `(name, sort)`.
    fn params_of(theory: &Theory, name: &str) -> Vec<(String, String)> {
        theory
            .find_sort(name)
            .unwrap_or_else(|| panic!("{name} is a sort of {}", theory.name))
            .params
            .iter()
            .map(|p| (p.name.to_string(), p.sort.head().to_string()))
            .collect()
    }

    #[test]
    fn dsl_step_refuses_a_shared_sort_of_different_arity() {
        let cases = [
            (
                vertex_edge_sides(false, true),
                "shared sort 'E' has arity 0 in left theory 'L', but 'E' has arity 2 in right \
                 theory 'R'",
            ),
            (
                vertex_edge_sides(true, false),
                "shared sort 'E' has arity 2 in left theory 'L', but 'E' has arity 0 in right \
                 theory 'R'",
            ),
        ];
        for (local, expected) in &cases {
            for mode in ["amalgamate", "explicit_identity"] {
                for identify in [&[][..], &[("sort", "V", "V")][..]] {
                    let Err(error) = compile_composition_with_report(
                        &step_with(("L", "R"), &["V", "E"], &[], mode, identify),
                        &builtin_resolver(),
                        local,
                    ) else {
                        panic!("{mode}, {identify:?}: E differs in arity");
                    };
                    let message = user_message(error);
                    assert_eq!(message, *expected, "{mode}, {identify:?}");
                }
            }
        }
    }

    #[test]
    fn dsl_step_shares_a_dependent_sort_of_equal_arity() {
        let local = vertex_edge_sides(true, true);
        let expected = vec![
            ("a".to_owned(), "V".to_owned()),
            ("b".to_owned(), "V".to_owned()),
        ];
        for mode in ["amalgamate", "explicit_identity"] {
            for identify in [&[][..], &[("sort", "V", "V")][..]] {
                let theory = compile_and_replay(
                    &step_with(("L", "R"), &["V", "E"], &[], mode, identify),
                    &local,
                    &local,
                )
                .unwrap_or_else(|e| panic!("{mode}, {identify:?}: {e}"));
                assert_eq!(sort_names(&theory), ["E", "V"], "{mode}, {identify:?}");
                assert_eq!(params_of(&theory, "E"), expected, "{mode}, {identify:?}");
            }
        }
    }

    #[test]
    fn dsl_identify_step_shares_a_dependent_sort_over_identified_parameter_sorts() {
        // The right theory names its vertex sort `N`: sharing `E` needs the
        // left `V` to be one symbol with the right `N`.
        let local: HashMap<String, Theory> = [
            ("L".to_owned(), vertex_edge("L", "V", true)),
            ("R".to_owned(), vertex_edge("R", "N", true)),
        ]
        .into();
        for mode in ["amalgamate", "explicit_identity"] {
            let theory = compile_and_replay(
                &step_with(("L", "R"), &["E"], &[], mode, &[("sort", "V", "N")]),
                &local,
                &local,
            )
            .unwrap_or_else(|e| panic!("{mode}: {e}"));
            assert_eq!(sort_names(&theory), ["E", "V"], "{mode}");
            assert_eq!(
                params_of(&theory, "E"),
                [
                    ("a".to_owned(), "V".to_owned()),
                    ("b".to_owned(), "V".to_owned())
                ],
                "{mode}"
            );
            // Without the identification the parameter sorts cannot be
            // mapped: refused in the step's terms, whether or not the step
            // identifies anything else.
            for identify in [&[][..], &[("sort", "E", "E")][..]] {
                let Err(error) = compile_composition_with_report(
                    &step_with(("L", "R"), &["E"], &[], mode, identify),
                    &builtin_resolver(),
                    &local,
                ) else {
                    panic!("{mode}, {identify:?}: V and N are not one symbol");
                };
                assert_eq!(
                    user_message(error),
                    "sort 'E', shared or identified with 'E', mentions the sort 'V' in its \
                     declaration, but 'V' is neither shared nor identified",
                    "{mode}, {identify:?}"
                );
            }
        }
    }

    #[test]
    fn dsl_step_refuses_a_dependent_shared_sort_whose_parameter_sort_it_does_not_share() {
        // Both sides declare `V` and `E(a: V, b: V)`; sharing `E` alone
        // leaves the sort its parameters range over unshared.
        let local = vertex_edge_sides(true, true);
        for mode in ["amalgamate", "explicit_identity"] {
            let Err(error) = compile_composition_with_report(
                &step_with(("L", "R"), &["E"], &[], mode, &[]),
                &builtin_resolver(),
                &local,
            ) else {
                panic!("{mode}: V is not shared");
            };
            assert_eq!(
                user_message(error),
                "sort 'E', shared or identified with 'E', mentions the sort 'V' in its \
                 declaration, but 'V' is neither shared nor identified",
                "{mode}"
            );
        }
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
