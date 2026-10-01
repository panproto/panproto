//! Validation plans: a schema and a protocol compiled into the checks an
//! instance must pass.

use std::collections::{BTreeMap, BTreeSet};

use panproto_gat::Name;
use panproto_schema::{
    ConstraintSemantics, ConstraintValue, Edge, InstanceShapeKind, Key, LengthUnit, Protocol,
    Reference, Schema,
};
use serde::{Deserialize, Serialize};

use crate::identity::{CanonicalSink, Digest, VALIDATION_PLAN_CONTEXT, schema_digest};
use crate::locality::Locality;
use crate::view_ref::ViewRef;

use super::report::{
    CheckClass, CheckKey, CheckKind, IncompleteCheck, Requirement, SchemaRef, SchemaSourceMap,
    write_requirement, write_schema_ref, write_source,
};

/// What a caller adds to a schema's own declarations when compiling a plan.
///
/// `requirements` overrides the requirement of every check of a declaration;
/// `external` declares application checks; `sources` locates declarations
/// for diagnostics. The default is empty.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationOptions {
    /// The requirement of the checks of each declaration, overriding the
    /// default.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub requirements: BTreeMap<SchemaRef, Requirement>,
    /// The application checks, by name.
    pub external: BTreeMap<Name, ExternalCheckDecl>,
    /// Where each declaration was written.
    pub sources: SchemaSourceMap,
}

/// An application check: one instance per element anchored at one of
/// `anchors`, decided by an external verdict.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalCheckDecl {
    /// The check's name, the key of its verdicts.
    pub name: Name,
    /// The vertices whose elements the check is about.
    pub anchors: BTreeSet<Name>,
    /// Whether the check decides the status.
    pub requirement: Requirement,
}

/// How many reads a validation may spend.
///
/// One unit is one element of a check instance's footprint; instances are
/// charged in ascending [`CheckInstance`](super::CheckInstance) order, so the
/// cut point is the same on every platform. The default, `None`, is
/// unlimited.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ValidationBudget {
    /// The most reads allowed, or `None` for no limit.
    pub max_reads: Option<u64>,
}

/// An external judgment of one element by one application check.
///
/// panproto records the witness digest and never evaluates the rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExternalVerdict {
    /// Whether the element passed.
    pub passed: bool,
    /// The digest of the evidence behind the verdict.
    pub witness: Digest,
}

/// The external verdicts supplied to one validation, keyed by check name and
/// element.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExternalVerdicts(
    /// The verdict for each check and element.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub BTreeMap<(Name, ViewRef), ExternalVerdict>,
);

impl ExternalVerdicts {
    /// The verdict of check `name` on `element`, if one was supplied.
    #[must_use]
    pub fn get(&self, name: &Name, element: &ViewRef) -> Option<&ExternalVerdict> {
        self.0.get(&(name.clone(), element.clone()))
    }

    /// Record the verdict of check `name` on `element`.
    pub fn insert(&mut self, name: Name, element: ViewRef, verdict: ExternalVerdict) {
        self.0.insert((name, element), verdict);
    }
}

/// The kinds of structural change an edit is made of, per shape.
///
/// Graph edits add and delete nodes and edges and change node and edge
/// attributes; table edits add and delete rows and foreign-key pairs, change
/// cells, and move rows between tables; tree edits are the steps of
/// [`TreeEdit`](crate::TreeEdit).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeltaElementKind {
    /// A graph node is added.
    AddNode,
    /// A graph node is deleted.
    DelNode,
    /// A graph edge is added.
    AddEdge,
    /// A graph edge is deleted.
    DelEdge,
    /// A node's value changes.
    AttrValue,
    /// A node's extra fields change.
    AttrField,
    /// A node's discriminator, shape, position or annotations change.
    AttrMeta,
    /// A node's anchor changes.
    AttrAnchor,
    /// An edge's label changes.
    AttrEdgeLabel,
    /// An edge's position changes.
    AttrEdgePosition,
    /// A table row is added.
    AddRow,
    /// A table row is deleted.
    DelRow,
    /// A foreign-key pair is added.
    AddPair,
    /// A foreign-key pair is deleted.
    DelPair,
    /// A cell changes.
    Cell,
    /// A row moves to another table.
    Table,
    /// [`TreeEdit::InsertNode`](crate::TreeEdit::InsertNode).
    InsertNode,
    /// [`TreeEdit::DeleteNode`](crate::TreeEdit::DeleteNode).
    DeleteNode,
    /// [`TreeEdit::ContractNode`](crate::TreeEdit::ContractNode).
    ContractNode,
    /// [`TreeEdit::RelabelNode`](crate::TreeEdit::RelabelNode).
    RelabelNode,
    /// [`TreeEdit::SetField`](crate::TreeEdit::SetField).
    SetField,
    /// [`TreeEdit::RemoveField`](crate::TreeEdit::RemoveField).
    RemoveField,
    /// [`TreeEdit::MoveSubtree`](crate::TreeEdit::MoveSubtree).
    MoveSubtree,
    /// [`TreeEdit::InsertFan`](crate::TreeEdit::InsertFan).
    InsertFan,
    /// [`TreeEdit::DeleteFan`](crate::TreeEdit::DeleteFan).
    DeleteFan,
    /// [`TreeEdit::JoinFeatures`](crate::TreeEdit::JoinFeatures).
    JoinFeatures,
}

impl DeltaElementKind {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 26] = [
        Self::AddNode,
        Self::DelNode,
        Self::AddEdge,
        Self::DelEdge,
        Self::AttrValue,
        Self::AttrField,
        Self::AttrMeta,
        Self::AttrAnchor,
        Self::AttrEdgeLabel,
        Self::AttrEdgePosition,
        Self::AddRow,
        Self::DelRow,
        Self::AddPair,
        Self::DelPair,
        Self::Cell,
        Self::Table,
        Self::InsertNode,
        Self::DeleteNode,
        Self::ContractNode,
        Self::RelabelNode,
        Self::SetField,
        Self::RemoveField,
        Self::MoveSubtree,
        Self::InsertFan,
        Self::DeleteFan,
        Self::JoinFeatures,
    ];

    /// The check kinds a change of this kind can change the verdict of
    /// (the fixed table of [`TriggerTable`]).
    #[must_use]
    pub const fn triggers(self) -> &'static [CheckKind] {
        match self {
            Self::AddNode => rows::ADD_NODE,
            Self::DelNode | Self::DelRow => rows::DELETE_ELEMENT,
            Self::AddEdge => rows::ADD_EDGE,
            Self::DelEdge => rows::DEL_EDGE,
            Self::AttrValue => rows::ATTR_VALUE,
            Self::AttrField | Self::SetField | Self::RemoveField => rows::FIELD,
            Self::AttrMeta => rows::ATTR_META,
            Self::AttrAnchor => rows::ATTR_ANCHOR,
            Self::AttrEdgeLabel => rows::ATTR_EDGE_LABEL,
            Self::AttrEdgePosition => rows::ATTR_EDGE_POSITION,
            Self::AddRow => rows::ADD_ROW,
            Self::AddPair => rows::ADD_PAIR,
            Self::DelPair => rows::DEL_PAIR,
            Self::Cell => rows::CELL,
            Self::Table => rows::TABLE,
            Self::InsertNode => rows::INSERT_NODE,
            Self::DeleteNode => rows::DELETE_NODE,
            Self::ContractNode | Self::JoinFeatures => rows::CONTRACT_NODE,
            Self::RelabelNode => rows::RELABEL_NODE,
            Self::MoveSubtree => rows::MOVE_SUBTREE,
            Self::InsertFan | Self::DeleteFan => rows::FAN,
        }
    }
}

