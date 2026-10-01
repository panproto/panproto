//! The vocabulary of validation reports.
//!
//! Every type here serializes externally tagged in `snake_case` (enums) or
//! with named fields in declaration order (structs). Maps whose keys are not
//! strings are written as lists of pairs in key order through
//! [`btree_map_as_vec`](panproto_schema::serde_helpers::btree_map_as_vec), so
//! that JSON and the Python object model can carry them.

use std::collections::BTreeMap;
use std::fmt;

use panproto_gat::{CheckStatus, Name};
use panproto_schema::{Constraint, Edge, InstanceShapeKind};
use serde::{Deserialize, Serialize};

use crate::fan_violation::FanViolation;
use crate::ginstance::EdgeId;
use crate::identity::{CanonicalSink, Digest, VALIDATION_REPORT_CONTEXT};
use crate::locality::{GlobalReason, Locality, SummaryKind};
use crate::segment::SegmentId;
use crate::value::Value;
use crate::view_ref::ViewRef;

/// The class of a check, after how much of an instance deciding it needs.
///
/// Reported on every diagnostic, incomplete check and coverage entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckClass {
    /// Decided once, from the schema and the protocol, when the plan is
    /// compiled.
    Static,
    /// Decided from one element and its incident elements.
    GraphLocal,
    /// Decided from an index or a traversal over the whole instance, which
    /// incremental validation maintains.
    MaintainedGlobal,
    /// Decided outside panproto, from an external verdict.
    BoundedExternal,
}

/// Whether a check's entries count toward the report's status.
///
/// An [`Advisory`](Self::Advisory) diagnostic or incomplete check is listed
/// in the report and never changes its status. This departs from SHACL, where
/// a result of any severity makes the report non-conforming (Knublauch and
/// Kontokostas 2017, 3.6.1.1, stated).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Requirement {
    /// The entry decides the status.
    #[default]
    Required,
    /// The entry is listed only.
    Advisory,
}

/// The kinds of check a validation plan compiles, in evaluation order.
///
/// The derived order is the declaration order: node typings first, then
/// edges against their declared endpoints, then the checks of one element,
/// then the checks over the whole instance. Each kind has a fixed
/// [`CheckClass`] ([`CheckKind::class`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckKind {
    /// The schema is well formed for its protocol.
    SchemaWellFormed,
    /// Every declaration the plan reads can be interpreted.
    DeclarationInterpretable,
    /// An element's anchor is a schema vertex (a fan node's, a hyper-edge).
    NodeAnchor,
    /// A tree holds no reserved fan vocabulary.
    ReservedVocabulary,
    /// An edge's label is a schema edge.
    EdgeDeclared,
    /// An edge's endpoints are anchored where its label says.
    EndpointCoherence,
    /// A fan is well formed and its labels are in its hyper-edge's
    /// signature.
    FanWellFormed,
    /// An element has an out-edge for each edge its vertex requires.
    RequiredEdge,
    /// An element's out-degree is within its bounds.
    OutDegree,
    /// No two out-edges of one element with one ordered label share a
    /// position.
    PositionDistinct,
    /// A value satisfies the constraints of its vertex.
    ValueConstraint,
    /// A required value is present.
    RequiredValue,
    /// An element's attributes are declared and of the declared kind.
    Attribute,
    /// A tree's root is one of its nodes.
    TreeRoot,
    /// A tree's parent map agrees with its arcs.
    TreeParentMap,
    /// The values of a key are unique in its scope.
    KeyUnique,
    /// A reference names exactly one element.
    ReferenceResolves,
    /// The edges of a declared acyclic self-edge form no cycle.
    Acyclic,
    /// Every node of a tree is reachable from its root.
    TreeReachable,
    /// An application check, decided by an external verdict.
    External,
}

