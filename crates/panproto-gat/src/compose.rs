//! Composition of theories along a labeled diagram.
//!
//! A [`TheoryDiagram`] names its theories by node label and relates them
//! by theory morphisms. [`compose`] computes the diagram's colimit: every
//! node's sorts and operations form one disjoint union per kind, the
//! least equivalence the morphisms generate identifies them, and each
//! class becomes one symbol of the result, which carries every node's
//! equations translated along the cocone. Sharing is decided by which
//! symbols the morphisms relate, never by spelling, unless the caller asks
//! for [`CompositionMode::Amalgamate`], which additionally identifies
//! same-name symbols of the same kind.
//!
//! The construction follows the colimit of theories of Goguen and Burstall
//! (goguenburstall1992institutions, Thm 11, proved: the colimit is built on
//! signatures and carries the translated sentences of every component).
//! Carrying it to dependent sort contexts is ours (inferred): every class
//! must agree on its members' signatures modulo the identification before
//! anything is built, and the result must pass the signature typechecker.
//!
//! A composition returns a [`Composition`]: the theory, one inclusion
//! morphism per node, a [`Provenance`] that sends every source symbol,
//! equation, directed equation and policy to its image, and a
//! [`CompositionReport`] that records every check that ran.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::check_status::CheckStatus;
use crate::eq::{
    DirectedEquation, Equation, Term, alpha_equivalent, alpha_equivalent_equation,
    normalize_with_status,
};
use crate::error::GatError;
use crate::ident::SiteRename;
use crate::morphism::{OpAssignment, TheoryMorphism, check_morphism};
use crate::op::{Implicit, Operation};
use crate::quotient::UnionFind;
use crate::sort::{Sort, SortExpr};
use crate::theory::{ConflictPolicy, Theory};
use crate::typecheck::{
    SignatureDefect, typecheck_equation, typecheck_signature, typecheck_theory_body,
};

/// The kind of a symbol of a theory.
///
/// Serialized in `snake_case`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymKind {
    /// A sort.
    Sort,
    /// An operation.
    Op,
    /// An equation.
    Eq,
    /// A directed equation.
    DirectedEq,
    /// A conflict policy.
    Policy,
}

impl SymKind {
    /// The `snake_case` name of this kind, as it serializes.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sort => "sort",
            Self::Op => "op",
            Self::Eq => "eq",
            Self::DirectedEq => "directed_eq",
            Self::Policy => "policy",
        }
    }
}

impl std::fmt::Display for SymKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A symbol of one node of a diagram: the node's label and the symbol's
/// name in that node's theory.
///
/// The derived order compares the label first, then the name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SymRef {
    /// The label of the node that declares (or uses) the symbol.
    pub node: Arc<str>,
    /// The symbol's name in that node's theory.
    pub name: Arc<str>,
}

impl SymRef {
    /// Build a symbol reference.
    #[must_use]
    pub fn new(node: impl Into<Arc<str>>, name: impl Into<Arc<str>>) -> Self {
        Self {
            node: node.into(),
            name: name.into(),
        }
    }
}

impl std::fmt::Display for SymRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.node, self.name)
    }
}

/// How the members of one sort class disagree.
///
/// Serialized externally tagged in `snake_case`; the unit variants are
/// bare strings (`"kind"`, `"closure"`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortMismatch {
    /// The two sorts have different numbers of parameters.
    Arity {
        /// The first member's arity.
        first: usize,
        /// The second member's arity.
        second: usize,
    },
    /// Parameter `index` ranges over different sort classes.
    ParamSort {
        /// The parameter's position.
        index: usize,
        /// The first member's parameter sort, as declared.
        first: Arc<str>,
        /// The second member's parameter sort, as declared.
        second: Arc<str>,
    },
    /// The two sorts have different [`crate::SortKind`]s.
    Kind,
    /// The two sorts have different closures, after their constructors are
    /// sent to their classes.
    Closure,
}

impl std::fmt::Display for SortMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Arity { first, second } => write!(f, "sort arities differ ({first} vs {second})"),
            Self::ParamSort {
                index,
                first,
                second,
            } => write!(f, "parameter {index} sorts differ ({first} vs {second})"),
            Self::Kind => f.write_str("sort kinds differ"),
            Self::Closure => f.write_str("sort closures differ"),
        }
    }
}

/// Why an identification the diagram asks for cannot be expressed as a
/// theory.
///
/// Serialized in `snake_case`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotRepresentableReason {
    /// Two identified operations differ only in the term arguments of a
    /// dependent input or output sort (for instance `Hom(x, y)` against
    /// `Hom(y, x)`); identifying them would need an equation between
    /// sorts, which a [`crate::Theory`] cannot state.
    NeedsSortEquation,
}

/// How two members of one operation class disagree, once every sort and
/// operation they mention is sent to its class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpMismatch {
    /// Only the term arguments of dependent sorts differ.
    NeedsSortEquation,
    /// The signatures differ in some other way.
    Signature,
}

impl std::fmt::Display for OpMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NeedsSortEquation => f.write_str(
                "operation signatures differ only in dependent sort arguments, \
                 which would need a sort equation",
            ),
            Self::Signature => f.write_str("operation signatures differ in the quotient"),
        }
    }
}

/// Send a sort's parameter sorts and closure constructors through the
/// given class maps. The name is left as it is.
pub(crate) fn translate_sort(
    sort: &crate::sort::Sort,
    sort_map: &std::collections::HashMap<Arc<str>, Arc<str>>,
    op_map: &std::collections::HashMap<Arc<str>, Arc<str>>,
) -> crate::sort::Sort {
    crate::sort::Sort {
        name: Arc::clone(&sort.name),
        params: sort
            .params
            .iter()
            .map(|p| crate::sort::SortParam {
                name: Arc::clone(&p.name),
                sort: p.sort.apply_maps(sort_map, op_map),
            })
            .collect(),
        kind: sort.kind.clone(),
        closure: match &sort.closure {
            crate::sort::SortClosure::Open => crate::sort::SortClosure::Open,
            crate::sort::SortClosure::Closed(ctors) => {
                let mut mapped: Vec<Arc<str>> = ctors
                    .iter()
                    .map(|c| op_map.get(c).cloned().unwrap_or_else(|| Arc::clone(c)))
                    .collect();
                mapped.sort();
                mapped.dedup();
                crate::sort::SortClosure::Closed(mapped)
            }
        },
    }
}

/// Send an operation's input and output sorts through the given class
/// maps. The name is left as it is.
pub(crate) fn translate_op(
    op: &crate::op::Operation,
    sort_map: &std::collections::HashMap<Arc<str>, Arc<str>>,
    op_map: &std::collections::HashMap<Arc<str>, Arc<str>>,
) -> crate::op::Operation {
    crate::op::Operation::with_implicit(
        Arc::clone(&op.name),
        op.inputs
            .iter()
            .map(|(n, s, imp)| (Arc::clone(n), s.apply_maps(sort_map, op_map), *imp))
            .collect(),
        op.output.apply_maps(sort_map, op_map),
    )
}

/// Compare two members of one sort class, both already translated into
/// class names. `first_decl` and `second_decl` are the members as
/// declared, used to render a parameter-sort mismatch.
pub(crate) fn sort_class_mismatch(
    first: &crate::sort::Sort,
    second: &crate::sort::Sort,
    first_decl: &crate::sort::Sort,
    second_decl: &crate::sort::Sort,
) -> Option<SortMismatch> {
    if first.arity() != second.arity() {
        return Some(SortMismatch::Arity {
            first: first.arity(),
            second: second.arity(),
        });
    }
    let rename = crate::sort::positional_param_rename(
        first.params.iter().map(|p| Arc::clone(&p.name)),
        second.params.iter().map(|p| Arc::clone(&p.name)),
    );
    for (index, (a, b)) in first.params.iter().zip(&second.params).enumerate() {
        if !a.sort.subst(&rename).alpha_eq(&b.sort) {
            return Some(SortMismatch::ParamSort {
                index,
                first: Arc::from(first_decl.params[index].sort.to_string()),
                second: Arc::from(second_decl.params[index].sort.to_string()),
            });
        }
    }
    if first.kind != second.kind {
        return Some(SortMismatch::Kind);
    }
    if first.closure != second.closure {
        return Some(SortMismatch::Closure);
    }
    None
}

/// Compare two members of one operation class, both already translated
/// into class names.
pub(crate) fn op_class_mismatch(
    first: &crate::op::Operation,
    second: &crate::op::Operation,
) -> Option<OpMismatch> {
    if crate::sort::signatures_equivalent_modulo_param_rename(
        &first.inputs,
        &first.output,
        &second.inputs,
        &second.output,
    ) {
        return None;
    }
    let same_shape = first.inputs.len() == second.inputs.len()
        && first
            .inputs
            .iter()
            .zip(&second.inputs)
            .all(|((_, a, ia), (_, b, ib))| {
                ia == ib && a.head() == b.head() && a.args().len() == b.args().len()
            })
        && first.output.head() == second.output.head()
        && first.output.args().len() == second.output.args().len();
    if same_shape {
        Some(OpMismatch::NeedsSortEquation)
    } else {
        Some(OpMismatch::Signature)
    }
}

// ─── diagrams and options ──────────────────────────────────────────

/// One node of a [`TheoryDiagram`]: a theory under a label.
///
/// The label names the node for edges, provenance and freshened names. It
/// is unique within its diagram, non-empty, and contains neither `.` nor
/// `#`, the two characters freshened names use as separators.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagramNode {
    /// The node's label.
    pub label: Arc<str>,
    /// The node's theory.
    pub theory: Theory,
}

/// One edge of a [`TheoryDiagram`]: a theory morphism from the node
/// labeled `src` to the node labeled `tgt`.
///
/// Endpoints are named by label, so a diagram means the same thing
/// whatever the order of its node and edge vectors.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagramEdge {
    /// The label of the morphism's domain node.
    pub src: Arc<str>,
    /// The label of the morphism's codomain node.
    pub tgt: Arc<str>,
    /// The morphism.
    pub morphism: TheoryMorphism,
}

/// A diagram of theories: labeled nodes related by theory morphisms.
///
/// [`compose`] computes its colimit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TheoryDiagram {
    /// The nodes.
    pub nodes: Vec<DiagramNode>,
    /// The edges.
    pub edges: Vec<DiagramEdge>,
}

impl TheoryDiagram {
    /// A span `left <- shared -> right` with legs `i1` and `i2`, whose
    /// colimit is the pushout of the two legs.
    #[must_use]
    pub fn span<A, L, R>(
        shared: (A, Theory),
        left: (L, Theory),
        right: (R, Theory),
        i1: TheoryMorphism,
        i2: TheoryMorphism,
    ) -> Self
    where
        A: Into<Arc<str>>,
        L: Into<Arc<str>>,
        R: Into<Arc<str>>,
    {
        let (shared_label, shared_theory) = (shared.0.into(), shared.1);
        let (left_label, left_theory) = (left.0.into(), left.1);
        let (right_label, right_theory) = (right.0.into(), right.1);
        Self {
            edges: vec![
                DiagramEdge {
                    src: Arc::clone(&shared_label),
                    tgt: Arc::clone(&left_label),
                    morphism: i1,
                },
                DiagramEdge {
                    src: Arc::clone(&shared_label),
                    tgt: Arc::clone(&right_label),
                    morphism: i2,
                },
            ],
            nodes: vec![
                DiagramNode {
                    label: shared_label,
                    theory: shared_theory,
                },
                DiagramNode {
                    label: left_label,
                    theory: left_theory,
                },
                DiagramNode {
                    label: right_label,
                    theory: right_theory,
                },
            ],
        }
    }

    /// Two parallel morphisms `f, g : src -> tgt`, whose colimit is their
    /// coequalizer: `tgt` with the images of `f` and `g` identified.
    #[must_use]
    pub fn parallel<S, T>(
        src: (S, Theory),
        tgt: (T, Theory),
        f: TheoryMorphism,
        g: TheoryMorphism,
    ) -> Self
    where
        S: Into<Arc<str>>,
        T: Into<Arc<str>>,
    {
        let (src_label, src_theory) = (src.0.into(), src.1);
        let (tgt_label, tgt_theory) = (tgt.0.into(), tgt.1);
        Self {
            edges: vec![
                DiagramEdge {
                    src: Arc::clone(&src_label),
                    tgt: Arc::clone(&tgt_label),
                    morphism: f,
                },
                DiagramEdge {
                    src: Arc::clone(&src_label),
                    tgt: Arc::clone(&tgt_label),
                    morphism: g,
                },
            ],
            nodes: vec![
                DiagramNode {
                    label: src_label,
                    theory: src_theory,
                },
                DiagramNode {
                    label: tgt_label,
                    theory: tgt_theory,
                },
            ],
        }
    }

    /// Nodes without edges, whose colimit is their coproduct.
    #[must_use]
    pub fn discrete<L: Into<Arc<str>>>(nodes: Vec<(L, Theory)>) -> Self {
        Self {
            nodes: nodes
                .into_iter()
                .map(|(label, theory)| DiagramNode {
                    label: label.into(),
                    theory,
                })
                .collect(),
            edges: Vec::new(),
        }
    }