/// The rows of the fixed trigger table.
mod rows {
    use super::CheckKind as K;

    pub const ADD_NODE: &[K] = &[
        K::NodeAnchor,
        K::FanWellFormed,
        K::RequiredEdge,
        K::OutDegree,
        K::ValueConstraint,
        K::RequiredValue,
        K::Attribute,
        K::KeyUnique,
        K::ReferenceResolves,
        K::External,
    ];
    pub const DELETE_ELEMENT: &[K] = &[K::KeyUnique, K::ReferenceResolves];
    pub const ADD_EDGE: &[K] = &[
        K::EdgeDeclared,
        K::EndpointCoherence,
        K::FanWellFormed,
        K::RequiredEdge,
        K::OutDegree,
        K::PositionDistinct,
        K::KeyUnique,
        K::ReferenceResolves,
        K::Acyclic,
        K::External,
    ];
    pub const DEL_EDGE: &[K] = &[
        K::FanWellFormed,
        K::RequiredEdge,
        K::OutDegree,
        K::PositionDistinct,
        K::KeyUnique,
        K::ReferenceResolves,
        K::Acyclic,
        K::External,
    ];
    pub const ATTR_VALUE: &[K] = &[
        K::ValueConstraint,
        K::RequiredValue,
        K::KeyUnique,
        K::ReferenceResolves,
    ];
    pub const FIELD: &[K] = &[K::Attribute, K::KeyUnique, K::ReferenceResolves];
    pub const ATTR_META: &[K] = &[K::NodeAnchor, K::FanWellFormed, K::KeyUnique, K::External];
    pub const ATTR_ANCHOR: &[K] = &[
        K::NodeAnchor,
        K::EndpointCoherence,
        K::FanWellFormed,
        K::RequiredEdge,
        K::OutDegree,
        K::PositionDistinct,
        K::ValueConstraint,
        K::RequiredValue,
        K::Attribute,
        K::KeyUnique,
        K::ReferenceResolves,
        K::Acyclic,
        K::External,
    ];
    pub const ATTR_EDGE_LABEL: &[K] = &[
        K::EdgeDeclared,
        K::EndpointCoherence,
        K::FanWellFormed,
        K::RequiredEdge,
        K::OutDegree,
        K::PositionDistinct,
        K::KeyUnique,
        K::ReferenceResolves,
        K::Acyclic,
    ];
    pub const ATTR_EDGE_POSITION: &[K] = &[K::PositionDistinct, K::KeyUnique];
    pub const ADD_ROW: &[K] = &[
        K::NodeAnchor,
        K::RequiredEdge,
        K::OutDegree,
        K::ValueConstraint,
        K::RequiredValue,
        K::Attribute,
        K::KeyUnique,
        K::ReferenceResolves,
        K::External,
    ];
    pub const ADD_PAIR: &[K] = &[
        K::EdgeDeclared,
        K::EndpointCoherence,
        K::RequiredEdge,
        K::OutDegree,
        K::KeyUnique,
        K::ReferenceResolves,
        K::External,
    ];
    pub const DEL_PAIR: &[K] = &[
        K::RequiredEdge,
        K::OutDegree,
        K::KeyUnique,
        K::ReferenceResolves,
        K::External,
    ];
    pub const CELL: &[K] = &[
        K::ValueConstraint,
        K::RequiredValue,
        K::Attribute,
        K::KeyUnique,
        K::ReferenceResolves,
    ];
    pub const TABLE: &[K] = &[
        K::NodeAnchor,
        K::EndpointCoherence,
        K::RequiredEdge,
        K::OutDegree,
        K::ValueConstraint,
        K::RequiredValue,
        K::Attribute,
        K::KeyUnique,
        K::ReferenceResolves,
        K::External,
    ];
    pub const INSERT_NODE: &[K] = &[
        K::NodeAnchor,
        K::ReservedVocabulary,
        K::EdgeDeclared,
        K::EndpointCoherence,
        K::RequiredEdge,
        K::OutDegree,
        K::ValueConstraint,
        K::RequiredValue,
        K::Attribute,
        K::TreeParentMap,
        K::KeyUnique,
        K::ReferenceResolves,
        K::TreeReachable,
        K::External,
    ];
    pub const DELETE_NODE: &[K] = &[
        K::FanWellFormed,
        K::RequiredEdge,
        K::OutDegree,
        K::TreeRoot,
        K::TreeParentMap,
        K::KeyUnique,
        K::ReferenceResolves,
        K::TreeReachable,
        K::External,
    ];
    pub const CONTRACT_NODE: &[K] = &[
        K::NodeAnchor,
        K::ReservedVocabulary,
        K::EdgeDeclared,
        K::EndpointCoherence,
        K::FanWellFormed,
        K::RequiredEdge,
        K::OutDegree,
        K::ValueConstraint,
        K::RequiredValue,
        K::Attribute,
        K::TreeRoot,
        K::TreeParentMap,
        K::KeyUnique,
        K::ReferenceResolves,
        K::TreeReachable,
        K::External,
    ];
    pub const RELABEL_NODE: &[K] = &[
        K::NodeAnchor,
        K::EndpointCoherence,
        K::RequiredEdge,
        K::OutDegree,
        K::ValueConstraint,
        K::RequiredValue,
        K::Attribute,
        K::KeyUnique,
        K::ReferenceResolves,
        K::External,
    ];
    pub const MOVE_SUBTREE: &[K] = &[
        K::ReservedVocabulary,
        K::EdgeDeclared,
        K::EndpointCoherence,
        K::RequiredEdge,
        K::OutDegree,
        K::TreeParentMap,
        K::KeyUnique,
        K::ReferenceResolves,
        K::TreeReachable,
        K::External,
    ];
    pub const FAN: &[K] = &[K::FanWellFormed];
}