impl CheckKind {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 20] = [
        Self::SchemaWellFormed,
        Self::DeclarationInterpretable,
        Self::NodeAnchor,
        Self::ReservedVocabulary,
        Self::EdgeDeclared,
        Self::EndpointCoherence,
        Self::FanWellFormed,
        Self::RequiredEdge,
        Self::OutDegree,
        Self::PositionDistinct,
        Self::ValueConstraint,
        Self::RequiredValue,
        Self::Attribute,
        Self::TreeRoot,
        Self::TreeParentMap,
        Self::KeyUnique,
        Self::ReferenceResolves,
        Self::Acyclic,
        Self::TreeReachable,
        Self::External,
    ];

    /// The class of this kind.
    ///
    /// | class | kinds |
    /// |---|---|
    /// | `Static` | `SchemaWellFormed`, `DeclarationInterpretable` |
    /// | `GraphLocal` | `NodeAnchor` through `TreeParentMap` |
    /// | `MaintainedGlobal` | `KeyUnique`, `ReferenceResolves`, `Acyclic`, `TreeReachable` |
    /// | `BoundedExternal` | `External` |
    #[must_use]
    pub const fn class(self) -> CheckClass {
        match self {
            Self::SchemaWellFormed | Self::DeclarationInterpretable => CheckClass::Static,
            Self::NodeAnchor
            | Self::ReservedVocabulary
            | Self::EdgeDeclared
            | Self::EndpointCoherence
            | Self::FanWellFormed
            | Self::RequiredEdge
            | Self::OutDegree
            | Self::PositionDistinct
            | Self::ValueConstraint
            | Self::RequiredValue
            | Self::Attribute
            | Self::TreeRoot
            | Self::TreeParentMap => CheckClass::GraphLocal,
            Self::KeyUnique | Self::ReferenceResolves | Self::Acyclic | Self::TreeReachable => {
                CheckClass::MaintainedGlobal
            }
            Self::External => CheckClass::BoundedExternal,
        }
    }

    /// Whether this kind can apply to instances of `shape`.
    ///
    /// `TreeRoot`, `TreeParentMap`, `TreeReachable` and `ReservedVocabulary`
    /// apply to trees only; `PositionDistinct` and `Acyclic` to graphs only
    /// (trees and tables carry no edge positions, and their acyclicity is
    /// structural or undeclared). Every other kind applies to all three
    /// shapes; a compiled `ReferenceResolves` check further applies only to
    /// the syntactic shape of its protocol.
    #[must_use]
    pub const fn applies_to(self, shape: InstanceShapeKind) -> bool {
        match self {
            Self::TreeRoot
            | Self::TreeParentMap
            | Self::TreeReachable
            | Self::ReservedVocabulary => {
                matches!(shape, InstanceShapeKind::Tree)
            }
            Self::PositionDistinct | Self::Acyclic => matches!(shape, InstanceShapeKind::Graph),
            _ => true,
        }
    }

    /// How much of a partitioned instance one instance of this kind reads.
    ///
    /// The static kinds and every local kind are [`Locality::Local`];
    /// `TreeRoot` is a fold of per-segment scalars; `KeyUnique` and
    /// `ReferenceResolves` are a sorted merge of per-segment key runs;
    /// `Acyclic` and `TreeReachable` are reachability over the boundary.
    #[must_use]
    pub const fn locality(self) -> Locality {
        match self {
            Self::TreeRoot => Locality::Summary(SummaryKind::Fold),
            Self::KeyUnique | Self::ReferenceResolves => {
                Locality::Summary(SummaryKind::SortedMerge)
            }
            Self::Acyclic | Self::TreeReachable => Locality::Summary(SummaryKind::Reach),
            _ => Locality::Local,
        }
    }

    /// The snake-case name of this kind, as serde writes it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::SchemaWellFormed => "schema_well_formed",
            Self::DeclarationInterpretable => "declaration_interpretable",
            Self::NodeAnchor => "node_anchor",
            Self::ReservedVocabulary => "reserved_vocabulary",
            Self::EdgeDeclared => "edge_declared",
            Self::EndpointCoherence => "endpoint_coherence",
            Self::FanWellFormed => "fan_well_formed",
            Self::RequiredEdge => "required_edge",
            Self::OutDegree => "out_degree",
            Self::PositionDistinct => "position_distinct",
            Self::ValueConstraint => "value_constraint",
            Self::RequiredValue => "required_value",
            Self::Attribute => "attribute",
            Self::TreeRoot => "tree_root",
            Self::TreeParentMap => "tree_parent_map",
            Self::KeyUnique => "key_unique",
            Self::ReferenceResolves => "reference_resolves",
            Self::Acyclic => "acyclic",
            Self::TreeReachable => "tree_reachable",
            Self::External => "external",
        }
    }

    /// The position of this kind in [`CheckKind::ALL`].
    #[must_use]
    pub const fn index(self) -> u8 {
        self as u8
    }
}

/// The schema element a check, and every diagnostic it produces, is about.
///
/// The derived order is the declaration order, then the payload.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchemaRef {
    /// The schema as a whole.
    Schema,
    /// A vertex.
    Vertex(Name),
    /// An edge.
    Edge(Edge),
    /// A hyper-edge.
    HyperEdge(Name),
    /// One label of a hyper-edge's signature.
    HyperLabel {
        /// The hyper-edge.
        hyper_edge: Name,
        /// The label.
        label: Name,
    },
    /// An edge `vertex` requires.
    Required {
        /// The requiring vertex.
        vertex: Name,
        /// The required edge.
        edge: Edge,
    },
    /// An ordered edge.
    Ordering(Edge),
    /// A constraint on a vertex.
    Constraint {
        /// The constrained vertex.
        vertex: Name,
        /// The constraint.
        constraint: Constraint,
    },
    /// A declared attribute of a vertex.
    Attribute {
        /// The vertex.
        vertex: Name,
        /// The attribute.
        field: String,
    },
    /// A key declared on a vertex.
    Key {
        /// The keyed vertex.
        vertex: Name,
        /// The key id.
        key: Name,
    },
    /// A reference, by id.
    Reference(Name),
    /// An application check, by name.
    External(Name),
}

/// The identity of one compiled check: its kind and the declaration it
/// checks.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CheckKey {
    /// The kind of check.
    pub kind: CheckKind,
    /// The declaration checked.
    pub decl: SchemaRef,
}

impl CheckKey {
    /// The key of a `kind` check of `decl`.
    #[must_use]
    pub const fn new(kind: CheckKind, decl: SchemaRef) -> Self {
        Self { kind, decl }
    }
}

/// One instance of a check: the check, and the element it is about.
///
/// `subject` is `None` for a check over the whole instance (`TreeRoot`,
/// `Acyclic`, `KeyUnique`), whose diagnostics list the elements involved.
/// The derived order (check, then subject) is the evaluation order and the
/// order a budget is consumed in.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CheckInstance {
    /// The check.
    pub check: CheckKey,
    /// The element the check is about, if it is about one.
    pub subject: Option<ViewRef>,
}

/// Where a declaration was written, in the source its schema was produced
/// from.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SourceLocation {
    /// The source, for instance a file path.
    pub source: String,
    /// The first byte of the declaration.
    pub start_byte: u32,
    /// One past the last byte of the declaration.
    pub end_byte: u32,
}

/// Where each declaration of a schema was written, supplied by whoever
/// produced the schema (the theory DSL, a protocol parser, an application's
/// compiler).
///
/// A source map is not part of [`Schema`](panproto_schema::Schema) or of its
/// canonical bytes, so it never changes a schema's identity. A diagnostic
/// whose declaration the map names carries that location.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SchemaSourceMap(
    /// The location of each declaration.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub BTreeMap<SchemaRef, SourceLocation>,
);

/// Which end of an edge a diagnostic is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointSide {
    /// The source.
    Src,
    /// The target.
    Tgt,
}