    /// The span `left <- shared -> right` whose legs send every sort and
    /// operation of `shared` to the same-name sort and operation of each
    /// side.
    ///
    /// A sort name that `shared` uses in a signature without declaring it
    /// is also sent to itself, so that a shared operation over a sort the
    /// shared theory leaves undeclared still has a total leg.
    ///
    /// # Errors
    ///
    /// Returns [`GatError::MissingSortMapping`] or
    /// [`GatError::MissingOpMapping`] when a sort or operation of `shared`
    /// is absent from `left` or `right`.
    pub fn by_name<A, L, R>(
        shared: (A, Theory),
        left: (L, Theory),
        right: (R, Theory),
    ) -> Result<Self, GatError>
    where
        A: Into<Arc<str>>,
        L: Into<Arc<str>>,
        R: Into<Arc<str>>,
    {
        let i1 = identity_leg(&shared.1, &left.1, "i1")?;
        let i2 = identity_leg(&shared.1, &right.1, "i2")?;
        Ok(Self::span(shared, left, right, i1, i2))
    }
}

/// The identity-on-names leg from `shared` into `target`.
fn identity_leg(shared: &Theory, target: &Theory, label: &str) -> Result<TheoryMorphism, GatError> {
    let mut sort_map = HashMap::new();
    for sort in &shared.sorts {
        if !target.has_sort(&sort.name) {
            return Err(GatError::MissingSortMapping(format!(
                "{label}: shared sort `{}` is not present in target theory `{}`",
                sort.name, target.name,
            )));
        }
        sort_map.insert(Arc::clone(&sort.name), Arc::clone(&sort.name));
    }
    for name in undeclared_sort_names(shared) {
        sort_map.insert(Arc::clone(&name), name);
    }
    let mut op_map = HashMap::new();
    for op in &shared.ops {
        if !target.has_op(&op.name) {
            return Err(GatError::MissingOpMapping(format!(
                "{label}: shared op `{}` is not present in target theory `{}`",
                op.name, target.name,
            )));
        }
        op_map.insert(Arc::clone(&op.name), Arc::clone(&op.name));
    }
    Ok(TheoryMorphism::new(
        format!("{label}_{}_into_{}", shared.name, target.name),
        Arc::clone(&shared.name),
        Arc::clone(&target.name),
        sort_map,
        op_map,
    ))
}

/// Every sort name a theory's signatures use without declaring it, in
/// ascending order.
pub(crate) fn undeclared_sort_names(theory: &Theory) -> BTreeSet<Arc<str>> {
    let mut names = BTreeSet::new();
    let mut note = |expr: &SortExpr| {
        if !theory.has_sort(expr.head()) {
            names.insert(Arc::clone(expr.head()));
        }
    };
    for sort in &theory.sorts {
        for param in &sort.params {
            note(&param.sort);
        }
    }
    for op in &theory.ops {
        for (_, sort, _) in &op.inputs {
            note(sort);
        }
        note(&op.output);
    }
    names
}

/// How [`compose`] decides which symbols to identify.
///
/// Serialized in `snake_case` (`"amalgamate"`, `"explicit_identity"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompositionMode {
    /// Identify what the diagram's morphisms relate and, in addition,
    /// every two same-kind symbols with the same name. This is the
    /// convention of [`crate::colimit`] and [`crate::colimit_by_name`],
    /// kept as a compatibility mode; it agrees with
    /// [`CompositionMode::ExplicitIdentity`] on name-clash-free diagrams.
    Amalgamate,
    /// Identify exactly what the diagram's morphisms relate. Same-name
    /// symbols the morphisms do not relate stay distinct and are named by
    /// the [`ClashPolicy`].
    ExplicitIdentity,
}

/// What [`compose`] does when two classes of one kind would share a name.
///
/// Applies in [`CompositionMode::ExplicitIdentity`] only; in
/// [`CompositionMode::Amalgamate`] same-name symbols are identified, so no
/// two classes share a name.
///
/// Serialized in `snake_case` (`"freshen"`, `"reject"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClashPolicy {
    /// Rename every clashing class to `"{least member label}.{name}"`,
    /// then resolve any remaining collision by appending `#1`, `#2`, ...
    /// in ascending order of class key.
    Freshen,
    /// Fail with [`CompositionError::NameClash`].
    Reject,
}

/// The options of one [`compose`] call.
///
/// There is no default: a caller names the mode, so the semantics of a
/// composition is always visible where it is requested.
///
/// ```compile_fail
/// let _ = panproto_gat::ComposeOptions::default();
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeOptions {
    /// Which symbols to identify.
    pub mode: CompositionMode,
    /// The name of the result theory.
    pub result_name: Arc<str>,
    /// What to do when two classes would get the same name.
    pub clash: ClashPolicy,
    /// The number of rewrite steps [`Composition::verify_universal`] may
    /// spend deciding one equality before it reports the check as
    /// incomplete.
    pub rewrite_budget: u32,
}

// ─── results ───────────────────────────────────────────────────────

/// The colimit of a [`TheoryDiagram`], with its cocone, provenance and
/// report.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Composition {
    /// The result theory.
    pub theory: Theory,
    /// One morphism per node, in `diagram.nodes` order, from the node's
    /// theory to the result, sending each symbol to its class.
    pub inclusions: Vec<TheoryMorphism>,
    /// Where every source element went.
    pub provenance: Provenance,
    /// Every check that ran and its outcome.
    pub report: CompositionReport,
    /// The rewrite budget of the options the composition ran with, used by
    /// [`Composition::verify_universal`]. Not serialized: a decoded
    /// composition verifies with [`DEFAULT_VERIFY_BUDGET`].
    #[serde(skip, default = "default_verify_budget")]
    rewrite_budget: u32,
}

/// The rewrite budget a decoded [`Composition`] verifies with; it is the
/// step budget [`crate::check_morphism`] uses for equation preservation.
pub const DEFAULT_VERIFY_BUDGET: u32 = 512;

const fn default_verify_budget() -> u32 {
    DEFAULT_VERIFY_BUDGET
}

impl PartialEq for Composition {
    fn eq(&self, other: &Self) -> bool {
        self.theory == other.theory
            && self.inclusions == other.inclusions
            && self.provenance == other.provenance
            && self.report == other.report
    }
}

impl Eq for Composition {}

impl Composition {
    /// The rewrite budget [`Composition::verify_universal`] spends on one
    /// equality.
    #[must_use]
    pub const fn rewrite_budget(&self) -> u32 {
        self.rewrite_budget
    }
}

/// An equation [`compose`] added to identify an operation with the derived
/// term a diagram edge sends it to.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct AddedEquation {
    /// The edge's position in the canonical edge order: edges sorted by
    /// source label, target label, then the morphism's canonical bytes.
    pub edge: usize,
    /// The operation the edge sends to a term.
    pub op: SymRef,
}

/// Where every source element of a composition went.
///
/// Each map is total over its kind: every sort (including a sort a node
/// uses without declaring), operation, equation, directed equation and
/// policy of every node has exactly one entry, whose value is the name of
/// its image in the result.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// Sorts.
    #[serde(with = "crate::serde_helpers::btree_map_as_vec")]
    pub sorts: BTreeMap<SymRef, Arc<str>>,
    /// Operations.
    #[serde(with = "crate::serde_helpers::btree_map_as_vec")]
    pub ops: BTreeMap<SymRef, Arc<str>>,
    /// Equations.
    #[serde(with = "crate::serde_helpers::btree_map_as_vec")]
    pub eqs: BTreeMap<SymRef, Arc<str>>,
    /// Directed equations.
    #[serde(with = "crate::serde_helpers::btree_map_as_vec")]
    pub directed_eqs: BTreeMap<SymRef, Arc<str>>,
    /// Conflict policies.
    #[serde(with = "crate::serde_helpers::btree_map_as_vec")]
    pub policies: BTreeMap<SymRef, Arc<str>>,
    /// Equations the composition added, by result name.
    pub added_eqs: BTreeMap<Arc<str>, AddedEquation>,
}

impl Provenance {
    /// The map for one kind.
    #[must_use]
    pub const fn map(&self, kind: SymKind) -> &BTreeMap<SymRef, Arc<str>> {
        match kind {
            SymKind::Sort => &self.sorts,
            SymKind::Op => &self.ops,
            SymKind::Eq => &self.eqs,
            SymKind::DirectedEq => &self.directed_eqs,
            SymKind::Policy => &self.policies,
        }
    }

    const fn map_mut(&mut self, kind: SymKind) -> &mut BTreeMap<SymRef, Arc<str>> {
        match kind {
            SymKind::Sort => &mut self.sorts,
            SymKind::Op => &mut self.ops,
            SymKind::Eq => &mut self.eqs,
            SymKind::DirectedEq => &mut self.directed_eqs,
            SymKind::Policy => &mut self.policies,
        }
    }

    /// Every source element of `kind` whose image is `result_name`, in
    /// ascending order.
    #[must_use]
    pub fn members(&self, kind: SymKind, result_name: &str) -> Vec<SymRef> {
        self.map(kind)
            .iter()
            .filter(|(_, image)| image.as_ref() == result_name)
            .map(|(source, _)| source.clone())
            .collect()
    }

    /// The name of the image of one source element.
    #[must_use]
    pub fn image(&self, kind: SymKind, source: &SymRef) -> Option<&Arc<str>> {
        self.map(kind).get(source)
    }

    /// The schema-level renames the node labeled `label` undergoes: its
    /// sorts become vertex kinds and its operations edge kinds, as
    /// [`TheoryMorphism::induce_schema_renames`] reads its inclusion.
    /// Sorted by site, then old name.
    #[must_use]
    pub fn site_renames(&self, label: &str) -> Vec<SiteRename> {
        let of_node = |map: &BTreeMap<SymRef, Arc<str>>| -> HashMap<Arc<str>, Arc<str>> {
            map.iter()
                .filter(|(source, _)| source.node.as_ref() == label)
                .map(|(source, image)| (Arc::clone(&source.name), Arc::clone(image)))
                .collect()
        };
        let inclusion = TheoryMorphism::new(
            format!("incl_{label}"),
            label,
            "composition",
            of_node(&self.sorts),
            of_node(&self.ops),
        );
        let mut renames = inclusion.induce_schema_renames();
        renames.sort_by(|a, b| {
            (format!("{:?}", a.site), &a.old).cmp(&(format!("{:?}", b.site), &b.old))
        });
        renames
    }
}

/// One renaming [`ClashPolicy::Freshen`] made.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Freshening {
    /// The kind of the renamed class.
    pub kind: SymKind,
    /// The class's members, ascending.
    pub members: Vec<SymRef>,
    /// The name the naming rule chose before the clash.
    pub candidate: Arc<str>,
    /// The name the class got.
    pub result: Arc<str>,
}

// ─── reports ───────────────────────────────────────────────────────

/// A check a composition report can record.
///
/// Serialized in `snake_case`. The derived order is the declaration
/// order, which is the order of a report's records.
///
/// | Check | Subject | Recorded |
/// |---|---|---|
/// | `LegIsMorphism` | `Edge(i)` | by every [`compose`], one per edge: the edge's morphism passes [`crate::check_morphism`] |
/// | `NodeWellFormed` | `Node(label)` | by every [`compose`], one per node: the node passes [`crate::typecheck_signature`], up to sorts it uses without declaring |
/// | `ClassSignaturesAgree` | `Diagram` | by every [`compose`]: the members of every class agree on their signatures modulo the identification |
/// | `CoconeCommutes` | `Edge(i)` | by every [`compose`], one per edge: the edge's square commutes, `PassedByEquation` when an equation [`compose`] added closes it |
/// | `InclusionIsMorphism` | `Node(label)` | by every [`compose`], one per node; `Inapplicable` when the node uses a sort no node declares |
/// | `EquationImage` | `Node(label)` | by every [`compose`], one per node: each of the node's equations has its image in the result; `Passed` for a node without equations |
/// | `EquationEntailed` | `Diagram` | by [`Composition::verify_universal`] only: the mediator preserves the result's equations; `Inapplicable` when neither the result nor the target theory has equations |
/// | `UniversalIdentity` | `Diagram` | by every [`compose`]: the canonical cocone factors through the result by the identity; `Inapplicable` when a node uses a sort no node declares |
/// | `UniversalAgainst` | `Diagram`, `Edge(i)`, `Cocone(i)` | by [`Composition::verify_universal`] only: the mediator is a morphism (`Diagram`), each edge's square commutes in the target (`Edge(i)`), and the mediator factors leg `i` (`Cocone(i)`) |
/// | `ResultWellTyped` | `Diagram` | by every [`compose`]: the result passes [`crate::typecheck_signature`] and [`crate::typecheck_theory`] |
/// | `NameClashFree` | `Diagram` | by every [`compose`]: no two nodes share a same-kind name the diagram's morphisms do not relate |
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompositionCheckKind {
    /// Each edge's morphism is a theory morphism.
    LegIsMorphism,
    /// Each node's signature is well formed.
    NodeWellFormed,
    /// The members of every class agree on their signatures.
    ClassSignaturesAgree,
    /// Each edge's square commutes.
    CoconeCommutes,
    /// Each inclusion is a theory morphism.
    InclusionIsMorphism,
    /// Each node's equations have their images in the result.
    EquationImage,
    /// The mediator preserves the result's equations.
    EquationEntailed,
    /// The canonical cocone factors through the identity.
    UniversalIdentity,
    /// An alternative cocone factors through the mediator.
    UniversalAgainst,
    /// The result is well typed.
    ResultWellTyped,
    /// The diagram is name-clash free.
    NameClashFree,
}