/// Which compiled check kinds each kind of structural change can affect.
///
/// This is the precomputed half of incremental validation (Cabot and
/// Teniente 2006, stated: per constraint, the structural events that can
/// violate it). The fixed table, before restriction to the kinds a plan
/// compiles, is [`DeltaElementKind::triggers`]:
///
/// | change | kinds |
/// |---|---|
/// | `AddNode` | `NodeAnchor`, `FanWellFormed`, `RequiredEdge`, `OutDegree`, `ValueConstraint`, `RequiredValue`, `Attribute`, `KeyUnique`, `ReferenceResolves`, `External` |
/// | `DelNode`, `DelRow` | `KeyUnique`, `ReferenceResolves` |
/// | `AddEdge` | `EdgeDeclared`, `EndpointCoherence`, `FanWellFormed`, `RequiredEdge`, `OutDegree`, `PositionDistinct`, `KeyUnique`, `ReferenceResolves`, `Acyclic`, `External` |
/// | `DelEdge` | `FanWellFormed`, `RequiredEdge`, `OutDegree`, `PositionDistinct`, `KeyUnique`, `ReferenceResolves`, `Acyclic`, `External` |
/// | `AttrValue` | `ValueConstraint`, `RequiredValue`, `KeyUnique`, `ReferenceResolves` |
/// | `AttrField`, `SetField`, `RemoveField` | `Attribute`, `KeyUnique`, `ReferenceResolves` |
/// | `AttrMeta` | `NodeAnchor`, `FanWellFormed`, `KeyUnique`, `External` |
/// | `AttrAnchor` | `NodeAnchor`, `EndpointCoherence`, `FanWellFormed`, `RequiredEdge`, `OutDegree`, `PositionDistinct`, `ValueConstraint`, `RequiredValue`, `Attribute`, `KeyUnique`, `ReferenceResolves`, `Acyclic`, `External` |
/// | `AttrEdgeLabel` | `EdgeDeclared`, `EndpointCoherence`, `FanWellFormed`, `RequiredEdge`, `OutDegree`, `PositionDistinct`, `KeyUnique`, `ReferenceResolves`, `Acyclic` |
/// | `AttrEdgePosition` | `PositionDistinct`, `KeyUnique` |
/// | `AddRow` | `NodeAnchor`, `RequiredEdge`, `OutDegree`, `ValueConstraint`, `RequiredValue`, `Attribute`, `KeyUnique`, `ReferenceResolves`, `External` |
/// | `AddPair` | `EdgeDeclared`, `EndpointCoherence`, `RequiredEdge`, `OutDegree`, `KeyUnique`, `ReferenceResolves`, `External` |
/// | `DelPair` | `RequiredEdge`, `OutDegree`, `KeyUnique`, `ReferenceResolves`, `External` |
/// | `Cell` | `ValueConstraint`, `RequiredValue`, `Attribute`, `KeyUnique`, `ReferenceResolves` |
/// | `Table` | `NodeAnchor`, `EndpointCoherence`, `RequiredEdge`, `OutDegree`, `ValueConstraint`, `RequiredValue`, `Attribute`, `KeyUnique`, `ReferenceResolves`, `External` |
/// | `InsertNode` | `NodeAnchor`, `ReservedVocabulary`, `EdgeDeclared`, `EndpointCoherence`, `RequiredEdge`, `OutDegree`, `ValueConstraint`, `RequiredValue`, `Attribute`, `TreeParentMap`, `KeyUnique`, `ReferenceResolves`, `TreeReachable`, `External` |
/// | `DeleteNode` | `FanWellFormed`, `RequiredEdge`, `OutDegree`, `TreeRoot`, `TreeParentMap`, `KeyUnique`, `ReferenceResolves`, `TreeReachable`, `External` |
/// | `ContractNode`, `JoinFeatures` | `NodeAnchor`, `ReservedVocabulary`, `EdgeDeclared`, `EndpointCoherence`, `FanWellFormed`, `RequiredEdge`, `OutDegree`, `ValueConstraint`, `RequiredValue`, `Attribute`, `TreeRoot`, `TreeParentMap`, `KeyUnique`, `ReferenceResolves`, `TreeReachable`, `External` |
/// | `RelabelNode` | `NodeAnchor`, `EndpointCoherence`, `RequiredEdge`, `OutDegree`, `ValueConstraint`, `RequiredValue`, `Attribute`, `KeyUnique`, `ReferenceResolves`, `External` |
/// | `MoveSubtree` | `ReservedVocabulary`, `EdgeDeclared`, `EndpointCoherence`, `RequiredEdge`, `OutDegree`, `TreeParentMap`, `KeyUnique`, `ReferenceResolves`, `TreeReachable`, `External` |
/// | `InsertFan`, `DeleteFan` | `FanWellFormed` |
///
/// A kind with no compiled check is dropped from its row, and a row left
/// empty is dropped. The kinds whose subject an edit removes are not listed:
/// their instances retire with the element.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TriggerTable(
    /// The check kinds each change can affect.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub BTreeMap<DeltaElementKind, BTreeSet<CheckKind>>,
);

impl TriggerTable {
    /// The fixed table restricted to the kinds in `compiled`.
    #[must_use]
    pub fn restricted_to(compiled: &BTreeSet<CheckKind>) -> Self {
        let mut rows = BTreeMap::new();
        for change in DeltaElementKind::ALL {
            let kinds: BTreeSet<CheckKind> = change
                .triggers()
                .iter()
                .copied()
                .filter(|k| compiled.contains(k))
                .collect();
            if !kinds.is_empty() {
                rows.insert(change, kinds);
            }
        }
        Self(rows)
    }

    /// The compiled kinds a change of kind `change` can affect.
    #[must_use]
    pub fn kinds(&self, change: DeltaElementKind) -> Option<&BTreeSet<CheckKind>> {
        self.0.get(&change)
    }
}

/// One compiled check.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckSpec {
    /// The check.
    pub key: CheckKey,
    /// Its class.
    pub class: CheckClass,
    /// The shapes it applies to, with how much of a partitioned instance of
    /// each shape one instance reads.
    pub locality: BTreeMap<InstanceShapeKind, Locality>,
    /// Whether it decides the status.
    pub requirement: Requirement,
    /// The meaning of the constraint it checks, for a constraint check.
    pub semantics: Option<ConstraintSemantics>,
    /// The vertices whose elements it is about; empty for a check about
    /// every element or the whole instance.
    pub anchors: BTreeSet<Name>,
}