/// A typed refutation: what one check found wrong with an instance.
///
/// Each variant carries what a checker needs to confirm it without
/// rerunning validation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Violation {
    /// An element is anchored at a name the schema does not declare.
    UnknownAnchor {
        /// The anchor.
        anchor: Name,
    },
    /// An edge is labelled with an edge the schema does not declare.
    UndeclaredEdge {
        /// The label.
        label: Edge,
    },
    /// An endpoint of an edge is anchored somewhere its label does not
    /// allow.
    IncoherentEndpoint {
        /// Which endpoint.
        side: EndpointSide,
        /// The vertex the label names at that end.
        expected: Name,
        /// The endpoint's anchor.
        found: Name,
    },
    /// A fan is malformed.
    MalformedFan {
        /// Why.
        reason: FanViolation,
    },
    /// A tree holds reserved fan vocabulary.
    ReservedVocabulary,
    /// An element lacks an out-edge its vertex requires.
    MissingRequiredEdge {
        /// The required edge.
        edge: Edge,
    },
    /// An element's out-degree is out of bounds.
    OutDegree {
        /// The edge counted, or `None` for every non-fan out-edge.
        edge: Option<Edge>,
        /// The lower bound.
        min: Option<u32>,
        /// The upper bound.
        max: Option<u32>,
        /// The out-degree found.
        found: u32,
        /// The out-edges counted, ascending.
        edges: Vec<ViewRef>,
    },
    /// Two or more out-edges with one ordered label share a position.
    DuplicatePosition {
        /// The position.
        position: u32,
        /// The edges, ascending.
        edges: Vec<EdgeId>,
    },
    /// A value breaks a constraint.
    ValueConstraint {
        /// The constraint sort.
        sort: Name,
        /// The constraint's value, as written.
        bound: String,
        /// The value checked, `None` when the value is absent.
        value: Option<Value>,
    },
    /// A required value is absent or null.
    MissingValue {
        /// The field, or `None` for the element's own value.
        field: Option<String>,
    },
    /// An attribute is not declared for the element's vertex.
    UndeclaredAttribute {
        /// The attribute.
        key: String,
    },
    /// An attribute's value is not of the declared kind.
    MistypedAttribute {
        /// The attribute.
        key: String,
        /// The declared kind.
        expected: Name,
        /// The kind of the value found.
        found: String,
    },
    /// Two or more elements share the value of a key.
    DuplicateKey {
        /// The key's parts and their shared values.
        key: Vec<(String, Value)>,
        /// The elements, ascending.
        elems: Vec<ViewRef>,
    },
    /// A reference names no element.
    DanglingReference {
        /// The reference.
        reference: Name,
        /// The key values it names.
        key: Vec<Value>,
    },
    /// A reference names more than one element.
    AmbiguousReference {
        /// The reference.
        reference: Name,
        /// The key values it names.
        key: Vec<Value>,
        /// The elements it names, ascending.
        candidates: Vec<ViewRef>,
    },
    /// The edges of a declared acyclic self-edge form a cycle.
    Cycle {
        /// The nodes of the strongly connected component, ascending.
        scc: Vec<ViewRef>,
        /// A closed walk through the component: consecutive edges join, and
        /// the first edge's source is the last edge's target.
        witness: Vec<EdgeId>,
    },
    /// A tree's root is not one of its nodes.
    MissingRoot,
    /// A tree node is not reachable from the root.
    Unreachable,
    /// A tree's parent map disagrees with its arcs.
    ParentMismatch {
        /// The parent the parent map records.
        recorded: Option<u32>,
        /// The parent an arc gives.
        actual: Option<u32>,
    },
    /// An external verdict rejected the element.
    ExternalRejected {
        /// The digest of the verdict's witness.
        witness: Digest,
    },
}

fn write_list<T: fmt::Display>(f: &mut fmt::Formatter<'_>, items: &[T]) -> fmt::Result {
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            f.write_str(", ")?;
        }
        write!(f, "{item}")?;
    }
    Ok(())
}

fn write_edge(f: &mut fmt::Formatter<'_>, edge: &Edge) -> fmt::Result {
    write!(f, "{} -> {} ({}", edge.src, edge.tgt, edge.kind)?;
    if let Some(name) = &edge.name {
        write!(f, " {name}")?;
    }
    f.write_str(")")
}

/// Renders a view reference for a message: `node 3`, `edge 4`, `arc 0 -> 3`,
/// `row 9` or `pair 1 -> 2`.
struct ShowRef<'a>(&'a ViewRef);

impl fmt::Display for ShowRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            ViewRef::Node(n) => write!(f, "node {n}"),
            ViewRef::Edge(e) => write!(f, "edge {e}"),
            ViewRef::Arc { parent, child } => write!(f, "arc {parent} -> {child}"),
            ViewRef::Row(r) => write!(f, "row {}", r.0),
            ViewRef::Pair { edge, src, tgt } => {
                write!(f, "pair {} -> {} of ", src.0, tgt.0)?;
                write_edge(f, edge)
            }
        }
    }
}

fn show_refs(refs: &[ViewRef]) -> Vec<ShowRef<'_>> {
    refs.iter().map(ShowRef).collect()
}