/// What a [`CheckRecord`] is about.
///
/// Serialized externally tagged in `snake_case`; `Diagram` is the bare
/// string `"diagram"`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckSubject {
    /// The diagram as a whole.
    Diagram,
    /// The node with this label.
    Node(Arc<str>),
    /// The edge at this position of `diagram.edges`.
    Edge(usize),
    /// The leg at this position of an alternative cocone.
    Cocone(usize),
}

/// Why a check did not apply.
///
/// Serialized externally tagged in `snake_case`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InapplicableReason {
    /// Some node uses sorts that no node declares, so the result is not a
    /// closed theory and a morphism out of it cannot be total.
    OpenTheory {
        /// The undeclared sort names, ascending.
        missing_sorts: Vec<Arc<str>>,
    },
    /// Neither the result nor the target theory has an equation to check.
    NoEquations,
}

/// Why a check did not complete.
///
/// Serialized externally tagged in `snake_case`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompositionIncomplete {
    /// Deciding an equality by rewriting ran out of steps.
    RewriteBudgetExhausted {
        /// The equation, or class, being decided.
        equation: Arc<str>,
        /// The budget spent.
        steps: u32,
    },
    /// The target theory has no terminating rewrite system to decide with.
    NoTerminatingRewriteSystem,
}

/// The outcome of one check.
///
/// Serialized externally tagged in `snake_case`; `Passed` is the bare
/// string `"passed"`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckOutcome {
    /// The check passed.
    Passed,
    /// The check passed because an equation the composition added makes it
    /// hold.
    PassedByEquation {
        /// That equation's name.
        equation: Arc<str>,
    },
    /// The check failed.
    Failed {
        /// Why.
        detail: String,
    },
    /// The check did not apply.
    Inapplicable {
        /// Why.
        reason: InapplicableReason,
    },
    /// The check did not complete.
    Incomplete {
        /// Why.
        reason: CompositionIncomplete,
    },
}

impl CheckOutcome {
    /// The verdict this outcome contributes to a report's status: a pass
    /// by equation is a pass, and a check that did not apply established
    /// nothing, so it counts as incomplete.
    #[must_use]
    pub const fn status(&self) -> CheckStatus {
        match self {
            Self::Passed | Self::PassedByEquation { .. } => CheckStatus::Passed,
            Self::Failed { .. } => CheckStatus::Failed,
            Self::Inapplicable { .. } | Self::Incomplete { .. } => CheckStatus::Incomplete,
        }
    }
}

/// One check and its outcome.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CheckRecord {
    /// The check.
    pub check: CompositionCheckKind,
    /// What it was about.
    pub subject: CheckSubject,
    /// Its outcome.
    pub outcome: CheckOutcome,
}

/// Every check a composition ran, and the verdict they add up to.
///
/// `status` is stored, and equals [`CheckStatus::all`] over the records'
/// [`CheckOutcome::status`]; decoding recomputes it and rejects a report
/// whose stored status differs. `checks` is sorted by `(check, subject)`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CompositionReport {
    /// The mode the composition ran in.
    pub mode: CompositionMode,
    /// The conjunction of every record's verdict.
    pub status: CheckStatus,
    /// Whether no two nodes share a same-kind name that the diagram's
    /// morphisms do not relate (mossakowskirabecodescu2017canonical Def. 5,
    /// stated).
    pub name_clash_free: bool,
    /// The records, sorted by `(check, subject)`.
    pub checks: Vec<CheckRecord>,
    /// Every renaming [`ClashPolicy::Freshen`] made.
    pub freshened: Vec<Freshening>,
}

impl CompositionReport {
    /// Build a report, sorting the records and computing the status.
    #[must_use]
    pub fn new(
        mode: CompositionMode,
        name_clash_free: bool,
        mut checks: Vec<CheckRecord>,
        mut freshened: Vec<Freshening>,
    ) -> Self {
        checks.sort();
        freshened.sort();
        let status = CheckStatus::all(checks.iter().map(|r| r.outcome.status()));
        Self {
            mode,
            status,
            name_clash_free,
            checks,
            freshened,
        }
    }

    /// The records of one check, in subject order.
    pub fn records(&self, check: CompositionCheckKind) -> impl Iterator<Item = &CheckRecord> {
        self.checks.iter().filter(move |r| r.check == check)
    }
}

impl<'de> Deserialize<'de> for CompositionReport {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            mode: CompositionMode,
            status: CheckStatus,
            name_clash_free: bool,
            checks: Vec<CheckRecord>,
            freshened: Vec<Freshening>,
        }
        let raw = Raw::deserialize(deserializer)?;
        let status = CheckStatus::all(raw.checks.iter().map(|r| r.outcome.status()));
        if status != raw.status {
            return Err(serde::de::Error::custom(format_args!(
                "composition report status is {:?} but its checks add up to {status:?}",
                raw.status
            )));
        }
        Ok(Self {
            mode: raw.mode,
            status,
            name_clash_free: raw.name_clash_free,
            checks: raw.checks,
            freshened: raw.freshened,
        })
    }
}

// ─── errors ────────────────────────────────────────────────────────

/// Why a composition was refused.
///
/// Serialized externally tagged in `snake_case`, for instance
/// `{"unknown_node_label": {"label": "T3"}}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompositionError {
    /// Two nodes share a label.
    DuplicateNodeLabel {
        /// The label.
        label: Arc<str>,
    },
    /// A label is empty or contains `.` or `#`.
    InvalidNodeLabel {
        /// The label.
        label: Arc<str>,
    },
    /// An edge names a node that is not in the diagram.
    UnknownNodeLabel {
        /// The label.
        label: Arc<str>,
    },
    /// A node's signature is ill formed beyond using undeclared sorts.
    InvalidNode {
        /// The node.
        label: Arc<str>,
        /// Its defects, other than undeclared sorts.
        defects: Vec<SignatureDefect>,
    },
    /// An edge's morphism is not a morphism between its endpoints.
    InvalidLeg {
        /// The edge's position in `diagram.edges`.
        edge: usize,
        /// The morphism check's error, rendered.
        detail: String,
    },
    /// Two sorts the diagram identifies disagree on their signatures.
    SortSignatureMismatch {
        /// One member.
        first: SymRef,
        /// Another member.
        second: SymRef,
        /// How they disagree.
        detail: SortMismatch,
    },
    /// Two operations the diagram identifies disagree on their signatures.
    OpSignatureMismatch {
        /// One member.
        first: SymRef,
        /// Another member.
        second: SymRef,
    },
    /// Two operations the diagram identifies could only be identified by
    /// an equation a theory cannot state.
    QuotientNotRepresentable {
        /// One member.
        first: SymRef,
        /// Another member.
        second: SymRef,
        /// What would be needed.
        reason: NotRepresentableReason,
    },
    /// In [`CompositionMode::Amalgamate`], two same-name elements that only
    /// the name rule relates are incompatible.
    NameConflict {
        /// Their kind.
        kind: SymKind,
        /// Their name.
        name: Arc<str>,
        /// One of them.
        first: SymRef,
        /// The other.
        second: SymRef,
    },
    /// In [`CompositionMode::ExplicitIdentity`] with [`ClashPolicy::Reject`],
    /// distinct classes would get the same name.
    NameClash {
        /// Their kind.
        kind: SymKind,
        /// The name.
        name: Arc<str>,
        /// The clashing classes, each ascending, in ascending order.
        classes: Vec<Vec<SymRef>>,
    },
    /// An equation translated into the result does not typecheck there.
    IllTypedTransport {
        /// The source equation, or for an added equation the operation it
        /// identifies.
        source: SymRef,
        /// The typechecker's error, rendered.
        detail: String,
    },
    /// The result's signature is ill formed, or a check of
    /// [`crate::typecheck_theory`] beyond the signature and the equations
    /// failed, in which case `defects` is empty.
    IllTypedResult {
        /// The signature defects.
        defects: Vec<SignatureDefect>,
    },
    /// An alternative cocone sends the members of one class to different
    /// images, so no mediator exists.
    NotACocone {
        /// The class, ascending.
        class: Vec<SymRef>,
    },
}

impl std::fmt::Display for CompositionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateNodeLabel { label } => write!(f, "duplicate node label `{label}`"),
            Self::InvalidNodeLabel { label } => write!(
                f,
                "invalid node label `{label}`: labels are non-empty and contain neither `.` nor `#`"
            ),
            Self::UnknownNodeLabel { label } => write!(f, "edge names unknown node `{label}`"),
            Self::InvalidNode { label, defects } => {
                write!(f, "node `{label}` is ill formed: ")?;
                write_joined(f, defects)
            }
            Self::InvalidLeg { edge, detail } => {
                write!(f, "edge {edge} is not a theory morphism: {detail}")
            }
            Self::SortSignatureMismatch {
                first,
                second,
                detail,
            } => write!(f, "cannot identify sorts {first} and {second}: {detail}"),
            Self::OpSignatureMismatch { first, second } => write!(
                f,
                "cannot identify operations {first} and {second}: their signatures differ"
            ),
            Self::QuotientNotRepresentable {
                first,
                second,
                reason,
            } => match reason {
                NotRepresentableReason::NeedsSortEquation => write!(
                    f,
                    "identifying operations {first} and {second} needs a sort equation, \
                     which a theory cannot state"
                ),
            },
            Self::NameConflict {
                kind,
                name,
                first,
                second,
            } => write!(
                f,
                "{kind} `{name}` is declared incompatibly by {first} and {second}"
            ),
            Self::NameClash {
                kind,
                name,
                classes,
            } => write!(
                f,
                "{} distinct {kind} classes would be named `{name}`",
                classes.len()
            ),
            Self::IllTypedTransport { source, detail } => {
                write!(f, "equation {source} is ill typed in the result: {detail}")
            }
            Self::IllTypedResult { defects } => {
                f.write_str("the composed theory is ill typed")?;
                if defects.is_empty() {
                    Ok(())
                } else {
                    f.write_str(": ")?;
                    write_joined(f, defects)
                }
            }
            Self::NotACocone { class } => {
                f.write_str("the legs are not a cocone: they send the members of {")?;
                write_joined(f, class)?;
                f.write_str("} to different images")
            }
        }
    }
}

impl std::error::Error for CompositionError {}

/// Write `items` separated by `; `.
fn write_joined<T: std::fmt::Display>(
    f: &mut std::fmt::Formatter<'_>,
    items: &[T],
) -> std::fmt::Result {
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            f.write_str("; ")?;
        }
        write!(f, "{item}")?;
    }
    Ok(())
}

impl From<CompositionError> for GatError {
    fn from(err: CompositionError) -> Self {
        Self::Composition(Box::new(err))
    }
}

// ─── construction ──────────────────────────────────────────────────

/// A symbol of the diagram: its kind and its node-qualified name.
type Key = (SymKind, SymRef);

/// How [`compose_with_naming`] names result classes.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Naming<'a> {
    /// The naming rule of [`compose`]: the most frequent member name, ties
    /// to the least by bytes.
    Majority,
    /// Name a class after its members in the first of these nodes that has
    /// any, by the same rule restricted to those members; a class with none
    /// falls back to [`Naming::Majority`]. The amalgamation wrappers use it
    /// to keep the names their pushouts have always had.
    PreferNodes(&'a [Arc<str>]),
}

/// One node, validated.
struct NodeInfo<'a> {
    /// The theory.
    theory: &'a Theory,
    /// Sort names the theory uses without declaring.
    implicit: BTreeSet<Arc<str>>,
}

impl NodeInfo<'_> {
    fn has_sort_symbol(&self, name: &str) -> bool {
        self.theory.has_sort(name) || self.implicit.contains(name)
    }
}

/// Compute the colimit of a diagram of theories.
///
/// The construction:
///
/// 1. The diagram is validated: labels are unique and well formed, edge
///    endpoints exist, every node passes [`crate::typecheck_signature`] up
///    to sorts it uses without declaring, and every edge's morphism passes
///    [`crate::check_morphism`] between its endpoints.
/// 2. Every node's sorts and operations form one disjoint union per kind.
///    A sort name a node uses without declaring is a symbol of that node
///    too; it is joined by name to every same-name sort another node
///    declares, and when none does it stays a free name of the result,
///    joined by name with the other nodes' uses of it.
/// 3. The edges identify each sort with its image and each operation with
///    the operation it is renamed to; in [`CompositionMode::Amalgamate`],
///    same-kind symbols with the same name are identified as well.
/// 4. Every class's members must agree on arity, parameter sort classes
///    (modulo renaming the parameters), kind and closure (sorts), or on
///    their signature once every sort and operation is sent to its class
///    (operations). A mismatch only in the term arguments of dependent
///    sorts would need a sort equation and is
///    [`CompositionError::QuotientNotRepresentable`].
/// 5. An edge that sends an operation `o` to a derived term `t` adds the
///    equation `o(params) = t` under the name
///    `"{src label}.{o}~{k}"`, with `k` the edge's position in the
///    canonical edge order (source label, target label, canonical bytes
///    of the morphism); isaev2017algebraic Prop. 2.1 and
///    schultzwisnesky2017algebraic §6.1 build coequalizers this way in the
///    non-dependent setting (stated), and carrying it to dependent sorts is
///    inferred.
/// 6. Each class becomes one sort or operation. Equations, directed
///    equations and policies are translated and collected as sets by
///    content, keeping every source in the provenance.
/// 7. The result must pass [`crate::typecheck_signature`] (ignoring free
///    names) and every check of [`crate::typecheck_theory`].
///
/// Class names: the result theory is named `opts.result_name`; a class is
/// named by the member name that occurs most often among its members, ties
/// going to the least name by bytes (majority names in the sense of
/// mossakowskirabecodescu2017canonical Def. 10, stated; the tiebreak is
/// ours). Clashes, possible only in [`CompositionMode::ExplicitIdentity`],
/// follow `opts.clash`. Result vectors are in ascending name order, so the
/// result does not depend on the order of the diagram's nodes or edges.
///
/// # Errors
///
/// Returns a [`CompositionError`] naming the first obstruction found.
pub fn compose(
    diagram: &TheoryDiagram,
    opts: &ComposeOptions,
) -> Result<Composition, CompositionError> {
    compose_with_naming(diagram, opts, Naming::Majority)
}