impl CheckSpec {
    /// Whether the check applies to instances of `shape`.
    #[must_use]
    pub fn applies_to(&self, shape: InstanceShapeKind) -> bool {
        self.locality.contains_key(&shape)
    }
}

/// A named format a [`ConstraintSemantics::Format`] constraint can check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// `date-time` or `datetime`: a date-time lexeme.
    DateTime,
    /// `date`: a date lexeme.
    Date,
    /// `time`: a time lexeme.
    Time,
    /// `uri`: a scheme, a colon, and no whitespace.
    Uri,
    /// `uuid`: 8-4-4-4-12 hexadecimal digits.
    Uuid,
    /// `ipv4`: a dotted-quad address.
    Ipv4,
    /// `ipv6`: an IPv6 address.
    Ipv6,
    /// `did`: `did:`, a lowercase method name, a colon and an identifier.
    Did,
}

impl Format {
    /// The format a name denotes, if it is one of the supported set.
    pub(crate) fn named(name: &str) -> Option<Self> {
        match name {
            "date-time" | "datetime" => Some(Self::DateTime),
            "date" => Some(Self::Date),
            "time" => Some(Self::Time),
            "uri" => Some(Self::Uri),
            "uuid" => Some(Self::Uuid),
            "ipv4" => Some(Self::Ipv4),
            "ipv6" => Some(Self::Ipv6),
            "did" => Some(Self::Did),
            _ => None,
        }
    }
}

/// A bound of a `Minimum` or `Maximum` constraint, read from its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Bound {
    /// A number, compared numerically.
    Number(crate::value::DecimalValue),
    /// A date, time or date-time, compared on the instant.
    Temporal(crate::value::Temporal),
}

/// What a value constraint tests.
#[derive(Clone, Debug)]
pub enum ValueTest {
    /// A length bound, upper when `upper`.
    Length {
        unit: LengthUnit,
        limit: u64,
        upper: bool,
    },
    /// An inclusive bound, upper when `upper`.
    Bound { bound: Bound, upper: bool },
    /// One of the listed members.
    Members(Vec<String>),
    /// A match of the regular expression.
    Pattern(regex::Regex),
    /// A lexeme of the format.
    Format(Format),
}

/// What evaluating one compiled check needs, beyond its [`CheckSpec`].
#[derive(Clone, Debug)]
pub enum Compiled {
    /// A structural check with no parameters.
    Structural,
    /// The elements of the check's anchor have an out-edge labelled like
    /// `edge`.
    Required { edge: Edge },
    /// The out-degree of the elements of the check's anchor (along `edge`,
    /// or every non-fan out-edge) lies in `[min, max]`.
    Degree {
        edge: Option<Edge>,
        min: Option<u32>,
        max: Option<u32>,
    },
    /// No two out-edges labelled `edge` of one element share a position.
    Ordering { edge: Edge },
    /// The values at `vertex` pass `test`.
    Value {
        vertex: Name,
        sort: Name,
        bound: String,
        test: ValueTest,
    },
    /// The values at `vertex` are present (`parts` `None`), or each part of
    /// a primary key of the elements of `vertex` is.
    RequiredValue {
        vertex: Name,
        parts: Option<Vec<panproto_schema::KeyPart>>,
    },
    /// The values of `key` are unique among the elements of `vertex`.
    KeyUnique { vertex: Name, key: Key },
    /// Each holder's value names exactly one element through `target`.
    Reference { reference: Reference, target: Key },
    /// The edges labelled by any of `labels` form no cycle.
    Acyclic { labels: BTreeSet<Edge> },
    /// An external check whose verdicts are filed under `name`, about the
    /// elements of the check's anchors.
    External { name: Name },
}

/// A schema and a protocol compiled into the checks an instance must pass.
///
/// [`ValidationPlan::compile`] is infallible: every problem found while
/// compiling is a static finding, an [`IncompleteCheck`] on the static check
/// that found it and on every check that depends on the declaration, never an
/// error, so a caller always reads a status. Every report the plan produces
/// lists the static findings, and none has status `Passed` while a required
/// one exists. A schema problem never makes a report `Failed`: the instance
/// is not what failed.
///
/// Validation is STRICT: every element must be typed, so an element whose
/// anchor the schema does not declare is a `NodeAnchor` failure, never an
/// element validation skips.
///
/// A plan is keyed by the digest of its schema, so plans for different
/// schemas (including schemas composed in different modes) are different
/// plans. The plan reads only the instance it is given; trees, tables and
/// graphs each have their own validator, and validation never routes an
/// instance through a shape bridge.
#[derive(Clone, Debug)]
pub struct ValidationPlan {
    schema_digest: Digest,
    digest: Digest,
    checks: BTreeMap<CheckKey, CheckSpec>,
    static_findings: Vec<IncompleteCheck>,
    triggers: TriggerTable,
    pub(crate) schema: Schema,
    pub(crate) compiled: BTreeMap<CheckKey, Compiled>,
    pub(crate) options: ValidationOptions,
}

impl ValidationPlan {
    /// The digest of the schema the plan was compiled from.
    #[must_use]
    pub const fn schema_digest(&self) -> &Digest {
        &self.schema_digest
    }

    /// The plan's identity: [`VALIDATION_PLAN_CONTEXT`] over the schema
    /// digest, the protocol's constraint semantics and the options.
    #[must_use]
    pub const fn digest(&self) -> &Digest {
        &self.digest
    }

    /// The compiled checks.
    #[must_use]
    pub const fn checks(&self) -> &BTreeMap<CheckKey, CheckSpec> {
        &self.checks
    }

    /// The problems found while compiling, sorted by check.
    #[must_use]
    pub fn static_findings(&self) -> &[IncompleteCheck] {
        &self.static_findings
    }

    /// The trigger table, restricted to the compiled kinds.
    #[must_use]
    pub const fn triggers(&self) -> &TriggerTable {
        &self.triggers
    }

    /// The schema the plan was compiled from.
    #[must_use]
    pub const fn schema(&self) -> &Schema {
        &self.schema
    }

    /// Whether `key` has a static finding, so that none of its instances is
    /// evaluated.
    pub(crate) fn is_statically_incomplete(&self, key: &CheckKey) -> bool {
        self.static_findings.iter().any(|f| &f.check == key)
    }

    /// Compile `schema` under `protocol` and `options`.
    #[must_use]
    pub fn compile(schema: &Schema, protocol: &Protocol, options: &ValidationOptions) -> Self {
        let mut builder = Builder::new(schema, protocol, options);
        builder.structural();
        builder.attributes();
        builder.required_edges();
        builder.orderings();
        builder.constraints();
        builder.keys();
        builder.references();
        builder.external();
        builder.hyper_edges();
        builder.schema_well_formed();
        builder.finish()
    }
}