fn show_value(value: &Value) -> String {
    match value {
        Value::Str(s) | Value::Token(s) | Value::CidLink(s) => format!("{s:?}"),
        Value::Int(i) => i.to_string(),
        Value::Float(x) => x.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Decimal(d) => d.to_string(),
        Value::BigInt(b) => b.to_string(),
        Value::Temporal(t) => t.to_string(),
        other => other.type_name().to_owned(),
    }
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownAnchor { anchor } => {
                write!(f, "anchor {anchor} is not declared by the schema")
            }
            Self::UndeclaredEdge { label } => {
                f.write_str("edge label ")?;
                write_edge(f, label)?;
                f.write_str(" is not declared by the schema")
            }
            Self::IncoherentEndpoint {
                side,
                expected,
                found,
            } => {
                let end = match side {
                    EndpointSide::Src => "source",
                    EndpointSide::Tgt => "target",
                };
                write!(
                    f,
                    "{end} is anchored at {found} but the edge expects {expected}"
                )
            }
            Self::MalformedFan { reason } => write!(f, "malformed fan: {reason}"),
            Self::ReservedVocabulary => f.write_str("reserved fan vocabulary outside a fan"),
            Self::MissingRequiredEdge { edge } => {
                f.write_str("missing required edge ")?;
                write_edge(f, edge)
            }
            Self::OutDegree {
                edge,
                min,
                max,
                found,
                ..
            } => {
                write!(f, "out-degree {found}")?;
                if let Some(edge) = edge {
                    f.write_str(" along ")?;
                    write_edge(f, edge)?;
                }
                match (min, max) {
                    (Some(lo), Some(hi)) => write!(f, " is outside [{lo}, {hi}]"),
                    (Some(lo), None) => write!(f, " is below the minimum {lo}"),
                    (None, Some(hi)) => write!(f, " is above the maximum {hi}"),
                    (None, None) => f.write_str(" is out of bounds"),
                }
            }
            Self::DuplicatePosition { position, edges } => {
                write!(f, "position {position} is shared by edges ")?;
                write_list(f, edges)
            }
            Self::ValueConstraint { sort, bound, value } => match value {
                Some(v) => write!(f, "value {} breaks {sort} = {bound:?}", show_value(v)),
                None => write!(f, "absent value breaks {sort} = {bound:?}"),
            },
            Self::MissingValue { field } => match field {
                Some(field) => write!(f, "required value {field} is absent"),
                None => f.write_str("required value is absent"),
            },
            Self::UndeclaredAttribute { key } => write!(f, "attribute {key} is not declared"),
            Self::MistypedAttribute {
                key,
                expected,
                found,
            } => write!(f, "attribute {key} holds a {found}, expected {expected}"),
            _ => self.fmt_tail(f),
        }
    }
}

impl Violation {
    /// The arms of `Display` from `DuplicateKey` on.
    fn fmt_tail(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateKey { key, elems } => {
                f.write_str("key (")?;
                for (i, (part, value)) in key.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{part} = {}", show_value(value))?;
                }
                f.write_str(") is shared by ")?;
                write_list(f, &show_refs(elems))
            }
            Self::DanglingReference { reference, key } => {
                let shown: Vec<String> = key.iter().map(show_value).collect();
                write!(
                    f,
                    "reference {reference} to ({}) names no element",
                    shown.join(", ")
                )
            }
            Self::AmbiguousReference {
                reference,
                key,
                candidates,
            } => {
                let shown: Vec<String> = key.iter().map(show_value).collect();
                write!(f, "reference {reference} to ({}) names ", shown.join(", "))?;
                write_list(f, &show_refs(candidates))
            }
            Self::Cycle { scc, witness } => {
                f.write_str("cycle through ")?;
                write_list(f, &show_refs(scc))?;
                f.write_str(" along edges ")?;
                write_list(f, witness)
            }
            Self::MissingRoot => f.write_str("the root is not a node of the tree"),
            Self::Unreachable => f.write_str("node is not reachable from the root"),
            Self::ParentMismatch { recorded, actual } => {
                let show = |p: &Option<u32>| p.map_or_else(|| "none".to_owned(), |p| p.to_string());
                write!(
                    f,
                    "parent map records {} but the arcs give {}",
                    show(recorded),
                    show(actual)
                )
            }
            Self::ExternalRejected { witness } => {
                write!(f, "rejected by an external verdict (witness {witness})")
            }
            _ => Ok(()),
        }
    }
}

/// One failed check instance.
///
/// The structured value is authoritative: [`Display`](fmt::Display) renders
/// a message for people, and no consumer parses it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// The check that failed.
    pub check: CheckKey,
    /// The check's class.
    pub class: CheckClass,
    /// Whether the diagnostic decides the status.
    pub requirement: Requirement,
    /// The element the check is about, if it is about one.
    pub subject: Option<ViewRef>,
    /// Every element the violation names, ascending, the subject included.
    pub elements: Vec<ViewRef>,
    /// What is wrong.
    pub violation: Violation,
    /// Where the checked declaration was written, if the plan's source map
    /// says.
    pub source: Option<SourceLocation>,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.check.kind.name())?;
        if let Some(subject) = &self.subject {
            write!(f, " at {}", ShowRef(subject))?;
        }
        write!(f, ": {}", self.violation)?;
        if let Some(source) = &self.source {
            write!(
                f,
                " (declared at {}:{}..{})",
                source.source, source.start_byte, source.end_byte
            )?;
        }
        Ok(())
    }
}

/// Why a check instance was not decided.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncompleteReason {
    /// The read budget ran out: the instances of the check at and after
    /// `from`, in [`CheckInstance`] order, were not evaluated. `spent` is
    /// what the evaluated instances read.
    Budget {
        /// The budget.
        limit: u64,
        /// The reads spent before the cut.
        spent: u64,
        /// The subject of the first instance of the check not evaluated.
        from: Option<ViewRef>,
    },
    /// The protocol gives no meaning to a constraint sort (or the meaning
    /// names a format outside the supported set).
    UnsupportedDeclaration {
        /// The sort.
        sort: Name,
    },
    /// A declaration cannot be read under its meaning.
    IllFormedDeclaration {
        /// Why.
        detail: String,
    },
    /// No verdict was supplied for an external check.
    MissingExternalWitness {
        /// The external check.
        name: Name,
    },
    /// An edit was checked without a validated baseline.
    NoValidatedBaseline,
    /// A single segment cannot decide the check.
    OutsideSegment {
        /// How much of the instance the check needs.
        locality: Locality,
    },
    /// A construct the plan compiler cannot evaluate.
    Unsupported {
        /// What.
        detail: String,
    },
}