/// [`compose`] with a choice of naming rule.
pub(crate) fn compose_with_naming(
    diagram: &TheoryDiagram,
    opts: &ComposeOptions,
    naming: Naming<'_>,
) -> Result<Composition, CompositionError> {
    let mut records: Vec<CheckRecord> = Vec::new();
    let nodes = validate_nodes(diagram, &mut records)?;
    validate_edges(diagram, &nodes, &mut records)?;

    let (mut edge_uf, sort_classes, op_classes) = identify(diagram, &nodes, opts.mode);
    check_classes(&sort_classes, &op_classes, &nodes, opts, &mut edge_uf)?;
    records.push(record(
        CompositionCheckKind::ClassSignaturesAgree,
        CheckSubject::Diagram,
        CheckOutcome::Passed,
    ));

    let mut freshened: Vec<Freshening> = Vec::new();
    let symbols = Symbols::name(
        sort_classes,
        op_classes,
        &nodes,
        opts,
        naming,
        &mut freshened,
    )?;
    let order = canonical_edge_order(diagram);
    let added = added_equations(diagram, &order, &nodes, symbols.op_map());
    let elements = Elements::collect(&nodes, &symbols, &added, opts, naming, &mut freshened)?;
    let (theory, provenance) = assemble(&opts.result_name, &nodes, &symbols, &elements, &added);

    gate_result(&theory, &symbols.free_names, &elements, &added)?;
    records.push(record(
        CompositionCheckKind::ResultWellTyped,
        CheckSubject::Diagram,
        CheckOutcome::Passed,
    ));

    let inclusions = build_inclusions(diagram, &nodes, &symbols, &opts.result_name);
    let cocone = Cocone {
        diagram,
        nodes: &nodes,
        symbols: &symbols,
        theory: &theory,
        provenance: &provenance,
        added: &added,
    };
    cocone.record_squares(&mut records);
    cocone.record_inclusions(&inclusions, &mut records);
    cocone.record_universal_identity(&mut records);
    let clash_free = name_clash_free(&nodes, &mut edge_uf, &provenance);
    records.push(record(
        CompositionCheckKind::NameClashFree,
        CheckSubject::Diagram,
        clash_free
            .as_ref()
            .map_or(CheckOutcome::Passed, |detail| CheckOutcome::Failed {
                detail: detail.clone(),
            }),
    ));

    Ok(Composition {
        theory,
        inclusions,
        provenance,
        report: CompositionReport::new(opts.mode, clash_free.is_none(), records, freshened),
        rewrite_budget: opts.rewrite_budget,
    })
}

/// Steps 2 and 3: the union-find over node-qualified symbols. Returns the
/// relation the edges generate (with undeclared sorts joined by name),
/// and the sort and operation classes of the full identification.
fn identify(
    diagram: &TheoryDiagram,
    nodes: &BTreeMap<Arc<str>, NodeInfo<'_>>,
    mode: CompositionMode,
) -> (UnionFind<Key>, Vec<Vec<SymRef>>, Vec<Vec<SymRef>>) {
    let mut uf: UnionFind<Key> = UnionFind::new();
    for (label, info) in nodes {
        let sym = |name: &Arc<str>| SymRef::new(Arc::clone(label), Arc::clone(name));
        for sort in &info.theory.sorts {
            uf.insert((SymKind::Sort, sym(&sort.name)));
        }
        for name in &info.implicit {
            uf.insert((SymKind::Sort, sym(name)));
        }
        for op in &info.theory.ops {
            uf.insert((SymKind::Op, sym(&op.name)));
        }
    }
    for edge in &diagram.edges {
        let (src, tgt) = (&nodes[&edge.src], &nodes[&edge.tgt]);
        let at =
            |label: &Arc<str>, name: &Arc<str>| SymRef::new(Arc::clone(label), Arc::clone(name));
        for (s, t) in &edge.morphism.sort_map {
            if src.has_sort_symbol(s) && tgt.has_sort_symbol(t) {
                uf.union(
                    &(SymKind::Sort, at(&edge.src, s)),
                    &(SymKind::Sort, at(&edge.tgt, t)),
                );
            }
        }
        for (o, image) in &edge.morphism.op_map {
            if let (true, OpAssignment::Op(t)) = (src.theory.has_op(o), image) {
                uf.union(
                    &(SymKind::Op, at(&edge.src, o)),
                    &(SymKind::Op, at(&edge.tgt, t)),
                );
            }
        }
    }
    join_implicit_sorts(nodes, &mut uf);
    let edge_uf = uf.clone();
    if mode == CompositionMode::Amalgamate {
        join_same_names(&mut uf);
    }
    let classes = uf.classes();
    let sort_classes = kind_classes(&classes, SymKind::Sort);
    let op_classes = kind_classes(&classes, SymKind::Op);
    (edge_uf, sort_classes, op_classes)
}

/// Step 4: tier-1 class checks, in tokens that name classes independently
/// of how they will be named.
fn check_classes(
    sort_classes: &[Vec<SymRef>],
    op_classes: &[Vec<SymRef>],
    nodes: &BTreeMap<Arc<str>, NodeInfo<'_>>,
    opts: &ComposeOptions,
    edge_uf: &mut UnionFind<Key>,
) -> Result<(), CompositionError> {
    let tokens = ClassMaps {
        sorts: class_maps(sort_classes, "\u{1}s"),
        ops: class_maps(op_classes, "\u{1}o"),
    };
    check_sort_classes(sort_classes, nodes, &tokens, opts, edge_uf)?;
    check_op_classes(op_classes, nodes, &tokens, opts, edge_uf)
}

/// Per node, the class map of its sorts and of its operations.
struct ClassMaps {
    /// Sorts, by node label.
    sorts: BTreeMap<Arc<str>, HashMap<Arc<str>, Arc<str>>>,
    /// Operations, by node label.
    ops: BTreeMap<Arc<str>, HashMap<Arc<str>, Arc<str>>>,
}

impl ClassMaps {
    fn sorts_of(&self, label: &str) -> &HashMap<Arc<str>, Arc<str>> {
        node_map(&self.sorts, label)
    }

    fn ops_of(&self, label: &str) -> &HashMap<Arc<str>, Arc<str>> {
        node_map(&self.ops, label)
    }
}

/// The sort and operation classes with their result names.
struct Symbols {
    /// Sort classes, each ascending.
    sort_classes: Vec<Vec<SymRef>>,
    /// Operation classes, each ascending.
    op_classes: Vec<Vec<SymRef>>,
    /// The result name of each sort class.
    sort_names: Vec<Arc<str>>,
    /// The result name of each operation class.
    op_names: Vec<Arc<str>>,
    /// Per node, each member's result name.
    maps: ClassMaps,
    /// Whether each sort class is a free name (no node declares it).
    free_classes: Vec<bool>,
    /// The result names of the free sort classes.
    free_names: BTreeSet<Arc<str>>,
}

impl Symbols {
    /// Name the sort and operation classes.
    fn name(
        sort_classes: Vec<Vec<SymRef>>,
        op_classes: Vec<Vec<SymRef>>,
        nodes: &BTreeMap<Arc<str>, NodeInfo<'_>>,
        opts: &ComposeOptions,
        naming: Naming<'_>,
        freshened: &mut Vec<Freshening>,
    ) -> Result<Self, CompositionError> {
        let none = BTreeMap::new();
        let sort_names =
            name_classes(SymKind::Sort, &sort_classes, &none, opts, naming, freshened)?;
        let op_names = name_classes(SymKind::Op, &op_classes, &none, opts, naming, freshened)?;
        let maps = ClassMaps {
            sorts: class_maps_named(&sort_classes, &sort_names),
            ops: class_maps_named(&op_classes, &op_names),
        };
        let free_classes: Vec<bool> = sort_classes
            .iter()
            .map(|members| {
                members
                    .iter()
                    .all(|m| !nodes[&m.node].theory.has_sort(&m.name))
            })
            .collect();
        let free_names = sort_names
            .iter()
            .zip(&free_classes)
            .filter(|(_, free)| **free)
            .map(|(name, _)| Arc::clone(name))
            .collect();
        Ok(Self {
            sort_classes,
            op_classes,
            sort_names,
            op_names,
            maps,
            free_classes,
            free_names,
        })
    }

    /// The operation class maps, by node label.
    const fn op_map(&self) -> &BTreeMap<Arc<str>, HashMap<Arc<str>, Arc<str>>> {
        &self.maps.ops
    }
}

/// Equations, directed equations and policies, grouped and named.
struct Elements {
    /// Equation groups.
    eqs: EqGroups,
    /// Their names.
    eq_names: Vec<Arc<str>>,
    /// Directed-equation groups.
    directed: EqGroups,
    /// Their names.
    directed_names: Vec<Arc<str>>,
    /// Policy groups.
    policies: PolicyGroups,
    /// Their names.
    policy_names: Vec<Arc<str>>,
}

impl Elements {
    /// Group and name every node's equations, directed equations and
    /// policies. The names of added equations are reserved.
    fn collect(
        nodes: &BTreeMap<Arc<str>, NodeInfo<'_>>,
        symbols: &Symbols,
        added: &[Added],
        opts: &ComposeOptions,
        naming: Naming<'_>,
        freshened: &mut Vec<Freshening>,
    ) -> Result<Self, CompositionError> {
        let reserved: BTreeMap<Arc<str>, SymRef> = added
            .iter()
            .map(|a| (Arc::clone(&a.equation.name), a.source.op.clone()))
            .collect();
        let none = BTreeMap::new();
        let eqs = group_equations(nodes, symbols.op_map(), false);
        let directed = group_equations(nodes, symbols.op_map(), true);
        let policies = group_policies(nodes);
        let eq_names = name_content_groups(
            SymKind::Eq,
            &eqs.members,
            &reserved,
            opts,
            naming,
            freshened,
        )?;
        let directed_names = name_content_groups(
            SymKind::DirectedEq,
            &directed.members,
            &none,
            opts,
            naming,
            freshened,
        )?;
        let policy_names = name_content_groups(
            SymKind::Policy,
            &policies.members,
            &none,
            opts,
            naming,
            freshened,
        )?;
        Ok(Self {
            eqs,
            eq_names,
            directed,
            directed_names,
            policies,
            policy_names,
        })
    }
}

/// Step 6: the result theory, and the provenance of every source element.
fn assemble(
    result_name: &Arc<str>,
    nodes: &BTreeMap<Arc<str>, NodeInfo<'_>>,
    symbols: &Symbols,
    elements: &Elements,
    added: &[Added],
) -> (Theory, Provenance) {
    let mut provenance = Provenance::default();
    for (kind, classes, names) in [
        (SymKind::Sort, &symbols.sort_classes, &symbols.sort_names),
        (SymKind::Op, &symbols.op_classes, &symbols.op_names),
        (SymKind::Eq, &elements.eqs.members, &elements.eq_names),
        (
            SymKind::DirectedEq,
            &elements.directed.members,
            &elements.directed_names,
        ),
        (
            SymKind::Policy,
            &elements.policies.members,
            &elements.policy_names,
        ),
    ] {
        for (members, name) in classes.iter().zip(names) {
            for m in members {
                provenance.map_mut(kind).insert(m.clone(), Arc::clone(name));
            }
        }
    }
    let mut eqs: Vec<Equation> = elements
        .eqs
        .reps
        .iter()
        .zip(&elements.eq_names)
        .map(|(eq, name)| Equation {
            name: Arc::clone(name),
            lhs: eq.lhs.clone(),
            rhs: eq.rhs.clone(),
        })
        .collect();
    for a in added {
        provenance
            .added_eqs
            .insert(Arc::clone(&a.equation.name), a.source.clone());
        eqs.push(a.equation.clone());
    }
    let directed_eqs: Vec<DirectedEquation> = elements
        .directed
        .directed_reps
        .iter()
        .zip(&elements.directed_names)
        .map(|(de, name)| DirectedEquation {
            name: Arc::clone(name),
            ..de.clone()
        })
        .collect();
    let policies: Vec<ConflictPolicy> = elements
        .policies
        .reps
        .iter()
        .zip(&elements.policy_names)
        .map(|(p, name)| ConflictPolicy {
            name: Arc::clone(name),
            ..p.clone()
        })
        .collect();
    let theory = Theory::full(
        Arc::clone(result_name),
        Vec::new(),
        sorted_by_name(build_sorts(symbols, nodes), |s| &s.name),
        sorted_by_name(build_ops(symbols, nodes), |o| &o.name),
        sorted_by_name(eqs, |e| &e.name),
        sorted_by_name(directed_eqs, |e| &e.name),
        sorted_by_name(policies, |p| &p.name),
    );
    (theory, provenance)
}