/// The plan under construction.
struct Builder<'a> {
    schema: &'a Schema,
    protocol: &'a Protocol,
    options: &'a ValidationOptions,
    checks: BTreeMap<CheckKey, CheckSpec>,
    compiled: BTreeMap<CheckKey, Compiled>,
    findings: Vec<IncompleteCheck>,
}

const ALL_SHAPES: [InstanceShapeKind; 3] = [
    InstanceShapeKind::Tree,
    InstanceShapeKind::Table,
    InstanceShapeKind::Graph,
];

impl<'a> Builder<'a> {
    const fn new(
        schema: &'a Schema,
        protocol: &'a Protocol,
        options: &'a ValidationOptions,
    ) -> Self {
        Self {
            schema,
            protocol,
            options,
            checks: BTreeMap::new(),
            compiled: BTreeMap::new(),
            findings: Vec::new(),
        }
    }

    /// Add a check applying to every shape its kind applies to.
    fn add(
        &mut self,
        key: CheckKey,
        compiled: Compiled,
        semantics: Option<ConstraintSemantics>,
        anchors: BTreeSet<Name>,
    ) {
        let shapes: Vec<InstanceShapeKind> = ALL_SHAPES
            .into_iter()
            .filter(|s| key.kind.applies_to(*s))
            .collect();
        self.add_on(
            key,
            compiled,
            semantics,
            anchors,
            &shapes,
            Requirement::Required,
        );
    }

    /// Add a check applying to `shapes`, with default requirement `default`.
    fn add_on(
        &mut self,
        key: CheckKey,
        compiled: Compiled,
        semantics: Option<ConstraintSemantics>,
        anchors: BTreeSet<Name>,
        shapes: &[InstanceShapeKind],
        default: Requirement,
    ) {
        let locality = shapes
            .iter()
            .filter(|s| key.kind.applies_to(**s))
            .map(|s| (*s, key.kind.locality()))
            .collect();
        let requirement = self
            .options
            .requirements
            .get(&key.decl)
            .copied()
            .unwrap_or(default);
        let spec = CheckSpec {
            key: key.clone(),
            class: key.kind.class(),
            locality,
            requirement,
            semantics,
            anchors,
        };
        self.checks.insert(key.clone(), spec);
        self.compiled.insert(key, compiled);
    }

    /// Record a static finding of `found_by` (one of the two static checks),
    /// and of every key in `dependents`.
    fn finding(
        &mut self,
        found_by: CheckKind,
        reason: &super::report::IncompleteReason,
        dependents: &[CheckKey],
    ) {
        let static_key = CheckKey::new(found_by, SchemaRef::Schema);
        let mut keys = vec![static_key];
        keys.extend(dependents.iter().cloned());
        for key in keys {
            let requirement = self
                .checks
                .get(&key)
                .map_or(Requirement::Required, |s| s.requirement);
            self.findings.push(IncompleteCheck {
                class: key.kind.class(),
                check: key,
                requirement,
                subject: None,
                reason: reason.clone(),
            });
        }
    }

    fn ill_formed(&mut self, detail: String, dependents: &[CheckKey]) {
        self.finding(
            CheckKind::DeclarationInterpretable,
            &super::report::IncompleteReason::IllFormedDeclaration { detail },
            dependents,
        );
    }

    fn structural(&mut self) {
        for kind in [
            CheckKind::SchemaWellFormed,
            CheckKind::DeclarationInterpretable,
            CheckKind::NodeAnchor,
            CheckKind::ReservedVocabulary,
            CheckKind::EdgeDeclared,
            CheckKind::EndpointCoherence,
            CheckKind::FanWellFormed,
            CheckKind::TreeRoot,
            CheckKind::TreeParentMap,
            CheckKind::TreeReachable,
        ] {
            self.add(
                CheckKey::new(kind, SchemaRef::Schema),
                Compiled::Structural,
                None,
                BTreeSet::new(),
            );
        }
    }

    fn attributes(&mut self) {
        let attrs = crate::attributes::AttributeSchema::from_schema(self.schema);
        let mut vertices: Vec<&Name> = attrs.attrs.keys().collect();
        vertices.sort();
        for vertex in vertices {
            self.add(
                CheckKey::new(CheckKind::Attribute, SchemaRef::Vertex(vertex.clone())),
                Compiled::Structural,
                None,
                BTreeSet::from([vertex.clone()]),
            );
        }
    }

    fn required_edges(&mut self) {
        let mut required: Vec<(&Name, &Edge)> = self
            .schema
            .required
            .iter()
            .flat_map(|(v, edges)| edges.iter().map(move |e| (v, e)))
            .collect();
        required.sort();
        for (vertex, edge) in required {
            let key = CheckKey::new(
                CheckKind::RequiredEdge,
                SchemaRef::Required {
                    vertex: vertex.clone(),
                    edge: edge.clone(),
                },
            );
            self.add(
                key.clone(),
                Compiled::Required { edge: edge.clone() },
                None,
                BTreeSet::from([vertex.clone()]),
            );
            if !self.schema.edges.contains_key(edge) {
                self.ill_formed(
                    format!(
                        "vertex {vertex} requires edge {} -> {} ({}), which the schema does not declare",
                        edge.src, edge.tgt, edge.kind
                    ),
                    &[key],
                );
            }
        }
    }

    fn orderings(&mut self) {
        let mut ordered: Vec<&Edge> = self.schema.orderings.keys().collect();
        ordered.sort();
        for edge in ordered {
            let key = CheckKey::new(
                CheckKind::PositionDistinct,
                SchemaRef::Ordering(edge.clone()),
            );
            self.add(
                key.clone(),
                Compiled::Ordering { edge: edge.clone() },
                None,
                BTreeSet::from([edge.src.clone()]),
            );
            if !self.schema.edges.contains_key(edge) {
                self.ill_formed(
                    format!(
                        "ordered edge {} -> {} ({}) is not declared by the schema",
                        edge.src, edge.tgt, edge.kind
                    ),
                    &[key],
                );
            }
        }
    }
}

/// The check kind a constraint with meaning `semantics` compiles to.
const fn constraint_kind(semantics: &ConstraintSemantics) -> CheckKind {
    match semantics {
        ConstraintSemantics::MinOutDegree | ConstraintSemantics::MaxOutDegree => {
            CheckKind::OutDegree
        }
        ConstraintSemantics::RequiredValue => CheckKind::RequiredValue,
        ConstraintSemantics::Acyclic => CheckKind::Acyclic,
        ConstraintSemantics::External => CheckKind::External,
        _ => CheckKind::ValueConstraint,
    }
}