impl fmt::Display for IncompleteReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Budget { limit, spent, .. } => {
                write!(f, "read budget {limit} exhausted after {spent} reads")
            }
            Self::UnsupportedDeclaration { sort } => {
                write!(f, "constraint sort {sort} has no supported meaning")
            }
            Self::IllFormedDeclaration { detail } => write!(f, "ill-formed declaration: {detail}"),
            Self::MissingExternalWitness { name } => {
                write!(f, "no verdict for external check {name}")
            }
            Self::NoValidatedBaseline => f.write_str("no validated baseline"),
            Self::OutsideSegment { .. } => f.write_str("not decidable within one segment"),
            Self::Unsupported { detail } => write!(f, "unsupported: {detail}"),
        }
    }
}

/// A check whose instances were not decided.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncompleteCheck {
    /// The check.
    pub check: CheckKey,
    /// The check's class.
    pub class: CheckClass,
    /// Whether the entry decides the status.
    pub requirement: Requirement,
    /// The instance not decided, or `None` for every instance of the check.
    pub subject: Option<ViewRef>,
    /// Why.
    pub reason: IncompleteReason,
}

/// How the instances of one check fared.
///
/// `passed + failed + incomplete == instances`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CheckCoverage {
    /// The check's class.
    pub class: CheckClass,
    /// The check's requirement.
    pub requirement: Requirement,
    /// The number of instances of the check.
    pub instances: u64,
    /// The instances decided and passed.
    pub passed: u64,
    /// The instances decided and failed.
    pub failed: u64,
    /// The instances not decided.
    pub incomplete: u64,
}

/// What a report covers.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportScope {
    /// A whole instance.
    Instance,
    /// A whole instance after an edit, against a baseline that had status
    /// `baseline_status` after `baseline_seq` admitted edits.
    Edit {
        /// The edits admitted before the baseline.
        baseline_seq: u64,
        /// The baseline's status.
        baseline_status: CheckStatus,
    },
    /// The local checks an edit touches, without a baseline.
    DeltaLocal,
    /// One segment of a partitioned instance.
    Segment {
        /// The segment.
        segment: SegmentId,
    },
}

/// The status a set of entries gives.
///
/// `Failed` if any required diagnostic exists, otherwise `Incomplete` if any
/// required incomplete check exists, otherwise `Passed`; advisory entries
/// never change it. This is strong Kleene conjunction over the required
/// entries, so a confirmed violation decides the status whatever else is
/// unknown.
#[must_use]
pub fn aggregate_status(diagnostics: &[Diagnostic], incomplete: &[IncompleteCheck]) -> CheckStatus {
    let failed = diagnostics
        .iter()
        .filter(|d| d.requirement == Requirement::Required)
        .map(|_| CheckStatus::Failed);
    let unknown = incomplete
        .iter()
        .filter(|i| i.requirement == Requirement::Required)
        .map(|_| CheckStatus::Incomplete);
    CheckStatus::all(failed.chain(unknown))
}

/// The result of validating an instance, an edit or a segment.
///
/// Diagnostics are sorted by `(check, subject, elements)` and incomplete
/// checks by `(check, subject)`, with ties broken by the canonical bytes of
/// the rest, so one report has one encoding. `status` is stored and equals
/// [`aggregate_status`] applied to the entries. Decoding rejects a report
/// whose entries are out of that order or whose stored status differs from
/// the one its entries give, so a decoded report has the digest of the
/// report that was encoded.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ReportWire")]
pub struct ValidationReport {
    /// The shape of the instance validated.
    pub shape: InstanceShapeKind,
    /// What the report covers.
    pub scope: ReportScope,
    /// The digest of the plan that produced the report.
    pub plan: Digest,
    /// The aggregate status.
    pub status: CheckStatus,
    /// Every failed check instance.
    pub diagnostics: Vec<Diagnostic>,
    /// Every check instance not decided, the plan's static findings
    /// included.
    pub incomplete: Vec<IncompleteCheck>,
    /// How the instances of each compiled check fared.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub coverage: BTreeMap<CheckKey, CheckCoverage>,
}

/// The decoded form of a report before its status is checked.
#[derive(Deserialize)]
struct ReportWire {
    shape: InstanceShapeKind,
    scope: ReportScope,
    plan: Digest,
    status: CheckStatus,
    diagnostics: Vec<Diagnostic>,
    incomplete: Vec<IncompleteCheck>,
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    coverage: BTreeMap<CheckKey, CheckCoverage>,
}

/// Why a decoded report was rejected.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ReportDecodeError {
    /// The stored status is not the status the entries give.
    #[error("report status {stored:?} does not match the status {computed:?} its entries give")]
    StatusMismatch {
        /// The status the report stored.
        stored: CheckStatus,
        /// The status its entries give.
        computed: CheckStatus,
    },
    /// The diagnostics are not in their documented order.
    #[error("report diagnostics are not in canonical order")]
    UnorderedDiagnostics,
    /// The incomplete checks are not in their documented order.
    #[error("report incomplete checks are not in canonical order")]
    UnorderedIncomplete,
}

impl TryFrom<ReportWire> for ValidationReport {
    type Error = ReportDecodeError;