/// A check record.
const fn record(
    check: CompositionCheckKind,
    subject: CheckSubject,
    outcome: CheckOutcome,
) -> CheckRecord {
    CheckRecord {
        check,
        subject,
        outcome,
    }
}

/// Sort a vector by the name `key` reads off each element.
fn sorted_by_name<T>(mut items: Vec<T>, key: impl Fn(&T) -> &Arc<str>) -> Vec<T> {
    items.sort_by(|a, b| key(a).cmp(key(b)));
    items
}

/// Step 1 for nodes: labels, and each node's signature up to undeclared
/// sorts. Returns the nodes keyed by label.
fn validate_nodes<'a>(
    diagram: &'a TheoryDiagram,
    records: &mut Vec<CheckRecord>,
) -> Result<BTreeMap<Arc<str>, NodeInfo<'a>>, CompositionError> {
    let mut nodes: BTreeMap<Arc<str>, NodeInfo<'a>> = BTreeMap::new();
    for node in &diagram.nodes {
        let label = &node.label;
        if label.is_empty() || label.contains('.') || label.contains('#') {
            return Err(CompositionError::InvalidNodeLabel {
                label: Arc::clone(label),
            });
        }
        if nodes.contains_key(label) {
            return Err(CompositionError::DuplicateNodeLabel {
                label: Arc::clone(label),
            });
        }
        nodes.insert(
            Arc::clone(label),
            NodeInfo {
                theory: &node.theory,
                implicit: undeclared_sort_names(&node.theory),
            },
        );
    }
    for edge in &diagram.edges {
        for label in [&edge.src, &edge.tgt] {
            if !nodes.contains_key(label) {
                return Err(CompositionError::UnknownNodeLabel {
                    label: Arc::clone(label),
                });
            }
        }
    }
    for node in &diagram.nodes {
        if let Err(defects) = typecheck_signature(&node.theory) {
            let rejected: Vec<SignatureDefect> = defects
                .into_iter()
                .filter(|d| {
                    d.undeclared_sort()
                        .is_none_or(|sort| node.theory.has_sort(sort))
                })
                .collect();
            if !rejected.is_empty() {
                return Err(CompositionError::InvalidNode {
                    label: Arc::clone(&node.label),
                    defects: rejected,
                });
            }
        }
        records.push(record(
            CompositionCheckKind::NodeWellFormed,
            CheckSubject::Node(Arc::clone(&node.label)),
            CheckOutcome::Passed,
        ));
    }
    Ok(nodes)
}

/// Step 1 for edges: every morphism is a morphism between its endpoints,
/// and sends each undeclared sort it maps to a sort its target declares or
/// uses.
fn validate_edges(
    diagram: &TheoryDiagram,
    nodes: &BTreeMap<Arc<str>, NodeInfo<'_>>,
    records: &mut Vec<CheckRecord>,
) -> Result<(), CompositionError> {
    for (index, edge) in diagram.edges.iter().enumerate() {
        let (src, tgt) = (&nodes[&edge.src], &nodes[&edge.tgt]);
        check_morphism(&edge.morphism, src.theory, tgt.theory).map_err(|err| {
            CompositionError::InvalidLeg {
                edge: index,
                detail: err.to_string(),
            }
        })?;
        for name in &src.implicit {
            if let Some(image) = edge.morphism.sort_map.get(name) {
                if !tgt.has_sort_symbol(image) {
                    return Err(CompositionError::InvalidLeg {
                        edge: index,
                        detail: format!(
                            "sort map sends `{name}`, which `{}` uses without declaring, to \
                             `{image}`, which `{}` neither declares nor uses",
                            edge.src, edge.tgt
                        ),
                    });
                }
            }
        }
        records.push(record(
            CompositionCheckKind::LegIsMorphism,
            CheckSubject::Edge(index),
            CheckOutcome::Passed,
        ));
    }
    Ok(())
}

/// Join each undeclared use of a sort name to every other node's
/// declaration of that name, or, when no node declares it, to every other
/// undeclared use of it.
fn join_implicit_sorts(nodes: &BTreeMap<Arc<str>, NodeInfo<'_>>, uf: &mut UnionFind<Key>) {
    let mut declared: BTreeMap<&Arc<str>, Vec<SymRef>> = BTreeMap::new();
    let mut implicit: BTreeMap<&Arc<str>, Vec<SymRef>> = BTreeMap::new();
    for (label, info) in nodes {
        for sort in &info.theory.sorts {
            declared
                .entry(&sort.name)
                .or_default()
                .push(SymRef::new(Arc::clone(label), Arc::clone(&sort.name)));
        }
        for name in &info.implicit {
            implicit
                .entry(name)
                .or_default()
                .push(SymRef::new(Arc::clone(label), Arc::clone(name)));
        }
    }
    for (name, users) in implicit {
        let partners: Vec<&SymRef> = declared
            .get(name)
            .map_or_else(|| users.iter().collect(), |decls| decls.iter().collect());
        for user in &users {
            for partner in &partners {
                uf.union(
                    &(SymKind::Sort, user.clone()),
                    &(SymKind::Sort, (*partner).clone()),
                );
            }
        }
    }
}

/// Identify every two same-kind symbols with the same name.
fn join_same_names(uf: &mut UnionFind<Key>) {
    let keys: Vec<Key> = uf.classes().into_values().flatten().collect();
    let mut first: BTreeMap<(SymKind, Arc<str>), Key> = BTreeMap::new();
    for key in keys {
        let slot = (key.0, Arc::clone(&key.1.name));
        match first.get(&slot) {
            Some(existing) => {
                let existing = existing.clone();
                uf.union(&existing, &key);
            }
            None => {
                first.insert(slot, key);
            }
        }
    }
}

/// The classes of one kind, each ascending, ordered by least member.
fn kind_classes(classes: &BTreeMap<Key, Vec<Key>>, kind: SymKind) -> Vec<Vec<SymRef>> {
    classes
        .iter()
        .filter(|(rep, _)| rep.0 == kind)
        .map(|(_, members)| members.iter().map(|(_, s)| s.clone()).collect())
        .collect()
}

/// Per node, the map from each member's name to a token naming its class.
fn class_maps(
    classes: &[Vec<SymRef>],
    prefix: &str,
) -> BTreeMap<Arc<str>, HashMap<Arc<str>, Arc<str>>> {
    let names: Vec<Arc<str>> = (0..classes.len())
        .map(|i| Arc::from(format!("{prefix}{i}")))
        .collect();
    class_maps_named(classes, &names)
}

/// Per node, the map from each member's name to its class's name.
fn class_maps_named(
    classes: &[Vec<SymRef>],
    names: &[Arc<str>],
) -> BTreeMap<Arc<str>, HashMap<Arc<str>, Arc<str>>> {
    let mut maps: BTreeMap<Arc<str>, HashMap<Arc<str>, Arc<str>>> = BTreeMap::new();
    for (members, name) in classes.iter().zip(names) {
        for m in members {
            maps.entry(Arc::clone(&m.node))
                .or_default()
                .insert(Arc::clone(&m.name), Arc::clone(name));
        }
    }
    maps
}

/// The class map of one node, empty when the node has no symbol of that
/// kind.
fn node_map<'m>(
    maps: &'m BTreeMap<Arc<str>, HashMap<Arc<str>, Arc<str>>>,
    label: &str,
) -> &'m HashMap<Arc<str>, Arc<str>> {
    static EMPTY: std::sync::OnceLock<HashMap<Arc<str>, Arc<str>>> = std::sync::OnceLock::new();
    maps.get(label)
        .unwrap_or_else(|| EMPTY.get_or_init(HashMap::new))
}

/// Tier-1 checks on sort classes.
fn check_sort_classes(
    classes: &[Vec<SymRef>],
    nodes: &BTreeMap<Arc<str>, NodeInfo<'_>>,
    tokens: &ClassMaps,
    opts: &ComposeOptions,
    edge_uf: &mut UnionFind<Key>,
) -> Result<(), CompositionError> {
    for members in classes {
        let entries: Vec<(&SymRef, Option<&Sort>)> = members
            .iter()
            .map(|m| (m, nodes[&m.node].theory.find_sort(&m.name)))
            .collect();
        for (i, (a, a_decl)) in entries.iter().enumerate() {
            for (b, b_decl) in &entries[i + 1..] {
                let mismatch = match (a_decl, b_decl) {
                    (Some(a_decl), Some(b_decl)) => sort_class_mismatch(
                        &translate_sort(a_decl, tokens.sorts_of(&a.node), tokens.ops_of(&a.node)),
                        &translate_sort(b_decl, tokens.sorts_of(&b.node), tokens.ops_of(&b.node)),
                        a_decl,
                        b_decl,
                    ),
                    // A sort a node uses without declaring has arity 0 and
                    // carries nothing else to compare.
                    (Some(decl), None) | (None, Some(decl)) if decl.arity() != 0 => {
                        let (first, second) = if a_decl.is_some() {
                            (decl.arity(), 0)
                        } else {
                            (0, decl.arity())
                        };
                        Some(SortMismatch::Arity { first, second })
                    }
                    _ => None,
                };
                if let Some(detail) = mismatch {
                    return Err(name_rule_conflict(SymKind::Sort, a, b, opts, edge_uf)
                        .unwrap_or_else(|| CompositionError::SortSignatureMismatch {
                            first: (*a).clone(),
                            second: (*b).clone(),
                            detail,
                        }));
                }
            }
        }
    }
    Ok(())
}

/// Tier-1 checks on operation classes.
fn check_op_classes(
    classes: &[Vec<SymRef>],
    nodes: &BTreeMap<Arc<str>, NodeInfo<'_>>,
    tokens: &ClassMaps,
    opts: &ComposeOptions,
    edge_uf: &mut UnionFind<Key>,
) -> Result<(), CompositionError> {
    for members in classes {
        let translated: Vec<(&SymRef, Operation)> = members
            .iter()
            .filter_map(|m| {
                let decl = nodes[&m.node].theory.find_op(&m.name)?;
                Some((
                    m,
                    translate_op(decl, tokens.sorts_of(&m.node), tokens.ops_of(&m.node)),
                ))
            })
            .collect();
        for (i, (a, a_op)) in translated.iter().enumerate() {
            for (b, b_op) in &translated[i + 1..] {
                if let Some(mismatch) = op_class_mismatch(a_op, b_op) {
                    let typed = match mismatch {
                        OpMismatch::NeedsSortEquation => {
                            CompositionError::QuotientNotRepresentable {
                                first: (*a).clone(),
                                second: (*b).clone(),
                                reason: NotRepresentableReason::NeedsSortEquation,
                            }
                        }
                        OpMismatch::Signature => CompositionError::OpSignatureMismatch {
                            first: (*a).clone(),
                            second: (*b).clone(),
                        },
                    };
                    return Err(
                        name_rule_conflict(SymKind::Op, a, b, opts, edge_uf).unwrap_or(typed)
                    );
                }
            }
        }
    }
    Ok(())
}

/// In [`CompositionMode::Amalgamate`], a mismatch between two same-name
/// members that only the name rule relates is a [`CompositionError::NameConflict`].
fn name_rule_conflict(
    kind: SymKind,
    a: &SymRef,
    b: &SymRef,
    opts: &ComposeOptions,
    edge_uf: &mut UnionFind<Key>,
) -> Option<CompositionError> {
    (opts.mode == CompositionMode::Amalgamate
        && a.name == b.name
        && !edge_uf.same(&(kind, a.clone()), &(kind, b.clone())))
    .then(|| CompositionError::NameConflict {
        kind,
        name: Arc::clone(&a.name),
        first: a.clone(),
        second: b.clone(),
    })
}

// ─── naming ────────────────────────────────────────────────────────

/// The most frequent name among `members`, ties to the least by bytes.
fn majority_name<'m>(members: impl IntoIterator<Item = &'m SymRef>) -> Option<Arc<str>> {
    let mut counts: BTreeMap<&Arc<str>, usize> = BTreeMap::new();
    for m in members {
        *counts.entry(&m.name).or_default() += 1;
    }
    let mut best: Option<(&Arc<str>, usize)> = None;
    for (name, count) in counts {
        if best.is_none_or(|(_, c)| count > c) {
            best = Some((name, count));
        }
    }
    best.map(|(name, _)| Arc::clone(name))
}

/// The candidate name of a class under `naming`.
fn candidate_name(members: &[SymRef], naming: Naming<'_>) -> Arc<str> {
    if let Naming::PreferNodes(labels) = naming {
        for label in labels {
            if let Some(name) = majority_name(members.iter().filter(|m| m.node == *label)) {
                return name;
            }
        }
    }
    majority_name(members).unwrap_or_else(|| Arc::from(""))
}