/// Read the bound of a `Minimum` or `Maximum` constraint: a number, else a
/// date, time or date-time.
fn parse_bound(text: &str) -> Option<Bound> {
    crate::value::DecimalValue::parse(text)
        .map(Bound::Number)
        .ok()
        .or_else(|| {
            crate::value::Temporal::parse(text)
                .ok()
                .map(Bound::Temporal)
        })
}

/// Read the members of an `Enum` constraint: a JSON array (each string
/// member as its contents, any other member as its JSON text), else the
/// comma-separated list the protocol parsers write.
fn parse_members(text: &str) -> Vec<String> {
    if text.trim_start().starts_with('[')
        && let Ok(items) = serde_json::from_str::<Vec<serde_json::Value>>(text)
    {
        return items
            .into_iter()
            .map(|item| match item {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            })
            .collect();
    }
    text.split(',').map(str::to_owned).collect()
}

impl Builder<'_> {
    fn constraints(&mut self) {
        let mut all: Vec<(&Name, &panproto_schema::Constraint)> = self
            .schema
            .constraints
            .iter()
            .flat_map(|(v, cs)| cs.iter().map(move |c| (v, c)))
            .collect();
        all.sort();
        all.dedup();
        for (vertex, constraint) in all {
            self.constraint(vertex, constraint);
        }
    }

    fn constraint(&mut self, vertex: &Name, constraint: &panproto_schema::Constraint) {
        let decl = SchemaRef::Constraint {
            vertex: vertex.clone(),
            constraint: constraint.clone(),
        };
        let anchors = BTreeSet::from([vertex.clone()]);
        let sort = constraint.sort.clone();
        let Some(semantics) = self
            .protocol
            .constraint_semantics
            .get(constraint.sort.as_ref())
            .cloned()
        else {
            let key = CheckKey::new(CheckKind::ValueConstraint, decl);
            self.add(key.clone(), Compiled::Structural, None, anchors);
            self.finding(
                CheckKind::DeclarationInterpretable,
                &super::report::IncompleteReason::UnsupportedDeclaration { sort },
                &[key],
            );
            return;
        };
        if !semantics.compiles_to_check() {
            if let Err(e) = semantics.parse_value(&constraint.sort, &constraint.value) {
                self.ill_formed(e.to_string(), &[]);
            }
            return;
        }
        let key = CheckKey::new(constraint_kind(&semantics), decl);
        let value = match semantics.parse_value(&constraint.sort, &constraint.value) {
            Ok(value) => value,
            Err(e) => {
                self.add(key.clone(), Compiled::Structural, Some(semantics), anchors);
                self.ill_formed(e.to_string(), &[key]);
                return;
            }
        };
        let compiled = self.compile_constraint(vertex, constraint, &semantics, value);
        match compiled {
            Ok(Some(compiled)) => self.add(key, compiled, Some(semantics), anchors),
            Ok(None) => {}
            Err(reason) => {
                self.add(key.clone(), Compiled::Structural, Some(semantics), anchors);
                self.finding(CheckKind::DeclarationInterpretable, &reason, &[key]);
            }
        }
    }

    /// The compiled form of one constraint whose value reads under its
    /// meaning: `Ok(None)` when it compiles to no check, `Err` with the
    /// static finding when it cannot be evaluated.
    fn compile_constraint(
        &self,
        vertex: &Name,
        constraint: &panproto_schema::Constraint,
        semantics: &ConstraintSemantics,
        value: ConstraintValue,
    ) -> Result<Option<Compiled>, super::report::IncompleteReason> {
        use super::report::IncompleteReason as R;
        let ill = |detail: String| R::IllFormedDeclaration { detail };
        let sort = constraint.sort.clone();
        let bound = constraint.value.clone();
        let value_check = |test| {
            Ok(Some(Compiled::Value {
                vertex: vertex.clone(),
                sort: sort.clone(),
                bound: bound.clone(),
                test,
            }))
        };
        match (semantics, value) {
            (
                ConstraintSemantics::MaxLength(_) | ConstraintSemantics::MinLength(_),
                ConstraintValue::Length { unit, limit },
            ) => value_check(ValueTest::Length {
                unit,
                limit,
                upper: matches!(semantics, ConstraintSemantics::MaxLength(_)),
            }),
            (
                ConstraintSemantics::Minimum | ConstraintSemantics::Maximum,
                ConstraintValue::Bound(text),
            ) => parse_bound(&text).map_or_else(
                || {
                    Err(ill(format!(
                        "{} = {text:?} is neither a number nor a date or time",
                        constraint.sort
                    )))
                },
                |b| {
                    value_check(ValueTest::Bound {
                        bound: b,
                        upper: matches!(semantics, ConstraintSemantics::Maximum),
                    })
                },
            ),
            (ConstraintSemantics::Enum, ConstraintValue::Text(text)) => {
                value_check(ValueTest::Members(parse_members(&text)))
            }
            (ConstraintSemantics::Const, ConstraintValue::Text(text)) => {
                value_check(ValueTest::Members(vec![text]))
            }
            (ConstraintSemantics::Pattern, ConstraintValue::Text(text)) => {
                match regex::Regex::new(&text) {
                    Ok(re) => value_check(ValueTest::Pattern(re)),
                    Err(e) => Err(ill(format!("pattern {text:?} does not compile: {e}"))),
                }
            }
            (ConstraintSemantics::Format(_), ConstraintValue::Format { format, .. }) => {
                Format::named(&format).map_or_else(
                    || Err(R::UnsupportedDeclaration { sort: sort.clone() }),
                    |f| value_check(ValueTest::Format(f)),
                )
            }
            (
                ConstraintSemantics::MinOutDegree | ConstraintSemantics::MaxOutDegree,
                ConstraintValue::Degree(n),
            ) => {
                let n = u32::try_from(n)
                    .map_err(|_| ill(format!("{sort} = {n} exceeds the largest degree")))?;
                let upper = matches!(semantics, ConstraintSemantics::MaxOutDegree);
                Ok(Some(Compiled::Degree {
                    edge: None,
                    min: (!upper).then_some(n),
                    max: upper.then_some(n),
                }))
            }
            (ConstraintSemantics::RequiredValue, ConstraintValue::Required(required)) => {
                Ok(required.then(|| Compiled::RequiredValue {
                    vertex: vertex.clone(),
                    parts: None,
                }))
            }
            (ConstraintSemantics::Acyclic, ConstraintValue::Acyclic(name)) => {
                let labels: BTreeSet<Edge> = self
                    .schema
                    .edges
                    .keys()
                    .filter(|e| {
                        &e.src == vertex && &e.tgt == vertex && e.name.as_ref() == Some(&name)
                    })
                    .cloned()
                    .collect();
                if labels.is_empty() {
                    Err(ill(format!(
                        "acyclic constraint on {vertex} names {name}, which is no self-edge of {vertex}"
                    )))
                } else {
                    Ok(Some(Compiled::Acyclic { labels }))
                }
            }
            (ConstraintSemantics::External, _) => Ok(Some(Compiled::External { name: sort })),
            (semantics, value) => Err(ill(format!(
                "{} reads as {value:?}, which {semantics:?} does not take",
                constraint.sort
            ))),
        }
    }
}