    fn try_from(wire: ReportWire) -> Result<Self, Self::Error> {
        // Compared by canonical bytes, so a float payload that is not equal
        // to itself still compares as the same entry.
        let bytes_of = |entries: &[Diagnostic]| -> Vec<Vec<u8>> {
            entries
                .iter()
                .map(|d| canonical(|w| write_diagnostic(w, d)))
                .collect()
        };
        let mut sorted = wire.diagnostics.clone();
        sort_diagnostics(&mut sorted);
        if bytes_of(&sorted) != bytes_of(&wire.diagnostics) {
            return Err(ReportDecodeError::UnorderedDiagnostics);
        }
        let incomplete_bytes = |entries: &[IncompleteCheck]| -> Vec<Vec<u8>> {
            entries
                .iter()
                .map(|i| canonical(|w| write_incomplete(w, i)))
                .collect()
        };
        let mut sorted = wire.incomplete.clone();
        sort_incomplete(&mut sorted);
        if incomplete_bytes(&sorted) != incomplete_bytes(&wire.incomplete) {
            return Err(ReportDecodeError::UnorderedIncomplete);
        }
        let (diagnostics, incomplete) = (wire.diagnostics, wire.incomplete);
        let computed = aggregate_status(&diagnostics, &incomplete);
        if computed != wire.status {
            return Err(ReportDecodeError::StatusMismatch {
                stored: wire.status,
                computed,
            });
        }
        Ok(Self {
            shape: wire.shape,
            scope: wire.scope,
            plan: wire.plan,
            status: wire.status,
            diagnostics,
            incomplete,
            coverage: wire.coverage,
        })
    }
}

impl ValidationReport {
    /// Build a report from its entries: sort them, and compute the status.
    #[must_use]
    pub fn new(
        shape: InstanceShapeKind,
        scope: ReportScope,
        plan: Digest,
        mut diagnostics: Vec<Diagnostic>,
        mut incomplete: Vec<IncompleteCheck>,
        coverage: BTreeMap<CheckKey, CheckCoverage>,
    ) -> Self {
        sort_diagnostics(&mut diagnostics);
        sort_incomplete(&mut incomplete);
        let status = aggregate_status(&diagnostics, &incomplete);
        Self {
            shape,
            scope,
            plan,
            status,
            diagnostics,
            incomplete,
            coverage,
        }
    }

    /// The canonical bytes of this report, as [`Self::digest`] hashes them.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut w = panproto_schema::CanonicalWriter::new();
        self.write_canonical(&mut w);
        w.into_bytes()
    }

    /// The identity of this report: [`VALIDATION_REPORT_CONTEXT`] over its
    /// canonical bytes.
    #[must_use]
    pub fn digest(&self) -> Digest {
        let mut hasher = Digest::hasher(VALIDATION_REPORT_CONTEXT);
        self.write_canonical(&mut hasher);
        hasher.finalize()
    }

    /// Write the canonical bytes of this report into `w`: the shape, the
    /// scope, the plan digest, the status, then the diagnostics, the
    /// incomplete checks and the coverage entries, each list behind its
    /// count and in its stored order.
    pub fn write_canonical<W: CanonicalSink + ?Sized>(&self, w: &mut W) {
        write_shape(w, self.shape);
        write_scope(w, &self.scope);
        w.write(&self.plan.0);
        write_status(w, self.status);
        w.len(self.diagnostics.len());
        for d in &self.diagnostics {
            write_diagnostic(w, d);
        }
        w.len(self.incomplete.len());
        for i in &self.incomplete {
            write_incomplete(w, i);
        }
        w.len(self.coverage.len());
        for (key, cov) in &self.coverage {
            write_check_key(w, key);
            write_coverage(w, cov);
        }
    }

    /// Whether the report's status is [`CheckStatus::Passed`].
    #[must_use]
    pub const fn passed(&self) -> bool {
        matches!(self.status, CheckStatus::Passed)
    }
}

fn canonical<F: FnOnce(&mut panproto_schema::CanonicalWriter)>(f: F) -> Vec<u8> {
    let mut w = panproto_schema::CanonicalWriter::new();
    f(&mut w);
    w.into_bytes()
}

/// Sort diagnostics by `(check, subject, elements)`, then by their canonical
/// bytes.
pub fn sort_diagnostics(diagnostics: &mut [Diagnostic]) {
    diagnostics.sort_by_cached_key(|d| {
        (
            d.check.clone(),
            d.subject.clone(),
            d.elements.clone(),
            canonical(|w| write_diagnostic(w, d)),
        )
    });
}

/// Sort incomplete checks by `(check, subject)`, then by their canonical
/// bytes.
pub fn sort_incomplete(incomplete: &mut [IncompleteCheck]) {
    incomplete.sort_by_cached_key(|i| {
        (
            i.check.clone(),
            i.subject.clone(),
            canonical(|w| write_incomplete(w, i)),
        )
    });
}

// ---------------------------------------------------------------------------
// Canonical encoders. Every enum is a one-byte tag (its declaration index)
// followed by its payload; every list is its count followed by its items;
// an `Option` is a presence byte followed by the value.
// ---------------------------------------------------------------------------

pub fn write_opt<W: CanonicalSink + ?Sized, T>(
    w: &mut W,
    value: Option<&T>,
    f: impl FnOnce(&mut W, &T),
) {
    match value {
        None => w.tag(0),
        Some(v) => {
            w.tag(1);
            f(w, v);
        }
    }
}

pub fn write_shape<W: CanonicalSink + ?Sized>(w: &mut W, shape: InstanceShapeKind) {
    w.tag(match shape {
        InstanceShapeKind::Tree => 0,
        InstanceShapeKind::Table => 1,
        InstanceShapeKind::Graph => 2,
    });
}

pub fn write_status<W: CanonicalSink + ?Sized>(w: &mut W, status: CheckStatus) {
    w.tag(match status {
        CheckStatus::Passed => 0,
        CheckStatus::Failed => 1,
        CheckStatus::Incomplete => 2,
    });
}