/// Name every class of one kind, resolving clashes by `opts.clash`.
///
/// `reserved` holds names already taken by elements that are not classes
/// (the equations [`compose`] adds), each with the symbol it stands for.
fn name_classes(
    kind: SymKind,
    classes: &[Vec<SymRef>],
    reserved: &BTreeMap<Arc<str>, SymRef>,
    opts: &ComposeOptions,
    naming: Naming<'_>,
    freshened: &mut Vec<Freshening>,
) -> Result<Vec<Arc<str>>, CompositionError> {
    let candidates: Vec<Arc<str>> = classes.iter().map(|m| candidate_name(m, naming)).collect();
    let mut by_name: BTreeMap<&Arc<str>, Vec<usize>> = BTreeMap::new();
    for (i, name) in candidates.iter().enumerate() {
        by_name.entry(name).or_default().push(i);
    }
    let clashing: Vec<(&Arc<str>, Vec<usize>)> = by_name
        .into_iter()
        .filter(|(name, idx)| idx.len() > 1 || reserved.contains_key(*name))
        .collect();
    if clashing.is_empty() {
        return Ok(candidates);
    }
    if let Some((name, idx)) = clashing.first() {
        if opts.mode == CompositionMode::Amalgamate {
            let first = classes[idx[0]][0].clone();
            let second = idx
                .get(1)
                .map(|&j| classes[j][0].clone())
                .or_else(|| reserved.get(*name).cloned())
                .unwrap_or_else(|| first.clone());
            return Err(CompositionError::NameConflict {
                kind,
                name: Arc::clone(name),
                first,
                second,
            });
        }
        if opts.clash == ClashPolicy::Reject {
            let mut clashed: Vec<Vec<SymRef>> = idx.iter().map(|&i| classes[i].clone()).collect();
            if let Some(owner) = reserved.get(*name) {
                clashed.push(vec![owner.clone()]);
            }
            clashed.sort();
            return Err(CompositionError::NameClash {
                kind,
                name: Arc::clone(name),
                classes: clashed,
            });
        }
    }

    // Freshen: qualify every clashing class by its least member label.
    let mut names = candidates.clone();
    for (_, idx) in &clashing {
        for &i in idx {
            let label = classes[i]
                .iter()
                .map(|m| &m.node)
                .min()
                .map_or_else(String::new, ToString::to_string);
            names[i] = Arc::from(format!("{label}.{}", candidates[i]));
        }
    }
    // Resolve what still collides by appending `#k` in class-key order.
    loop {
        let mut by_name: BTreeMap<Arc<str>, Vec<usize>> = BTreeMap::new();
        for (i, name) in names.iter().enumerate() {
            by_name.entry(Arc::clone(name)).or_default().push(i);
        }
        let colliding: Vec<(Arc<str>, Vec<usize>)> = by_name
            .into_iter()
            .filter(|(name, idx)| idx.len() > 1 || reserved.contains_key(name))
            .collect();
        if colliding.is_empty() {
            break;
        }
        for (name, mut idx) in colliding {
            idx.sort_by(|&a, &b| classes[a].cmp(&classes[b]));
            for (k, i) in idx.into_iter().enumerate() {
                names[i] = Arc::from(format!("{name}#{}", k + 1));
            }
        }
    }
    for (i, name) in names.iter().enumerate() {
        if *name != candidates[i] {
            freshened.push(Freshening {
                kind,
                members: classes[i].clone(),
                candidate: Arc::clone(&candidates[i]),
                result: Arc::clone(name),
            });
        }
    }
    Ok(names)
}

/// Name groups of equations, directed equations or policies. In
/// [`CompositionMode::Amalgamate`], a name carried by members of two
/// different groups is a [`CompositionError::NameConflict`] first.
fn name_content_groups(
    kind: SymKind,
    groups: &[Vec<SymRef>],
    reserved: &BTreeMap<Arc<str>, SymRef>,
    opts: &ComposeOptions,
    naming: Naming<'_>,
    freshened: &mut Vec<Freshening>,
) -> Result<Vec<Arc<str>>, CompositionError> {
    if opts.mode == CompositionMode::Amalgamate {
        let mut owner: BTreeMap<&Arc<str>, (usize, &SymRef)> = BTreeMap::new();
        for (g, members) in groups.iter().enumerate() {
            for m in members {
                match owner.get(&m.name) {
                    Some(&(h, first)) if h != g => {
                        let (first, second) = if first <= m { (first, m) } else { (m, first) };
                        return Err(CompositionError::NameConflict {
                            kind,
                            name: Arc::clone(&m.name),
                            first: first.clone(),
                            second: second.clone(),
                        });
                    }
                    Some(_) => {}
                    None => {
                        owner.insert(&m.name, (g, m));
                    }
                }
            }
        }
    }
    name_classes(kind, groups, reserved, opts, naming, freshened)
}

// ─── edges, equations and the result ───────────────────────────────

/// Canonical bytes of a theory morphism: its name, domain and codomain,
/// then its sort map and operation map in key order, every string length
/// prefixed and every term written by an exhaustive match.
fn morphism_bytes(m: &TheoryMorphism) -> Vec<u8> {
    fn string(out: &mut Vec<u8>, s: &str) {
        out.extend_from_slice(&(s.len() as u64).to_le_bytes());
        out.extend_from_slice(s.as_bytes());
    }
    fn count(out: &mut Vec<u8>, n: usize) {
        out.extend_from_slice(&(n as u64).to_le_bytes());
    }
    fn term(out: &mut Vec<u8>, t: &Term) {
        match t {
            Term::Var(v) => {
                out.push(0);
                string(out, v);
            }
            Term::App { op, args } => {
                out.push(1);
                string(out, op);
                count(out, args.len());
                for a in args {
                    term(out, a);
                }
            }
            Term::Case {
                scrutinee,
                branches,
            } => {
                out.push(2);
                term(out, scrutinee);
                count(out, branches.len());
                for b in branches {
                    string(out, &b.constructor);
                    count(out, b.binders.len());
                    for binder in &b.binders {
                        string(out, binder);
                    }
                    term(out, &b.body);
                }
            }
            Term::Hole { name } => {
                out.push(3);
                match name {
                    Some(n) => {
                        out.push(1);
                        string(out, n);
                    }
                    None => out.push(0),
                }
            }
            Term::Let { name, bound, body } => {
                out.push(4);
                string(out, name);
                term(out, bound);
                term(out, body);
            }
        }
    }
    let mut out = Vec::new();
    string(&mut out, &m.name);
    string(&mut out, &m.domain);
    string(&mut out, &m.codomain);
    let sorts: BTreeMap<&Arc<str>, &Arc<str>> = m.sort_map.iter().collect();
    count(&mut out, sorts.len());
    for (k, v) in sorts {
        string(&mut out, k);
        string(&mut out, v);
    }
    let ops: BTreeMap<&Arc<str>, &OpAssignment> = m.op_map.iter().collect();
    count(&mut out, ops.len());
    for (k, v) in ops {
        string(&mut out, k);
        match v {
            OpAssignment::Op(name) => {
                out.push(0);
                string(&mut out, name);
            }
            OpAssignment::Term(t) => {
                out.push(1);
                term(&mut out, t);
            }
        }
    }
    out
}

/// The edge indices in canonical order: ascending source label, target
/// label, then canonical bytes of the morphism.
fn canonical_edge_order(diagram: &TheoryDiagram) -> Vec<usize> {
    let keys: Vec<(&Arc<str>, &Arc<str>, Vec<u8>)> = diagram
        .edges
        .iter()
        .map(|e| (&e.src, &e.tgt, morphism_bytes(&e.morphism)))
        .collect();
    let mut order: Vec<usize> = (0..diagram.edges.len()).collect();
    order.sort_by(|&a, &b| keys[a].cmp(&keys[b]));
    order
}

/// An equation added for a derived-term leg.
struct Added {
    /// The equation, in result names.
    equation: Equation,
    /// Its provenance entry.
    source: AddedEquation,
    /// The edge's position in `diagram.edges`.
    edge_index: usize,
}

/// Step 5: one equation `o(params) = t` per derived-term assignment.
fn added_equations(
    diagram: &TheoryDiagram,
    order: &[usize],
    nodes: &BTreeMap<Arc<str>, NodeInfo<'_>>,
    op_map: &BTreeMap<Arc<str>, HashMap<Arc<str>, Arc<str>>>,
) -> Vec<Added> {
    let mut added = Vec::new();
    for (k, &edge_index) in order.iter().enumerate() {
        let edge = &diagram.edges[edge_index];
        let src = &nodes[&edge.src];
        let terms: BTreeMap<&Arc<str>, &Term> = edge
            .morphism
            .op_map
            .iter()
            .filter_map(|(o, image)| match image {
                OpAssignment::Term(t) => Some((o, t)),
                OpAssignment::Op(_) => None,
            })
            .collect();
        for (o, t) in terms {
            let Some(decl) = src.theory.find_op(o) else {
                continue;
            };
            let src_ops = node_map(op_map, &edge.src);
            let lhs = Term::App {
                op: src_ops.get(o).cloned().unwrap_or_else(|| Arc::clone(o)),
                args: decl
                    .inputs
                    .iter()
                    .filter(|(_, _, imp)| matches!(imp, Implicit::No))
                    .map(|(name, _, _)| Term::Var(Arc::clone(name)))
                    .collect(),
            };
            let rhs = t.rename_ops(node_map(op_map, &edge.tgt));
            let name: Arc<str> = Arc::from(format!("{}.{o}~{k}", edge.src));
            added.push(Added {
                equation: Equation { name, lhs, rhs },
                source: AddedEquation {
                    edge: k,
                    op: SymRef::new(Arc::clone(&edge.src), Arc::clone(o)),
                },
                edge_index,
            });
        }
    }
    added
}

/// One sort per class that some node declares, built from its least
/// declaring member and translated into result names.
fn build_sorts(symbols: &Symbols, nodes: &BTreeMap<Arc<str>, NodeInfo<'_>>) -> Vec<Sort> {
    symbols
        .sort_classes
        .iter()
        .zip(&symbols.sort_names)
        .zip(&symbols.free_classes)
        .filter(|(_, free)| !**free)
        .filter_map(|((members, name), _)| {
            let (m, decl) = members
                .iter()
                .find_map(|m| nodes[&m.node].theory.find_sort(&m.name).map(|d| (m, d)))?;
            let mut sort = translate_sort(
                decl,
                symbols.maps.sorts_of(&m.node),
                symbols.maps.ops_of(&m.node),
            );
            sort.name = Arc::clone(name);
            Some(sort)
        })
        .collect()
}

/// One operation per class, built from its least member and translated
/// into result names.
fn build_ops(symbols: &Symbols, nodes: &BTreeMap<Arc<str>, NodeInfo<'_>>) -> Vec<Operation> {
    symbols
        .op_classes
        .iter()
        .zip(&symbols.op_names)
        .filter_map(|(members, name)| {
            let (m, decl) = members
                .iter()
                .find_map(|m| nodes[&m.node].theory.find_op(&m.name).map(|d| (m, d)))?;
            let mut op = translate_op(
                decl,
                symbols.maps.sorts_of(&m.node),
                symbols.maps.ops_of(&m.node),
            );
            op.name = Arc::clone(name);
            Some(op)
        })
        .collect()
}

/// Equations (or directed equations) grouped by translated content.
struct EqGroups {
    /// Each group's members, ascending; groups ordered by least member.
    members: Vec<Vec<SymRef>>,
    /// Each group's least member, translated (undirected equations).
    reps: Vec<Equation>,
    /// Each group's least member, translated (directed equations).
    directed_reps: Vec<DirectedEquation>,
}

/// Group every node's equations (or, with `directed`, directed equations)
/// by translated content: alpha-equivalence, in either orientation for
/// undirected equations and in the stated orientation for directed ones.
fn group_equations(
    nodes: &BTreeMap<Arc<str>, NodeInfo<'_>>,
    op_map: &BTreeMap<Arc<str>, HashMap<Arc<str>, Arc<str>>>,
    directed: bool,
) -> EqGroups {
    let mut items: Vec<(SymRef, Term, Term, Option<DirectedEquation>)> = Vec::new();
    for (label, info) in nodes {
        let ops = node_map(op_map, label);
        if directed {
            for de in &info.theory.directed_eqs {
                let renamed = de.rename_ops(ops);
                items.push((
                    SymRef::new(Arc::clone(label), Arc::clone(&de.name)),
                    renamed.lhs.clone(),
                    renamed.rhs.clone(),
                    Some(renamed),
                ));
            }
        } else {
            for eq in &info.theory.eqs {
                let renamed = eq.rename_ops(ops);
                items.push((
                    SymRef::new(Arc::clone(label), Arc::clone(&eq.name)),
                    renamed.lhs,
                    renamed.rhs,
                    None,
                ));
            }
        }
    }
    items.sort_by(|a, b| a.0.cmp(&b.0));
    let mut groups = EqGroups {
        members: Vec::new(),
        reps: Vec::new(),
        directed_reps: Vec::new(),
    };
    let mut rep_terms: Vec<(Term, Term)> = Vec::new();
    for (source, lhs, rhs, de) in items {
        let found = rep_terms.iter().position(|(l, r)| {
            alpha_equivalent_equation(l, r, &lhs, &rhs)
                || (!directed && alpha_equivalent_equation(l, r, &rhs, &lhs))
        });
        if let Some(g) = found {
            groups.members[g].push(source);
        } else {
            groups.members.push(vec![source.clone()]);
            if let Some(de) = de {
                groups.directed_reps.push(de);
            } else {
                groups.reps.push(Equation {
                    name: Arc::clone(&source.name),
                    lhs: lhs.clone(),
                    rhs: rhs.clone(),
                });
            }
            rep_terms.push((lhs, rhs));
        }
    }
    groups
}