impl Builder<'_> {
    fn keys(&mut self) {
        let declared: Vec<(Name, Key)> = self
            .schema
            .declarations
            .keys
            .iter()
            .flat_map(|(v, keys)| keys.iter().map(move |k| (v.clone(), k.clone())))
            .collect();
        for (vertex, key) in declared {
            let decl = SchemaRef::Key {
                vertex: vertex.clone(),
                key: key.id.clone(),
            };
            let anchors = BTreeSet::from([vertex.clone()]);
            if key.role == panproto_schema::KeyRole::Primary {
                self.add(
                    CheckKey::new(CheckKind::RequiredValue, decl.clone()),
                    Compiled::RequiredValue {
                        vertex: vertex.clone(),
                        parts: Some(key.parts.clone()),
                    },
                    None,
                    anchors.clone(),
                );
            }
            self.add(
                CheckKey::new(CheckKind::KeyUnique, decl),
                Compiled::KeyUnique { vertex, key },
                None,
                anchors,
            );
        }
    }

    fn references(&mut self) {
        let syntactic = self.protocol.syntactic_shape;
        let semantic = self.protocol.semantic_shape;
        let references: Vec<Reference> = self
            .schema
            .declarations
            .references
            .values()
            .cloned()
            .collect();
        for reference in references {
            let decl = SchemaRef::Reference(reference.id.clone());
            let anchors = BTreeSet::from([reference.holder.clone()]);
            if syntactic != semantic {
                let key = CheckKey::new(CheckKind::ReferenceResolves, decl.clone());
                let default = if reference.on_unresolved == panproto_schema::OnUnresolved::Error {
                    Requirement::Required
                } else {
                    Requirement::Advisory
                };
                let target = self.schema.declarations.resolving_key(&reference).cloned();
                let supported = matches!(
                    reference.source,
                    panproto_schema::RefSource::KeyValue { .. }
                );
                let compiled = match (&target, supported) {
                    (Some(target), true) => Compiled::Reference {
                        reference: reference.clone(),
                        target: target.clone(),
                    },
                    _ => Compiled::Structural,
                };
                self.add_on(
                    key.clone(),
                    compiled,
                    None,
                    anchors.clone(),
                    &[syntactic],
                    default,
                );
                if !supported {
                    self.finding(
                        CheckKind::DeclarationInterpretable,
                        &super::report::IncompleteReason::Unsupported {
                            detail: format!(
                                "reference {} reads its value through a path, standoff or variable source, which validation does not resolve",
                                reference.id
                            ),
                        },
                        &[key],
                    );
                } else if target.is_none() {
                    self.ill_formed(
                        format!(
                            "reference {} names no key of {} to resolve through",
                            reference.id, reference.target_vertex
                        ),
                        &[key],
                    );
                }
            }
            if let Some(edge) = &reference.edge {
                if reference.cardinality == panproto_schema::Cardinality::One {
                    self.add_on(
                        CheckKey::new(CheckKind::OutDegree, decl.clone()),
                        Compiled::Degree {
                            edge: Some(edge.clone()),
                            min: None,
                            max: Some(1),
                        },
                        None,
                        anchors.clone(),
                        &[semantic],
                        Requirement::Required,
                    );
                }
                if !reference.nullable {
                    self.add_on(
                        CheckKey::new(CheckKind::RequiredEdge, decl.clone()),
                        Compiled::Required { edge: edge.clone() },
                        None,
                        anchors.clone(),
                        &[semantic],
                        Requirement::Required,
                    );
                }
            }
        }
    }

    fn external(&mut self) {
        let shapes: Vec<InstanceShapeKind> = ALL_SHAPES.to_vec();
        for (name, decl) in &self.options.external {
            self.add_on(
                CheckKey::new(CheckKind::External, SchemaRef::External(name.clone())),
                Compiled::External {
                    name: decl.name.clone(),
                },
                None,
                decl.anchors.clone(),
                &shapes,
                decl.requirement,
            );
        }
    }

    fn hyper_edges(&mut self) {
        let fan_key = CheckKey::new(CheckKind::FanWellFormed, SchemaRef::Schema);
        let mut hyper_edges: Vec<&panproto_schema::HyperEdge> =
            self.schema.hyper_edges.values().collect();
        hyper_edges.sort_by(|a, b| a.id.cmp(&b.id));
        let mut problems = Vec::new();
        for h in hyper_edges {
            let mut signature: Vec<(&Name, &Name)> = h.signature.iter().collect();
            signature.sort();
            for (label, vertex) in &signature {
                if !self.schema.has_vertex(vertex) {
                    problems.push(format!(
                        "hyper-edge {} labels {label} with {vertex}, which is not a vertex",
                        h.id
                    ));
                }
            }
            if self.protocol.name == "sql" && h.kind == "table" {
                let table = &h.parent_label;
                let columns: BTreeSet<(Name, Name)> = self
                    .schema
                    .outgoing_edges(table.as_ref())
                    .iter()
                    .filter(|e| e.kind == "prop")
                    .filter_map(|e| e.name.clone().map(|n| (n, e.tgt.clone())))
                    .collect();
                let declared: BTreeSet<(Name, Name)> = signature
                    .iter()
                    .map(|(l, v)| ((*l).clone(), (*v).clone()))
                    .collect();
                if columns != declared {
                    problems.push(format!(
                        "table {table}: hyper-edge {} signature does not equal the table's columns",
                        h.id
                    ));
                }
            }
        }
        for detail in problems {
            self.ill_formed(detail, std::slice::from_ref(&fan_key));
        }
    }

    /// The compiled checks a schema well-formedness error makes
    /// uninterpretable.
    fn dependents_of(&self, error: &panproto_schema::ValidationError) -> Vec<CheckKey> {
        use panproto_schema::{KeyRefError as E, ValidationError as V};
        let keys_where = |pred: &dyn Fn(&CheckKey) -> bool| -> Vec<CheckKey> {
            self.checks.keys().filter(|k| pred(k)).cloned().collect()
        };
        let decls = &self.schema.declarations;
        let key_decl = |k: &CheckKey, f: &dyn Fn(&Name, &Key) -> bool| match &k.decl {
            SchemaRef::Key { vertex, key } => decls.key(vertex, key).is_some_and(|d| f(vertex, d)),
            _ => false,
        };
        let ref_decl = |k: &CheckKey, f: &dyn Fn(&Reference) -> bool| match &k.decl {
            SchemaRef::Reference(id) => decls.references.get(id).is_some_and(f),
            _ => false,
        };
        match error {
            V::DanglingRequiredEdge { vertex, .. } => keys_where(
                &|k| matches!(&k.decl, SchemaRef::Required { vertex: v, .. } if v.as_ref() == vertex),
            ),
            V::InvalidConstraintSort { vertex, sort } => keys_where(&|k| {
                matches!(&k.decl, SchemaRef::Constraint { vertex: v, constraint }
                    if v.as_ref() == vertex && constraint.sort.as_ref() == sort)
            }),
            V::KeyRef(e) => match e {
                E::UnknownVertex { vertex } => keys_where(&|k| {
                    key_decl(k, &|v, _| v == vertex)
                        || ref_decl(k, &|r| &r.holder == vertex || &r.target_vertex == vertex)
                }),
                E::DuplicateKeyId { vertex, key } => keys_where(
                    &|k| matches!(&k.decl, SchemaRef::Key { vertex: v, key: id } if v == vertex && id == key),
                ),
                E::PathDoesNotResolve { owner, path } => keys_where(&|k| {
                    key_decl(k, &|v, d| {
                        v == owner
                            && d.parts.iter().any(
                                |p| matches!(p, panproto_schema::KeyPart::Path(p) if p == path),
                            )
                    }) || ref_decl(k, &|r| &r.holder == owner)
                }),
                E::PositionNotOrdered { key, edge } => keys_where(&|k| {
                    key_decl(k, &|_, d| {
                        &d.id == key
                            && d.parts.iter().any(|p| {
                                matches!(p, panproto_schema::KeyPart::Position { within } if within == edge)
                            })
                    })
                }),
                E::UnknownTargetKey { reference, .. }
                | E::ArityMismatch { reference, .. }
                | E::BinderNotIdentifier { reference, .. }
                | E::EdgeMismatch { reference, .. }
                | E::EdgeRequired { reference } => {
                    keys_where(&|k| matches!(&k.decl, SchemaRef::Reference(id) if id == reference))
                }
                E::WithinNotAncestor { key, vertex } => keys_where(&|k| {
                    key_decl(k, &|_, d| {
                        &d.id == key && d.scope == panproto_schema::KeyScope::Within(vertex.clone())
                    })
                }),
            },
            _ => Vec::new(),
        }
    }

    fn schema_well_formed(&mut self) {
        let errors = panproto_schema::validate(self.schema, self.protocol);
        for error in errors {
            let dependents = self.dependents_of(&error);
            self.finding(
                CheckKind::SchemaWellFormed,
                &super::report::IncompleteReason::IllFormedDeclaration {
                    detail: error.to_string(),
                },
                &dependents,
            );
        }
    }
}

