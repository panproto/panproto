//! Failure witnesses: checking a diagnostic against the instance it refutes.
//!
//! A certifying algorithm returns, with each answer, a witness that a checker
//! much simpler than the algorithm confirms (`McConnell` et al. 2011, stated).
//! A [`Diagnostic`]'s witness is its [`Violation`] payload with its subject
//! and elements, and [`Diagnostic::verify`] is the checker: it reads the
//! elements the violation names and confirms the refutation, without
//! rerunning the validator over the instance. A cycle is confirmed by
//! walking its edges, a duplicate key by reading the key of each element it
//! names, an unknown anchor by one schema lookup, and so on.
//!
//! Besides the violation itself, a diagnostic verifies only when it is the
//! diagnostic of a compiled check of the plan with the check's class,
//! requirement and declared source, when its subject is an instance of that
//! check in the instance, and when its elements are exactly the elements the
//! violation names, ascending.
//!
//! Verification confirms that a diagnostic refutes the instance, not that it
//! is the diagnostic the validator reports. Where one fault has several
//! witnesses, each verifies: a `Cycle` verifies with any closed walk of
//! edges labelled acyclic through its component, so the validator's walk
//! rotated to start at another node, or walked twice, verifies too; and a
//! `ParentMismatch` verifies with the parent of any arc into the node other
//! than the recorded parent, while the validator names the first. A
//! diagnostic altered so that it no longer refutes the instance (another
//! subject, another element, a walk that does not close or leaves the
//! component, a key the named elements do not share) fails to verify.
//!
//! ```
//! use panproto_gat::CheckStatus;
//! use panproto_inst::validation::{
//!     ExternalVerdicts, ValidationBudget, ValidationOptions, ValidationPlan, Violation,
//! };
//! use panproto_inst::{GInstance, Node, ViewRef};
//! use panproto_schema::{Protocol, Schema, Vertex};
//!
//! let mut schema = Schema::default();
//! schema.vertices.insert(
//!     "person".into(),
//!     Vertex { id: "person".into(), kind: "object".into(), nsid: None },
//! );
//! let plan = ValidationPlan::compile(&schema, &Protocol::default(), &ValidationOptions::default());
//! let ghost = GInstance::new().with_node(Node::new(0, "ghost"));
//! let report =
//!     plan.validate_graph(&ghost, &ValidationBudget::default(), &ExternalVerdicts::default());
//! assert_eq!(report.status, CheckStatus::Failed);
//! let diagnostic = &report.diagnostics[0];
//! assert!(diagnostic.verify(&plan, &ghost));
//!
//! // A diagnostic naming another anchor is not a refutation of this graph.
//! let mut forged = diagnostic.clone();
//! forged.violation = Violation::UnknownAnchor { anchor: "phantom".into() };
//! assert!(!forged.verify(&plan, &ghost));
//! ```
//!
//! A cycle's walk is one witness among several:
//!
//! ```
//! use panproto_inst::validation::{
//!     CheckKind, ExternalVerdicts, ValidationBudget, ValidationOptions, ValidationPlan, Violation,
//! };
//! use panproto_inst::{GInstance, Node};
//! use panproto_schema::{
//!     Constraint, ConstraintSemantics, Edge, Protocol, Schema, Vertex,
//! };
//!
//! let next = Edge { src: "item".into(), tgt: "item".into(), kind: "prop".into(), name: Some("next".into()) };
//! let mut schema = Schema::default();
//! schema.vertices.insert(
//!     "item".into(),
//!     Vertex { id: "item".into(), kind: "object".into(), nsid: None },
//! );
//! schema.edges.insert(next.clone(), "prop".into());
//! schema.outgoing.entry("item".into()).or_default().push(next.clone());
//! schema.incoming.entry("item".into()).or_default().push(next.clone());
//! schema.between.entry(("item".into(), "item".into())).or_default().push(next.clone());
//! schema.constraints.insert(
//!     "item".into(),
//!     vec![Constraint { sort: "acyclic".into(), value: "next".into() }],
//! );
//! let protocol = Protocol {
//!     constraint_sorts: vec!["acyclic".into()],
//!     constraint_semantics: [("acyclic".to_owned(), ConstraintSemantics::Acyclic)].into(),
//!     ..Protocol::default()
//! };
//! let plan = ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
//! let ring = GInstance::new()
//!     .with_node(Node::new(0, "item"))
//!     .with_node(Node::new(1, "item"))
//!     .with_node(Node::new(2, "item"))
//!     .with_edge(0, 1, next.clone())
//!     .with_edge(1, 2, next.clone())
//!     .with_edge(2, 0, next);
//! let report =
//!     plan.validate_graph(&ring, &ValidationBudget::default(), &ExternalVerdicts::default());
//! let cycle = report
//!     .diagnostics
//!     .iter()
//!     .find(|d| d.check.kind == CheckKind::Acyclic)
//!     .expect("the ring is a cycle");
//! let Violation::Cycle { scc, witness } = &cycle.violation else { unreachable!() };
//! assert!(cycle.verify(&plan, &ring));
//!
//! // The same walk from another node, and walked twice: other witnesses of
//! // the same cycle, which verify although the validator reports neither.
//! let mut rotated = cycle.clone();
//! let mut walk = witness.clone();
//! walk.rotate_left(1);
//! rotated.violation = Violation::Cycle { scc: scc.clone(), witness: walk };
//! assert!(rotated.verify(&plan, &ring) && rotated != *cycle);
//! let mut doubled = cycle.clone();
//! doubled.violation = Violation::Cycle { scc: scc.clone(), witness: witness.repeat(2) };
//! assert!(doubled.verify(&plan, &ring) && doubled != *cycle);
//!
//! // A walk that does not close is no witness.
//! let mut open = cycle.clone();
//! open.violation = Violation::Cycle { scc: scc.clone(), witness: witness[..2].to_vec() };
//! assert!(!open.verify(&plan, &ring));
//! ```