/// Conflict policies grouped by content (value kind and strategy).
struct PolicyGroups {
    /// Each group's members, ascending; groups ordered by least member.
    members: Vec<Vec<SymRef>>,
    /// Each group's least member.
    reps: Vec<ConflictPolicy>,
}

/// Group every node's policies by content. A policy mentions no sort or
/// operation, so its translation is the identity.
fn group_policies(nodes: &BTreeMap<Arc<str>, NodeInfo<'_>>) -> PolicyGroups {
    let mut items: Vec<(SymRef, &ConflictPolicy)> = nodes
        .iter()
        .flat_map(|(label, info)| {
            info.theory
                .policies
                .iter()
                .map(move |p| (SymRef::new(Arc::clone(label), Arc::clone(&p.name)), p))
        })
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    let mut groups = PolicyGroups {
        members: Vec::new(),
        reps: Vec::new(),
    };
    for (source, policy) in items {
        let found = groups
            .reps
            .iter()
            .position(|r| r.value_kind == policy.value_kind && r.strategy == policy.strategy);
        if let Some(g) = found {
            groups.members[g].push(source);
        } else {
            groups.members.push(vec![source]);
            groups.reps.push(policy.clone());
        }
    }
    groups
}

/// Step 7: the result's signature (ignoring free names), every translated
/// equation, and the remaining checks of [`crate::typecheck_theory`].
fn gate_result(
    theory: &Theory,
    free_names: &BTreeSet<Arc<str>>,
    elements: &Elements,
    added: &[Added],
) -> Result<(), CompositionError> {
    let (eq_groups, eq_names) = (&elements.eqs, &elements.eq_names);
    if let Err(defects) = typecheck_signature(theory) {
        let remaining: Vec<SignatureDefect> = defects
            .into_iter()
            .filter(|d| d.undeclared_sort().is_none_or(|s| !free_names.contains(s)))
            .collect();
        if !remaining.is_empty() {
            return Err(CompositionError::IllTypedResult { defects: remaining });
        }
    }
    for ((members, rep), name) in eq_groups.members.iter().zip(&eq_groups.reps).zip(eq_names) {
        let eq = Equation {
            name: Arc::clone(name),
            lhs: rep.lhs.clone(),
            rhs: rep.rhs.clone(),
        };
        if let Err(err) = typecheck_equation(&eq, theory) {
            return Err(CompositionError::IllTypedTransport {
                source: members[0].clone(),
                detail: err.to_string(),
            });
        }
    }
    for a in added {
        if let Err(err) = typecheck_equation(&a.equation, theory) {
            return Err(CompositionError::IllTypedTransport {
                source: a.source.op.clone(),
                detail: err.to_string(),
            });
        }
    }
    typecheck_theory_body(theory).map_err(|_| CompositionError::IllTypedResult {
        defects: Vec::new(),
    })
}

/// Step 8: the inclusion of each node, in `diagram.nodes` order.
fn build_inclusions(
    diagram: &TheoryDiagram,
    nodes: &BTreeMap<Arc<str>, NodeInfo<'_>>,
    symbols: &Symbols,
    result_name: &Arc<str>,
) -> Vec<TheoryMorphism> {
    diagram
        .nodes
        .iter()
        .map(|node| {
            let theory = nodes[&node.label].theory;
            TheoryMorphism::new(
                format!("incl_{}_{result_name}", node.label),
                Arc::clone(&theory.name),
                Arc::clone(result_name),
                symbols.maps.sorts_of(&node.label).clone(),
                symbols.maps.ops_of(&node.label).clone(),
            )
        })
        .collect()
}

/// A built composition, read by the checks that inspect its cocone.
struct Cocone<'a> {
    /// The diagram.
    diagram: &'a TheoryDiagram,
    /// Its nodes, by label.
    nodes: &'a BTreeMap<Arc<str>, NodeInfo<'a>>,
    /// The named classes.
    symbols: &'a Symbols,
    /// The result theory.
    theory: &'a Theory,
    /// The provenance.
    provenance: &'a Provenance,
    /// The added equations.
    added: &'a [Added],
}

impl Cocone<'_> {
    /// Record `CoconeCommutes` for every edge: each sort and renamed
    /// operation lands in the class of its image, and a derived-term
    /// assignment is closed by the equation added for it.
    fn record_squares(&self, records: &mut Vec<CheckRecord>) {
        for (index, edge) in self.diagram.edges.iter().enumerate() {
            let src = &self.nodes[&edge.src];
            let maps = &self.symbols.maps;
            let mut broken: Option<String> = None;
            for (s, t) in &edge.morphism.sort_map {
                if src.has_sort_symbol(s)
                    && maps.sorts_of(&edge.src).get(s) != maps.sorts_of(&edge.tgt).get(t)
                {
                    broken.get_or_insert_with(|| format!("sort `{s}` and its image `{t}` differ"));
                }
            }
            for (o, image) in &edge.morphism.op_map {
                if let (true, OpAssignment::Op(t)) = (src.theory.has_op(o), image) {
                    if maps.ops_of(&edge.src).get(o) != maps.ops_of(&edge.tgt).get(t) {
                        broken.get_or_insert_with(|| {
                            format!("operation `{o}` and its image `{t}` differ")
                        });
                    }
                }
            }
            let outcome = match (broken, self.added.iter().find(|a| a.edge_index == index)) {
                (Some(detail), _) => CheckOutcome::Failed { detail },
                (None, Some(a)) => CheckOutcome::PassedByEquation {
                    equation: Arc::clone(&a.equation.name),
                },
                (None, None) => CheckOutcome::Passed,
            };
            records.push(record(
                CompositionCheckKind::CoconeCommutes,
                CheckSubject::Edge(index),
                outcome,
            ));
        }
    }

    /// Record `InclusionIsMorphism` and `EquationImage` for every node.
    fn record_inclusions(&self, inclusions: &[TheoryMorphism], records: &mut Vec<CheckRecord>) {
        for (node, inclusion) in self.diagram.nodes.iter().zip(inclusions) {
            let info = &self.nodes[&node.label];
            let node_sorts = self.symbols.maps.sorts_of(&node.label);
            let missing: BTreeSet<Arc<str>> = info
                .implicit
                .iter()
                .filter_map(|s| node_sorts.get(s))
                .filter(|image| self.symbols.free_names.contains(*image))
                .cloned()
                .collect();
            let outcome = if missing.is_empty() {
                check_morphism(inclusion, info.theory, self.theory).map_or_else(
                    |err| CheckOutcome::Failed {
                        detail: err.to_string(),
                    },
                    |()| CheckOutcome::Passed,
                )
            } else {
                CheckOutcome::Inapplicable {
                    reason: InapplicableReason::OpenTheory {
                        missing_sorts: missing.into_iter().collect(),
                    },
                }
            };
            records.push(record(
                CompositionCheckKind::InclusionIsMorphism,
                CheckSubject::Node(Arc::clone(&node.label)),
                outcome,
            ));
            records.push(record(
                CompositionCheckKind::EquationImage,
                CheckSubject::Node(Arc::clone(&node.label)),
                self.lost_equation(&node.label, info)
                    .map_or(CheckOutcome::Passed, |detail| CheckOutcome::Failed {
                        detail,
                    }),
            ));
        }
    }

    /// The first equation or directed equation of a node without an image
    /// in the result, described.
    fn lost_equation(&self, label: &Arc<str>, info: &NodeInfo<'_>) -> Option<String> {
        let ops = self.symbols.maps.ops_of(label);
        for eq in &info.theory.eqs {
            let source = SymRef::new(Arc::clone(label), Arc::clone(&eq.name));
            let renamed = eq.rename_ops(ops);
            let present = self
                .provenance
                .image(SymKind::Eq, &source)
                .and_then(|name| self.theory.find_eq(name))
                .is_some_and(|img| {
                    alpha_equivalent_equation(&img.lhs, &img.rhs, &renamed.lhs, &renamed.rhs)
                        || alpha_equivalent_equation(&img.lhs, &img.rhs, &renamed.rhs, &renamed.lhs)
                });
            if !present {
                return Some(format!("equation {source} has no image"));
            }
        }
        for de in &info.theory.directed_eqs {
            let source = SymRef::new(Arc::clone(label), Arc::clone(&de.name));
            let renamed = de.rename_ops(ops);
            let present = self
                .provenance
                .image(SymKind::DirectedEq, &source)
                .and_then(|name| self.theory.find_directed_eq(name))
                .is_some_and(|img| {
                    alpha_equivalent_equation(&img.lhs, &img.rhs, &renamed.lhs, &renamed.rhs)
                });
            if !present {
                return Some(format!("directed equation {source} has no image"));
            }
        }
        None
    }

    /// Record `UniversalIdentity`: the canonical cocone factors through the
    /// result by the identity, which is a morphism of the result.
    fn record_universal_identity(&self, records: &mut Vec<CheckRecord>) {
        let outcome = if self.symbols.free_names.is_empty() {
            check_morphism(
                &TheoryMorphism::identity(self.theory),
                self.theory,
                self.theory,
            )
            .map_or_else(
                |err| CheckOutcome::Failed {
                    detail: err.to_string(),
                },
                |()| CheckOutcome::Passed,
            )
        } else {
            CheckOutcome::Inapplicable {
                reason: InapplicableReason::OpenTheory {
                    missing_sorts: self.symbols.free_names.iter().cloned().collect(),
                },
            }
        };
        records.push(record(
            CompositionCheckKind::UniversalIdentity,
            CheckSubject::Diagram,
            outcome,
        ));
    }
}

/// `None` when no two nodes share a same-kind name that the diagram's
/// morphisms do not relate; otherwise the first such name, described.
///
/// Sorts and operations are related by the identification the edges
/// generate (with undeclared sorts joined by name); equations, directed
/// equations and policies, which no morphism relates, are related when
/// their translations coincide.
fn name_clash_free(
    nodes: &BTreeMap<Arc<str>, NodeInfo<'_>>,
    edge_uf: &mut UnionFind<Key>,
    provenance: &Provenance,
) -> Option<String> {
    let mut symbols: BTreeMap<(SymKind, Arc<str>), Vec<SymRef>> = BTreeMap::new();
    for (label, info) in nodes {
        let sorts = info
            .theory
            .sorts
            .iter()
            .map(|s| &s.name)
            .chain(info.implicit.iter());
        for name in sorts {
            symbols
                .entry((SymKind::Sort, Arc::clone(name)))
                .or_default()
                .push(SymRef::new(Arc::clone(label), Arc::clone(name)));
        }
        for op in &info.theory.ops {
            symbols
                .entry((SymKind::Op, Arc::clone(&op.name)))
                .or_default()
                .push(SymRef::new(Arc::clone(label), Arc::clone(&op.name)));
        }
    }
    for ((kind, name), refs) in &symbols {
        for (i, a) in refs.iter().enumerate() {
            for b in &refs[i + 1..] {
                if a.node != b.node && !edge_uf.same(&(*kind, a.clone()), &(*kind, b.clone())) {
                    return Some(format!(
                        "{kind} `{name}` is declared by {a} and {b} unrelated"
                    ));
                }
            }
        }
    }
    for kind in [SymKind::Eq, SymKind::DirectedEq, SymKind::Policy] {
        let mut by_name: BTreeMap<&Arc<str>, Vec<(&SymRef, &Arc<str>)>> = BTreeMap::new();
        for (source, image) in provenance.map(kind) {
            by_name
                .entry(&source.name)
                .or_default()
                .push((source, image));
        }
        for (name, entries) in by_name {
            for (i, (a, ai)) in entries.iter().enumerate() {
                for (b, bi) in &entries[i + 1..] {
                    if a.node != b.node && ai != bi {
                        return Some(format!(
                            "{kind} `{name}` is declared by {a} and {b} with different content"
                        ));
                    }
                }
            }
        }
    }
    None
}

// ─── the universal property ────────────────────────────────────────

/// Whether two terms are equal in a theory, as far as rewriting within a
/// budget can tell.
enum Decision {
    /// Shown equal.
    Equal,
    /// Their normal forms differ.
    Distinct,
    /// Rewriting ran out of steps.
    Exhausted,
}

/// Decide `a = b` in `q`: alpha-equivalence first, then normal forms under
/// `q`'s directed equations, then normal forms under every equation of `q`
/// oriented both ways (as [`crate::check_morphism`] does).
fn decide_equal(a: &Term, b: &Term, q: &Theory, budget: usize) -> Decision {
    if alpha_equivalent(a, b) {
        return Decision::Equal;
    }
    let (na, ea) = normalize_with_status(a, &q.directed_eqs, budget);
    let (nb, eb) = normalize_with_status(b, &q.directed_eqs, budget);
    if !ea && !eb && alpha_equivalent(&na, &nb) {
        return Decision::Equal;
    }
    let rules = crate::nat_transform::full_rewrite_rules(q);
    let (na, ea) = normalize_with_status(a, &rules, budget);
    let (nb, eb) = normalize_with_status(b, &rules, budget);
    if ea || eb {
        Decision::Exhausted
    } else if alpha_equivalent(&na, &nb) {
        Decision::Equal
    } else {
        Decision::Distinct
    }
}