impl Builder<'_> {
    fn finish(self) -> ValidationPlan {
        let mut findings = self.findings;
        super::report::sort_incomplete(&mut findings);
        findings.dedup();
        let kinds: BTreeSet<CheckKind> = self.checks.keys().map(|k| k.kind).collect();
        let schema_digest = schema_digest(self.schema);
        let digest = plan_digest(&schema_digest, self.protocol, self.options);
        ValidationPlan {
            schema_digest,
            digest,
            checks: self.checks,
            static_findings: findings,
            triggers: TriggerTable::restricted_to(&kinds),
            schema: self.schema.clone(),
            compiled: self.compiled,
            options: self.options.clone(),
        }
    }
}

/// [`VALIDATION_PLAN_CONTEXT`] over the schema digest, then the protocol's
/// constraint semantics (count, then each sort and meaning in ascending sort
/// order), then the options: the requirement overrides, the external checks
/// and the source map, each behind its count and in key order.
fn plan_digest(schema: &Digest, protocol: &Protocol, options: &ValidationOptions) -> Digest {
    let mut w = Digest::hasher(VALIDATION_PLAN_CONTEXT);
    w.write(&schema.0);
    w.len(protocol.constraint_semantics.len());
    for (sort, semantics) in &protocol.constraint_semantics {
        w.str(sort);
        write_semantics(&mut w, semantics);
    }
    w.len(options.requirements.len());
    for (decl, requirement) in &options.requirements {
        write_schema_ref(&mut w, decl);
        write_requirement(&mut w, *requirement);
    }
    w.len(options.external.len());
    for (name, decl) in &options.external {
        w.name(name);
        w.name(&decl.name);
        w.len(decl.anchors.len());
        for anchor in &decl.anchors {
            w.name(anchor);
        }
        write_requirement(&mut w, decl.requirement);
    }
    w.len(options.sources.0.len());
    for (decl, source) in &options.sources.0 {
        write_schema_ref(&mut w, decl);
        write_source(&mut w, source);
    }
    w.finalize()
}

fn write_semantics<W: CanonicalSink + ?Sized>(w: &mut W, semantics: &ConstraintSemantics) {
    let unit = |w: &mut W, unit: &LengthUnit| {
        w.tag(match unit {
            LengthUnit::Chars => 0,
            LengthUnit::Utf8Bytes => 1,
            LengthUnit::Graphemes => 2,
        });
    };
    match semantics {
        ConstraintSemantics::Annotation => w.tag(0),
        ConstraintSemantics::MaxLength(u) => {
            w.tag(1);
            unit(w, u);
        }
        ConstraintSemantics::MinLength(u) => {
            w.tag(2);
            unit(w, u);
        }
        ConstraintSemantics::Minimum => w.tag(3),
        ConstraintSemantics::Maximum => w.tag(4),
        ConstraintSemantics::Enum => w.tag(5),
        ConstraintSemantics::Const => w.tag(6),
        ConstraintSemantics::Pattern => w.tag(7),
        ConstraintSemantics::Format(family) => {
            w.tag(8);
            w.name(family);
        }
        ConstraintSemantics::MinOutDegree => w.tag(9),
        ConstraintSemantics::MaxOutDegree => w.tag(10),
        ConstraintSemantics::RequiredValue => w.tag(11),
        ConstraintSemantics::Acyclic => w.tag(12),
        ConstraintSemantics::External => w.tag(13),
        ConstraintSemantics::ValueParameter => w.tag(14),
    }
}