fn write_scope<W: CanonicalSink + ?Sized>(w: &mut W, scope: &ReportScope) {
    match scope {
        ReportScope::Instance => w.tag(0),
        ReportScope::Edit {
            baseline_seq,
            baseline_status,
        } => {
            w.tag(1);
            w.u64(*baseline_seq);
            write_status(w, *baseline_status);
        }
        ReportScope::DeltaLocal => w.tag(2),
        ReportScope::Segment { segment } => {
            w.tag(3);
            w.u32(segment.0);
        }
    }
}

pub fn write_view_ref<W: CanonicalSink + ?Sized>(w: &mut W, r: &ViewRef) {
    match r {
        ViewRef::Node(n) => {
            w.tag(0);
            w.u32(*n);
        }
        ViewRef::Edge(e) => {
            w.tag(1);
            w.u32(e.0);
        }
        ViewRef::Arc { parent, child } => {
            w.tag(2);
            w.u32(*parent);
            w.u32(*child);
        }
        ViewRef::Row(r) => {
            w.tag(3);
            w.u32(r.0);
        }
        ViewRef::Pair { edge, src, tgt } => {
            w.tag(4);
            w.edge(edge);
            w.u32(src.0);
            w.u32(tgt.0);
        }
    }
}

fn write_view_refs<W: CanonicalSink + ?Sized>(w: &mut W, refs: &[ViewRef]) {
    w.len(refs.len());
    for r in refs {
        write_view_ref(w, r);
    }
}

pub fn write_kind<W: CanonicalSink + ?Sized>(w: &mut W, kind: CheckKind) {
    w.tag(kind.index());
}

pub fn write_class<W: CanonicalSink + ?Sized>(w: &mut W, class: CheckClass) {
    w.tag(class as u8);
}

pub fn write_requirement<W: CanonicalSink + ?Sized>(w: &mut W, requirement: Requirement) {
    w.tag(requirement as u8);
}

pub fn write_constraint<W: CanonicalSink + ?Sized>(w: &mut W, c: &Constraint) {
    w.name(&c.sort);
    w.str(&c.value);
}

pub fn write_schema_ref<W: CanonicalSink + ?Sized>(w: &mut W, r: &SchemaRef) {
    match r {
        SchemaRef::Schema => w.tag(0),
        SchemaRef::Vertex(v) => {
            w.tag(1);
            w.name(v);
        }
        SchemaRef::Edge(e) => {
            w.tag(2);
            w.edge(e);
        }
        SchemaRef::HyperEdge(h) => {
            w.tag(3);
            w.name(h);
        }
        SchemaRef::HyperLabel { hyper_edge, label } => {
            w.tag(4);
            w.name(hyper_edge);
            w.name(label);
        }
        SchemaRef::Required { vertex, edge } => {
            w.tag(5);
            w.name(vertex);
            w.edge(edge);
        }
        SchemaRef::Ordering(e) => {
            w.tag(6);
            w.edge(e);
        }
        SchemaRef::Constraint { vertex, constraint } => {
            w.tag(7);
            w.name(vertex);
            write_constraint(w, constraint);
        }
        SchemaRef::Attribute { vertex, field } => {
            w.tag(8);
            w.name(vertex);
            w.str(field);
        }
        SchemaRef::Key { vertex, key } => {
            w.tag(9);
            w.name(vertex);
            w.name(key);
        }
        SchemaRef::Reference(r) => {
            w.tag(10);
            w.name(r);
        }
        SchemaRef::External(n) => {
            w.tag(11);
            w.name(n);
        }
    }
}

pub fn write_check_key<W: CanonicalSink + ?Sized>(w: &mut W, key: &CheckKey) {
    write_kind(w, key.kind);
    write_schema_ref(w, &key.decl);
}

pub fn write_source<W: CanonicalSink + ?Sized>(w: &mut W, s: &SourceLocation) {
    w.str(&s.source);
    w.u32(s.start_byte);
    w.u32(s.end_byte);
}

fn write_edge_ids<W: CanonicalSink + ?Sized>(w: &mut W, edges: &[EdgeId]) {
    w.len(edges.len());
    for e in edges {
        w.u32(e.0);
    }
}

fn write_fan_violation<W: CanonicalSink + ?Sized>(w: &mut W, v: &FanViolation) {
    match v {
        FanViolation::MissingParent => w.tag(0),
        FanViolation::MultipleParents { edges } => {
            w.tag(1);
            write_edge_ids(w, edges);
        }
        FanViolation::DuplicateLabel { label, edges } => {
            w.tag(2);
            w.name(label);
            write_edge_ids(w, edges);
        }
        FanViolation::UnknownLabel { label } => {
            w.tag(3);
            w.name(label);
        }
        FanViolation::ReservedKindOutsideFan { edge } => {
            w.tag(4);
            w.u32(edge.0);
        }
        FanViolation::ForeignIncidentEdge { edge } => {
            w.tag(5);
            w.u32(edge.0);
        }
    }
}

fn write_opt_u32<W: CanonicalSink + ?Sized>(w: &mut W, value: Option<u32>) {
    write_opt(w, value.as_ref(), |w, v| w.u32(*v));
}

fn write_values<W: CanonicalSink + ?Sized>(w: &mut W, values: &[Value]) {
    w.len(values.len());
    for v in values {
        w.value(v);
    }
}