/// The image of operation `op` of `domain` under `leg`, as a term over the
/// variables `params` (the result operation's explicit parameters), or
/// `None` when the leg does not map it.
fn op_image(leg: &TheoryMorphism, domain: &Theory, op: &str, params: &[Arc<str>]) -> Option<Term> {
    let decl = domain.find_op(op)?;
    let explicit: Vec<&Arc<str>> = decl
        .inputs
        .iter()
        .filter(|(_, _, imp)| matches!(imp, Implicit::No))
        .map(|(n, _, _)| n)
        .collect();
    let rename: FxHashMap<Arc<str>, Term> = explicit
        .iter()
        .zip(params)
        .map(|(from, to)| (Arc::clone(from), Term::Var(Arc::clone(to))))
        .collect();
    match leg.op_map.get(op)? {
        OpAssignment::Op(target) => Some(Term::App {
            op: Arc::clone(target),
            args: params.iter().map(|p| Term::Var(Arc::clone(p))).collect(),
        }),
        OpAssignment::Term(t) => Some(t.substitute(&rename)),
    }
}

/// The assignment a term image stands for: a rename when the term is an
/// operation applied to exactly `params`, a derived term otherwise.
fn assignment_of(term: Term, params: &[Arc<str>]) -> OpAssignment {
    if let Term::App { op, args } = &term {
        let plain = args.len() == params.len()
            && args
                .iter()
                .zip(params)
                .all(|(a, p)| matches!(a, Term::Var(v) if v == p));
        if plain {
            return OpAssignment::Op(Arc::clone(op));
        }
    }
    OpAssignment::Term(term)
}

impl Composition {
    /// Check the universal property against an alternative cocone
    /// `(q, legs)` and return the mediating morphism with the report
    /// extended by `EquationEntailed` and `UniversalAgainst`.
    ///
    /// `legs[i]` is the leg from `diagram.nodes[i]` into `q`. The legs form
    /// a cocone when every edge's square commutes; operation images are
    /// compared by rewriting in `q` within [`Composition::rewrite_budget`]
    /// steps. The mediator is built class by class, and is well defined
    /// only if every member of a class has one image. It is then checked
    /// with [`crate::check_morphism`] (the `UniversalAgainst` record on the
    /// diagram), and its preservation of the result's equations is decided
    /// first syntactically and then by rewriting from `q`'s equations (the
    /// `EquationEntailed` record); a budget exhausted there is `Incomplete`,
    /// never `Passed`. Morphisms carry the obligation that each translated
    /// axiom follows from the codomain's axioms (kestrel2009specware
    /// §2.3.3, stated; lynch2024gatlab §4.1, stated).
    ///
    /// # Errors
    ///
    /// Returns [`CompositionError::NotACocone`] naming a class whose
    /// members the legs send to different images, or an operation whose
    /// derived-term square does not commute; with an empty class when
    /// `legs` does not have one leg per node or `diagram` is not the
    /// diagram this composition was built from.
    pub fn verify_universal(
        &self,
        diagram: &TheoryDiagram,
        q: &Theory,
        legs: &[TheoryMorphism],
    ) -> Result<(TheoryMorphism, CompositionReport), CompositionError> {
        if legs.len() != diagram.nodes.len() || self.inclusions.len() != diagram.nodes.len() {
            return Err(CompositionError::NotACocone { class: Vec::new() });
        }
        let budget = self.rewrite_budget as usize;
        let leg_of: BTreeMap<&Arc<str>, (&TheoryMorphism, &Theory)> = diagram
            .nodes
            .iter()
            .zip(legs)
            .map(|(node, leg)| (&node.label, (leg, &node.theory)))
            .collect();
        let mut incomplete: Option<Arc<str>> = None;

        let sort_map = self.mediator_sorts(&leg_of)?;

        let op_map = self.mediator_ops(&leg_of, q, budget, &mut incomplete)?;

        let mediator = TheoryMorphism::new(
            format!("mediator_{}_to_{}", self.theory.name, q.name),
            Arc::clone(&self.theory.name),
            Arc::clone(&q.name),
            sort_map,
            op_map,
        );
        let mut records: Vec<CheckRecord> = self
            .report
            .checks
            .iter()
            .filter(|r| {
                !matches!(
                    r.check,
                    CompositionCheckKind::EquationEntailed | CompositionCheckKind::UniversalAgainst
                )
            })
            .cloned()
            .collect();
        self.record_edge_squares(diagram, q, &leg_of, budget, &mut records)?;
        self.record_factorization(&mediator, legs, &mut records);
        let signature_only = Theory::full(
            Arc::clone(&self.theory.name),
            Vec::new(),
            self.theory.sorts.clone(),
            self.theory.ops.clone(),
            Vec::new(),
            self.theory.directed_eqs.clone(),
            self.theory.policies.clone(),
        );
        let universal = match (check_morphism(&mediator, &signature_only, q), incomplete) {
            (Err(err), _) => CheckOutcome::Failed {
                detail: err.to_string(),
            },
            (Ok(()), Some(name)) => CheckOutcome::Incomplete {
                reason: CompositionIncomplete::RewriteBudgetExhausted {
                    equation: name,
                    steps: self.rewrite_budget,
                },
            },
            (Ok(()), None) => CheckOutcome::Passed,
        };
        records.push(record(
            CompositionCheckKind::UniversalAgainst,
            CheckSubject::Diagram,
            universal,
        ));
        records.push(record(
            CompositionCheckKind::EquationEntailed,
            CheckSubject::Diagram,
            self.equation_entailment(&mediator, q, budget),
        ));
        let report = CompositionReport::new(
            self.report.mode,
            self.report.name_clash_free,
            records,
            self.report.freshened.clone(),
        );
        Ok((mediator, report))
    }

    /// The mediator's sort map: the one image of every member of each
    /// sort class.
    fn mediator_sorts(
        &self,
        leg_of: &BTreeMap<&Arc<str>, (&TheoryMorphism, &Theory)>,
    ) -> Result<HashMap<Arc<str>, Arc<str>>, CompositionError> {
        let mut sort_map: HashMap<Arc<str>, Arc<str>> = HashMap::new();
        let result_sorts: BTreeSet<&Arc<str>> = self.provenance.sorts.values().collect();
        for name in result_sorts {
            let class = self.provenance.members(SymKind::Sort, name);
            let mut image: Option<&Arc<str>> = None;
            for m in &class {
                let (leg, theory) = leg_of
                    .get(&m.node)
                    .ok_or_else(|| CompositionError::NotACocone { class: Vec::new() })?;
                match leg.sort_map.get(&m.name) {
                    Some(t) if image.is_none_or(|i| i == t) => image = Some(t),
                    None if !theory.has_sort(&m.name) => {}
                    _ => return Err(CompositionError::NotACocone { class }),
                }
            }
            if let Some(image) = image {
                sort_map.insert(Arc::clone(name), Arc::clone(image));
            }
        }
        Ok(sort_map)
    }

    /// The mediator's operation map: the image of every member of each
    /// operation class, compared by rewriting in `q`. A comparison that
    /// runs out of budget is noted in `incomplete`.
    fn mediator_ops(
        &self,
        leg_of: &BTreeMap<&Arc<str>, (&TheoryMorphism, &Theory)>,
        q: &Theory,
        budget: usize,
        incomplete: &mut Option<Arc<str>>,
    ) -> Result<HashMap<Arc<str>, OpAssignment>, CompositionError> {
        let mut op_map: HashMap<Arc<str>, OpAssignment> = HashMap::new();
        for op in &self.theory.ops {
            let params: Vec<Arc<str>> = op
                .inputs
                .iter()
                .filter(|(_, _, imp)| matches!(imp, Implicit::No))
                .map(|(n, _, _)| Arc::clone(n))
                .collect();
            let class = self.provenance.members(SymKind::Op, &op.name);
            let mut chosen: Option<Term> = None;
            for m in &class {
                let (leg, theory) = leg_of
                    .get(&m.node)
                    .ok_or_else(|| CompositionError::NotACocone { class: Vec::new() })?;
                let Some(image) = op_image(leg, theory, &m.name, &params) else {
                    return Err(CompositionError::NotACocone { class });
                };
                match &chosen {
                    None => chosen = Some(image),
                    Some(first) => match decide_equal(first, &image, q, budget) {
                        Decision::Equal => {}
                        Decision::Distinct => return Err(CompositionError::NotACocone { class }),
                        Decision::Exhausted => {
                            incomplete.get_or_insert_with(|| Arc::clone(&op.name));
                        }
                    },
                }
            }
            if let Some(term) = chosen {
                op_map.insert(Arc::clone(&op.name), assignment_of(term, &params));
            }
        }
        Ok(op_map)
    }

    /// Record, per leg, whether the mediator factors it through the
    /// leg's node's inclusion on sorts.
    fn record_factorization(
        &self,
        mediator: &TheoryMorphism,
        legs: &[TheoryMorphism],
        records: &mut Vec<CheckRecord>,
    ) {
        for (i, (inclusion, leg)) in self.inclusions.iter().zip(legs).enumerate() {
            let factors = inclusion.sort_map.iter().all(|(s, r)| {
                leg.sort_map
                    .get(s)
                    .is_none_or(|t| mediator.sort_map.get(r) == Some(t))
            });
            records.push(record(
                CompositionCheckKind::UniversalAgainst,
                CheckSubject::Cocone(i),
                if factors {
                    CheckOutcome::Passed
                } else {
                    CheckOutcome::Failed {
                        detail: format!("the mediator does not factor leg {i}"),
                    }
                },
            ));
        }
    }

    /// Record, per edge, whether its square commutes in `q`: an edge that
    /// sends an operation to a derived term commutes when the two legs'
    /// images of the operation and of the term are equal in `q`.
    fn record_edge_squares(
        &self,
        diagram: &TheoryDiagram,
        q: &Theory,
        leg_of: &BTreeMap<&Arc<str>, (&TheoryMorphism, &Theory)>,
        budget: usize,
        records: &mut Vec<CheckRecord>,
    ) -> Result<(), CompositionError> {
        for (index, edge) in diagram.edges.iter().enumerate() {
            let (Some((src_leg, src)), Some((tgt_leg, tgt))) =
                (leg_of.get(&edge.src), leg_of.get(&edge.tgt))
            else {
                return Err(CompositionError::NotACocone { class: Vec::new() });
            };
            let mut outcome = CheckOutcome::Passed;
            let terms: BTreeMap<&Arc<str>, &Term> = edge
                .morphism
                .op_map
                .iter()
                .filter_map(|(o, image)| match image {
                    OpAssignment::Term(t) => Some((o, t)),
                    OpAssignment::Op(_) => None,
                })
                .collect();
            for (o, t) in terms {
                let Some(decl) = src.find_op(o) else {
                    continue;
                };
                let params: Vec<Arc<str>> = decl
                    .inputs
                    .iter()
                    .filter(|(_, _, imp)| matches!(imp, Implicit::No))
                    .map(|(n, _, _)| Arc::clone(n))
                    .collect();
                let class = vec![SymRef::new(Arc::clone(&edge.src), Arc::clone(o))];
                let Some(left) = op_image(src_leg, src, o, &params) else {
                    return Err(CompositionError::NotACocone { class });
                };
                let right = tgt_leg.apply_to_term_substituting(t, tgt);
                match decide_equal(&left, &right, q, budget) {
                    Decision::Equal => {}
                    Decision::Distinct => return Err(CompositionError::NotACocone { class }),
                    Decision::Exhausted => {
                        outcome = CheckOutcome::Incomplete {
                            reason: CompositionIncomplete::RewriteBudgetExhausted {
                                equation: Arc::from(format!("{}.{o}", edge.src)),
                                steps: self.rewrite_budget,
                            },
                        };
                    }
                }
            }
            records.push(record(
                CompositionCheckKind::UniversalAgainst,
                CheckSubject::Edge(index),
                outcome,
            ));
        }
        Ok(())
    }

    /// Whether `mediator` sends every equation of the result to an
    /// equation that holds in `q`.
    fn equation_entailment(
        &self,
        mediator: &TheoryMorphism,
        q: &Theory,
        budget: usize,
    ) -> CheckOutcome {
        if self.theory.eqs.is_empty() {
            return if q.eqs.is_empty() {
                CheckOutcome::Inapplicable {
                    reason: InapplicableReason::NoEquations,
                }
            } else {
                CheckOutcome::Passed
            };
        }
        let mut exhausted: Option<Arc<str>> = None;
        for eq in &self.theory.eqs {
            let lhs = mediator.apply_to_term_substituting(&eq.lhs, &self.theory);
            let rhs = mediator.apply_to_term_substituting(&eq.rhs, &self.theory);
            let syntactic = q.eqs.iter().any(|c| {
                alpha_equivalent_equation(&c.lhs, &c.rhs, &lhs, &rhs)
                    || alpha_equivalent_equation(&c.lhs, &c.rhs, &rhs, &lhs)
            });
            if syntactic {
                continue;
            }
            match decide_equal(&lhs, &rhs, q, budget) {
                Decision::Equal => {}
                Decision::Distinct => {
                    return CheckOutcome::Failed {
                        detail: format!(
                            "the image of equation `{}` does not hold in `{}`",
                            eq.name, q.name
                        ),
                    };
                }
                Decision::Exhausted => {
                    exhausted.get_or_insert_with(|| Arc::clone(&eq.name));
                }
            }
        }
        exhausted.map_or(CheckOutcome::Passed, |equation| CheckOutcome::Incomplete {
            reason: CompositionIncomplete::RewriteBudgetExhausted {
                equation,
                steps: self.rewrite_budget,
            },
        })
    }
}
