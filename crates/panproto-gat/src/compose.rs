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
//! Colimits of theories with dependent sorts exist: the category of
//! C-systems has all colimits and is equivalent to a category of
//! generalized algebraic theories (voevodsky2016subsystems Rem 2.8,
//! stated), and a coequalizer adjoins sort- and term-equality judgements
//! (garner2015combinatorial Prop 12, proved). That [`compose`] computes
//! this colimit, restricted to theories without sort equations, whenever
//! it does not refuse is inferred: every class must agree on its members'
//! signatures modulo the identification before anything is built, and the
//! result must pass the signature typechecker.
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
use crate::eq::{DirectedEquation, Equation, Term, alpha_equivalent_equation};
use crate::error::GatError;
use crate::ident::SiteRename;
use crate::morphism::{OpAssignment, TheoryMorphism, check_morphism, normalize_bounded};
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
    ///
    /// This is the representability limit of
    /// [`NotRepresentableReason::NeedsSortEquation`] met at a sort class:
    /// the colimit of the diagram exists among Cartmell's generalized
    /// algebraic theories and identifies the two parameter sorts by a
    /// sort-equality axiom (almeida2026model Construction 2.17, stated in
    /// a preprint), which a [`crate::Theory`] cannot state. The comparison
    /// is syntactic modulo the class map, so it also refuses parameter
    /// sorts that agree only modulo term equations.
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
    /// `Hom(y, x)`), once every sort and operation is sent to its class.
    ///
    /// This is a representability limit, not the absence of a colimit:
    /// the colimit exists among Cartmell's generalized algebraic theories
    /// and adds a sort-equality axiom (almeida2026model Construction 2.17,
    /// stated in a preprint), and a [`crate::Theory`] cannot state a sort
    /// equation. The comparison is syntactic modulo the class map and does
    /// not apply the sort conversions that term equations induce
    /// (garner2015combinatorial Table 1, stated), so this reason is also
    /// given for an identification that holds only modulo the diagram's
    /// term equations and needs no sort equation; that refusal is
    /// conservative.
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
                 which agree only under a sort equation a theory cannot state \
                 or under a term equation the syntactic comparison does not apply",
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
///
/// Parameter sorts are compared by their name-free forms, each variable
/// resolved to the nearest enclosing binder of its name (the earlier
/// parameters, then `Let` and `Case` binders), as
/// [`crate::sort::sort_params_equivalent_modulo_rename`] does; the result
/// is exact whether or not the parameter names are pairwise distinct.
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
    let first_names: Vec<Arc<str>> = first.params.iter().map(|p| Arc::clone(&p.name)).collect();
    let second_names: Vec<Arc<str>> = second.params.iter().map(|p| Arc::clone(&p.name)).collect();
    for (index, (a, b)) in first.params.iter().zip(&second.params).enumerate() {
        if !crate::sort::sort_exprs_agree(
            &a.sort,
            &first_names[..index],
            &b.sort,
            &second_names[..index],
        ) {
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
///
/// The members agree iff their name-free forms are equal
/// ([`crate::sort::signatures_equivalent_modulo_param_rename`], exact
/// whether or not the input names are pairwise distinct).
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
    /// The number of rewrite steps a verification of the composition may
    /// spend deciding one equality before it reports the check as
    /// incomplete; recorded in recipes and passed to
    /// [`Composition::verify_universal_within`].
    pub rewrite_budget: u32,
}

// ─── results ───────────────────────────────────────────────────────

/// The colimit of a [`TheoryDiagram`], with its cocone, provenance and
/// report.
///
/// A composition is exactly its four fields, so it can be taken apart and
/// rebuilt anywhere:
///
/// ```
/// use std::sync::Arc;
/// use panproto_gat::{
///     ClashPolicy, ComposeOptions, Composition, CompositionMode, Sort, Theory, TheoryDiagram,
///     compose,
/// };
///
/// let t = Theory::new("T", vec![Sort::simple("A")], vec![], vec![]);
/// let opts = ComposeOptions {
///     mode: CompositionMode::ExplicitIdentity,
///     result_name: Arc::from("R"),
///     clash: ClashPolicy::Reject,
///     rewrite_budget: 64,
/// };
/// let c = compose(&TheoryDiagram::discrete(vec![("T", t)]), &opts)?;
/// let Composition { theory, inclusions, provenance, report } = c.clone();
/// assert_eq!(Composition { theory, inclusions, provenance, report }, c);
/// # Ok::<(), panproto_gat::CompositionError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
}

/// The rewrite budget [`Composition::verify_universal`] spends deciding one
/// equality.
///
/// It is the step budget [`crate::check_morphism`] uses for equation
/// preservation. [`Composition::verify_universal_within`] takes another
/// budget, such as the one a [`ComposeOptions`] records.
pub const DEFAULT_VERIFY_BUDGET: u32 = 512;

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
/// | `NameClashFree` | `Diagram` | by every [`compose`]: every same-kind name two nodes share without the diagram's morphisms relating it (a clash) is resolved by the mode, `Amalgamate` identifying the symbols and `Freshen` renaming them apart; always `Passed` in a returned composition, since `Reject` returns [`CompositionError::NameClash`] instead. Whether the diagram has no clash at all is [`CompositionReport::name_clash_free`] |
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
    /// Two operations the diagram identifies have signatures that agree
    /// only under a sort equation, or only modulo term equations, once
    /// every sort and operation is sent to its class.
    ///
    /// The refusal is a representability limit: the colimit exists among
    /// Cartmell's generalized algebraic theories, adding a sort-equality
    /// axiom when one is needed (almeida2026model Construction 2.17, stated
    /// in a preprint), and a theory cannot state a sort equation. The
    /// check is syntactic modulo the class map, so it also refuses an
    /// identification that needs no sort equation because the diagram's
    /// term equations already make the sorts derivably equal
    /// (garner2015combinatorial Table 1, stated). See
    /// [`NotRepresentableReason`].
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
    /// distinct classes would get the same name; or, under either clash
    /// policy, a sort name some node uses without declaring is declared in
    /// several classes of the relation the edges generate, which the use
    /// would identify.
    NameClash {
        /// Their kind.
        kind: SymKind,
        /// The name.
        name: Arc<str>,
        /// The clashing classes, each ascending, in ascending order.
        classes: Vec<Vec<SymRef>>,
    },
    /// An equation or directed equation translated into the result does
    /// not typecheck there (a directed equation is checked as the equation
    /// between its two sides).
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
                    "cannot identify operations {first} and {second}: their signatures \
                     agree only under a sort equation, which a theory cannot state, \
                     or under a term equation, which the syntactic comparison does \
                     not apply"
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
///    too; it is joined by name to the same-name sorts other nodes
///    declare, and when none does it stays a free name of the result,
///    joined by name with the other nodes' uses of it. Outside
///    [`CompositionMode::Amalgamate`] those declarations must lie in one
///    class of the relation the edges generate; a name whose declarations
///    lie in several classes would identify declarations no morphism
///    relates, and is [`CompositionError::NameClash`] under either clash
///    policy. Every join is decided against the edge classes and all of
///    them are applied together, so the outcome depends neither on the
///    order in which names are visited nor on how sorts are named.
/// 3. The edges identify each sort with its image and each operation with
///    the operation it is renamed to; in [`CompositionMode::Amalgamate`],
///    same-kind symbols with the same name are identified as well. The
///    classes partition the node-qualified symbols, one partition per
///    kind: signatures over equivalence classes of names
///    (sannellatarlecki1985program Sec 2, stated), the colimit as
///    equivalence classes over the disjoint union (smith2006composition
///    Sec 2, stated), computed by a union-find as for colimit quotients
///    (pattersonlynchfairbanks2022categorical Sec 5.4, stated). Keying the
///    partition by [`SymKind`] is an implementation choice (inferred).
/// 4. Every class's members must agree on arity, parameter sort classes
///    (by their name-free forms, which do not depend on the parameter
///    names), kind and closure (sorts), or on their signature once every
///    sort and operation is sent to its class (operations). Identifying
///    first and then checking that the identified members agree follows
///    the sharing rule of milnertofteharpermacqueen1997definition rule 78
///    and the order of kilpatrickdreyerpeytonjonesmarlow2014backpack Secs
///    3.2 to 3.3, which unifies module identities and then refuses a kind
///    mismatch (stated; the correspondence is inferred). The comparison
///    is syntactic modulo the class map: it does not apply the sort
///    conversions that term equations induce (garner2015combinatorial
///    Table 1, stated), including the equations step 5 adds. A mismatch
///    only in the term arguments of dependent sorts is
///    [`CompositionError::QuotientNotRepresentable`], whether
///    the identification needs a sort equation, which a theory cannot
///    state, or holds modulo a term equation, which this conservative
///    check refuses although the colimit needs no sort equation.
/// 5. An edge that sends an operation `o` to a derived term `t` adds the
///    equation `o(params) = t` under the name
///    `"{src label}.{o}~{k}"`, with `k` the edge's position in the
///    canonical edge order (source label, target label, canonical bytes
///    of the morphism). A coequalizer of generalized algebraic theories
///    adjoins such term-equality judgements (garner2015combinatorial
///    Prop 12, proved); isaev2017algebraic Prop. 2.1 and
///    schultzwisnesky2017algebraic §6.1 build coequalizers this way in the
///    non-dependent setting (stated); the span case and the name scheme
///    are inferred.
/// 6. Each class becomes one sort or operation. Equations, directed
///    equations and policies are translated and collected as sets by
///    content, keeping every source in the provenance.
/// 7. The result must pass [`crate::typecheck_signature`] (ignoring free
///    names) and every check of [`crate::typecheck_theory`].
///
/// Class names: the result theory is named `opts.result_name`; a class is
/// named by the member name that occurs most often among its members, ties
/// going to the least name by bytes (majority names in the sense of
/// mossakowskirabecodescu2017canonical Def. 10, stated, which is Hets'
/// naming convention, kutzneuhausmossakowskicodescu2014blending, stated;
/// no precedent for the tiebreak was found in the venues searched).
/// Clashes, possible only in [`CompositionMode::ExplicitIdentity`], follow
/// `opts.clash`. Result vectors are in ascending name order, so the
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

    let (mut edge_uf, sort_classes, op_classes) = identify(diagram, &nodes, opts.mode)?;
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
    check_transports(diagram, &nodes, &symbols, &added, &opts.result_name)?;
    let elements = Elements::collect(&nodes, &symbols, &added, opts, naming, &mut freshened)?;
    let (theory, provenance) = assemble(&opts.result_name, &nodes, &symbols, &elements, &added);

    gate_result(&theory, &symbols.free_names)?;
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
    // A clash that survives to here was resolved by the mode: `Amalgamate`
    // identifies same-name symbols and `Freshen` renamed them apart (a
    // `Reject` clash returned `NameClash` during naming). The diagram's own
    // property is the report's `name_clash_free`.
    let clash_free = name_clash_free(&nodes, &mut edge_uf, &provenance);
    records.push(record(
        CompositionCheckKind::NameClashFree,
        CheckSubject::Diagram,
        CheckOutcome::Passed,
    ));

    Ok(Composition {
        theory,
        inclusions,
        provenance,
        report: CompositionReport::new(opts.mode, clash_free, records, freshened),
    })
}

/// The relation the edges generate, and the sort and operation classes of
/// the full identification.
type Identification = (UnionFind<Key>, Vec<Vec<SymRef>>, Vec<Vec<SymRef>>);