/// Write a violation: its tag (declaration index), then its fields in order.
pub fn write_violation<W: CanonicalSink + ?Sized>(w: &mut W, v: &Violation) {
    match v {
        Violation::UnknownAnchor { anchor } => {
            w.tag(0);
            w.name(anchor);
        }
        Violation::UndeclaredEdge { label } => {
            w.tag(1);
            w.edge(label);
        }
        Violation::IncoherentEndpoint {
            side,
            expected,
            found,
        } => {
            w.tag(2);
            w.tag(*side as u8);
            w.name(expected);
            w.name(found);
        }
        Violation::MalformedFan { reason } => {
            w.tag(3);
            write_fan_violation(w, reason);
        }
        Violation::ReservedVocabulary => w.tag(4),
        Violation::MissingRequiredEdge { edge } => {
            w.tag(5);
            w.edge(edge);
        }
        Violation::OutDegree {
            edge,
            min,
            max,
            found,
            edges,
        } => {
            w.tag(6);
            write_opt(w, edge.as_ref(), W::edge);
            write_opt_u32(w, *min);
            write_opt_u32(w, *max);
            w.u32(*found);
            write_view_refs(w, edges);
        }
        Violation::DuplicatePosition { position, edges } => {
            w.tag(7);
            w.u32(*position);
            write_edge_ids(w, edges);
        }
        Violation::ValueConstraint { sort, bound, value } => {
            w.tag(8);
            w.name(sort);
            w.str(bound);
            write_opt(w, value.as_ref(), W::value);
        }
        Violation::MissingValue { field } => {
            w.tag(9);
            write_opt(w, field.as_ref(), |w, f| w.str(f));
        }
        Violation::UndeclaredAttribute { key } => {
            w.tag(10);
            w.str(key);
        }
        Violation::MistypedAttribute {
            key,
            expected,
            found,
        } => {
            w.tag(11);
            w.str(key);
            w.name(expected);
            w.str(found);
        }
        _ => write_violation_tail(w, v),
    }
}

/// The arms of [`write_violation`] from `DuplicateKey` on.
fn write_violation_tail<W: CanonicalSink + ?Sized>(w: &mut W, v: &Violation) {
    match v {
        Violation::DuplicateKey { key, elems } => {
            w.tag(12);
            w.len(key.len());
            for (part, value) in key {
                w.str(part);
                w.value(value);
            }
            write_view_refs(w, elems);
        }
        Violation::DanglingReference { reference, key } => {
            w.tag(13);
            w.name(reference);
            write_values(w, key);
        }
        Violation::AmbiguousReference {
            reference,
            key,
            candidates,
        } => {
            w.tag(14);
            w.name(reference);
            write_values(w, key);
            write_view_refs(w, candidates);
        }
        Violation::Cycle { scc, witness } => {
            w.tag(15);
            write_view_refs(w, scc);
            write_edge_ids(w, witness);
        }
        Violation::MissingRoot => w.tag(16),
        Violation::Unreachable => w.tag(17),
        Violation::ParentMismatch { recorded, actual } => {
            w.tag(18);
            write_opt_u32(w, *recorded);
            write_opt_u32(w, *actual);
        }
        Violation::ExternalRejected { witness } => {
            w.tag(19);
            w.write(&witness.0);
        }
        _ => {}
    }
}

/// Write a diagnostic: its fields in declaration order.
pub fn write_diagnostic<W: CanonicalSink + ?Sized>(w: &mut W, d: &Diagnostic) {
    write_check_key(w, &d.check);
    write_class(w, d.class);
    write_requirement(w, d.requirement);
    write_opt(w, d.subject.as_ref(), |w, s| write_view_ref(w, s));
    write_view_refs(w, &d.elements);
    write_violation(w, &d.violation);
    write_opt(w, d.source.as_ref(), |w, s| write_source(w, s));
}

pub fn write_locality<W: CanonicalSink + ?Sized>(w: &mut W, l: &Locality) {
    match l {
        Locality::Local => w.tag(0),
        Locality::Exchange => w.tag(1),
        Locality::Summary(kind) => {
            w.tag(2);
            w.tag(match kind {
                SummaryKind::Reach => 0,
                SummaryKind::SortedMerge => 1,
                SummaryKind::Fold => 2,
            });
        }
        Locality::Global(reason) => {
            w.tag(3);
            match reason {
                GlobalReason::TraversalBuiltin { vertex } => {
                    w.tag(0);
                    w.name(vertex);
                }
                GlobalReason::StructuralChildDemand { vertex, field } => {
                    w.tag(1);
                    w.name(vertex);
                    w.str(field);
                }
                GlobalReason::PiProduct => w.tag(2),
                GlobalReason::ClosureWitnessOrder => w.tag(3),
                GlobalReason::Query => w.tag(4),
                GlobalReason::SequentialComposite => w.tag(5),
            }
        }
    }
}

fn write_reason<W: CanonicalSink + ?Sized>(w: &mut W, r: &IncompleteReason) {
    match r {
        IncompleteReason::Budget { limit, spent, from } => {
            w.tag(0);
            w.u64(*limit);
            w.u64(*spent);
            write_opt(w, from.as_ref(), |w, f| write_view_ref(w, f));
        }
        IncompleteReason::UnsupportedDeclaration { sort } => {
            w.tag(1);
            w.name(sort);
        }
        IncompleteReason::IllFormedDeclaration { detail } => {
            w.tag(2);
            w.str(detail);
        }
        IncompleteReason::MissingExternalWitness { name } => {
            w.tag(3);
            w.name(name);
        }
        IncompleteReason::NoValidatedBaseline => w.tag(4),
        IncompleteReason::OutsideSegment { locality } => {
            w.tag(5);
            write_locality(w, locality);
        }
        IncompleteReason::Unsupported { detail } => {
            w.tag(6);
            w.str(detail);
        }
    }
}

/// Write an incomplete check: its fields in declaration order.
pub fn write_incomplete<W: CanonicalSink + ?Sized>(w: &mut W, i: &IncompleteCheck) {
    write_check_key(w, &i.check);
    write_class(w, i.class);
    write_requirement(w, i.requirement);
    write_opt(w, i.subject.as_ref(), |w, s| write_view_ref(w, s));
    write_reason(w, &i.reason);
}

fn write_coverage<W: CanonicalSink + ?Sized>(w: &mut W, c: &CheckCoverage) {
    write_class(w, c.class);
    write_requirement(w, c.requirement);
    w.u64(c.instances);
    w.u64(c.passed);
    w.u64(c.failed);
    w.u64(c.incomplete);
}