use std::collections::{BTreeSet, HashSet, VecDeque};

use panproto_gat::Name;
use panproto_schema::{CanonicalWriter, Cardinality, Edge, Key, KeyPart, RefSource, Reference};

use crate::attributes::{AttributeSchema, AttributeViolation};
use crate::fan_violation::FanViolation;
use crate::functor::FInstance;
use crate::ginstance::{EdgeId, FanDomain, GInstance};
use crate::value::Value;
use crate::view_ref::{RowId, ViewRef};
use crate::wtype::WInstance;

use super::checks::{
    AnchorFamilies, Context, EdgeRec, InstanceView, TreeArcs, edges_at, evaluate, instances,
};
use super::plan::{Compiled, ExternalVerdicts, ValidationPlan};
use super::report::{CheckKind, Diagnostic, EndpointSide, FanRecord, Violation, write_violation};
use super::state::{key_values, part_name, part_value, path_values, scope_of};

mod sealed {
    use super::InstanceView;

    /// Reads an instance of any shape through the validator's view of it.
    pub trait Sealed {
        /// Call `f` with the view of this instance.
        fn with_view<R>(&self, f: impl FnOnce(InstanceView<'_>) -> R) -> R;
    }
}

/// An instance validation reads: a [`GInstance`], a [`WInstance`] or an
/// [`FInstance`]. Sealed: these are the three shapes.
pub trait CheckedInstance: sealed::Sealed {}

impl sealed::Sealed for GInstance {
    fn with_view<R>(&self, f: impl FnOnce(InstanceView<'_>) -> R) -> R {
        f(InstanceView::Graph(self))
    }
}

impl sealed::Sealed for WInstance {
    fn with_view<R>(&self, f: impl FnOnce(InstanceView<'_>) -> R) -> R {
        let arcs = TreeArcs::new(self);
        f(InstanceView::Tree(self, &arcs))
    }
}

impl sealed::Sealed for FInstance {
    fn with_view<R>(&self, f: impl FnOnce(InstanceView<'_>) -> R) -> R {
        f(InstanceView::Table(self))
    }
}

impl CheckedInstance for GInstance {}
impl CheckedInstance for WInstance {}
impl CheckedInstance for FInstance {}

impl Diagnostic {
    /// Whether this diagnostic is a true refutation of `instance` under
    /// `plan`: its check is compiled by the plan with this class,
    /// requirement and source; its subject is an instance of the check; its
    /// elements are exactly those its violation names, ascending; and the
    /// violation holds of the elements it names.
    ///
    /// A fault with several witnesses verifies with each of them, not only
    /// with the one the validator reports (see the [module
    /// documentation](self)): any closed walk of a `Cycle`'s component, and
    /// any arc's parent other than the recorded one for a `ParentMismatch`.
    ///
    /// Each violation is confirmed by reading what it names: an
    /// `UnknownAnchor` by looking its anchor up in the schema, an
    /// `IncoherentEndpoint` by reading the edge's endpoint, a `Cycle` by
    /// walking its edges (consecutive edges join, the last edge returns to
    /// the first edge's source, every edge carries a label declared acyclic)
    /// and checking that its component is the strongly connected component
    /// the walk lies in, a `DuplicateKey` by reading the key of each element
    /// it names and confirming that no other element of its scope shares it,
    /// a `DanglingReference` or `AmbiguousReference` by reading the elements
    /// of the target vertex that carry the named key, and so on. A
    /// `ValueConstraint` is confirmed by reading the value it names at the
    /// subject and testing it against the compiled constraint.
    ///
    /// An `ExternalRejected` diagnostic is confirmed as an instance of its
    /// external check: the verdict is the application's, and panproto never
    /// evaluates the rule behind it. [`Diagnostic::verify_with_verdicts`]
    /// also confirms the verdict.
    #[must_use]
    pub fn verify<I: CheckedInstance>(&self, plan: &ValidationPlan, instance: &I) -> bool {
        instance.with_view(|view| verify_in(self, plan, view, None))
    }