/// Steps 2 and 3: the union-find over node-qualified symbols. Returns the
/// relation the edges generate (with undeclared sorts joined by name),
/// and the sort and operation classes of the full identification.
///
/// # Errors
///
/// [`CompositionError::NameClash`] when, outside
/// [`CompositionMode::Amalgamate`], an undeclared sort name would have to
/// join same-name declarations the edges keep apart.
fn identify(
    diagram: &TheoryDiagram,
    nodes: &BTreeMap<Arc<str>, NodeInfo<'_>>,
    mode: CompositionMode,
) -> Result<Identification, CompositionError> {
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
    join_implicit_sorts(nodes, &mut uf, mode)?;
    let edge_uf = uf.clone();
    if mode == CompositionMode::Amalgamate {
        join_same_names(&mut uf);
    }
    let classes = uf.classes();
    let sort_classes = kind_classes(&classes, SymKind::Sort);
    let op_classes = kind_classes(&classes, SymKind::Op);
    Ok((edge_uf, sort_classes, op_classes))
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
        let eqs = group_equations(nodes, symbols.op_map(), false, opts.mode)?;
        let directed = group_equations(nodes, symbols.op_map(), true, opts.mode)?;
        let policies = group_policies(nodes, opts.mode)?;
        let eq_names = name_classes(
            SymKind::Eq,
            &eqs.members,
            &reserved,
            opts,
            naming,
            freshened,
        )?;
        let directed_names = name_classes(
            SymKind::DirectedEq,
            &directed.members,
            &none,
            opts,
            naming,
            freshened,
        )?;
        let policy_names = name_classes(
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

/// Join each undeclared use of a sort name to the other nodes'
/// declarations of that name, or, when no node declares it, to every other
/// undeclared use of it.
///
/// Every join is decided against the classes of the relation the edges
/// generate, before any join is applied, and the joins are then added to
/// the union-find together as one closure. So the result depends neither on
/// the order in which names are visited nor on how sorts are named: a join
/// made for one name never changes what is decided for another.
///
/// In [`CompositionMode::Amalgamate`] a use joins every same-name
/// declaration, which the name rule identifies anyway. Otherwise a name
/// whose declarations lie in one edge class has its uses joined to that
/// class; a name whose declarations lie in several edge classes could only
/// join them all, identifying declarations no morphism relates, so the
/// composition is refused.
///
/// # Errors
///
/// [`CompositionError::NameClash`] for the first such name in ascending
/// byte order, listing the edge classes of its declarations.
fn join_implicit_sorts(
    nodes: &BTreeMap<Arc<str>, NodeInfo<'_>>,
    uf: &mut UnionFind<Key>,
    mode: CompositionMode,
) -> Result<(), CompositionError> {
    let mut declared: BTreeMap<&Arc<str>, Vec<Key>> = BTreeMap::new();
    let mut implicit: BTreeMap<&Arc<str>, Vec<Key>> = BTreeMap::new();
    for (label, info) in nodes {
        for sort in &info.theory.sorts {
            declared.entry(&sort.name).or_default().push((
                SymKind::Sort,
                SymRef::new(Arc::clone(label), Arc::clone(&sort.name)),
            ));
        }
        for name in &info.implicit {
            implicit.entry(name).or_default().push((
                SymKind::Sort,
                SymRef::new(Arc::clone(label), Arc::clone(name)),
            ));
        }
    }
    let edge_classes = uf.classes();
    let class_of: BTreeMap<&Key, &Key> = edge_classes
        .iter()
        .flat_map(|(rep, members)| members.iter().map(move |m| (m, rep)))
        .collect();
    let mut joins: Vec<(&Key, &Key)> = Vec::new();
    for (name, users) in &implicit {
        let Some(decls) = declared.get(name) else {
            joins.extend(users.windows(2).map(|pair| (&pair[0], &pair[1])));
            continue;
        };
        let reps: BTreeSet<&Key> = decls.iter().map(|d| class_of[d]).collect();
        if mode != CompositionMode::Amalgamate && reps.len() > 1 {
            let mut classes: Vec<Vec<SymRef>> = reps
                .iter()
                .map(|rep| {
                    let mut class: Vec<SymRef> =
                        edge_classes[*rep].iter().map(|(_, s)| s.clone()).collect();
                    class.sort();
                    class
                })
                .collect();
            classes.sort();
            return Err(CompositionError::NameClash {
                kind: SymKind::Sort,
                name: Arc::clone(name),
                classes,
            });
        }
        for user in users {
            joins.extend(decls.iter().map(|decl| (user, decl)));
        }
    }
    for (a, b) in joins {
        uf.union(a, b);
    }
    Ok(())
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
        let declared: Vec<(&SymRef, &Sort)> = members
            .iter()
            .filter_map(|m| nodes[&m.node].theory.find_sort(&m.name).map(|d| (m, d)))
            .collect();
        let Some(&(first, first_decl)) = declared.first() else {
            continue;
        };
        let translated = |m: &SymRef, decl: &Sort| {
            translate_sort(decl, tokens.sorts_of(&m.node), tokens.ops_of(&m.node))
        };
        let first_translated = translated(first, first_decl);
        let mismatch = declared[1..]
            .iter()
            .find_map(|&(m, decl)| {
                sort_class_mismatch(&first_translated, &translated(m, decl), first_decl, decl)
                    .map(|detail| (m, detail))
            })
            .or_else(|| {
                // A sort a node uses without declaring has arity 0 and
                // carries nothing else to compare.
                (first_decl.arity() != 0)
                    .then(|| {
                        members
                            .iter()
                            .find(|m| !nodes[&m.node].theory.has_sort(&m.name))
                    })
                    .flatten()
                    .map(|m| {
                        (
                            m,
                            SortMismatch::Arity {
                                first: first_decl.arity(),
                                second: 0,
                            },
                        )
                    })
            });
        if let Some((second, detail)) = mismatch {
            return Err(
                name_rule_conflict(SymKind::Sort, first, second, opts, edge_uf).unwrap_or_else(
                    || CompositionError::SortSignatureMismatch {
                        first: first.clone(),
                        second: second.clone(),
                        detail,
                    },
                ),
            );
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
        let Some((first, first_op)) = translated.first() else {
            continue;
        };
        let mismatch = translated[1..]
            .iter()
            .find_map(|(m, op)| op_class_mismatch(first_op, op).map(|mismatch| (*m, mismatch)));
        if let Some((second, mismatch)) = mismatch {
            let typed = match mismatch {
                OpMismatch::NeedsSortEquation => CompositionError::QuotientNotRepresentable {
                    first: (*first).clone(),
                    second: second.clone(),
                    reason: NotRepresentableReason::NeedsSortEquation,
                },
                OpMismatch::Signature => CompositionError::OpSignatureMismatch {
                    first: (*first).clone(),
                    second: second.clone(),
                },
            };
            return Err(
                name_rule_conflict(SymKind::Op, first, second, opts, edge_uf).unwrap_or(typed),
            );
        }
    }
    Ok(())
}

/// In [`CompositionMode::Amalgamate`], a mismatch between two same-name
/// members that the edges alone do not relate (only the name rule joined
/// them) is a [`CompositionError::NameConflict`].
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

/// Group `items` (taken in ascending source order) by content: an item
/// joins the group of equal content, whatever its name. Under
/// [`CompositionMode::Amalgamate`] a name carried by members of two
/// different groups is then a [`CompositionError::NameConflict`]: scanning
/// the groups in order and each group's members in ascending order, the
/// first member whose name an earlier group holds conflicts with the first
/// member of that name, the two reported in ascending order. Returns each
/// group's members (ascending), groups ordered by least member, and each
/// group's least member's item.
fn classify_by_content<T>(
    kind: SymKind,
    mut items: Vec<(SymRef, T)>,
    same_content: impl Fn(&T, &T) -> bool,
    mode: CompositionMode,
) -> Result<(Vec<Vec<SymRef>>, Vec<T>), CompositionError> {
    items.sort_by(|a, b| a.0.cmp(&b.0));
    let mut members: Vec<Vec<SymRef>> = Vec::new();
    let mut reps: Vec<T> = Vec::new();
    for (source, item) in items {
        if let Some(g) = reps.iter().position(|rep| same_content(rep, &item)) {
            members[g].push(source);
        } else {
            members.push(vec![source]);
            reps.push(item);
        }
    }
    if mode == CompositionMode::Amalgamate {
        let mut owner: BTreeMap<&Arc<str>, (usize, &SymRef)> = BTreeMap::new();
        for (g, group) in members.iter().enumerate() {
            for m in group {
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
    Ok((members, reps))
}

/// Group every node's equations (or, with `directed`, directed equations)
/// by translated content: for undirected equations, alpha-equivalence of
/// the two sides in either orientation; for directed equations,
/// alpha-equivalence in the stated orientation together with equal
/// implementation, inverse, value kinds and classification. See
/// [`classify_by_content`].
fn group_equations(
    nodes: &BTreeMap<Arc<str>, NodeInfo<'_>>,
    op_map: &BTreeMap<Arc<str>, HashMap<Arc<str>, Arc<str>>>,
    directed: bool,
    mode: CompositionMode,
) -> Result<EqGroups, CompositionError> {
    let mut items: Vec<(SymRef, DirectedEquation)> = Vec::new();
    for (label, info) in nodes {
        let ops = node_map(op_map, label);
        if directed {
            for de in &info.theory.directed_eqs {
                items.push((
                    SymRef::new(Arc::clone(label), Arc::clone(&de.name)),
                    de.rename_ops(ops),
                ));
            }
        } else {
            for eq in &info.theory.eqs {
                let renamed = eq.rename_ops(ops);
                items.push((
                    SymRef::new(Arc::clone(label), Arc::clone(&eq.name)),
                    DirectedEquation {
                        name: renamed.name,
                        lhs: renamed.lhs,
                        rhs: renamed.rhs,
                        impl_term: panproto_expr::Expr::Var(Arc::from("")),
                        inverse: None,
                        source_kind: None,
                        target_kind: None,
                        coercion_class: crate::sort::CoercionClass::default(),
                    },
                ));
            }
        }
    }
    let kind = if directed {
        SymKind::DirectedEq
    } else {
        SymKind::Eq
    };
    let same = |a: &DirectedEquation, b: &DirectedEquation| {
        (alpha_equivalent_equation(&a.lhs, &a.rhs, &b.lhs, &b.rhs)
            || (!directed && alpha_equivalent_equation(&a.lhs, &a.rhs, &b.rhs, &b.lhs)))
            && a.impl_term == b.impl_term
            && a.inverse == b.inverse
            && a.source_kind == b.source_kind
            && a.target_kind == b.target_kind
            && a.coercion_class == b.coercion_class
    };
    let (members, reps) = classify_by_content(kind, items, same, mode)?;
    let (reps, directed_reps) = if directed {
        (Vec::new(), reps)
    } else {
        (
            reps.into_iter()
                .map(|de| Equation {
                    name: de.name,
                    lhs: de.lhs,
                    rhs: de.rhs,
                })
                .collect(),
            Vec::new(),
        )
    };
    Ok(EqGroups {
        members,
        reps,
        directed_reps,
    })
}

/// Conflict policies grouped by content (value kind and strategy).
struct PolicyGroups {
    /// Each group's members, ascending; groups ordered by least member.
    members: Vec<Vec<SymRef>>,
    /// Each group's least member.
    reps: Vec<ConflictPolicy>,
}

/// Group every node's policies by content (value kind and strategy), as
/// [`classify_by_content`] does. A policy mentions no sort or operation, so
/// its translation is the identity.
fn group_policies(
    nodes: &BTreeMap<Arc<str>, NodeInfo<'_>>,
    mode: CompositionMode,
) -> Result<PolicyGroups, CompositionError> {
    let items: Vec<(SymRef, ConflictPolicy)> = nodes
        .iter()
        .flat_map(|(label, info)| {
            info.theory.policies.iter().map(move |p| {
                (
                    SymRef::new(Arc::clone(label), Arc::clone(&p.name)),
                    p.clone(),
                )
            })
        })
        .collect();
    let same = |a: &ConflictPolicy, b: &ConflictPolicy| {
        a.value_kind == b.value_kind && a.strategy == b.strategy
    };
    let (members, reps) = classify_by_content(SymKind::Policy, items, same, mode)?;
    Ok(PolicyGroups { members, reps })
}

/// Step 6, before equations are grouped: every node equation translated
/// into result names (nodes in diagram order, equations in declaration
/// order), then every added equation (in canonical edge order, operations
/// ascending), then every node's directed equations read as the equation
/// between their two sides, each typechecked against the result's sorts
/// and operations.
///
/// # Errors
///
/// [`CompositionError::IllTypedTransport`] for the first that does not
/// typecheck, naming the source equation, or for an added equation the
/// operation it identifies.
fn check_transports(
    diagram: &TheoryDiagram,
    nodes: &BTreeMap<Arc<str>, NodeInfo<'_>>,
    symbols: &Symbols,
    added: &[Added],
    result_name: &Arc<str>,
) -> Result<(), CompositionError> {
    let signature = Theory::new(
        Arc::clone(result_name),
        sorted_by_name(build_sorts(symbols, nodes), |s| &s.name),
        sorted_by_name(build_ops(symbols, nodes), |o| &o.name),
        Vec::new(),
    );
    let refuse = |source: SymRef, err: GatError| CompositionError::IllTypedTransport {
        source,
        detail: err.to_string(),
    };
    for node in &diagram.nodes {
        let ops = node_map(symbols.op_map(), &node.label);
        for eq in &nodes[&node.label].theory.eqs {
            typecheck_equation(&eq.rename_ops(ops), &signature).map_err(|err| {
                refuse(
                    SymRef::new(Arc::clone(&node.label), Arc::clone(&eq.name)),
                    err,
                )
            })?;
        }
    }
    for a in added {
        typecheck_equation(&a.equation, &signature)
            .map_err(|err| refuse(a.source.op.clone(), err))?;
    }
    for node in &diagram.nodes {
        let ops = node_map(symbols.op_map(), &node.label);
        for de in &nodes[&node.label].theory.directed_eqs {
            let renamed = de.rename_ops(ops);
            let eq = Equation {
                name: Arc::clone(&renamed.name),
                lhs: renamed.lhs,
                rhs: renamed.rhs,
            };
            typecheck_equation(&eq, &signature).map_err(|err| {
                refuse(
                    SymRef::new(Arc::clone(&node.label), Arc::clone(&de.name)),
                    err,
                )
            })?;
        }
    }
    Ok(())
}

/// Step 7: the result's signature (ignoring free names) and the remaining
/// checks of [`crate::typecheck_theory`] beyond the equations, which
/// [`check_transports`] typechecked.
fn gate_result(theory: &Theory, free_names: &BTreeSet<Arc<str>>) -> Result<(), CompositionError> {
    if let Err(defects) = typecheck_signature(theory) {
        let remaining: Vec<SignatureDefect> = defects
            .into_iter()
            .filter(|d| d.undeclared_sort().is_none_or(|s| !free_names.contains(s)))
            .collect();
        if !remaining.is_empty() {
            return Err(CompositionError::IllTypedResult { defects: remaining });
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

/// Whether no two nodes share a same-kind name that the diagram's
/// morphisms do not relate (mossakowskirabecodescu2017canonical Def. 5).
///
/// Sorts and operations are related by the identification the edges
/// generate (with undeclared sorts joined by name); equations, directed
/// equations and policies, which no morphism relates, are related when
/// their translations coincide.
fn name_clash_free(
    nodes: &BTreeMap<Arc<str>, NodeInfo<'_>>,
    edge_uf: &mut UnionFind<Key>,
    provenance: &Provenance,
) -> bool {
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
    for ((kind, _), refs) in &symbols {
        for (i, a) in refs.iter().enumerate() {
            for b in &refs[i + 1..] {
                if a.node != b.node && !edge_uf.same(&(*kind, a.clone()), &(*kind, b.clone())) {
                    return false;
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
        for entries in by_name.into_values() {
            for (i, (a, ai)) in entries.iter().enumerate() {
                for (b, bi) in &entries[i + 1..] {
                    if a.node != b.node && ai != bi {
                        return false;
                    }
                }
            }
        }
    }
    true
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
fn decide_equal(a: &Term, b: &Term, q: &Theory, budget: u32) -> Decision {
    let budget = budget as usize;
    if alpha_equivalent_equation(a, b, a, a) {
        return Decision::Equal;
    }
    let (na, ea) = normalize_bounded(a, &q.directed_eqs, budget);
    let (nb, eb) = normalize_bounded(b, &q.directed_eqs, budget);
    if !ea && !eb && alpha_equivalent_equation(&na, &nb, &na, &na) {
        return Decision::Equal;
    }
    let rules = crate::nat_transform::full_rewrite_rules(q);
    let (na, ea) = normalize_bounded(a, &rules, budget);
    let (nb, eb) = normalize_bounded(b, &rules, budget);
    if ea || eb {
        Decision::Exhausted
    } else if alpha_equivalent_equation(&na, &nb, &na, &na) {
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
    /// compared by rewriting in `q` within [`DEFAULT_VERIFY_BUDGET`]
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
        self.verify_universal_within(diagram, q, legs, DEFAULT_VERIFY_BUDGET)
    }

    /// [`Composition::verify_universal`] with `budget` rewrite steps for
    /// each equality in place of [`DEFAULT_VERIFY_BUDGET`], for instance
    /// the [`ComposeOptions::rewrite_budget`] the composition ran with.
    ///
    /// # Errors
    ///
    /// As [`Composition::verify_universal`].
    pub fn verify_universal_within(
        &self,
        diagram: &TheoryDiagram,
        q: &Theory,
        legs: &[TheoryMorphism],
        budget: u32,
    ) -> Result<(TheoryMorphism, CompositionReport), CompositionError> {
        if legs.len() != diagram.nodes.len() || self.inclusions.len() != diagram.nodes.len() {
            return Err(CompositionError::NotACocone { class: Vec::new() });
        }
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
        Self::record_edge_squares(diagram, q, &leg_of, budget, &mut records)?;
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
                    steps: budget,
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
        budget: u32,
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
        diagram: &TheoryDiagram,
        q: &Theory,
        leg_of: &BTreeMap<&Arc<str>, (&TheoryMorphism, &Theory)>,
        budget: u32,
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
                                steps: budget,
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
        budget: u32,
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
                steps: budget,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sort::SortParam;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn options(mode: CompositionMode, clash: ClashPolicy, name: &str) -> ComposeOptions {
        ComposeOptions {
            mode,
            result_name: Arc::from(name),
            clash,
            rewrite_budget: DEFAULT_VERIFY_BUDGET,
        }
    }

    fn explicit(clash: ClashPolicy) -> ComposeOptions {
        options(CompositionMode::ExplicitIdentity, clash, "R")
    }

    fn amalgamate() -> ComposeOptions {
        options(CompositionMode::Amalgamate, ClashPolicy::Reject, "R")
    }

    /// A theory of simple sorts and operations `(name, inputs, output)`.
    fn th(name: &str, sorts: &[&str], ops: &[(&str, &[&str], &str)], eqs: Vec<Equation>) -> Theory {
        Theory::new(
            name,
            sorts.iter().map(|s| Sort::simple(*s)).collect(),
            ops.iter()
                .map(|(o, ins, out)| {
                    Operation::new(
                        *o,
                        ins.iter()
                            .enumerate()
                            .map(|(i, s)| (Arc::from(format!("x{i}")), SortExpr::from(*s)))
                            .collect(),
                        *out,
                    )
                })
                .collect(),
            eqs,
        )
    }

    /// A renaming morphism.
    fn leg(dom: &str, cod: &str, sorts: &[(&str, &str)], ops: &[(&str, &str)]) -> TheoryMorphism {
        TheoryMorphism::new(
            format!("{dom}_to_{cod}"),
            dom,
            cod,
            sorts
                .iter()
                .map(|(a, b)| (Arc::from(*a), Arc::from(*b)))
                .collect(),
            ops.iter()
                .map(|(a, b)| (Arc::from(*a), Arc::from(*b)))
                .collect::<HashMap<Arc<str>, Arc<str>>>(),
        )
    }

    /// The error of a composition that must fail.
    fn refused(result: Result<Composition, CompositionError>) -> Result<CompositionError, String> {
        match result {
            Ok(c) => Err(format!("composition accepted: {:?}", c.theory)),
            Err(e) => Ok(e),
        }
    }

    fn names<T>(items: &[T], name: impl Fn(&T) -> &Arc<str>) -> Vec<String> {
        items.iter().map(|i| name(i).to_string()).collect()
    }

    fn sym(node: &str, name: &str) -> SymRef {
        SymRef::new(node, name)
    }

    /// `T1` and `T2` share `S` through an apex and each declare their own
    /// `X`, `Label`, `f : S -> X` and `h : S -> S`.
    fn same_name_span() -> TheoryDiagram {
        let side = |name: &str| {
            th(
                name,
                &["S", "X", "Label"],
                &[("f", &["S"], "X"), ("h", &["S"], "S")],
                vec![],
            )
        };
        TheoryDiagram::span(
            ("Sh", th("Sh", &["S"], &[], vec![])),
            ("T1", side("T1")),
            ("T2", side("T2")),
            leg("Sh", "T1", &[("S", "S")], &[]),
            leg("Sh", "T2", &[("S", "S")], &[]),
        )
    }

    #[test]
    fn composition_mode_serde_names() -> TestResult {
        assert_eq!(
            serde_json::to_string(&[
                CompositionMode::Amalgamate,
                CompositionMode::ExplicitIdentity
            ])?,
            r#"["amalgamate","explicit_identity"]"#
        );
        assert_eq!(
            serde_json::to_string(&[ClashPolicy::Freshen, ClashPolicy::Reject])?,
            r#"["freshen","reject"]"#
        );
        let back: CompositionMode = serde_json::from_str(r#""explicit_identity""#)?;
        assert_eq!(back, CompositionMode::ExplicitIdentity);
        let opts = explicit(ClashPolicy::Freshen);
        let json = serde_json::to_value(&opts)?;
        let keys: Vec<&String> = json
            .as_object()
            .map(|o| o.keys().collect())
            .unwrap_or_default();
        assert_eq!(keys.len(), 4);
        assert_eq!(serde_json::from_value::<ComposeOptions>(json)?, opts);
        Ok(())
    }

    #[test]
    fn invalid_and_duplicate_node_labels_are_rejected() -> TestResult {
        let t = th("T", &["S"], &[], vec![]);
        for bad in ["", "a.b", "a#b"] {
            let d = TheoryDiagram::discrete(vec![(bad, t.clone())]);
            assert_eq!(
                refused(compose(&d, &amalgamate()))?,
                CompositionError::InvalidNodeLabel { label: bad.into() }
            );
        }
        let d = TheoryDiagram::discrete(vec![("A", t.clone()), ("A", t)]);
        assert_eq!(
            refused(compose(&d, &amalgamate()))?,
            CompositionError::DuplicateNodeLabel { label: "A".into() }
        );
        Ok(())
    }

    #[test]
    fn an_edge_to_an_unknown_label_is_rejected() -> TestResult {
        let t = th("T", &["S"], &[], vec![]);
        let mut d = TheoryDiagram::discrete(vec![("A", t.clone())]);
        d.edges.push(DiagramEdge {
            src: "A".into(),
            tgt: "B".into(),
            morphism: TheoryMorphism::identity(&t),
        });
        assert_eq!(
            refused(compose(&d, &amalgamate()))?,
            CompositionError::UnknownNodeLabel { label: "B".into() }
        );
        Ok(())
    }

    #[test]
    fn an_ill_typed_node_is_rejected() -> TestResult {
        let bad = Theory::new(
            "Bad",
            vec![Sort::simple("Ob"), Sort::simple("V")],
            vec![Operation::new(
                "mk",
                vec![(Arc::from("o"), SortExpr::from("Ob"))],
                SortExpr::app("V", vec![Term::var("o")]),
            )],
            vec![],
        );
        let d = TheoryDiagram::discrete(vec![("B", bad)]);
        match refused(compose(&d, &amalgamate()))? {
            CompositionError::InvalidNode { label, defects } => {
                assert_eq!(&*label, "B");
                assert!(
                    matches!(defects[..], [SignatureDefect::SortArity { .. }]),
                    "{defects:?}"
                );
            }
            other => return Err(format!("expected InvalidNode, got {other:?}").into()),
        }
        // A node whose only defect is an undeclared sort is admitted.
        let open = th(
            "Open",
            &["Node"],
            &[("anchor", &["Node"], "Vertex")],
            vec![],
        );
        compose(&TheoryDiagram::discrete(vec![("O", open)]), &amalgamate())?;
        Ok(())
    }

    #[test]
    fn diagram_constructors_build_the_contract_shapes() -> TestResult {
        let s = th("S", &["A"], &[], vec![]);
        let l = th("L", &["A", "B"], &[], vec![]);
        let r = th("R", &["A", "C"], &[], vec![]);
        let i1 = leg("S", "L", &[("A", "A")], &[]);
        let i2 = leg("S", "R", &[("A", "A")], &[]);
        let span = TheoryDiagram::span(
            ("s", s.clone()),
            ("l", l.clone()),
            ("r", r.clone()),
            i1.clone(),
            i2,
        );
        assert_eq!(names(&span.nodes, |n| &n.label), ["s", "l", "r"]);
        let ends: Vec<(String, String)> = span
            .edges
            .iter()
            .map(|e| (e.src.to_string(), e.tgt.to_string()))
            .collect();
        assert_eq!(ends, [("s".into(), "l".into()), ("s".into(), "r".into())]);
        assert_eq!(span.edges[0].morphism, i1);

        let by_name = TheoryDiagram::by_name(("s", s.clone()), ("l", l.clone()), ("r", r.clone()))?;
        assert_eq!(by_name.nodes, span.nodes);
        assert_eq!(by_name.edges[0].morphism.sort_map, i1.sort_map);
        let missing = TheoryDiagram::by_name(("s", l.clone()), ("l", s.clone()), ("r", r));
        assert!(
            matches!(missing, Err(GatError::MissingSortMapping(_))),
            "{missing:?}"
        );
        let with_op = th("S2", &["A"], &[("c", &[], "A")], vec![]);
        let missing = TheoryDiagram::by_name(("s", with_op), ("l", l.clone()), ("r", l.clone()));
        assert!(
            matches!(missing, Err(GatError::MissingOpMapping(_))),
            "{missing:?}"
        );

        let par = TheoryDiagram::parallel(("s", s.clone()), ("l", l.clone()), i1.clone(), i1);
        assert_eq!(par.nodes.len(), 2);
        assert!(par.edges.iter().all(|e| &*e.src == "s" && &*e.tgt == "l"));

        let disc = TheoryDiagram::discrete(vec![("s", s), ("l", l)]);
        assert_eq!(disc.nodes.len(), 2);
        assert!(disc.edges.is_empty());
        let json = serde_json::to_value(&disc)?;
        assert!(json.get("nodes").is_some() && json.get("edges").is_some());
        assert_eq!(serde_json::from_value::<TheoryDiagram>(json)?, disc);
        Ok(())
    }

    #[test]
    fn same_name_nonshared_symbols_stay_distinct_in_explicit_mode() -> TestResult {
        let d = same_name_span();
        let c = compose(&d, &explicit(ClashPolicy::Freshen))?;
        assert_eq!(
            names(&c.theory.sorts, |s| &s.name),
            ["S", "T1.Label", "T1.X", "T2.Label", "T2.X"]
        );
        assert_eq!(
            names(&c.theory.ops, |o| &o.name),
            ["T1.f", "T1.h", "T2.f", "T2.h"]
        );
        assert!(!c.report.name_clash_free);
        match refused(compose(&d, &explicit(ClashPolicy::Reject)))? {
            CompositionError::NameClash {
                kind,
                name,
                classes,
            } => {
                assert_eq!((kind, &*name), (SymKind::Sort, "Label"));
                assert_eq!(
                    classes,
                    vec![vec![sym("T1", "Label")], vec![sym("T2", "Label")]]
                );
            }
            other => return Err(format!("expected NameClash, got {other:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn same_name_nonshared_symbols_merge_in_amalgamate_mode() -> TestResult {
        let c = compose(&same_name_span(), &amalgamate())?;
        assert_eq!(names(&c.theory.sorts, |s| &s.name), ["Label", "S", "X"]);
        assert_eq!(names(&c.theory.ops, |o| &o.name), ["f", "h"]);
        assert!(c.report.freshened.is_empty());
        Ok(())
    }

    #[test]
    fn clash_policy_is_ignored_in_amalgamate_mode() -> TestResult {
        let d = same_name_span();
        let reject = compose(
            &d,
            &options(CompositionMode::Amalgamate, ClashPolicy::Reject, "R"),
        )?;
        let freshen = compose(
            &d,
            &options(CompositionMode::Amalgamate, ClashPolicy::Freshen, "R"),
        )?;
        assert_eq!(reject, freshen);
        Ok(())
    }

    #[test]
    fn the_result_is_named_by_the_options() -> TestResult {
        let d = same_name_span();
        let mut reversed = d.clone();
        reversed.nodes.reverse();
        reversed.edges.reverse();
        for diagram in [&d, &reversed] {
            let c = compose(
                diagram,
                &options(CompositionMode::Amalgamate, ClashPolicy::Reject, "Custom"),
            )?;
            assert_eq!(&*c.theory.name, "Custom");
            assert!(c.inclusions.iter().all(|i| &*i.codomain == "Custom"));
        }
        Ok(())
    }

    #[test]
    fn morphisms_identify_differently_named_symbols() -> TestResult {
        let a = th("A", &["Node"], &[], vec![]);
        let b = th("B", &["Vertex"], &[], vec![]);
        let c = th("C", &["Vertex"], &[], vec![]);
        let to = |cod: &str| leg("A", cod, &[("Node", "Vertex")], &[]);
        // A tie between `Node` and `Vertex` goes to the least name.
        let pair = TheoryDiagram {
            nodes: TheoryDiagram::discrete(vec![("a", a.clone()), ("b", b.clone())]).nodes,
            edges: vec![DiagramEdge {
                src: "a".into(),
                tgt: "b".into(),
                morphism: to("B"),
            }],
        };
        let r = compose(&pair, &explicit(ClashPolicy::Reject))?;
        assert_eq!(names(&r.theory.sorts, |s| &s.name), ["Node"]);
        // Two `Vertex` members outvote one `Node`.
        let triple = TheoryDiagram {
            nodes: TheoryDiagram::discrete(vec![("a", a), ("b", b), ("c", c)]).nodes,
            edges: vec![
                DiagramEdge {
                    src: "a".into(),
                    tgt: "b".into(),
                    morphism: to("B"),
                },
                DiagramEdge {
                    src: "a".into(),
                    tgt: "c".into(),
                    morphism: to("C"),
                },
            ],
        };
        let r = compose(&triple, &explicit(ClashPolicy::Reject))?;
        assert_eq!(names(&r.theory.sorts, |s| &s.name), ["Vertex"]);
        assert_eq!(
            r.provenance.members(SymKind::Sort, "Vertex"),
            [sym("a", "Node"), sym("b", "Vertex"), sym("c", "Vertex")]
        );
        assert!(r.report.name_clash_free);
        Ok(())
    }

    /// `T1.X`, `T2.X` and `T3.X` clash with `T1`'s other `X`, and the least
    /// label of both classes is `T1`, so the qualified names collide too.
    fn doubly_clashing_diagram() -> TheoryDiagram {
        let t1 = th("T1", &["X", "Z"], &[], vec![]);
        let t2 = th("T2", &["X"], &[], vec![]);
        let t3 = th("T3", &["X"], &[], vec![]);
        TheoryDiagram {
            nodes: TheoryDiagram::discrete(vec![("T1", t1), ("T2", t2), ("T3", t3)]).nodes,
            edges: vec![
                DiagramEdge {
                    src: "T2".into(),
                    tgt: "T1".into(),
                    morphism: leg("T2", "T1", &[("X", "Z")], &[]),
                },
                DiagramEdge {
                    src: "T3".into(),
                    tgt: "T1".into(),
                    morphism: leg("T3", "T1", &[("X", "Z")], &[]),
                },
            ],
        }
    }

    #[test]
    fn every_freshening_is_reported() -> TestResult {
        let c = compose(&doubly_clashing_diagram(), &explicit(ClashPolicy::Freshen))?;
        assert_eq!(names(&c.theory.sorts, |s| &s.name), ["T1.X#1", "T1.X#2"]);
        assert_eq!(
            c.report.freshened,
            vec![
                Freshening {
                    kind: SymKind::Sort,
                    members: vec![sym("T1", "X")],
                    candidate: "X".into(),
                    result: "T1.X#1".into(),
                },
                Freshening {
                    kind: SymKind::Sort,
                    members: vec![sym("T1", "Z"), sym("T2", "X"), sym("T3", "X")],
                    candidate: "X".into(),
                    result: "T1.X#2".into(),
                },
            ]
        );
        let s = compose(&same_name_span(), &explicit(ClashPolicy::Freshen))?;
        let reported: Vec<(SymKind, String, String)> = s
            .report
            .freshened
            .iter()
            .map(|f| (f.kind, f.candidate.to_string(), f.result.to_string()))
            .collect();
        assert_eq!(reported.len(), 8, "{reported:?}");
        assert!(reported.contains(&(SymKind::Op, "f".into(), "T2.f".into())));
        Ok(())
    }

    #[test]
    fn freshened_classes_keep_every_member_in_provenance() -> TestResult {
        let mut d = same_name_span();
        d.nodes.push(DiagramNode {
            label: "T3".into(),
            theory: th("T3", &["S"], &[], vec![]),
        });
        let c = compose(&d, &explicit(ClashPolicy::Freshen))?;
        assert_eq!(
            c.provenance.members(SymKind::Sort, "Sh.S"),
            [sym("Sh", "S"), sym("T1", "S"), sym("T2", "S")]
        );
        assert_eq!(
            c.provenance.members(SymKind::Sort, "T3.S"),
            [sym("T3", "S")]
        );
        Ok(())
    }

    #[test]
    fn amalgamate_name_conflicts_are_typed() -> TestResult {
        let t1 = th("T1", &["A", "B"], &[("f", &["A"], "B")], vec![]);
        let t2 = th("T2", &["A", "B"], &[("f", &["B"], "A")], vec![]);
        let d = TheoryDiagram::discrete(vec![("T1", t1), ("T2", t2)]);
        assert_eq!(
            refused(compose(&d, &amalgamate()))?,
            CompositionError::NameConflict {
                kind: SymKind::Op,
                name: "f".into(),
                first: sym("T1", "f"),
                second: sym("T2", "f"),
            }
        );
        let simple = th("T1", &["X"], &[], vec![]);
        let dependent = Theory::new(
            "T2",
            vec![
                Sort::simple("O"),
                Sort::dependent("X", vec![SortParam::new("o", "O")]),
            ],
            vec![],
            vec![],
        );
        let d = TheoryDiagram::discrete(vec![("T1", simple), ("T2", dependent)]);
        assert_eq!(
            refused(compose(&d, &amalgamate()))?,
            CompositionError::NameConflict {
                kind: SymKind::Sort,
                name: "X".into(),
                first: sym("T1", "X"),
                second: sym("T2", "X"),
            }
        );
        Ok(())
    }

    #[test]
    fn op_signature_mismatch_is_reported() -> TestResult {
        // `check_morphism` does not compare which inputs are implicit, so
        // both legs are morphisms; the class of `g` still disagrees.
        let explicit_g = th("Sh", &["A"], &[("g", &["A"], "A")], vec![]);
        let implicit_g = Theory::new(
            "T2",
            vec![Sort::simple("A")],
            vec![Operation::with_implicit(
                "g",
                vec![(Arc::from("x0"), SortExpr::from("A"), Implicit::Yes)],
                "A",
            )],
            vec![],
        );
        let d = TheoryDiagram::by_name(
            ("Sh", explicit_g.clone()),
            ("T1", explicit_g),
            ("T2", implicit_g),
        )?;
        assert_eq!(
            refused(compose(&d, &explicit(ClashPolicy::Reject)))?,
            CompositionError::OpSignatureMismatch {
                first: sym("Sh", "g"),
                second: sym("T2", "g"),
            }
        );
        Ok(())
    }

    #[test]
    fn op_class_needing_a_sort_equation_is_not_representable() -> TestResult {
        let hom = |name: &str, op: &str, swapped: bool| {
            let args = if swapped {
                vec![Term::var("y"), Term::var("x")]
            } else {
                vec![Term::var("x"), Term::var("y")]
            };
            Theory::new(
                name,
                vec![
                    Sort::simple("O"),
                    Sort::dependent(
                        "H",
                        vec![SortParam::new("a", "O"), SortParam::new("b", "O")],
                    ),
                ],
                vec![Operation::new(
                    op,
                    vec![
                        (Arc::from("x"), SortExpr::from("O")),
                        (Arc::from("y"), SortExpr::from("O")),
                    ],
                    SortExpr::app("H", args),
                )],
                vec![],
            )
        };
        // `m.r` is renamed to `l.p` by the edge and glued to `r.r` by name;
        // `l.p` and `r.r` differ only in the order of `H`'s arguments.
        let d = TheoryDiagram {
            nodes: TheoryDiagram::discrete(vec![
                ("l", hom("L", "p", false)),
                ("m", hom("M", "r", false)),
                ("r", hom("R", "r", true)),
            ])
            .nodes,
            edges: vec![DiagramEdge {
                src: "m".into(),
                tgt: "l".into(),
                morphism: leg("M", "L", &[("O", "O"), ("H", "H")], &[("r", "p")]),
            }],
        };
        assert_eq!(
            refused(compose(&d, &amalgamate()))?,
            CompositionError::QuotientNotRepresentable {
                first: sym("l", "p"),
                second: sym("r", "r"),
                reason: NotRepresentableReason::NeedsSortEquation,
            }
        );
        Ok(())
    }

    #[test]
    fn coequalizer_of_parallel_morphisms_identifies_their_images() -> TestResult {
        let src = th("Src", &["A"], &[("c", &[], "A")], vec![]);
        let tgt = th(
            "Tgt",
            &["X", "Y"],
            &[("cx", &[], "X"), ("cy", &[], "Y")],
            vec![],
        );
        let f = leg("Src", "Tgt", &[("A", "X")], &[("c", "cx")]);
        let g = leg("Src", "Tgt", &[("A", "Y")], &[("c", "cy")]);
        let d = TheoryDiagram::parallel(("s", src), ("t", tgt.clone()), f, g);
        let c = compose(&d, &explicit(ClashPolicy::Reject))?;
        // {s.A, t.X, t.Y} ties three ways and takes the least name.
        assert_eq!(names(&c.theory.sorts, |s| &s.name), ["A"]);
        assert_eq!(names(&c.theory.ops, |o| &o.name), ["c"]);
        assert_eq!(
            c.inclusions[1].sort_map.get("X").map(AsRef::as_ref),
            Some("A")
        );
        assert_eq!(
            c.inclusions[1].sort_map.get("Y").map(AsRef::as_ref),
            Some("A")
        );
        check_morphism(&c.inclusions[1], &tgt, &c.theory)?;
        Ok(())
    }

    /// `T1` and `T2` over a shared `A` and `h : A -> A`, each with one
    /// equation over `h`.
    fn equation_span(e1: Equation, e2: Equation) -> TheoryDiagram {
        let side = |name: &str, eq: Equation| th(name, &["A"], &[("h", &["A"], "A")], vec![eq]);
        let apex = th("Sh", &["A"], &[("h", &["A"], "A")], vec![]);
        TheoryDiagram::span(
            ("Sh", apex),
            ("T1", side("T1", e1)),
            ("T2", side("T2", e2)),
            leg("Sh", "T1", &[("A", "A")], &[("h", "h")]),
            leg("Sh", "T2", &[("A", "A")], &[("h", "h")]),
        )
    }

    fn idempotent(name: &str, var: &str) -> Equation {
        Equation::new(
            name,
            Term::app("h", vec![Term::app("h", vec![Term::var(var)])]),
            Term::app("h", vec![Term::var(var)]),
        )
    }

    fn involutive(name: &str) -> Equation {
        Equation::new(
            name,
            Term::app("h", vec![Term::app("h", vec![Term::var("x")])]),
            Term::var("x"),
        )
    }

    #[test]
    fn an_equation_and_its_reverse_are_one_axiom() -> TestResult {
        let forward = idempotent("e", "x");
        let reverse = Equation::new("e", forward.rhs.clone(), forward.lhs.clone());
        let d = equation_span(forward.clone(), reverse);
        for opts in [explicit(ClashPolicy::Reject), amalgamate()] {
            let c = compose(&d, &opts)?;
            assert_eq!(names(&c.theory.eqs, |e| &e.name), ["e"]);
            assert_eq!(c.theory.eqs[0].lhs, forward.lhs);
            assert_eq!(
                c.provenance.members(SymKind::Eq, "e"),
                [sym("T1", "e"), sym("T2", "e")]
            );
        }
        Ok(())
    }

    #[test]
    fn alpha_equivalent_equations_merge_and_keep_both_provenance_entries() -> TestResult {
        let d = equation_span(idempotent("e1", "x"), idempotent("e2_different_name", "y"));
        for opts in [explicit(ClashPolicy::Reject), amalgamate()] {
            let c = compose(&d, &opts)?;
            assert_eq!(names(&c.theory.eqs, |e| &e.name), ["e1"]);
            assert_eq!(
                c.provenance.members(SymKind::Eq, "e1"),
                [sym("T1", "e1"), sym("T2", "e2_different_name")]
            );
        }
        // Differing equations under one name stay distinct.
        let d = equation_span(idempotent("ax", "x"), involutive("ax"));
        let c = compose(&d, &explicit(ClashPolicy::Freshen))?;
        assert_eq!(names(&c.theory.eqs, |e| &e.name), ["T1.ax", "T2.ax"]);
        Ok(())
    }

    #[test]
    fn amalgamate_equation_conflicts_are_name_conflicts() -> TestResult {
        let d = equation_span(idempotent("ax", "x"), involutive("ax"));
        assert_eq!(
            refused(compose(&d, &amalgamate()))?,
            CompositionError::NameConflict {
                kind: SymKind::Eq,
                name: "ax".into(),
                first: sym("T1", "ax"),
                second: sym("T2", "ax"),
            }
        );
        Ok(())
    }

    fn with_directed(name: &str, eqs: Vec<DirectedEquation>) -> Theory {
        Theory::full(
            name,
            Vec::new(),
            vec![Sort::simple("A")],
            vec![
                Operation::unary("f", "x", "A", "A"),
                Operation::nullary("c", "A"),
            ],
            Vec::new(),
            eqs,
            Vec::new(),
        )
    }

    fn rule(name: &str, lhs: Term, rhs: Term) -> DirectedEquation {
        DirectedEquation::new(name, lhs, rhs, panproto_expr::Expr::Var("_".into()))
    }

    fn with_policies(name: &str, policies: Vec<ConflictPolicy>) -> Theory {
        Theory::full(
            name,
            Vec::new(),
            vec![Sort::simple("A")],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            policies,
        )
    }

    fn policy(name: &str, strategy: crate::theory::ConflictStrategy) -> ConflictPolicy {
        ConflictPolicy {
            name: name.into(),
            value_kind: crate::sort::ValueKind::Str,
            strategy,
        }
    }

    #[test]
    fn amalgamate_directed_equation_conflicts_are_name_conflicts() -> TestResult {
        let fc = || {
            rule(
                "rule",
                Term::app("f", vec![Term::constant("c")]),
                Term::constant("c"),
            )
        };
        let ff = || rule("rule", Term::app("f", vec![Term::var("x")]), Term::var("x"));
        let shared = th("Sh", &["A"], &[], vec![]);
        let result = crate::colimit::colimit_by_name(
            &with_directed("T1", vec![fc()]),
            &with_directed("T2", vec![ff()]),
            &shared,
        );
        match result {
            Err(GatError::Composition(err)) => assert_eq!(
                *err,
                CompositionError::NameConflict {
                    kind: SymKind::DirectedEq,
                    name: "rule".into(),
                    first: sym("left", "rule"),
                    second: sym("right", "rule"),
                }
            ),
            other => return Err(format!("expected NameConflict, got {other:?}").into()),
        }
        // Same name and content merge, keeping both sources.
        let r = crate::colimit::pushout_by_name(
            &with_directed("T1", vec![fc()]),
            &with_directed("T2", vec![fc()]),
            &shared,
        )?;
        assert_eq!(names(&r.theory.directed_eqs, |e| &e.name), ["rule"]);
        assert_eq!(
            r.provenance.members(SymKind::DirectedEq, "rule"),
            [sym("left", "rule"), sym("right", "rule")]
        );
        Ok(())
    }

    #[test]
    fn amalgamate_policy_conflicts_are_name_conflicts() -> TestResult {
        use crate::theory::ConflictStrategy::{KeepLeft, KeepRight};
        let shared = th("Sh", &["A"], &[], vec![]);
        let result = crate::colimit::colimit_by_name(
            &with_policies("T1", vec![policy("p", KeepLeft)]),
            &with_policies("T2", vec![policy("p", KeepRight)]),
            &shared,
        );
        match result {
            Err(GatError::Composition(err)) => assert_eq!(
                *err,
                CompositionError::NameConflict {
                    kind: SymKind::Policy,
                    name: "p".into(),
                    first: sym("left", "p"),
                    second: sym("right", "p"),
                }
            ),
            other => return Err(format!("expected NameConflict, got {other:?}").into()),
        }
        let r = crate::colimit::pushout_by_name(
            &with_policies("T1", vec![policy("p", KeepLeft)]),
            &with_policies("T2", vec![policy("p", KeepLeft)]),
            &shared,
        )?;
        assert_eq!(names(&r.theory.policies, |p| &p.name), ["p"]);
        assert_eq!(
            r.provenance.members(SymKind::Policy, "p"),
            [sym("left", "p"), sym("right", "p")]
        );
        Ok(())
    }

    #[test]
    fn policies_are_a_set_by_content_with_provenance() -> TestResult {
        use crate::theory::ConflictStrategy::{KeepLeft, KeepRight};
        let d = TheoryDiagram::discrete(vec![
            ("T1", with_policies("T1", vec![policy("p1", KeepLeft)])),
            (
                "T2",
                with_policies("T2", vec![policy("p2", KeepLeft), policy("p1", KeepRight)]),
            ),
        ]);
        let c = compose(&d, &explicit(ClashPolicy::Freshen))?;
        assert_eq!(names(&c.theory.policies, |p| &p.name), ["T1.p1", "T2.p1"]);
        assert_eq!(
            c.provenance.members(SymKind::Policy, "T1.p1"),
            [sym("T1", "p1"), sym("T2", "p2")]
        );
        assert_eq!(
            c.provenance.members(SymKind::Policy, "T2.p1"),
            [sym("T2", "p1")]
        );
        Ok(())
    }

    #[test]
    fn equations_are_transported_with_provenance() -> TestResult {
        // `T2` calls the shared operation `g`; the equation lands in `h`.
        let apex = th("Sh", &["A"], &[("h", &["A"], "A")], vec![]);
        let t1 = th("T1", &["A"], &[("h", &["A"], "A")], vec![]);
        let t2 = Theory::new(
            "T2",
            vec![Sort::simple("A")],
            vec![Operation::unary("g", "x", "A", "A")],
            vec![Equation::new(
                "g_idem",
                Term::app("g", vec![Term::app("g", vec![Term::var("x")])]),
                Term::app("g", vec![Term::var("x")]),
            )],
        );
        let d = TheoryDiagram::span(
            ("Sh", apex),
            ("T1", t1),
            ("T2", t2),
            leg("Sh", "T1", &[("A", "A")], &[("h", "h")]),
            leg("Sh", "T2", &[("A", "A")], &[("h", "g")]),
        );
        let c = compose(&d, &explicit(ClashPolicy::Reject))?;
        let eq = c.theory.find_eq("g_idem").ok_or("equation lost")?;
        assert_eq!(eq.rhs, Term::app("h", vec![Term::var("x")]));
        assert_eq!(
            c.provenance
                .image(SymKind::Eq, &sym("T2", "g_idem"))
                .map(AsRef::as_ref),
            Some("g_idem")
        );
        Ok(())
    }

    #[test]
    fn directed_equations_are_transported_with_provenance() -> TestResult {
        let t = with_directed(
            "T",
            vec![rule(
                "r",
                Term::app("f", vec![Term::constant("c")]),
                Term::constant("c"),
            )],
        );
        let mut renamed = with_directed("U", Vec::new());
        renamed.ops = vec![
            Operation::unary("g", "x", "A", "A"),
            Operation::nullary("c", "A"),
        ];
        let renamed = Theory::full(
            "U",
            Vec::new(),
            renamed.sorts,
            renamed.ops,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let d = TheoryDiagram {
            nodes: TheoryDiagram::discrete(vec![("t", t), ("u", renamed)]).nodes,
            edges: vec![DiagramEdge {
                src: "u".into(),
                tgt: "t".into(),
                morphism: leg("U", "T", &[("A", "A")], &[("g", "f"), ("c", "c")]),
            }],
        };
        let c = compose(&d, &explicit(ClashPolicy::Reject))?;
        // The class {t.f, u.g} ties and takes `f`; the rule keeps its name.
        let r = c
            .theory
            .find_directed_eq("r")
            .ok_or("directed equation lost")?;
        assert_eq!(r.lhs, Term::app("f", vec![Term::constant("c")]));
        assert_eq!(
            c.provenance.members(SymKind::DirectedEq, "r"),
            [sym("t", "r")]
        );
        Ok(())
    }

    /// Two directed equations named alike with different content are two
    /// classes in explicit-identity mode, and each takes its freshened
    /// result name, not its source name.
    #[test]
    fn clashing_directed_equations_take_their_freshened_names() -> TestResult {
        let fc = rule(
            "rule",
            Term::app("f", vec![Term::constant("c")]),
            Term::constant("c"),
        );
        let ff = rule("rule", Term::app("f", vec![Term::var("x")]), Term::var("x"));
        let d = TheoryDiagram::discrete(vec![
            ("T1", with_directed("T1", vec![fc])),
            ("T2", with_directed("T2", vec![ff])),
        ]);
        let c = compose(&d, &explicit(ClashPolicy::Freshen))?;
        assert_eq!(
            names(&c.theory.directed_eqs, |e| &e.name),
            ["T1.rule", "T2.rule"]
        );
        for label in ["T1", "T2"] {
            let result = format!("{label}.rule");
            assert_eq!(
                c.provenance.members(SymKind::DirectedEq, &result),
                [sym(label, "rule")]
            );
            assert!(c.theory.find_directed_eq(&result).is_some());
        }
        Ok(())
    }

    #[test]
    fn policies_are_transported_with_provenance() -> TestResult {
        use crate::theory::ConflictStrategy::KeepLeft;
        let d = TheoryDiagram::discrete(vec![(
            "T1",
            with_policies("T1", vec![policy("p", KeepLeft)]),
        )]);
        let c = compose(&d, &explicit(ClashPolicy::Reject))?;
        assert_eq!(c.theory.policies, vec![policy("p", KeepLeft)]);
        assert_eq!(
            c.provenance
                .image(SymKind::Policy, &sym("T1", "p"))
                .map(AsRef::as_ref),
            Some("p")
        );
        Ok(())
    }

    #[test]
    fn ill_typed_transport_is_reported() -> TestResult {
        let t = th(
            "T",
            &["A"],
            &[],
            vec![Equation::new("loose", Term::var("x"), Term::var("y"))],
        );
        match refused(compose(
            &TheoryDiagram::discrete(vec![("T", t)]),
            &amalgamate(),
        ))? {
            CompositionError::IllTypedTransport { source, .. } => {
                assert_eq!(source, sym("T", "loose"));
            }
            other => return Err(format!("expected IllTypedTransport, got {other:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn an_ill_typed_directed_equation_is_reported() -> TestResult {
        let t = with_directed("T", vec![rule("loose", Term::var("x"), Term::var("y"))]);
        for opts in [explicit(ClashPolicy::Reject), amalgamate()] {
            match refused(compose(
                &TheoryDiagram::discrete(vec![("T", t.clone())]),
                &opts,
            ))? {
                CompositionError::IllTypedTransport { source, .. } => {
                    assert_eq!(source, sym("T", "loose"));
                }
                other => return Err(format!("expected IllTypedTransport, got {other:?}").into()),
            }
        }
        // A well-typed rule still composes.
        let ok = with_directed(
            "T",
            vec![rule(
                "collapse",
                Term::app("f", vec![Term::var("x")]),
                Term::constant("c"),
            )],
        );
        let c = compose(
            &TheoryDiagram::discrete(vec![("T", ok)]),
            &explicit(ClashPolicy::Reject),
        )?;
        assert_eq!(names(&c.theory.directed_eqs, |e| &e.name), ["collapse"]);
        Ok(())
    }

    #[test]
    fn a_closed_sort_gap_is_an_ill_typed_result() -> TestResult {
        // `stray` produces the closed sort `Bool` without being one of its
        // constructors: a defect beyond the signature, which the result gate
        // reports with no signature defects.
        let t = Theory::new(
            "T",
            vec![Sort::closed("Bool", vec![], ["t", "f"])],
            vec![
                Operation::nullary("t", "Bool"),
                Operation::nullary("f", "Bool"),
                Operation::nullary("stray", "Bool"),
            ],
            vec![],
        );
        assert_eq!(
            refused(compose(
                &TheoryDiagram::discrete(vec![("T", t)]),
                &amalgamate()
            ))?,
            CompositionError::IllTypedResult {
                defects: Vec::new()
            }
        );
        Ok(())
    }

    fn checks_of(report: &CompositionReport) -> Vec<(CompositionCheckKind, CheckSubject)> {
        report
            .checks
            .iter()
            .map(|r| (r.check, r.subject.clone()))
            .collect()
    }

    #[test]
    fn every_composition_reports_each_applicable_check() -> TestResult {
        use CompositionCheckKind as K;
        let c = compose(&same_name_span(), &amalgamate())?;
        let node = |l: &str| CheckSubject::Node(l.into());
        let mut expected = vec![(K::ClassSignaturesAgree, CheckSubject::Diagram)];
        for k in [K::LegIsMorphism, K::CoconeCommutes] {
            expected.extend([(k, CheckSubject::Edge(0)), (k, CheckSubject::Edge(1))]);
        }
        for k in [K::NodeWellFormed, K::InclusionIsMorphism, K::EquationImage] {
            expected.extend([(k, node("Sh")), (k, node("T1")), (k, node("T2"))]);
        }
        for k in [K::UniversalIdentity, K::ResultWellTyped, K::NameClashFree] {
            expected.push((k, CheckSubject::Diagram));
        }
        expected.sort();
        assert_eq!(checks_of(&c.report), expected);
        // `T1` and `T2` share names their legs do not relate; `Amalgamate`
        // identifies them, so no check fails.
        assert!(!c.report.name_clash_free);
        for r in &c.report.checks {
            assert!(!matches!(r.outcome, CheckOutcome::Failed { .. }), "{r:?}");
        }
        assert_eq!(c.report.status, CheckStatus::Passed);
        Ok(())
    }

    #[test]
    fn equation_free_nodes_pass_equation_checks_vacuously() -> TestResult {
        let c = compose(&same_name_span(), &explicit(ClashPolicy::Freshen))?;
        let image: Vec<&CheckOutcome> = c
            .report
            .records(CompositionCheckKind::EquationImage)
            .map(|r| &r.outcome)
            .collect();
        assert_eq!(image, [&CheckOutcome::Passed; 3]);
        Ok(())
    }

    #[test]
    fn open_theory_checks_are_reported_inapplicable_not_passed() -> TestResult {
        let flat = th(
            "ThFlat",
            &["Node", "Field", "Value"],
            &[
                ("field_node", &["Field"], "Node"),
                ("node_anchor", &["Node"], "Vertex"),
            ],
            vec![],
        );
        let meta = th(
            "ThMeta",
            &["Node", "Value"],
            &[("tag", &["Node"], "Value")],
            vec![],
        );
        let apex = th("NV", &["Node", "Value"], &[], vec![]);
        let d = TheoryDiagram::by_name(("nv", apex), ("flat", flat.clone()), ("meta", meta))?;
        let c = compose(&d, &explicit(ClashPolicy::Reject))?;
        let open = CheckOutcome::Inapplicable {
            reason: InapplicableReason::OpenTheory {
                missing_sorts: vec!["Vertex".into()],
            },
        };
        let inclusion: Vec<(CheckSubject, CheckOutcome)> = c
            .report
            .records(CompositionCheckKind::InclusionIsMorphism)
            .map(|r| (r.subject.clone(), r.outcome.clone()))
            .collect();
        assert_eq!(
            inclusion,
            [
                (CheckSubject::Node("flat".into()), open.clone()),
                (CheckSubject::Node("meta".into()), CheckOutcome::Passed),
                (CheckSubject::Node("nv".into()), CheckOutcome::Passed),
            ]
        );
        let universal: Vec<&CheckOutcome> = c
            .report
            .records(CompositionCheckKind::UniversalIdentity)
            .map(|r| &r.outcome)
            .collect();
        assert_eq!(universal, [&open]);
        assert_eq!(c.report.status, CheckStatus::Incomplete);
        assert!(c.theory.find_sort("Vertex").is_none());
        // Joined to a declared `Vertex`, the same node is closed.
        let graph = th("ThGraph", &["Vertex"], &[], vec![]);
        let d = TheoryDiagram::discrete(vec![("flat", flat), ("graph", graph)]);
        let c = compose(&d, &explicit(ClashPolicy::Reject))?;
        assert_eq!(c.report.status, CheckStatus::Passed);
        assert_eq!(
            c.provenance.members(SymKind::Sort, "Vertex"),
            [sym("flat", "Vertex"), sym("graph", "Vertex")]
        );
        Ok(())
    }

    #[test]
    fn an_undeclared_sort_does_not_bridge_unrelated_declarations() -> TestResult {
        let a = th("A", &["V"], &[("x", &[], "V")], vec![]);
        let b = th("B", &["V"], &[("y", &[], "V")], vec![]);
        let c = th("C", &[], &[("u", &["V"], "V")], vec![]);
        let nodes = |with_c: bool| {
            let mut nodes = vec![("a", a.clone()), ("b", b.clone())];
            if with_c {
                nodes.push(("c", c.clone()));
            }
            TheoryDiagram::discrete(nodes)
        };
        let clash = CompositionError::NameClash {
            kind: SymKind::Sort,
            name: "V".into(),
            classes: vec![vec![sym("a", "V")], vec![sym("b", "V")]],
        };
        // Without the open node the two declarations clash or are freshened.
        assert_eq!(
            refused(compose(&nodes(false), &explicit(ClashPolicy::Reject)))?,
            clash
        );
        let freshened = compose(&nodes(false), &explicit(ClashPolicy::Freshen))?;
        assert_eq!(names(&freshened.theory.sorts, |s| &s.name), ["a.V", "b.V"]);
        assert!(!freshened.report.name_clash_free);
        // The open node's `V` cannot pick one of the two classes, and it must
        // not merge them: both clash policies refuse.
        for policy in [ClashPolicy::Reject, ClashPolicy::Freshen] {
            assert_eq!(
                refused(compose(&nodes(true), &explicit(policy)))?,
                clash,
                "{policy:?}"
            );
        }
        // Amalgamation identifies same-name sorts anyway.
        let amalgamated = compose(&nodes(true), &amalgamate())?;
        assert_eq!(names(&amalgamated.theory.sorts, |s| &s.name), ["V"]);
        // An edge relating the declarations leaves one class to join.
        let apex = th("S", &["V"], &[], vec![]);
        let related = TheoryDiagram {
            nodes: vec![
                DiagramNode {
                    label: "s".into(),
                    theory: apex,
                },
                DiagramNode {
                    label: "a".into(),
                    theory: a,
                },
                DiagramNode {
                    label: "b".into(),
                    theory: b,
                },
                DiagramNode {
                    label: "c".into(),
                    theory: c,
                },
            ],
            edges: vec![
                DiagramEdge {
                    src: "s".into(),
                    tgt: "a".into(),
                    morphism: leg("S", "A", &[("V", "V")], &[]),
                },
                DiagramEdge {
                    src: "s".into(),
                    tgt: "b".into(),
                    morphism: leg("S", "B", &[("V", "V")], &[]),
                },
            ],
        };
        let joined = compose(&related, &explicit(ClashPolicy::Reject))?;
        assert_eq!(names(&joined.theory.sorts, |s| &s.name), ["V"]);
        assert_eq!(
            joined.provenance.members(SymKind::Sort, "V"),
            [sym("a", "V"), sym("b", "V"), sym("c", "V"), sym("s", "V")]
        );
        assert!(joined.report.name_clash_free);
        Ok(())
    }

    /// Five nodes: `b` declares `u` and `c` declares `w`, related by
    /// `b -> c`; `a` uses `u` without declaring it and `y` declares `w`,
    /// related by `a -> y`; `e` uses `w` without declaring it.
    fn undeclared_order_diagram(u: &str) -> TheoryDiagram {
        let node = |label: &str, theory: Theory| DiagramNode {
            label: label.into(),
            theory,
        };
        let edge = |src: &str, tgt: &str, morphism: TheoryMorphism| DiagramEdge {
            src: src.into(),
            tgt: tgt.into(),
            morphism,
        };
        TheoryDiagram {
            nodes: vec![
                node("a", th("A", &[], &[("f", &[], u)], vec![])),
                node("b", th("B", &[u], &[], vec![])),
                node("c", th("C", &["w"], &[], vec![])),
                node("e", th("E", &[], &[("h", &[], "w")], vec![])),
                node("y", th("Y", &["w"], &[("g", &[], "w")], vec![])),
            ],
            edges: vec![
                edge("b", "c", leg("B", "C", &[(u, "w")], &[])),
                edge("a", "y", leg("A", "Y", &[(u, "w")], &[("f", "g")])),
            ],
        }
    }

    #[test]
    fn undeclared_sort_joins_do_not_depend_on_name_order() -> TestResult {
        // `e`'s `w` meets declarations in two edge classes, whichever name
        // sorts first: `U` sorts before `w` and `zz` after it.
        for u in ["U", "zz"] {
            let clash = CompositionError::NameClash {
                kind: SymKind::Sort,
                name: "w".into(),
                classes: vec![
                    vec![sym("a", u), sym("y", "w")],
                    vec![sym("b", u), sym("c", "w")],
                ],
            };
            for policy in [ClashPolicy::Reject, ClashPolicy::Freshen] {
                assert_eq!(
                    refused(compose(&undeclared_order_diagram(u), &explicit(policy)))?,
                    clash,
                    "{u} {policy:?}"
                );
            }
            let amalgamated = compose(&undeclared_order_diagram(u), &amalgamate())?;
            assert_eq!(names(&amalgamated.theory.sorts, |s| &s.name), ["w"]);
        }
        Ok(())
    }

    #[test]
    fn decoding_a_composition_report_with_a_wrong_status_is_rejected() -> TestResult {
        let c = compose(&same_name_span(), &explicit(ClashPolicy::Freshen))?;
        let mut json = serde_json::to_value(&c.report)?;
        assert_eq!(
            serde_json::from_value::<CompositionReport>(json.clone())?,
            c.report
        );
        // Every check passes, so the status is `passed`.
        assert_eq!(json["status"], "passed");
        json["status"] = serde_json::Value::from("failed");
        let decoded = serde_json::from_value::<CompositionReport>(json);
        assert!(decoded.is_err(), "{decoded:?}");
        Ok(())
    }

    #[test]
    fn composition_check_kind_order_is_the_contract_order() -> TestResult {
        use CompositionCheckKind as K;
        let order = [
            K::LegIsMorphism,
            K::NodeWellFormed,
            K::ClassSignaturesAgree,
            K::CoconeCommutes,
            K::InclusionIsMorphism,
            K::EquationImage,
            K::EquationEntailed,
            K::UniversalIdentity,
            K::UniversalAgainst,
            K::ResultWellTyped,
            K::NameClashFree,
        ];
        assert!(order.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(
            serde_json::to_value(order)?,
            serde_json::json!([
                "leg_is_morphism",
                "node_well_formed",
                "class_signatures_agree",
                "cocone_commutes",
                "inclusion_is_morphism",
                "equation_image",
                "equation_entailed",
                "universal_identity",
                "universal_against",
                "result_well_typed",
                "name_clash_free"
            ])
        );
        Ok(())
    }

    fn round_trips<T>(value: &T, tag: &str) -> TestResult
    where
        T: Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let json = serde_json::to_value(value)?;
        let found = json
            .as_str()
            .map(str::to_owned)
            .or_else(|| json.as_object().and_then(|o| o.keys().next().cloned()));
        assert_eq!(found.as_deref(), Some(tag), "{json}");
        assert_eq!(&serde_json::from_value::<T>(json)?, value);
        Ok(())
    }

    #[test]
    fn composition_report_types_round_trip_every_variant() -> TestResult {
        round_trips(&CheckSubject::Diagram, "diagram")?;
        round_trips(&CheckSubject::Node("a".into()), "node")?;
        round_trips(&CheckSubject::Edge(1), "edge")?;
        round_trips(&CheckSubject::Cocone(2), "cocone")?;
        let open = InapplicableReason::OpenTheory {
            missing_sorts: vec!["V".into()],
        };
        round_trips(&open, "open_theory")?;
        round_trips(&InapplicableReason::NoEquations, "no_equations")?;
        let budget = CompositionIncomplete::RewriteBudgetExhausted {
            equation: "e".into(),
            steps: 3,
        };
        round_trips(&budget, "rewrite_budget_exhausted")?;
        round_trips(
            &CompositionIncomplete::NoTerminatingRewriteSystem,
            "no_terminating_rewrite_system",
        )?;
        round_trips(&CheckOutcome::Passed, "passed")?;
        round_trips(
            &CheckOutcome::PassedByEquation {
                equation: "e".into(),
            },
            "passed_by_equation",
        )?;
        round_trips(&CheckOutcome::Failed { detail: "d".into() }, "failed")?;
        round_trips(&CheckOutcome::Inapplicable { reason: open }, "inapplicable")?;
        round_trips(&CheckOutcome::Incomplete { reason: budget }, "incomplete")?;
        for (kind, tag) in [
            (SymKind::Sort, "sort"),
            (SymKind::Op, "op"),
            (SymKind::Eq, "eq"),
            (SymKind::DirectedEq, "directed_eq"),
            (SymKind::Policy, "policy"),
        ] {
            round_trips(&kind, tag)?;
        }
        round_trips(
            &SortMismatch::Arity {
                first: 0,
                second: 1,
            },
            "arity",
        )?;
        round_trips(
            &SortMismatch::ParamSort {
                index: 0,
                first: "A".into(),
                second: "B".into(),
            },
            "param_sort",
        )?;
        round_trips(&SortMismatch::Kind, "kind")?;
        round_trips(&SortMismatch::Closure, "closure")?;
        round_trips(
            &NotRepresentableReason::NeedsSortEquation,
            "needs_sort_equation",
        )?;
        let c = compose(&same_name_span(), &explicit(ClashPolicy::Freshen))?;
        let json = serde_json::to_string(&c)?;
        let back: Composition = serde_json::from_str(&json)?;
        assert_eq!(back, c);
        Ok(())
    }

    #[test]
    fn composition_error_tags_are_the_contract_tags_in_order() -> TestResult {
        let (a, b) = (sym("A", "x"), sym("B", "x"));
        let errors = [
            CompositionError::DuplicateNodeLabel { label: "A".into() },
            CompositionError::InvalidNodeLabel {
                label: "A.b".into(),
            },
            CompositionError::UnknownNodeLabel { label: "C".into() },
            CompositionError::InvalidNode {
                label: "A".into(),
                defects: Vec::new(),
            },
            CompositionError::InvalidLeg {
                edge: 0,
                detail: "d".into(),
            },
            CompositionError::SortSignatureMismatch {
                first: a.clone(),
                second: b.clone(),
                detail: SortMismatch::Kind,
            },
            CompositionError::OpSignatureMismatch {
                first: a.clone(),
                second: b.clone(),
            },
            CompositionError::QuotientNotRepresentable {
                first: a.clone(),
                second: b.clone(),
                reason: NotRepresentableReason::NeedsSortEquation,
            },
            CompositionError::NameConflict {
                kind: SymKind::Sort,
                name: "x".into(),
                first: a.clone(),
                second: b.clone(),
            },
            CompositionError::NameClash {
                kind: SymKind::Sort,
                name: "x".into(),
                classes: vec![vec![a.clone()], vec![b]],
            },
            CompositionError::IllTypedTransport {
                source: a.clone(),
                detail: "d".into(),
            },
            CompositionError::IllTypedResult {
                defects: Vec::new(),
            },
            CompositionError::NotACocone { class: vec![a] },
        ];
        let tags = [
            "duplicate_node_label",
            "invalid_node_label",
            "unknown_node_label",
            "invalid_node",
            "invalid_leg",
            "sort_signature_mismatch",
            "op_signature_mismatch",
            "quotient_not_representable",
            "name_conflict",
            "name_clash",
            "ill_typed_transport",
            "ill_typed_result",
            "not_a_cocone",
        ];
        assert_eq!(errors.len(), tags.len());
        for (err, tag) in errors.iter().zip(tags) {
            round_trips(err, tag)?;
            assert!(!err.to_string().is_empty());
        }
        Ok(())
    }

    /// Every source element of every node, by kind.
    fn sources(d: &TheoryDiagram) -> Vec<(SymKind, SymRef)> {
        let mut out = Vec::new();
        for node in &d.nodes {
            let at = |name: &Arc<str>| sym(&node.label, name);
            let t = &node.theory;
            out.extend(t.sorts.iter().map(|s| (SymKind::Sort, at(&s.name))));
            out.extend(
                undeclared_sort_names(t)
                    .iter()
                    .map(|s| (SymKind::Sort, at(s))),
            );
            out.extend(t.ops.iter().map(|o| (SymKind::Op, at(&o.name))));
            out.extend(t.eqs.iter().map(|e| (SymKind::Eq, at(&e.name))));
            out.extend(
                t.directed_eqs
                    .iter()
                    .map(|e| (SymKind::DirectedEq, at(&e.name))),
            );
            out.extend(t.policies.iter().map(|p| (SymKind::Policy, at(&p.name))));
        }
        out
    }

    fn assert_total(d: &TheoryDiagram, c: &Composition) {
        let all = sources(d);
        for (kind, source) in &all {
            assert!(
                c.provenance.image(*kind, source).is_some(),
                "{kind} {source} has no image"
            );
        }
        let entries: usize = [
            SymKind::Sort,
            SymKind::Op,
            SymKind::Eq,
            SymKind::DirectedEq,
            SymKind::Policy,
        ]
        .iter()
        .map(|k| c.provenance.map(*k).len())
        .sum();
        assert_eq!(
            entries,
            all.len(),
            "provenance holds entries for non-sources"
        );
    }

    #[test]
    fn provenance_is_total_for_every_source_element() -> TestResult {
        use crate::theory::ConflictStrategy::KeepLeft;
        let d = equation_span(idempotent("e1", "x"), idempotent("e2_different_name", "y"));
        assert_total(&d, &compose(&d, &explicit(ClashPolicy::Reject))?);
        let d = TheoryDiagram::discrete(vec![
            ("T1", with_policies("T1", vec![policy("p1", KeepLeft)])),
            ("T2", with_policies("T2", vec![policy("p2", KeepLeft)])),
            (
                "R",
                with_directed(
                    "R",
                    vec![rule(
                        "r",
                        Term::app("f", vec![Term::constant("c")]),
                        Term::constant("c"),
                    )],
                ),
            ),
            (
                "O",
                th("O", &["Node"], &[("anchor", &["Node"], "Vertex")], vec![]),
            ),
        ]);
        let c = compose(&d, &explicit(ClashPolicy::Freshen))?;
        assert_total(&d, &c);
        assert_eq!(
            c.provenance.members(SymKind::Policy, "p1"),
            [sym("T1", "p1"), sym("T2", "p2")]
        );
        Ok(())
    }

    #[test]
    fn provenance_accessors_invert_the_class_map() -> TestResult {
        let apex = th("S", &["Pt"], &[("pt", &[], "Pt")], vec![]);
        let t1 = th("T1", &["Point"], &[("p", &[], "Point")], vec![]);
        let t2 = th("T2", &["P2"], &[("q", &[], "P2")], vec![]);
        let d = TheoryDiagram::span(
            ("S", apex),
            ("T1", t1),
            ("T2", t2),
            leg("S", "T1", &[("Pt", "Point")], &[("pt", "p")]),
            leg("S", "T2", &[("Pt", "P2")], &[("pt", "q")]),
        );
        let c = compose(&d, &explicit(ClashPolicy::Reject))?;
        for (kind, source) in sources(&d) {
            let image = c.provenance.image(kind, &source).ok_or("no image")?;
            assert!(c.provenance.members(kind, image).contains(&source));
        }
        // {S.Pt, T1.Point, T2.P2} ties three ways and takes the least name.
        assert_eq!(names(&c.theory.sorts, |s| &s.name), ["P2"]);
        let renames = |label: &str| -> Vec<(String, String)> {
            c.provenance
                .site_renames(label)
                .iter()
                .map(|r| (r.old.to_string(), r.new.to_string()))
                .collect()
        };
        // {S.pt, T1.p, T2.q} takes `p`.
        assert_eq!(renames("T1"), [("Point".to_owned(), "P2".to_owned())]);
        assert_eq!(renames("T2"), [("q".to_owned(), "p".to_owned())]);
        Ok(())
    }

    #[test]
    fn inclusions_are_morphisms_and_send_each_symbol_to_its_class() -> TestResult {
        let d = same_name_span();
        for opts in [explicit(ClashPolicy::Freshen), amalgamate()] {
            let c = compose(&d, &opts)?;
            for (node, inclusion) in d.nodes.iter().zip(&c.inclusions) {
                check_morphism(inclusion, &node.theory, &c.theory)?;
                for (s, image) in &inclusion.sort_map {
                    assert_eq!(
                        c.provenance.image(SymKind::Sort, &sym(&node.label, s)),
                        Some(image)
                    );
                }
                for (o, image) in &inclusion.op_map {
                    assert_eq!(
                        c.provenance.image(SymKind::Op, &sym(&node.label, o)),
                        image.as_op()
                    );
                }
            }
        }
        Ok(())
    }

    /// A diagram that is not name-clash free still composes soundly when
    /// the mode resolves every clash: `Amalgamate` identifies same-name
    /// symbols by definition and `Freshen` renames them apart. Such a
    /// composition reports `Passed`; the diagram's own property is in
    /// `name_clash_free`.
    #[test]
    fn a_name_clash_the_mode_resolves_does_not_fail_the_report() -> TestResult {
        use CompositionCheckKind as K;
        for opts in [amalgamate(), explicit(ClashPolicy::Freshen)] {
            let c = compose(&same_name_span(), &opts)?;
            assert!(!c.report.name_clash_free, "{:?}", opts.mode);
            let outcomes: Vec<&CheckOutcome> = c
                .report
                .records(K::NameClashFree)
                .map(|r| &r.outcome)
                .collect();
            assert_eq!(outcomes, [&CheckOutcome::Passed], "{:?}", opts.mode);
            assert_eq!(c.report.status, CheckStatus::Passed, "{:?}", opts.mode);
        }
        assert!(
            !compose(&same_name_span(), &explicit(ClashPolicy::Freshen))?
                .report
                .freshened
                .is_empty()
        );
        // Under `Reject` the same clashes return no composition at all.
        let differ = equation_span(idempotent("ax", "x"), involutive("ax"));
        for d in [same_name_span(), differ] {
            assert!(matches!(
                compose(&d, &explicit(ClashPolicy::Reject)),
                Err(CompositionError::NameClash { .. })
            ));
        }
        Ok(())
    }

    /// An equation used right to left can grow a term without bound
    /// (`x` rewrites to `h(h(x))`). Deciding entailment with the default
    /// budget then gives up as incomplete instead of exhausting the stack
    /// of the thread it runs on.
    #[test]
    fn entailment_by_growing_rewrites_is_incomplete_on_a_default_stack() -> TestResult {
        let t = th(
            "T",
            &["A"],
            &[("h", &["A"], "A")],
            vec![idempotent("e", "x")],
        );
        let q = th("Q", &["A"], &[("h", &["A"], "A")], vec![involutive("k")]);
        let d = TheoryDiagram::discrete(vec![("T", t)]);
        let c = compose(&d, &explicit(ClashPolicy::Reject))?;
        let legs = [leg("T", "Q", &[("A", "A")], &[("h", "h")])];
        let (_, report) = c.verify_universal(&d, &q, &legs)?;
        let entailed: Vec<&CheckOutcome> = report
            .records(CompositionCheckKind::EquationEntailed)
            .map(|r| &r.outcome)
            .collect();
        assert_eq!(
            entailed,
            [&CheckOutcome::Incomplete {
                reason: CompositionIncomplete::RewriteBudgetExhausted {
                    equation: "e".into(),
                    steps: DEFAULT_VERIFY_BUDGET,
                },
            }]
        );
        Ok(())
    }

    /// A composition carries only its four serialized fields, so a decoded
    /// copy verifies exactly as the original does.
    #[test]
    fn a_decoded_composition_verifies_as_the_original() -> TestResult {
        let t = th(
            "T",
            &["A"],
            &[("h", &["A"], "A")],
            vec![idempotent("e", "x")],
        );
        let q = th("Q", &["A"], &[("h", &["A"], "A")], vec![involutive("k")]);
        let d = TheoryDiagram::discrete(vec![("T", t)]);
        let mut opts = explicit(ClashPolicy::Reject);
        opts.rewrite_budget = 16;
        let c = compose(&d, &opts)?;
        let back: Composition = serde_json::from_str(&serde_json::to_string(&c)?)?;
        assert_eq!(back, c);
        let legs = [leg("T", "Q", &[("A", "A")], &[("h", "h")])];
        let (m1, original) = c.verify_universal(&d, &q, &legs)?;
        let (m2, decoded) = back.verify_universal(&d, &q, &legs)?;
        assert_eq!(m1, m2);
        assert_eq!(original, decoded);
        Ok(())
    }

    #[test]
    fn name_clash_free_is_false_exactly_when_an_unrelated_name_is_shared() -> TestResult {
        assert!(
            !compose(&same_name_span(), &amalgamate())?
                .report
                .name_clash_free
        );
        let distinct = TheoryDiagram::span(
            ("Sh", th("Sh", &["S"], &[], vec![])),
            ("T1", th("T1", &["S", "X"], &[("f", &["S"], "X")], vec![])),
            ("T2", th("T2", &["S", "Y"], &[("g", &["S"], "Y")], vec![])),
            leg("Sh", "T1", &[("S", "S")], &[]),
            leg("Sh", "T2", &[("S", "S")], &[]),
        );
        let c = compose(&distinct, &explicit(ClashPolicy::Reject))?;
        assert!(c.report.name_clash_free);
        assert_eq!(c.report.status, CheckStatus::Passed);
        // Same-name equations: related when their content coincides.
        let same = equation_span(idempotent("ax", "x"), idempotent("ax", "y"));
        assert!(
            compose(&same, &explicit(ClashPolicy::Reject))?
                .report
                .name_clash_free
        );
        let differ = equation_span(idempotent("ax", "x"), involutive("ax"));
        assert!(
            !compose(&differ, &explicit(ClashPolicy::Freshen))?
                .report
                .name_clash_free
        );
        Ok(())
    }

    /// The source symbols of a class of a composition in which the node
    /// `stage` is an earlier composition: members of `stage` are replaced
    /// by the members of their class there.
    fn flatten(
        kind: SymKind,
        members: Vec<SymRef>,
        stage: &str,
        earlier: &Composition,
    ) -> BTreeSet<SymRef> {
        members
            .into_iter()
            .flat_map(|m| {
                if &*m.node == stage {
                    earlier.provenance.members(kind, &m.name)
                } else {
                    vec![m]
                }
            })
            .collect()
    }

    /// The one-diagram composition, the iterated one, and the bijection of
    /// result sort names between them.
    type Iterated = (Composition, Composition, BTreeMap<Arc<str>, Arc<str>>);

    /// Compose `a -> b` and then `a -> c` against the one diagram with both
    /// edges, and return the bijection of result sort names the two
    /// provenance maps induce, iterated name to one-diagram name.
    fn iterated_against_one_diagram(
        a: &Theory,
        b: &Theory,
        c: &Theory,
        ab: &TheoryMorphism,
        ac: &TheoryMorphism,
    ) -> Result<Iterated, Box<dyn std::error::Error>> {
        let opts = explicit(ClashPolicy::Reject);
        let edge = |src: &str, tgt: &str, m: &TheoryMorphism| DiagramEdge {
            src: src.into(),
            tgt: tgt.into(),
            morphism: m.clone(),
        };
        let one = TheoryDiagram {
            nodes: TheoryDiagram::discrete(vec![
                ("a", a.clone()),
                ("b", b.clone()),
                ("c", c.clone()),
            ])
            .nodes,
            edges: vec![edge("a", "b", ab), edge("a", "c", ac)],
        };
        let one = compose(&one, &opts)?;
        let first = TheoryDiagram {
            nodes: TheoryDiagram::discrete(vec![("a", a.clone()), ("b", b.clone())]).nodes,
            edges: vec![edge("a", "b", ab)],
        };
        let first = compose(&first, &opts)?;
        let into_first = first.inclusions[0].clone();
        let second = TheoryDiagram {
            nodes: TheoryDiagram::discrete(vec![
                ("a", a.clone()),
                ("r1", first.theory.clone()),
                ("c", c.clone()),
            ])
            .nodes,
            edges: vec![edge("a", "r1", &into_first), edge("a", "c", ac)],
        };
        let second = compose(&second, &opts)?;
        let mut bijection = BTreeMap::new();
        for sort in &second.theory.sorts {
            let class = flatten(
                SymKind::Sort,
                second.provenance.members(SymKind::Sort, &sort.name),
                "r1",
                &first,
            );
            let partner = one
                .theory
                .sorts
                .iter()
                .find(|s| {
                    one.provenance
                        .members(SymKind::Sort, &s.name)
                        .into_iter()
                        .collect::<BTreeSet<_>>()
                        == class
                })
                .ok_or("no one-diagram class with the same membership")?;
            bijection.insert(Arc::clone(&sort.name), Arc::clone(&partner.name));
        }
        Ok((one, second, bijection))
    }

    #[test]
    fn iterated_pushouts_agree_up_to_the_provenance_bijection() -> TestResult {
        // Node / Vertex / Vertex: the iterated class ties twice and stays
        // `Node`, the one-diagram class is `Vertex`.
        let a = th("A", &["Node"], &[], vec![]);
        let b = th("B", &["Vertex"], &[], vec![]);
        let c = th("C", &["Vertex"], &[], vec![]);
        let ab = leg("A", "B", &[("Node", "Vertex")], &[]);
        let ac = leg("A", "C", &[("Node", "Vertex")], &[]);
        let (one, iterated, bijection) = iterated_against_one_diagram(&a, &b, &c, &ab, &ac)?;
        assert_eq!(names(&one.theory.sorts, |s| &s.name), ["Vertex"]);
        assert_eq!(names(&iterated.theory.sorts, |s| &s.name), ["Node"]);
        assert_eq!(
            bijection,
            BTreeMap::from([(Arc::from("Node"), Arc::from("Vertex"))])
        );
        assert!(one.report.freshened.is_empty() && iterated.report.freshened.is_empty());

        // Every class has a single member name: byte-identical.
        let a = th("A", &["V", "E"], &[("s", &["E"], "V")], vec![]);
        let b = th(
            "B",
            &["V", "E", "L"],
            &[("s", &["E"], "V"), ("l", &["E"], "L")],
            vec![],
        );
        let c = th(
            "C",
            &["V", "E", "K"],
            &[("s", &["E"], "V"), ("k", &["V"], "K")],
            vec![],
        );
        let ab = leg("A", "B", &[("V", "V"), ("E", "E")], &[("s", "s")]);
        let ac = leg("A", "C", &[("V", "V"), ("E", "E")], &[("s", "s")]);
        let (one, iterated, bijection) = iterated_against_one_diagram(&a, &b, &c, &ab, &ac)?;
        assert!(bijection.iter().all(|(x, y)| x == y));
        assert_eq!(
            serde_json::to_vec(&one.theory)?,
            serde_json::to_vec(&iterated.theory)?
        );
        Ok(())
    }

    /// The graph and constraint theories over `Vertex`, the first with an
    /// idempotent `loop`.
    fn graph_constraint() -> TheoryDiagram {
        let g = th(
            "G",
            &["Vertex", "Edge"],
            &[
                ("src", &["Edge"], "Vertex"),
                ("loop", &["Vertex"], "Vertex"),
            ],
            vec![Equation::new(
                "idem",
                Term::app("loop", vec![Term::app("loop", vec![Term::var("v")])]),
                Term::app("loop", vec![Term::var("v")]),
            )],
        );
        let c = th(
            "C",
            &["Vertex", "Constraint"],
            &[("target", &["Constraint"], "Vertex")],
            vec![],
        );
        TheoryDiagram::span(
            ("V", th("V", &["Vertex"], &[], vec![])),
            ("G", g),
            ("C", c),
            leg("V", "G", &[("Vertex", "Vertex")], &[]),
            leg("V", "C", &[("Vertex", "Vertex")], &[]),
        )
    }

    /// A theory containing the graph-constraint pushout, plus `Extra`.
    fn larger() -> Theory {
        th(
            "Q",
            &["Vertex", "Edge", "Constraint", "Extra"],
            &[
                ("src", &["Edge"], "Vertex"),
                ("loop", &["Vertex"], "Vertex"),
                ("target", &["Constraint"], "Vertex"),
            ],
            vec![Equation::new(
                "idem",
                Term::app("loop", vec![Term::app("loop", vec![Term::var("w")])]),
                Term::app("loop", vec![Term::var("w")]),
            )],
        )
    }

    fn larger_legs(constraint_vertex: &str) -> Vec<TheoryMorphism> {
        vec![
            leg("V", "Q", &[("Vertex", "Vertex")], &[]),
            leg(
                "G",
                "Q",
                &[("Vertex", "Vertex"), ("Edge", "Edge")],
                &[("src", "src"), ("loop", "loop")],
            ),
            leg(
                "C",
                "Q",
                &[("Vertex", constraint_vertex), ("Constraint", "Constraint")],
                &[("target", "target")],
            ),
        ]
    }

    #[test]
    fn verify_universal_builds_the_mediator() -> TestResult {
        let d = graph_constraint();
        let c = compose(&d, &explicit(ClashPolicy::Reject))?;
        let (mediator, report) = c.verify_universal(&d, &larger(), &larger_legs("Vertex"))?;
        for s in ["Vertex", "Edge", "Constraint"] {
            assert_eq!(mediator.sort_map.get(s).map(AsRef::as_ref), Some(s));
        }
        assert_eq!(
            mediator.op_map.get("target"),
            Some(&OpAssignment::op("target"))
        );
        let universal: Vec<(CheckSubject, CheckOutcome)> = report
            .records(CompositionCheckKind::UniversalAgainst)
            .map(|r| (r.subject.clone(), r.outcome.clone()))
            .collect();
        assert_eq!(
            universal,
            [
                (CheckSubject::Diagram, CheckOutcome::Passed),
                (CheckSubject::Edge(0), CheckOutcome::Passed),
                (CheckSubject::Edge(1), CheckOutcome::Passed),
                (CheckSubject::Cocone(0), CheckOutcome::Passed),
                (CheckSubject::Cocone(1), CheckOutcome::Passed),
                (CheckSubject::Cocone(2), CheckOutcome::Passed),
            ]
        );
        let entailed: Vec<&CheckOutcome> = report
            .records(CompositionCheckKind::EquationEntailed)
            .map(|r| &r.outcome)
            .collect();
        assert_eq!(entailed, [&CheckOutcome::Passed]);
        assert_eq!(report.status, CheckStatus::Passed);

        // A derived-term leg: the mediator sends `e` to `inv(u())`.
        let sp = th("SP", &["A"], &[("e", &[], "A")], vec![]);
        let tp1 = th("TP1", &["A"], &[("e", &[], "A")], vec![]);
        let tp2 = th(
            "TP2",
            &["A"],
            &[("u", &[], "A"), ("inv", &["A"], "A")],
            vec![],
        );
        let inv_u = Term::app("inv", vec![Term::constant("u")]);
        let mut to_term = leg("SP", "TP2", &[("A", "A")], &[]);
        to_term
            .op_map
            .insert("e".into(), OpAssignment::Term(inv_u.clone()));
        let d = TheoryDiagram::span(
            ("SP", sp),
            ("TP1", tp1),
            ("TP2", tp2.clone()),
            leg("SP", "TP1", &[("A", "A")], &[("e", "e")]),
            to_term.clone(),
        );
        let c = compose(&d, &explicit(ClashPolicy::Reject))?;
        let mut via_tp1 = leg("TP1", "TP2", &[("A", "A")], &[]);
        via_tp1
            .op_map
            .insert("e".into(), OpAssignment::Term(inv_u.clone()));
        let legs = [to_term, via_tp1, TheoryMorphism::identity(&tp2)];
        let (mediator, report) = c.verify_universal(&d, &tp2, &legs)?;
        assert_eq!(mediator.op_map.get("e"), Some(&OpAssignment::Term(inv_u)));
        assert_eq!(report.status, CheckStatus::Passed, "{:?}", report.checks);
        Ok(())
    }

    #[test]
    fn verify_universal_rejects_a_non_cocone() -> TestResult {
        let d = graph_constraint();
        let c = compose(&d, &explicit(ClashPolicy::Reject))?;
        match c.verify_universal(&d, &larger(), &larger_legs("Extra")) {
            Err(CompositionError::NotACocone { class }) => {
                assert_eq!(
                    class,
                    [sym("C", "Vertex"), sym("G", "Vertex"), sym("V", "Vertex")]
                );
            }
            other => return Err(format!("expected NotACocone, got {other:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn a_cocone_sending_identified_operations_to_different_inputs_is_refused() -> TestResult {
        // `S.o(x, y) : A` goes to `T.p` along one edge and to `T.q` along the
        // other, so `S.o`, `T.p` and `T.q` form one class. A cocone sending
        // `S.o` and `T.p` to the input `x` and `T.q` to the input `y` sends
        // the class to two different terms: it is no cocone, although each
        // image, taken alone, is a single variable.
        let sort_a = || SortExpr::from("A");
        let binary = |name: &str| {
            Operation::new(
                name,
                vec![("x".into(), sort_a()), ("y".into(), sort_a())],
                "A",
            )
        };
        let source = Theory::new("S", vec![Sort::simple("A")], vec![binary("o")], vec![]);
        let target = Theory::new(
            "T",
            vec![Sort::simple("A")],
            vec![binary("p"), binary("q")],
            vec![],
        );
        let leg = |image: &str| {
            let mut op_map: HashMap<Arc<str>, OpAssignment> = HashMap::new();
            op_map.insert("o".into(), OpAssignment::op(image));
            TheoryMorphism::new(
                format!("o_to_{image}"),
                "S",
                "T",
                HashMap::from([(Arc::from("A"), Arc::from("A"))]),
                op_map,
            )
        };
        let diagram = TheoryDiagram::parallel(("S", source), ("T", target), leg("p"), leg("q"));
        let composition = compose(&diagram, &explicit(ClashPolicy::Reject))?;
        let apex = Theory::new("Q", vec![Sort::simple("A")], vec![], vec![]);
        let to_q = |images: &[(&str, &str)]| {
            let op_map: HashMap<Arc<str>, OpAssignment> = images
                .iter()
                .map(|(op, var)| (Arc::from(*op), OpAssignment::Term(Term::var(*var))))
                .collect();
            TheoryMorphism::new(
                "to_q",
                "T",
                "Q",
                HashMap::from([(Arc::from("A"), Arc::from("A"))]),
                op_map,
            )
        };
        let legs = [to_q(&[("o", "x")]), to_q(&[("p", "x"), ("q", "y")])];
        match composition.verify_universal(&diagram, &apex, &legs) {
            Err(CompositionError::NotACocone { .. }) => Ok(()),
            other => Err(format!("expected NotACocone, got {other:?}").into()),
        }
    }

    #[test]
    fn entailment_beyond_the_rewrite_budget_is_incomplete() -> TestResult {
        let t = th(
            "T",
            &["A"],
            &[("h", &["A"], "A")],
            vec![idempotent("e", "x")],
        );
        let q = th("Q", &["A"], &[("h", &["A"], "A")], vec![involutive("k")]);
        let d = TheoryDiagram::discrete(vec![("T", t)]);
        let mut opts = explicit(ClashPolicy::Reject);
        opts.rewrite_budget = 16;
        let c = compose(&d, &opts)?;
        let legs = [leg("T", "Q", &[("A", "A")], &[("h", "h")])];
        let (_, report) = c.verify_universal_within(&d, &q, &legs, opts.rewrite_budget)?;
        let entailed: Vec<&CheckOutcome> = report
            .records(CompositionCheckKind::EquationEntailed)
            .map(|r| &r.outcome)
            .collect();
        assert_eq!(
            entailed,
            [&CheckOutcome::Incomplete {
                reason: CompositionIncomplete::RewriteBudgetExhausted {
                    equation: "e".into(),
                    steps: 16,
                },
            }]
        );
        assert_eq!(report.status, CheckStatus::Incomplete);
        // With no equations on either side the check does not apply.
        let bare = compose(
            &TheoryDiagram::discrete(vec![("T", th("T", &["A"], &[], vec![]))]),
            &opts,
        )?;
        let (_, report) = bare.verify_universal(
            &TheoryDiagram::discrete(vec![("T", th("T", &["A"], &[], vec![]))]),
            &th("Q", &["A"], &[], vec![]),
            &[leg("T", "Q", &[("A", "A")], &[])],
        )?;
        let entailed: Vec<&CheckOutcome> = report
            .records(CompositionCheckKind::EquationEntailed)
            .map(|r| &r.outcome)
            .collect();
        assert_eq!(
            entailed,
            [&CheckOutcome::Inapplicable {
                reason: InapplicableReason::NoEquations
            }]
        );
        Ok(())
    }

    mod property {
        use super::*;
        use proptest::prelude::*;

        /// One side of a generated star diagram.
        #[derive(Clone, Debug)]
        struct SideSpec {
            /// Whether the side renames the apex symbols (to `M_*`).
            rename: bool,
            /// Which of the extra sorts `X`, `Y` it declares.
            extra_sorts: Vec<bool>,
            /// Extra operations `(inputs, output)`, as indices into its sorts.
            extra_ops: Vec<(Vec<usize>, usize)>,
            /// Whether it states an equation `e` on its first operation.
            equation: bool,
        }

        /// A star: an apex and sides that each extend it.
        #[derive(Clone, Debug)]
        struct Spec {
            base_sorts: usize,
            base_ops: Vec<(Vec<usize>, usize)>,
            sides: Vec<SideSpec>,
        }

        fn arb_ops() -> impl Strategy<Value = Vec<(Vec<usize>, usize)>> {
            prop::collection::vec((prop::collection::vec(0..6usize, 0..=2), 0..6usize), 0..=2)
        }

        fn arb_spec() -> impl Strategy<Value = Spec> {
            let side = (
                any::<bool>(),
                prop::collection::vec(any::<bool>(), 2),
                arb_ops(),
                any::<bool>(),
            )
                .prop_map(|(rename, extra_sorts, extra_ops, equation)| SideSpec {
                    rename,
                    extra_sorts,
                    extra_ops,
                    equation,
                });
            (1..=2usize, arb_ops(), prop::collection::vec(side, 1..=3)).prop_map(
                |(base_sorts, base_ops, sides)| Spec {
                    base_sorts,
                    base_ops,
                    sides,
                },
            )
        }

        type OpSpec = (String, Vec<String>, String);

        /// A theory of simple sorts and the given operations, with an
        /// equation `e : op(x..) = x_j` on the first operation whose output
        /// sort is one of its input sorts, when `equation` is set.
        fn theory(name: &str, sorts: &[String], ops: &[OpSpec], equation: bool) -> Theory {
            let operations: Vec<Operation> = ops
                .iter()
                .map(|(o, ins, out)| {
                    Operation::new(
                        o.as_str(),
                        ins.iter()
                            .enumerate()
                            .map(|(i, s)| (Arc::from(format!("x{i}")), SortExpr::from(s.as_str())))
                            .collect(),
                        out.as_str(),
                    )
                })
                .collect();
            let eqs = ops
                .iter()
                .find_map(|(o, ins, out)| {
                    let j = ins.iter().position(|s| s == out)?;
                    let args = (0..ins.len()).map(|i| Term::var(format!("x{i}"))).collect();
                    Some(Equation::new(
                        "e",
                        Term::app(o.as_str(), args),
                        Term::var(format!("x{j}")),
                    ))
                })
                .filter(|_| equation)
                .into_iter()
                .collect();
            Theory::new(
                name,
                sorts.iter().map(|s| Sort::simple(s.as_str())).collect(),
                operations,
                eqs,
            )
        }

        fn ops_over(prefix: &str, specs: &[(Vec<usize>, usize)], sorts: &[String]) -> Vec<OpSpec> {
            specs
                .iter()
                .enumerate()
                .map(|(i, (ins, out))| {
                    (
                        format!("{prefix}{i}"),
                        ins.iter().map(|k| sorts[k % sorts.len()].clone()).collect(),
                        sorts[out % sorts.len()].clone(),
                    )
                })
                .collect()
        }

        fn build(spec: &Spec) -> TheoryDiagram {
            let base_sorts: Vec<String> = ["A", "B"][..spec.base_sorts]
                .iter()
                .map(|s| (*s).to_owned())
                .collect();
            let base_ops = ops_over("f", &spec.base_ops, &base_sorts);
            let apex = theory("Apex", &base_sorts, &base_ops, false);
            let mut nodes = vec![("apex".to_owned(), apex)];
            let mut edges = Vec::new();
            for (i, side) in spec.sides.iter().enumerate() {
                let label = format!("s{i}");
                let rn = |n: &str| {
                    if side.rename {
                        format!("M_{n}")
                    } else {
                        n.to_owned()
                    }
                };
                let mut sorts: Vec<String> = base_sorts.iter().map(|s| rn(s)).collect();
                for (k, extra) in ["X", "Y"].iter().enumerate() {
                    if side.extra_sorts[k] {
                        sorts.push((*extra).to_owned());
                    }
                }
                let mut ops: Vec<OpSpec> = base_ops
                    .iter()
                    .map(|(o, ins, out)| (rn(o), ins.iter().map(|s| rn(s)).collect(), rn(out)))
                    .collect();
                ops.extend(ops_over("p", &side.extra_ops, &sorts));
                nodes.push((label.clone(), theory(&label, &sorts, &ops, side.equation)));
                let sort_map: HashMap<Arc<str>, Arc<str>> = base_sorts
                    .iter()
                    .map(|s| (Arc::from(s.as_str()), Arc::from(rn(s))))
                    .collect();
                let op_map: HashMap<Arc<str>, Arc<str>> = base_ops
                    .iter()
                    .map(|(o, _, _)| (Arc::from(o.as_str()), Arc::from(rn(o))))
                    .collect();
                edges.push(DiagramEdge {
                    src: "apex".into(),
                    tgt: label.as_str().into(),
                    morphism: TheoryMorphism::new(
                        format!("to_{label}"),
                        "Apex",
                        label.as_str(),
                        sort_map,
                        op_map,
                    ),
                });
            }
            TheoryDiagram {
                nodes: nodes
                    .into_iter()
                    .map(|(label, theory)| DiagramNode {
                        label: label.into(),
                        theory,
                    })
                    .collect(),
                edges,
            }
        }

        /// Prefix every symbol of a theory, keeping the order of names.
        fn prefixed(t: &Theory) -> Theory {
            let p = |n: &Arc<str>| Arc::from(format!("r_{n}"));
            let sorts: HashMap<Arc<str>, Arc<str>> = t
                .sorts
                .iter()
                .map(|s| (Arc::clone(&s.name), p(&s.name)))
                .collect();
            let ops: HashMap<Arc<str>, Arc<str>> = t
                .ops
                .iter()
                .map(|o| (Arc::clone(&o.name), p(&o.name)))
                .collect();
            Theory::new(
                Arc::clone(&t.name),
                t.sorts.iter().map(|s| Sort::simple(p(&s.name))).collect(),
                t.ops
                    .iter()
                    .map(|o| {
                        let mut op = translate_op(o, &sorts, &ops);
                        op.name = p(&o.name);
                        op
                    })
                    .collect(),
                t.eqs
                    .iter()
                    .map(|e| {
                        let mut eq = e.rename_ops(&ops);
                        eq.name = p(&e.name);
                        eq
                    })
                    .collect(),
            )
        }

        fn prefixed_diagram(d: &TheoryDiagram) -> TheoryDiagram {
            let p = |n: &Arc<str>| Arc::from(format!("r_{n}"));
            TheoryDiagram {
                nodes: d
                    .nodes
                    .iter()
                    .map(|n| DiagramNode {
                        label: Arc::clone(&n.label),
                        theory: prefixed(&n.theory),
                    })
                    .collect(),
                edges: d
                    .edges
                    .iter()
                    .map(|e| DiagramEdge {
                        src: Arc::clone(&e.src),
                        tgt: Arc::clone(&e.tgt),
                        morphism: TheoryMorphism::new(
                            Arc::clone(&e.morphism.name),
                            Arc::clone(&e.morphism.domain),
                            Arc::clone(&e.morphism.codomain),
                            e.morphism
                                .sort_map
                                .iter()
                                .map(|(a, b)| (p(a), p(b)))
                                .collect(),
                            e.morphism
                                .op_rename_map()
                                .iter()
                                .map(|(a, b)| (p(a), p(b)))
                                .collect::<HashMap<Arc<str>, Arc<str>>>(),
                        ),
                    })
                    .collect(),
            }
        }

        /// The classes of a composition, as sets of source symbols per kind.
        fn classes(c: &Composition) -> BTreeSet<(SymKind, BTreeSet<SymRef>)> {
            let mut out = BTreeSet::new();
            for kind in [SymKind::Sort, SymKind::Op, SymKind::Eq] {
                let images: BTreeSet<&Arc<str>> = c.provenance.map(kind).values().collect();
                for image in images {
                    out.insert((
                        kind,
                        c.provenance.members(kind, image).into_iter().collect(),
                    ));
                }
            }
            out
        }

        fn unprefix(
            classes: BTreeSet<(SymKind, BTreeSet<SymRef>)>,
        ) -> BTreeSet<(SymKind, BTreeSet<SymRef>)> {
            classes
                .into_iter()
                .map(|(k, members)| {
                    (
                        k,
                        members
                            .into_iter()
                            .map(|m| {
                                SymRef::new(
                                    Arc::clone(&m.node),
                                    m.name.strip_prefix("r_").unwrap_or(&m.name),
                                )
                            })
                            .collect(),
                    )
                })
                .collect()
        }

        fn fail(e: impl std::fmt::Display) -> TestCaseError {
            TestCaseError::fail(e.to_string())
        }

        /// The sort names of an open diagram.
        const POOL: [&str; 4] = ["p", "q", "r", "s"];

        /// A diagram whose nodes may use sorts they do not declare.
        #[derive(Clone, Debug)]
        struct OpenSpec {
            /// Per node, which pool sorts it declares and the output sorts
            /// (pool indices) of its constants.
            nodes: Vec<(Vec<bool>, Vec<usize>)>,
            /// Edges `(from, to, choices)`: the endpoints are reduced modulo
            /// the node count and ordered upwards, and `choices[k]` picks the
            /// image of the source's `k`-th sort symbol.
            edges: Vec<(usize, usize, Vec<usize>)>,
        }

        fn arb_open_spec() -> impl Strategy<Value = OpenSpec> {
            let node = (
                prop::collection::vec(any::<bool>(), POOL.len()),
                prop::collection::vec(0..POOL.len(), 0..=2),
            );
            let edge = (
                0..5usize,
                0..5usize,
                prop::collection::vec(0..8usize, POOL.len()),
            );
            (
                prop::collection::vec(node, 2..=5),
                prop::collection::vec(edge, 0..=5),
            )
                .prop_map(|(nodes, edges)| OpenSpec { nodes, edges })
        }

        /// Build the open diagram with every sort name `POOL[k]` written
        /// `POOL[rename[k]]`. Each edge copies its source's constants into
        /// its target, so every leg is a morphism: a declared sort goes to a
        /// declared sort, an undeclared one to any pool sort, which the
        /// copied constant then uses.
        fn build_open(spec: &OpenSpec, rename: &[usize]) -> TheoryDiagram {
            let n = spec.nodes.len();
            let name = |k: usize| POOL[rename[k]].to_owned();
            let mut declared: Vec<Vec<usize>> = spec
                .nodes
                .iter()
                .map(|(d, _)| (0..POOL.len()).filter(|k| d[*k]).collect())
                .collect();
            let mut ops: Vec<Vec<(String, usize)>> = spec
                .nodes
                .iter()
                .map(|(_, outs)| {
                    outs.iter()
                        .enumerate()
                        .map(|(i, k)| (format!("c{i}"), *k))
                        .collect()
                })
                .collect();
            let mut edges: Vec<(usize, usize, &Vec<usize>)> = spec
                .edges
                .iter()
                .filter_map(|(a, b, choices)| {
                    let (a, b) = (a % n, b % n);
                    (a != b).then(|| (a.min(b), a.max(b), choices))
                })
                .collect();
            edges.sort_by_key(|(src, _, _)| *src);
            let mut built: Vec<OpenLeg> = Vec::new();
            for (index, (src, tgt, choices)) in edges.into_iter().enumerate() {
                let mut symbols: Vec<usize> = declared[src].clone();
                for (_, k) in &ops[src] {
                    if !symbols.contains(k) {
                        symbols.push(*k);
                    }
                }
                if let (Some(first), true) =
                    (declared[src].first().copied(), declared[tgt].is_empty())
                {
                    declared[tgt].push(first);
                }
                let image = |at: usize, k: usize| {
                    let choice = choices[at];
                    if declared[src].contains(&k) {
                        declared[tgt][choice % declared[tgt].len()]
                    } else {
                        choice % POOL.len()
                    }
                };
                let sort_map: HashMap<Arc<str>, Arc<str>> = symbols
                    .iter()
                    .enumerate()
                    .map(|(at, k)| (Arc::from(name(*k)), Arc::from(name(image(at, *k)))))
                    .collect();
                let images: HashMap<usize, usize> = symbols
                    .iter()
                    .enumerate()
                    .map(|(at, k)| (*k, image(at, *k)))
                    .collect();
                let copies: Vec<(String, String, usize)> = ops[src]
                    .iter()
                    .map(|(o, k)| (o.clone(), format!("e{index}_{o}"), images[k]))
                    .collect();
                let op_map: HashMap<Arc<str>, Arc<str>> = copies
                    .iter()
                    .map(|(o, copy, _)| (Arc::from(o.as_str()), Arc::from(copy.as_str())))
                    .collect();
                ops[tgt].extend(copies.into_iter().map(|(_, copy, k)| (copy, k)));
                built.push((src, tgt, sort_map, op_map));
            }
            assemble_open(&declared, &ops, built, &name)
        }

        /// One leg of an open diagram: its endpoints, sort map and operation map.
        type OpenLeg = (
            usize,
            usize,
            HashMap<Arc<str>, Arc<str>>,
            HashMap<Arc<str>, Arc<str>>,
        );

        /// The nodes `n{i}` (theories `T{i}`) and legs of an open diagram.
        fn assemble_open(
            declared: &[Vec<usize>],
            ops: &[Vec<(String, usize)>],
            legs: Vec<OpenLeg>,
            name: &dyn Fn(usize) -> String,
        ) -> TheoryDiagram {
            let theory_of = |i: usize| {
                Theory::new(
                    format!("T{i}"),
                    declared[i].iter().map(|k| Sort::simple(name(*k))).collect(),
                    ops[i]
                        .iter()
                        .map(|(o, k)| Operation::new(o.as_str(), vec![], name(*k)))
                        .collect(),
                    vec![],
                )
            };
            TheoryDiagram {
                nodes: (0..declared.len())
                    .map(|i| DiagramNode {
                        label: format!("n{i}").into(),
                        theory: theory_of(i),
                    })
                    .collect(),
                edges: legs
                    .into_iter()
                    .map(|(src, tgt, sort_map, op_map)| DiagramEdge {
                        src: format!("n{src}").into(),
                        tgt: format!("n{tgt}").into(),
                        morphism: TheoryMorphism::new(
                            format!("m{src}_{tgt}"),
                            format!("T{src}"),
                            format!("T{tgt}"),
                            sort_map,
                            op_map,
                        ),
                    })
                    .collect(),
            }
        }

        /// Sort classes renamed back along the pool permutation `rename`.
        fn unrename(
            classes: BTreeSet<(SymKind, BTreeSet<SymRef>)>,
            rename: &[usize],
        ) -> BTreeSet<(SymKind, BTreeSet<SymRef>)> {
            let back = |n: &Arc<str>| -> Arc<str> {
                POOL.iter().position(|p| **p == **n).map_or_else(
                    || Arc::clone(n),
                    |j| Arc::from(POOL[rename.iter().position(|r| *r == j).unwrap_or(j)]),
                )
            };
            classes
                .into_iter()
                .map(|(kind, members)| {
                    let members = members
                        .into_iter()
                        .map(|m| {
                            if kind == SymKind::Sort {
                                SymRef::new(Arc::clone(&m.node), back(&m.name))
                            } else {
                                m
                            }
                        })
                        .collect();
                    (kind, members)
                })
                .collect()
        }

        /// The outcome of a composition up to its error payload.
        fn outcome_tag(r: &Result<Composition, CompositionError>) -> String {
            match r {
                Ok(_) => "ok".to_owned(),
                Err(CompositionError::NameClash { kind, .. }) => format!("name_clash {kind:?}"),
                Err(e) => format!("{:?}", std::mem::discriminant(e)),
            }
        }

        fn all_modes() -> [ComposeOptions; 3] {
            [
                explicit(ClashPolicy::Freshen),
                explicit(ClashPolicy::Reject),
                amalgamate(),
            ]
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(64))]

            #[test]
            fn result_symbols_are_one_per_class(spec in arb_spec()) {
                let d = build(&spec);
                for opts in all_modes() {
                    let Ok(c) = compose(&d, &opts) else { continue };
                    for (kind, declared) in [(SymKind::Sort, c.theory.sorts.len()), (SymKind::Op, c.theory.ops.len())] {
                        let images: BTreeSet<&Arc<str>> = c.provenance.map(kind).values().collect();
                        prop_assert_eq!(images.len(), declared);
                    }
                    for sort in &c.theory.sorts {
                        prop_assert!(!c.provenance.members(SymKind::Sort, &sort.name).is_empty());
                    }
                    for op in &c.theory.ops {
                        prop_assert!(!c.provenance.members(SymKind::Op, &op.name).is_empty());
                    }
                }
            }

            #[test]
            fn compose_results_typecheck(spec in arb_spec()) {
                let d = build(&spec);
                let freshened = compose(&d, &explicit(ClashPolicy::Freshen)).map_err(fail)?;
                crate::typecheck::typecheck_theory(&freshened.theory).map_err(fail)?;
                for opts in all_modes() {
                    if let Ok(c) = compose(&d, &opts) {
                        prop_assert!(typecheck_signature(&c.theory).is_ok());
                        crate::typecheck::typecheck_theory(&c.theory).map_err(fail)?;
                        for (node, inclusion) in d.nodes.iter().zip(&c.inclusions) {
                            check_morphism(inclusion, &node.theory, &c.theory).map_err(fail)?;
                        }
                    }
                }
            }

            #[test]
            fn result_vectors_are_sorted_and_extends_is_empty(spec in arb_spec()) {
                let d = build(&spec);
                for opts in all_modes() {
                    let Ok(c) = compose(&d, &opts) else { continue };
                    let t = &c.theory;
                    prop_assert!(t.extends.is_empty());
                    prop_assert!(t.sorts.windows(2).all(|w| w[0].name < w[1].name));
                    prop_assert!(t.ops.windows(2).all(|w| w[0].name < w[1].name));
                    prop_assert!(t.eqs.windows(2).all(|w| w[0].name < w[1].name));
                    prop_assert!(t.directed_eqs.windows(2).all(|w| w[0].name < w[1].name));
                    prop_assert!(t.policies.windows(2).all(|w| w[0].name < w[1].name));
                }
            }

            #[test]
            fn check_records_are_sorted(spec in arb_spec()) {
                let d = build(&spec);
                for opts in all_modes() {
                    let Ok(c) = compose(&d, &opts) else { continue };
                    prop_assert!(c
                        .report
                        .checks
                        .windows(2)
                        .all(|w| (w[0].check, &w[0].subject) < (w[1].check, &w[1].subject)));
                }
            }

            #[test]
            fn diagram_colimit_is_invariant_under_node_and_edge_permutation(
                spec in arb_spec(),
                rotate in 0..4usize,
            ) {
                let d = build(&spec);
                let mut permuted = d.clone();
                permuted.nodes.reverse();
                let n = permuted.nodes.len();
                permuted.nodes.rotate_left(rotate % n);
                permuted.edges.reverse();
                for opts in all_modes() {
                    let (a, b) = (compose(&d, &opts), compose(&permuted, &opts));
                    prop_assert_eq!(a.is_ok(), b.is_ok());
                    let (Ok(a), Ok(b)) = (a, b) else { continue };
                    prop_assert_eq!(serde_json::to_vec(&a.theory).map_err(fail)?, serde_json::to_vec(&b.theory).map_err(fail)?);
                    prop_assert_eq!(serde_json::to_vec(&a.provenance).map_err(fail)?, serde_json::to_vec(&b.provenance).map_err(fail)?);
                    prop_assert_eq!(&a.report.freshened, &b.report.freshened);
                    for (node, inclusion) in d.nodes.iter().zip(&a.inclusions) {
                        let at = permuted.nodes.iter().position(|m| m.label == node.label).ok_or_else(|| fail("label lost"))?;
                        prop_assert_eq!(serde_json::to_vec(inclusion).map_err(fail)?, serde_json::to_vec(&b.inclusions[at]).map_err(fail)?);
                    }
                }
            }

            #[test]
            fn renaming_symbols_by_a_bijection_renames_the_result(spec in arb_spec()) {
                let d = build(&spec);
                let renamed = prefixed_diagram(&d);
                for opts in all_modes() {
                    let (a, b) = (compose(&d, &opts), compose(&renamed, &opts));
                    prop_assert_eq!(a.is_ok(), b.is_ok());
                    let (Ok(a), Ok(b)) = (a, b) else { continue };
                    prop_assert_eq!(classes(&a), unprefix(classes(&b)));
                    if a.report.freshened.is_empty() {
                        let expected = prefixed(&a.theory);
                        prop_assert_eq!(names(&b.theory.sorts, |s| &s.name), names(&expected.sorts, |s| &s.name));
                        prop_assert_eq!(&b.theory.ops, &expected.ops);
                    }
                }
            }

            #[test]
            fn explicit_and_amalgamate_agree_on_name_clash_free_diagrams(spec in arb_spec()) {
                let d = build(&spec);
                let explicit = compose(&d, &explicit(ClashPolicy::Reject));
                if let Ok(e) = explicit {
                    if e.report.name_clash_free {
                        let a = compose(&d, &amalgamate()).map_err(fail)?;
                        prop_assert_eq!(&a.theory, &e.theory);
                        prop_assert_eq!(&a.provenance, &e.provenance);
                        prop_assert_eq!(&a.inclusions, &e.inclusions);
                    }
                }
            }
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(512))]

            #[test]
            fn open_diagrams_compose_independently_of_order_and_names(
                spec in arb_open_spec(),
                rename in Just(vec![0usize, 1, 2, 3]).prop_shuffle(),
                rotate in 0..5usize,
            ) {
                let identity = [0usize, 1, 2, 3];
                let d = build_open(&spec, &identity);
                let renamed = build_open(&spec, &rename);
                let mut permuted = d.clone();
                permuted.nodes.reverse();
                let n = permuted.nodes.len();
                permuted.nodes.rotate_left(rotate % n);
                permuted.edges.reverse();
                // Freshen and Amalgamate resolve every clash of result names,
                // so the only refusal they share with Reject that depends on
                // names is the one for undeclared sorts, and their outcome
                // must survive the renaming. Reject also refuses a clash of
                // result names, and which names clash depends on how ties
                // between member names are broken; its classes must still
                // agree whenever both sides compose.
                for opts in all_modes() {
                    let (a, b, p) = (compose(&d, &opts), compose(&renamed, &opts), compose(&permuted, &opts));
                    if opts.clash != ClashPolicy::Reject || opts.mode == CompositionMode::Amalgamate {
                        prop_assert_eq!(outcome_tag(&a), outcome_tag(&b), "renamed by {:?}: {:?} against {:?}", rename, a.as_ref().err(), b.as_ref().err());
                    }
                    prop_assert_eq!(&a.as_ref().err(), &p.as_ref().err());
                    if let (Ok(a), Ok(p)) = (&a, &p) {
                        prop_assert_eq!(serde_json::to_vec(&a.theory).map_err(fail)?, serde_json::to_vec(&p.theory).map_err(fail)?);
                        prop_assert_eq!(serde_json::to_vec(&a.provenance).map_err(fail)?, serde_json::to_vec(&p.provenance).map_err(fail)?);
                    }
                    let (Ok(a), Ok(b)) = (a, b) else { continue };
                    prop_assert_eq!(classes(&a), unrename(classes(&b), &rename));
                    prop_assert_eq!(a.report.name_clash_free, b.report.name_clash_free);
                }
            }
        }
    }
}