    /// [`Diagnostic::verify`], confirming an `ExternalRejected` diagnostic
    /// against the verdicts the validation read: the verdict of its check on
    /// its subject rejects it, with the witness the diagnostic names.
    #[must_use]
    pub fn verify_with_verdicts<I: CheckedInstance>(
        &self,
        plan: &ValidationPlan,
        instance: &I,
        external: &ExternalVerdicts,
    ) -> bool {
        instance.with_view(|view| verify_in(self, plan, view, Some(external)))
    }
}

/// The canonical bytes of a violation, which compare values by their bytes
/// (a `NaN` value equals itself).
fn violation_bytes(v: &Violation) -> Vec<u8> {
    let mut w = CanonicalWriter::new();
    write_violation(&mut w, v);
    w.into_bytes()
}

/// Whether two values are one value: equal canonical bytes.
fn same_value(a: &Value, b: &Value) -> bool {
    a.canonical_cmp(b).is_eq()
}

/// Whether two value lists are one list, value by value.
fn same_values(a: &[Value], b: &[Value]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same_value(x, y))
}

/// `refs`, ascending and without repeats.
fn sorted(mut refs: Vec<ViewRef>) -> Vec<ViewRef> {
    refs.sort();
    refs.dedup();
    refs
}

fn verify_in(
    d: &Diagnostic,
    plan: &ValidationPlan,
    view: InstanceView<'_>,
    external: Option<&ExternalVerdicts>,
) -> bool {
    let Some(spec) = plan.checks().get(&d.check) else {
        return false;
    };
    if plan.is_statically_incomplete(&d.check)
        || !spec.applies_to(view.shape())
        || spec.class != d.class
        || spec.requirement != d.requirement
        || plan.options.sources.0.get(&d.check.decl) != d.source.as_ref()
    {
        return false;
    }
    let none = ExternalVerdicts::default();
    let ctx = Context::new(plan, view, external.unwrap_or(&none));
    if !instances(&ctx, &d.check).contains(&d.subject) {
        return false;
    }
    expected_elements(&ctx, d)
        .is_some_and(|elements| elements == d.elements && holds(&ctx, d, external.is_some()))
}

/// The edges a graph fan violation names.
fn fan_edges(reason: &FanViolation) -> Vec<ViewRef> {
    match reason {
        FanViolation::MultipleParents { edges } | FanViolation::DuplicateLabel { edges, .. } => {
            edges.iter().map(|e| ViewRef::Edge(*e)).collect()
        }
        FanViolation::ReservedKindOutsideFan { edge }
        | FanViolation::ForeignIncidentEdge { edge } => vec![ViewRef::Edge(*edge)],
        FanViolation::MissingParent
        | FanViolation::UnknownLabel { .. }
        | FanViolation::UnknownHyperEdge { .. }
        | FanViolation::MissingChild { .. } => Vec::new(),
    }
}

/// The endpoint at `side` of the edge `e`.
fn endpoint(view: InstanceView<'_>, e: &ViewRef, side: EndpointSide) -> Option<ViewRef> {
    let rec = edges_at(view, e).into_iter().next()?;
    Some(match side {
        EndpointSide::Src => rec.src,
        EndpointSide::Tgt => rec.tgt,
    })
}

/// The elements the diagnostic's violation names, with its subject,
/// ascending; `None` when the violation cannot have the diagnostic's
/// subject.
fn expected_elements(ctx: &Context<'_>, d: &Diagnostic) -> Option<Vec<ViewRef>> {
    let view = ctx.view;
    let subject = d.subject.clone();
    let with_subject = |mut refs: Vec<ViewRef>| -> Option<Vec<ViewRef>> {
        refs.push(subject.clone()?);
        Some(sorted(refs))
    };
    match &d.violation {
        Violation::UnknownAnchor { .. }
        | Violation::UndeclaredEdge { .. }
        | Violation::ReservedVocabulary
        | Violation::MissingRequiredEdge { .. }
        | Violation::ValueConstraint { .. }
        | Violation::MissingValue { .. }
        | Violation::UndeclaredAttribute { .. }
        | Violation::MistypedAttribute { .. }
        | Violation::DanglingReference { .. }
        | Violation::Unreachable
        | Violation::ExternalRejected { .. } => with_subject(Vec::new()),
        Violation::IncoherentEndpoint { side, .. } => {
            let e = subject.clone()?;
            with_subject(vec![endpoint(view, &e, *side)?])
        }
        Violation::MalformedFan { reason, fan } => match (view, fan) {
            (InstanceView::Graph(_), None) => with_subject(fan_edges(reason)),
            (InstanceView::Tree(..), Some(record)) => with_subject(
                record
                    .children
                    .iter()
                    .map(|(_, child)| ViewRef::Node(*child))
                    .collect(),
            ),
            _ => None,
        },
        Violation::OutDegree { edges, .. } => with_subject(edges.clone()),
        Violation::DuplicatePosition { edges, .. } => {
            with_subject(edges.iter().map(|e| ViewRef::Edge(*e)).collect())
        }
        Violation::DuplicateKey { elems, .. } => subject.is_none().then(|| sorted(elems.clone())),
        Violation::AmbiguousReference { candidates, .. } => with_subject(candidates.clone()),
        Violation::Cycle { scc, witness } => {
            let mut refs = scc.clone();
            refs.extend(witness.iter().map(|e| ViewRef::Edge(*e)));
            subject.is_none().then(|| sorted(refs))
        }
        Violation::MissingRoot => subject.is_none().then(Vec::new),
        Violation::ParentMismatch { actual, .. } => {
            let ViewRef::Node(id) = subject.clone()? else {
                return None;
            };
            with_subject(
                actual
                    .iter()
                    .map(|p| ViewRef::Arc {
                        parent: *p,
                        child: id,
                    })
                    .collect(),
            )
        }
    }
}

/// Whether `refs` is strictly ascending.
fn ascending<T: Ord>(refs: &[T]) -> bool {
    refs.windows(2).all(|w| w[0] < w[1])
}

/// Whether the violation holds of what it names, for the diagnostic's check
/// and subject. `verdicts` says whether an external rejection is confirmed
/// against the context's verdicts.
fn holds(ctx: &Context<'_>, d: &Diagnostic, verdicts: bool) -> bool {
    match d.check.kind {
        CheckKind::NodeAnchor
        | CheckKind::EdgeDeclared
        | CheckKind::EndpointCoherence
        | CheckKind::FanWellFormed
        | CheckKind::ReservedVocabulary => element_holds(ctx, d),
        CheckKind::RequiredEdge
        | CheckKind::OutDegree
        | CheckKind::PositionDistinct
        | CheckKind::ValueConstraint
        | CheckKind::RequiredValue
        | CheckKind::Attribute => local_holds(ctx, d),
        CheckKind::KeyUnique | CheckKind::ReferenceResolves | CheckKind::Acyclic => {
            global_holds(ctx, d)
        }
        CheckKind::TreeRoot | CheckKind::TreeParentMap | CheckKind::TreeReachable => {
            tree_holds(ctx, d)
        }
        CheckKind::External => external_holds(ctx, d, verdicts),
        CheckKind::SchemaWellFormed | CheckKind::DeclarationInterpretable => false,
    }
}

/// The anchor, edge, endpoint, fan and reserved-vocabulary violations.
fn element_holds(ctx: &Context<'_>, d: &Diagnostic) -> bool {
    match (d.check.kind, &d.violation, d.subject.as_ref()) {
        (CheckKind::NodeAnchor, Violation::UnknownAnchor { anchor }, Some(n)) => {
            ctx.view
                .node(n)
                .is_some_and(|node| !node.fan && node.anchor == anchor)
                && !ctx.plan.schema.has_vertex(anchor)
        }
        (CheckKind::EdgeDeclared, Violation::UndeclaredEdge { label }, Some(e)) => {
            !ctx.plan.schema.edges.contains_key(label)
                && edges_at(ctx.view, e)
                    .iter()
                    .any(|rec| rec.label == label && !ctx.view.is_fan_edge(rec))
        }
        (
            CheckKind::EndpointCoherence,
            Violation::IncoherentEndpoint {
                side,
                expected,
                found,
            },
            Some(e),
        ) => incoherent(ctx, e, *side, expected, found),
        (CheckKind::FanWellFormed, Violation::MalformedFan { reason, fan }, Some(f)) => {
            malformed_fan(ctx, f, reason, fan.as_ref())
        }
        (CheckKind::ReservedVocabulary, Violation::ReservedVocabulary, Some(x)) => ctx
            .view
            .tree()
            .is_some_and(|t| crate::wtype_check::reserved_vocabulary(t).contains(x)),
        _ => false,
    }
}

/// The violations of one element's out-edges, values and attributes.
fn local_holds(ctx: &Context<'_>, d: &Diagnostic) -> bool {
    let compiled = ctx.plan.compiled.get(&d.check);
    match (d.check.kind, &d.violation, d.subject.as_ref()) {
        (CheckKind::RequiredEdge, Violation::MissingRequiredEdge { edge }, Some(n)) => {
            matches!(compiled, Some(Compiled::Required { edge: e }) if e == edge)
                && !non_fan_out_edges(ctx.view, n)
                    .iter()
                    .any(|rec| rec.label == edge)
        }
        (CheckKind::OutDegree, Violation::OutDegree { .. }, Some(n)) => {
            out_degree(ctx, compiled, n, &d.violation)
        }
        (
            CheckKind::PositionDistinct,
            Violation::DuplicatePosition { position, edges },
            Some(n),
        ) => match compiled {
            Some(Compiled::Ordering { edge: label }) => {
                let mut at: Vec<EdgeId> = ctx
                    .view
                    .out_edges(n)
                    .into_iter()
                    .filter(|rec| rec.label == label && rec.position == Some(*position))
                    .filter_map(|rec| match rec.at {
                        ViewRef::Edge(id) => Some(id),
                        _ => None,
                    })
                    .collect();
                at.sort();
                edges.len() > 1 && ascending(edges) && at == *edges
            }
            _ => false,
        },
        (CheckKind::ValueConstraint, Violation::ValueConstraint { .. }, Some(n)) => {
            value_refuted(ctx, d, n)
        }
        (CheckKind::RequiredValue, Violation::MissingValue { field }, Some(n)) => {
            missing_value(ctx, d, n, field.as_ref())
        }
        (
            CheckKind::Attribute,
            Violation::UndeclaredAttribute { .. } | Violation::MistypedAttribute { .. },
            Some(n),
        ) => attribute_refuted(ctx, n, &d.violation),
        _ => false,
    }
}

/// The key, reference and cycle violations.
fn global_holds(ctx: &Context<'_>, d: &Diagnostic) -> bool {
    let compiled = ctx.plan.compiled.get(&d.check);
    match (compiled, &d.violation, d.subject.as_ref()) {
        (
            Some(Compiled::KeyUnique { vertex, key: k }),
            Violation::DuplicateKey { key, elems },
            None,
        ) => duplicate_key(ctx, vertex, k, key, elems),
        (Some(Compiled::Reference { .. }), Violation::MissingValue { field }, Some(h)) => {
            missing_value(ctx, d, h, field.as_ref())
        }
        (
            Some(Compiled::Reference {
                reference: r,
                target,
            }),
            Violation::DanglingReference { reference, key },
            Some(h),
        ) => {
            r.id == *reference
                && holder_names(ctx, h, r, target, key)
                && target_elements(ctx, r, target, key).is_empty()
        }
        (
            Some(Compiled::Reference {
                reference: r,
                target,
            }),
            Violation::AmbiguousReference {
                reference,
                key,
                candidates,
            },
            Some(h),
        ) => {
            r.id == *reference
                && candidates.len() > 1
                && holder_names(ctx, h, r, target, key)
                && target_elements(ctx, r, target, key) == *candidates
        }
        (Some(Compiled::Acyclic { labels }), Violation::Cycle { scc, witness }, None) => {
            cycle(ctx, labels, scc, witness)
        }
        _ => false,
    }
}

/// The root, parent-map and reachability violations of a tree.
fn tree_holds(ctx: &Context<'_>, d: &Diagnostic) -> bool {
    let Some(t) = ctx.view.tree() else {
        return false;
    };
    match (d.check.kind, &d.violation, d.subject.as_ref()) {
        (CheckKind::TreeRoot, Violation::MissingRoot, None) => !t.nodes.contains_key(&t.root),
        (CheckKind::TreeReachable, Violation::Unreachable, Some(ViewRef::Node(id))) => {
            t.nodes.contains_key(id) && !reachable_from_root(t).contains(id)
        }
        (
            CheckKind::TreeParentMap,
            Violation::ParentMismatch { recorded, actual },
            Some(ViewRef::Node(id)),
        ) => {
            let into: Vec<u32> = t
                .arcs
                .iter()
                .filter(|(_, c, _)| c == id)
                .map(|(p, _, _)| *p)
                .collect();
            t.parent_map.get(id).copied() == *recorded
                && actual.as_ref().map_or_else(
                    || into.is_empty() && recorded.is_some(),
                    |p| into.contains(p) && Some(*p) != *recorded,
                )
        }
        _ => false,
    }
}

/// An external rejection names an instance of its external check; with
/// `verdicts`, the verdict of that check on the subject rejects it with the
/// witness the violation names.
fn external_holds(ctx: &Context<'_>, d: &Diagnostic, verdicts: bool) -> bool {
    match (
        ctx.plan.compiled.get(&d.check),
        &d.violation,
        d.subject.as_ref(),
    ) {
        (Some(Compiled::External { name }), Violation::ExternalRejected { witness }, Some(n)) => {
            !verdicts
                || ctx
                    .external
                    .get(name, n)
                    .is_some_and(|v| !v.passed && v.witness == *witness)
        }
        _ => false,
    }
}

/// The out-edges of `n` that are not fan edges.
fn non_fan_out_edges<'a>(view: InstanceView<'a>, n: &ViewRef) -> Vec<EdgeRec<'a>> {
    view.out_edges(n)
        .into_iter()
        .filter(|rec| !view.is_fan_edge(rec))
        .collect()
}

/// The nodes of a tree reachable from its root through its children.
fn reachable_from_root(tree: &WInstance) -> HashSet<u32> {
    let mut reachable = HashSet::new();
    let mut queue = VecDeque::new();
    if tree.nodes.contains_key(&tree.root) {
        reachable.insert(tree.root);
        queue.push_back(tree.root);
    }
    while let Some(current) = queue.pop_front() {
        for &child in tree.children(current) {
            if reachable.insert(child) {
                queue.push_back(child);
            }
        }
    }
    reachable
}

/// The endpoint at `side` of a non-fan edge at `e`, whose label names
/// `expected` there, is anchored at `found`, which the anchor families of
/// `expected` do not admit.
fn incoherent(
    ctx: &Context<'_>,
    e: &ViewRef,
    side: EndpointSide,
    expected: &Name,
    found: &Name,
) -> bool {
    let families = AnchorFamilies::new(&ctx.plan.schema);
    edges_at(ctx.view, e).iter().any(|rec| {
        let (end, declared) = match side {
            EndpointSide::Src => (&rec.src, &rec.label.src),
            EndpointSide::Tgt => (&rec.tgt, &rec.label.tgt),
        };
        !ctx.view.is_fan_edge(rec)
            && declared == expected
            && ctx.view.node(end).is_some_and(|node| node.anchor == found)
            && !families.accepts(expected, found)
    })
}

/// A graph fan node breaks `reason` (one of the violations
/// [`FanDomain::violations_at`] lists for it under the plan's schema); a tree
/// fan with record `fan`, at parent `f`, breaks `reason`.
fn malformed_fan(
    ctx: &Context<'_>,
    f: &ViewRef,
    reason: &FanViolation,
    fan: Option<&FanRecord>,
) -> bool {
    let schema = &ctx.plan.schema;
    match (ctx.view, f, fan) {
        (InstanceView::Graph(g), ViewRef::Node(id), None) => {
            FanDomain::violations_at(g, *id, Some(schema)).contains(reason)
        }
        (InstanceView::Tree(t, _), ViewRef::Node(parent), Some(record)) => {
            record.parent == *parent
                && t.fans.iter().any(|fan| FanRecord::of(fan) == *record)
                && match reason {
                    FanViolation::MissingParent => !t.nodes.contains_key(&record.parent),
                    FanViolation::MissingChild { label } => record
                        .children
                        .iter()
                        .any(|(l, child)| l == label && !t.nodes.contains_key(child)),
                    FanViolation::UnknownHyperEdge { hyper_edge } => {
                        *hyper_edge == record.hyper_edge
                            && !schema.hyper_edges.contains_key(hyper_edge)
                    }
                    FanViolation::UnknownLabel { label } => {
                        schema
                            .hyper_edges
                            .get(&record.hyper_edge)
                            .is_some_and(|h| !h.signature.contains_key(label))
                            && record.children.iter().any(|(l, _)| l == label)
                    }
                    FanViolation::MultipleParents { .. }
                    | FanViolation::DuplicateLabel { .. }
                    | FanViolation::ReservedKindOutsideFan { .. }
                    | FanViolation::ForeignIncidentEdge { .. } => false,
                }
        }
        _ => false,
    }
}

/// The out-degree the violation reports is the compiled bound's count of
/// `n`'s non-fan out-edges (along the bound's edge), it lists exactly the
/// edges counted, ascending, and the count breaks the bound.
fn out_degree(
    ctx: &Context<'_>,
    compiled: Option<&Compiled>,
    n: &ViewRef,
    violation: &Violation,
) -> bool {
    let (
        Some(Compiled::Degree {
            edge: c_edge,
            min: c_min,
            max: c_max,
        }),
        Violation::OutDegree {
            edge,
            min,
            max,
            found,
            edges,
        },
    ) = (compiled, violation)
    else {
        return false;
    };
    let counted = sorted(
        non_fan_out_edges(ctx.view, n)
            .into_iter()
            .filter(|rec| edge.as_ref().is_none_or(|label| rec.label == label))
            .map(|rec| rec.at)
            .collect(),
    );
    let count = u32::try_from(counted.len()).unwrap_or(u32::MAX);
    c_edge == edge
        && c_min == min
        && c_max == max
        && ascending(edges)
        && counted == *edges
        && count == *found
        && (min.is_some_and(|lo| count < lo) || max.is_some_and(|hi| count > hi))
}

/// The value the violation names is a value of the constrained vertex at
/// `n`, under the compiled constraint's sort and bound, and the compiled
/// test refutes it at `n`.
fn value_refuted(ctx: &Context<'_>, d: &Diagnostic, n: &ViewRef) -> bool {
    let (
        Some(Compiled::Value {
            vertex,
            sort,
            bound,
            ..
        }),
        Violation::ValueConstraint {
            sort: named_sort,
            bound: named_bound,
            value: Some(value),
        },
    ) = (ctx.plan.compiled.get(&d.check), &d.violation)
    else {
        return false;
    };
    let named = ctx
        .view
        .value_sites_at(&ctx.plan.schema, vertex, n)
        .iter()
        .any(|(_, presence)| presence.value().is_some_and(|x| same_value(x, value)));
    let bytes = violation_bytes(&d.violation);
    named
        && named_sort == sort
        && named_bound == bound
        && evaluate(ctx, &d.check, Some(n))
            .findings
            .iter()
            .any(|f| violation_bytes(&f.violation) == bytes)
}

/// The named field of `n` has no value: a value site of a required vertex,
/// a part of a primary key, or a key path of a reference that may not be
/// null.
fn missing_value(ctx: &Context<'_>, d: &Diagnostic, n: &ViewRef, field: Option<&String>) -> bool {
    let view = ctx.view;
    match (d.check.kind, ctx.plan.compiled.get(&d.check)) {
        (
            CheckKind::RequiredValue,
            Some(Compiled::RequiredValue {
                vertex,
                parts: None,
            }),
        ) => view
            .value_sites_at(&ctx.plan.schema, vertex, n)
            .iter()
            .any(|(f, presence)| f.as_ref() == field && presence.value().is_none()),
        (
            CheckKind::RequiredValue,
            Some(Compiled::RequiredValue {
                parts: Some(parts), ..
            }),
        ) => field.is_some_and(|name| {
            parts.iter().any(|part| {
                part_name(part) == *name
                    && !(matches!(view, InstanceView::Table(_))
                        && matches!(part, KeyPart::Position { .. }))
                    && part_value(view, n, part).is_none()
            })
        }),
        (CheckKind::ReferenceResolves, Some(Compiled::Reference { reference, target })) => {
            let RefSource::KeyValue { parts } = &reference.source else {
                return false;
            };
            !reference.nullable
                && parts.len() == target.parts.len()
                && (reference.cardinality == Cardinality::One || parts.len() == 1)
                && field.is_some_and(|name| {
                    parts.iter().any(|path| {
                        part_name(&KeyPart::Path(path.clone())) == *name
                            && path_values(view, n, path).is_empty()
                    })
                })
        }
        _ => false,
    }
}

/// The attribute discipline of the schema reports the violation's attribute
/// at `n`.
fn attribute_refuted(ctx: &Context<'_>, n: &ViewRef, violation: &Violation) -> bool {
    let attrs = AttributeSchema::from_schema(&ctx.plan.schema);
    let (found, id) = match (ctx.view, n) {
        (InstanceView::Graph(g), ViewRef::Node(id)) => (attrs.validate_ginstance(g), *id),
        (InstanceView::Tree(t, _), ViewRef::Node(id)) => (attrs.validate_wtype(t), *id),
        (InstanceView::Table(f), ViewRef::Row(RowId(id))) => (attrs.validate_finstance(f), *id),
        _ => return false,
    };
    found.iter().any(|v| match (v, violation) {
        (
            AttributeViolation::Undeclared { node_id, key, .. },
            Violation::UndeclaredAttribute { key: k },
        ) => *node_id == id && key == k,
        (
            AttributeViolation::Mistyped {
                node_id,
                key,
                expected,
                found,
                ..
            },
            Violation::MistypedAttribute {
                key: k,
                expected: e,
                found: f,
            },
        ) => *node_id == id && key == k && expected == e && found == f,
        _ => false,
    })
}

/// The elements share one key value in one scope, the violation names the
/// key's parts with that value, and no other element of the keyed vertex
/// in that scope carries it.
fn duplicate_key(
    ctx: &Context<'_>,
    vertex: &Name,
    key: &Key,
    named: &[(String, Value)],
    elems: &[ViewRef],
) -> bool {
    let view = ctx.view;
    if matches!(view, InstanceView::Table(_))
        && key
            .parts
            .iter()
            .any(|p| matches!(p, KeyPart::Position { .. }))
    {
        return false;
    }
    let Some(first) = elems.first() else {
        return false;
    };
    let names_match = named.len() == key.parts.len()
        && named
            .iter()
            .zip(&key.parts)
            .all(|((name, _), part)| *name == part_name(part));
    let values: Vec<Value> = named.iter().map(|(_, v)| v.clone()).collect();
    let scope = scope_of(view, first, &key.scope);
    let carries = |element: &ViewRef| {
        key_values(view, element, key).is_some_and(|vs| same_values(&vs, &values))
            && scope_of(view, element, &key.scope) == scope
    };
    let anchored = |element: &ViewRef| {
        view.node(element)
            .is_some_and(|node| !node.fan && node.anchor == vertex)
    };
    let all: Vec<ViewRef> = view
        .anchored(vertex)
        .into_iter()
        .filter(|e| carries(e))
        .collect();
    names_match
        && elems.len() > 1
        && ascending(elems)
        && elems.iter().all(|e| anchored(e) && carries(e))
        && all == elems
}

/// The key values `holder` names through `reference`: one value per part
/// for a single-target reference, one single-part value per target for a
/// many-target one; empty when a single-target reference that may be null
/// names nothing.
fn holder_keys(
    view: InstanceView<'_>,
    holder: &ViewRef,
    reference: &Reference,
    target: &Key,
) -> Option<Vec<Vec<Value>>> {
    let RefSource::KeyValue { parts } = &reference.source else {
        return None;
    };
    if parts.len() != target.parts.len() {
        return None;
    }
    match reference.cardinality {
        Cardinality::One => {
            let mut values = Vec::with_capacity(parts.len());
            for path in parts {
                match path_values(view, holder, path).first() {
                    Some(v) => values.push((*v).clone()),
                    None if reference.nullable => return Some(Vec::new()),
                    None => return None,
                }
            }
            Some(vec![values])
        }
        Cardinality::Many => {
            let [path] = parts.as_slice() else {
                return None;
            };
            Some(
                path_values(view, holder, path)
                    .into_iter()
                    .map(|v| vec![v.clone()])
                    .collect(),
            )
        }
    }
}

/// The holder names `key` through the reference.
fn holder_names(
    ctx: &Context<'_>,
    holder: &ViewRef,
    reference: &Reference,
    target: &Key,
    key: &[Value],
) -> bool {
    holder_keys(ctx.view, holder, reference, target)
        .is_some_and(|keys| keys.iter().any(|k| same_values(k, key)))
}

/// The elements of the reference's target vertex whose value of the target
/// key is `key`, ascending.
fn target_elements(
    ctx: &Context<'_>,
    reference: &Reference,
    target: &Key,
    key: &[Value],
) -> Vec<ViewRef> {
    ctx.view
        .anchored(&reference.target_vertex)
        .into_iter()
        .filter(|e| key_values(ctx.view, e, target).is_some_and(|vs| same_values(&vs, key)))
        .collect()
}

/// The witness is a closed walk along edges whose labels are declared
/// acyclic (consecutive edges join, the last edge returns to the first
/// edge's source), and `scc` is the strongly connected component of the
/// labelled edges that holds the walk, ascending.
fn cycle(ctx: &Context<'_>, labels: &BTreeSet<Edge>, scc: &[ViewRef], witness: &[EdgeId]) -> bool {
    let Some(g) = ctx.view.graph() else {
        return false;
    };
    let mut ends = Vec::with_capacity(witness.len());
    for id in witness {
        match g.edge(*id) {
            Some(e) if labels.contains(&e.edge) => ends.push((e.src, e.tgt)),
            _ => return false,
        }
    }
    let (Some(&(start, _)), Some(&(_, last))) = (ends.first(), ends.last()) else {
        return false;
    };
    let joined = ends.windows(2).all(|w| w[0].1 == w[1].0) && last == start;
    let labelled: Vec<(u32, u32)> = g
        .edges_sorted()
        .filter(|(_, e)| labels.contains(&e.edge))
        .map(|(_, e)| (e.src, e.tgt))
        .collect();
    let reach = |forward: bool| -> BTreeSet<u32> {
        let mut seen = BTreeSet::from([start]);
        let mut queue = VecDeque::from([start]);
        while let Some(u) = queue.pop_front() {
            for &(s, t) in &labelled {
                let (from, to) = if forward { (s, t) } else { (t, s) };
                if from == u && seen.insert(to) {
                    queue.push_back(to);
                }
            }
        }
        seen
    };
    let component: Vec<ViewRef> = reach(true)
        .intersection(&reach(false))
        .map(|n| ViewRef::Node(*n))
        .collect();
    joined && component == scc
}
