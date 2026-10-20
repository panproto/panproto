//! Finite validation carriers whose alphabets are fixed by the contract.
//!
//! C9 fixes the element and structural-schema bounds, but it does not fix a
//! finite alphabet for every declaration constructor. This module therefore
//! enumerates the complete settled structural carrier and exposes the missing
//! declaration parameters as data instead of presenting a smaller carrier as
//! the complete C9 universe.

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{Fan, GEdge, Graph, Node, NodeShape, SEdge, Table, Tree};
use crate::pages::validation::Semantics;
use crate::schema::{Key, KeyPart, KeyRole, KeyScope, Schema};
use crate::value::Value;

/// The largest number of vertices in the settled C9 structural schema scope.
pub const MAX_VERTICES: usize = 3;
/// The largest number of schema edges in the settled structural scope.
pub const MAX_SCHEMA_EDGES: usize = 2;
/// The largest number of graph elements or table rows in one occurrence.
pub const MAX_ELEMENTS: usize = 3;
/// Number of settled structural schemas.
pub const STRUCTURAL_SCHEMA_COUNT: u64 = 60;
/// Number of graph occurrences over the settled structural schemas.
pub const GRAPH_CASE_COUNT: u64 = 71_610;
/// Number of independently labelled table occurrences.
pub const TABLE_CASE_COUNT: u64 = 571;
/// Number of rooted, ordered tree occurrences over the structural schemas.
pub const TREE_CASE_COUNT: u64 = 871_902;
/// Number of explicitly settled declaration cases.
pub const DECLARATION_CASE_COUNT: u64 = 29;
/// Number of invalid/native-boundary cases.
pub const BOUNDARY_CASE_COUNT: u64 = 12;

const GRAPH_CASE_CAPACITY: usize = 71_610;
const TABLE_CASE_CAPACITY: usize = 571;

fn count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn id(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// A declaration family required by V1--V11/C9.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DeclarationFamily {
    /// Required edge.
    RequiredEdge,
    /// Edge ordering.
    Ordering,
    /// Minimum/maximum length.
    Length,
    /// Extended-grapheme-cluster length.
    Graphemes,
    /// Numeric range.
    NumericRange,
    /// Pattern or lexical predicate.
    Pattern,
    /// Finite value membership.
    Enum,
    /// Exact constant equality.
    Const,
    /// Named lexical format.
    Format,
    /// Minimum/maximum count.
    Count,
    /// Required value.
    RequiredValue,
    /// Acyclicity.
    Acyclic,
    /// Key declaration.
    Key,
    /// Reference declaration.
    Reference,
    /// Hyper-edge declaration.
    HyperEdge,
    /// Attribute declaration.
    Attribute,
    /// External predicate.
    External,
    /// A parameter used by value coercion rather than validation.
    ValueParameter,
    /// Registered protocol constraint.
    Registered,
}

/// A finite C9 parameter absent from the binding contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MissingFiniteParameter {
    /// Vertex-kind alphabet.
    VertexKinds,
    /// Edge kind/name alphabet beyond the settled `prop/e` pair.
    EdgeVocabulary,
    /// Whether each bounded node id is present.
    NodePresenceStates,
    /// Values and presence forms available at an own-value site.
    NodeValueAlphabet,
    /// Node-shape/discriminator choices.
    NodeDiscriminatorAlphabet,
    /// Field-name alphabet.
    FieldNameAlphabet,
    /// Field-value/presence alphabet.
    FieldValueAlphabet,
    /// Node and edge position alphabet.
    NodePositionAlphabet,
    /// Annotation-key and annotation-value alphabet.
    AnnotationAlphabet,
    /// Maximum map cardinalities (fields, annotations, signatures and declarations).
    MapBounds,
    /// Maximum name and text lengths.
    NameBounds,
    /// Declaration-family selection and constructor arguments.
    DeclarationArguments,
    /// External-predicate result/evidence alphabet.
    ExternalEvidence,
    /// Registered constraint sorts and their semantics.
    RegisteredConstraints,
    /// Node-shape choices beyond the plain and reserved boundary witnesses.
    NodeShapes,
    /// Edge-position alphabet beyond absent and zero.
    EdgePositions,
    /// Hyper-edge signatures, labels and fan multiplicities.
    FanSignatures,
    /// Table-schema/table-name/column alphabets.
    TableSchemaAlphabet,
    /// Table cell presence and value alphabet.
    TableCellAlphabet,
    /// Table relation, source-row and target-row alphabets.
    TableRelationAlphabet,
    /// Bounds and constructors for malformed graph/table/tree carriers.
    MalformedCarrierBounds,
    /// Bounds for rooted-tree node, arc and root choices.
    RootedTreeBounds,
    /// Bounds for fan lists, child maps and duplicate fan occurrences.
    FanListBounds,
    /// Byte alphabets and length bounds for canonical admission bundles.
    AdmissionBundleBytes,
    /// Labelled id alphabet for invalid/dangling native carriers.
    NativeIds,
}

/// Exact status of the bounded validation enumerator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopeCertificate {
    /// Structural schemas enumerated literally.
    pub structural_schemas: u64,
    /// Graph occurrences enumerated literally.
    pub graph_cases: u64,
    /// Table occurrences enumerated literally.
    pub table_cases: u64,
    /// Tree occurrences enumerated literally.
    pub tree_cases: u64,
    /// Settled declaration cases.
    pub declaration_cases: u64,
    /// Settled invalid/native-boundary cases.
    pub boundary_cases: u64,
    /// Present scalar values; absence is the third cell/node choice.
    pub values: Vec<Value>,
    /// Declaration families the eventual complete C9 alphabet must span.
    pub declaration_families: Vec<DeclarationFamily>,
    /// Parameters that prevent a truthful complete-C9 count.
    pub missing: Vec<MissingFiniteParameter>,
}

/// The exact settled-scope certificate and the explicit C9 incompleteness.
#[must_use]
pub fn scope_certificate() -> ScopeCertificate {
    ScopeCertificate {
        structural_schemas: STRUCTURAL_SCHEMA_COUNT,
        graph_cases: GRAPH_CASE_COUNT,
        table_cases: TABLE_CASE_COUNT,
        tree_cases: TREE_CASE_COUNT,
        declaration_cases: DECLARATION_CASE_COUNT,
        boundary_cases: BOUNDARY_CASE_COUNT,
        values: vec![Value::Int(0), Value::Int(1)],
        declaration_families: vec![
            DeclarationFamily::RequiredEdge,
            DeclarationFamily::Ordering,
            DeclarationFamily::Length,
            DeclarationFamily::Graphemes,
            DeclarationFamily::NumericRange,
            DeclarationFamily::Pattern,
            DeclarationFamily::Enum,
            DeclarationFamily::Const,
            DeclarationFamily::Format,
            DeclarationFamily::Count,
            DeclarationFamily::RequiredValue,
            DeclarationFamily::Acyclic,
            DeclarationFamily::Key,
            DeclarationFamily::Reference,
            DeclarationFamily::HyperEdge,
            DeclarationFamily::Attribute,
            DeclarationFamily::External,
            DeclarationFamily::ValueParameter,
            DeclarationFamily::Registered,
        ],
        missing: vec![
            MissingFiniteParameter::VertexKinds,
            MissingFiniteParameter::EdgeVocabulary,
            MissingFiniteParameter::NodePresenceStates,
            MissingFiniteParameter::NodeValueAlphabet,
            MissingFiniteParameter::NodeDiscriminatorAlphabet,
            MissingFiniteParameter::FieldNameAlphabet,
            MissingFiniteParameter::FieldValueAlphabet,
            MissingFiniteParameter::NodePositionAlphabet,
            MissingFiniteParameter::AnnotationAlphabet,
            MissingFiniteParameter::MapBounds,
            MissingFiniteParameter::NameBounds,
            MissingFiniteParameter::DeclarationArguments,
            MissingFiniteParameter::ExternalEvidence,
            MissingFiniteParameter::RegisteredConstraints,
            MissingFiniteParameter::NodeShapes,
            MissingFiniteParameter::EdgePositions,
            MissingFiniteParameter::FanSignatures,
            MissingFiniteParameter::TableSchemaAlphabet,
            MissingFiniteParameter::TableCellAlphabet,
            MissingFiniteParameter::TableRelationAlphabet,
            MissingFiniteParameter::MalformedCarrierBounds,
            MissingFiniteParameter::RootedTreeBounds,
            MissingFiniteParameter::FanListBounds,
            MissingFiniteParameter::AdmissionBundleBytes,
            MissingFiniteParameter::NativeIds,
        ],
    }
}

/// Every settled structural schema with `0..=3` vertices of kind `object`
/// and at most two `prop(e)` edges.
#[must_use]
pub fn schemas() -> Vec<Schema> {
    (0..=MAX_VERTICES)
        .flat_map(|vertices| super::schemas(vertices, &["object"], &["e"], MAX_SCHEMA_EDGES))
        .collect()
}

/// Closed count for [`schemas`]:
/// `sum_(v=0)^3 sum_(e=0)^min(2,v^2) C(v^2,e) = 60`.
#[must_use]
pub fn schemas_count() -> u64 {
    (0..=MAX_VERTICES)
        .map(|vertices| {
            (0..=MAX_SCHEMA_EDGES.min(vertices * vertices))
                .map(|edges| super::choose(count(vertices * vertices), count(edges)))
                .sum::<u64>()
        })
        .sum()
}

/// Independent combinatorial ordinal of a structural schema.
///
/// Vertices are the initial segment `v0..v(n-1)`. Candidate edges are in
/// source-major, target-minor order, and the edge set is ranked by its binary
/// mask among masks of population at most two. This does not call [`schemas`]
/// or the subset enumerator.
#[must_use]
pub fn schema_ordinal(schema: &Schema) -> Option<u64> {
    let vertices = schema.vertices.len();
    if vertices > MAX_VERTICES
        || schema
            .vertices
            .iter()
            .enumerate()
            .any(|(index, (name, kind))| name != &format!("v{index}") || kind != "object")
    {
        return None;
    }
    let mut mask = 0u16;
    for edge in &schema.edges {
        if edge.kind != "prop" || edge.name.as_deref() != Some("e") {
            return None;
        }
        let source = edge.src.strip_prefix('v')?.parse::<usize>().ok()?;
        let target = edge.tgt.strip_prefix('v')?.parse::<usize>().ok()?;
        if source >= vertices || target >= vertices {
            return None;
        }
        mask |= 1u16.checked_shl(u32::try_from(source * vertices + target).ok()?)?;
    }
    if mask.count_ones() > u32::try_from(MAX_SCHEMA_EDGES).ok()? {
        return None;
    }
    let offset: u64 = (0..vertices)
        .map(|size| {
            (0..=MAX_SCHEMA_EDGES.min(size * size))
                .map(|edges| super::choose(count(size * size), count(edges)))
                .sum::<u64>()
        })
        .sum();
    let rank = (0..=mask)
        .filter(|candidate| candidate.count_ones() <= 2)
        .count()
        .checked_sub(1)?;
    Some(offset + count(rank))
}

/// Settled declaration-case identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SettledDeclaration {
    /// Annotation.
    Annotation,
    /// Maximum character length.
    MaxLengthChars,
    /// Maximum byte length.
    MaxLengthBytes,
    /// Minimum character length.
    MinLengthChars,
    /// Minimum byte length.
    MinLengthBytes,
    /// Maximum extended-grapheme length.
    MaxLengthGraphemes,
    /// Minimum extended-grapheme length.
    MinLengthGraphemes,
    /// Numeric minimum.
    Minimum,
    /// Numeric maximum.
    Maximum,
    /// Finite string enumeration.
    Enum,
    /// One exact string constant.
    Const,
    /// Settled anchored-literal pattern.
    Pattern,
    /// Settled URI format.
    Format,
    /// Externally witnessed predicate.
    External,
    /// Value-coercion parameter.
    ValueParameter,
    /// Minimum out-degree.
    MinOutDegree,
    /// Maximum out-degree.
    MaxOutDegree,
    /// Required value.
    RequiredValue,
    /// Acyclic self-edge.
    Acyclic,
    /// Unsupported registered sort.
    Unsupported,
    /// Malformed length bound.
    MalformedLength,
    /// Required edge.
    RequiredEdge,
    /// Ordered edge.
    Ordering,
    /// Primary path key.
    PrimaryPath,
    /// Primary position key.
    PrimaryPosition,
    /// Unique key.
    Unique,
    /// Identifier key.
    Identifier,
    /// Ordered sibling sort key.
    SortKey,
    /// Non-resolving index key.
    Index,
}

/// One declaration schema and its constraint-sort meanings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclarationCase {
    /// Case identity.
    pub declaration: SettledDeclaration,
    /// Schema carrying the declaration.
    pub schema: Schema,
    /// Meanings supplied by the protocol.
    pub semantics: BTreeMap<String, Semantics>,
}

fn constraint_case(
    declaration: SettledDeclaration,
    sort: &str,
    bound: &str,
    semantics: Option<Semantics>,
) -> DeclarationCase {
    let mut schema = Schema::default()
        .vertex("v", "object")
        .edge("v", "v", "prop", Some("e"));
    schema
        .constraints
        .insert("v".to_owned(), vec![(sort.to_owned(), bound.to_owned())]);
    DeclarationCase {
        declaration,
        schema,
        semantics: semantics
            .map(|meaning| BTreeMap::from([(sort.to_owned(), meaning)]))
            .unwrap_or_default(),
    }
}

fn key_case(declaration: SettledDeclaration, role: KeyRole, position: bool) -> DeclarationCase {
    let edge = SEdge::new("v", "v", "prop", Some("e"));
    let mut schema = Schema::default()
        .vertex("v", "object")
        .edge("v", "v", "prop", Some("e"));
    if position {
        schema.orderings.insert(edge.clone(), 0);
    }
    schema.keys.insert(
        "v".to_owned(),
        vec![Key {
            id: format!("{declaration:?}"),
            parts: vec![if position {
                KeyPart::Position { within: edge }
            } else {
                KeyPart::Path(Vec::new())
            }],
            role,
            scope: KeyScope::Document,
            equality: Vec::new(),
        }],
    );
    DeclarationCase {
        declaration,
        schema,
        semantics: BTreeMap::new(),
    }
}

fn sort_key_case() -> DeclarationCase {
    let carrier = SEdge::new("parent", "v", "prop", Some("children"));
    let mut schema = Schema::default()
        .vertex("parent", "object")
        .vertex("v", "object")
        .edge("parent", "v", "prop", Some("children"));
    schema.orderings.insert(carrier, 0);
    schema.keys.insert(
        "v".to_owned(),
        vec![Key {
            id: "sort".to_owned(),
            parts: vec![KeyPart::Path(Vec::new())],
            role: KeyRole::SortKey,
            scope: KeyScope::Within("parent".to_owned()),
            equality: Vec::new(),
        }],
    );
    DeclarationCase {
        declaration: SettledDeclaration::SortKey,
        schema,
        semantics: BTreeMap::new(),
    }
}

fn settled_key_cases() -> [DeclarationCase; 6] {
    [
        key_case(SettledDeclaration::PrimaryPath, KeyRole::Primary, false),
        key_case(SettledDeclaration::PrimaryPosition, KeyRole::Primary, true),
        key_case(SettledDeclaration::Unique, KeyRole::Unique, false),
        key_case(SettledDeclaration::Identifier, KeyRole::Identifier, false),
        sort_key_case(),
        key_case(SettledDeclaration::Index, KeyRole::Index, false),
    ]
}

fn scalar_constraint_cases() -> Vec<DeclarationCase> {
    vec![
        constraint_case(
            SettledDeclaration::Annotation,
            "annotation",
            "x",
            Some(Semantics::Annotation),
        ),
        constraint_case(
            SettledDeclaration::MaxLengthChars,
            "maxChars",
            "1",
            Some(Semantics::MaxLength(true)),
        ),
        constraint_case(
            SettledDeclaration::MaxLengthBytes,
            "maxBytes",
            "1",
            Some(Semantics::MaxLength(false)),
        ),
        constraint_case(
            SettledDeclaration::MinLengthChars,
            "minChars",
            "1",
            Some(Semantics::MinLength(true)),
        ),
        constraint_case(
            SettledDeclaration::MinLengthBytes,
            "minBytes",
            "1",
            Some(Semantics::MinLength(false)),
        ),
        constraint_case(
            SettledDeclaration::MaxLengthGraphemes,
            "maxGraphemes",
            "1",
            Some(Semantics::MaxGraphemes),
        ),
        constraint_case(
            SettledDeclaration::MinLengthGraphemes,
            "minGraphemes",
            "1",
            Some(Semantics::MinGraphemes),
        ),
        constraint_case(
            SettledDeclaration::Minimum,
            "minimum",
            "0",
            Some(Semantics::Minimum),
        ),
        constraint_case(
            SettledDeclaration::Maximum,
            "maximum",
            "1",
            Some(Semantics::Maximum),
        ),
    ]
}

fn other_constraint_cases() -> Vec<DeclarationCase> {
    vec![
        constraint_case(
            SettledDeclaration::Enum,
            "enum",
            "a|b",
            Some(Semantics::Enum),
        ),
        constraint_case(
            SettledDeclaration::Const,
            "const",
            "a",
            Some(Semantics::Const),
        ),
        constraint_case(
            SettledDeclaration::Pattern,
            "pattern",
            "^a$",
            Some(Semantics::Pattern),
        ),
        constraint_case(
            SettledDeclaration::Format,
            "format",
            "uri",
            Some(Semantics::Format),
        ),
        constraint_case(
            SettledDeclaration::External,
            "external",
            "audit",
            Some(Semantics::External),
        ),
        constraint_case(
            SettledDeclaration::ValueParameter,
            "max_length",
            "12",
            Some(Semantics::ValueParameter),
        ),
        constraint_case(
            SettledDeclaration::MinOutDegree,
            "minOut",
            "1",
            Some(Semantics::MinOutDegree),
        ),
        constraint_case(
            SettledDeclaration::MaxOutDegree,
            "maxOut",
            "1",
            Some(Semantics::MaxOutDegree),
        ),
        constraint_case(
            SettledDeclaration::RequiredValue,
            "required",
            "true",
            Some(Semantics::RequiredValue),
        ),
        constraint_case(
            SettledDeclaration::Acyclic,
            "acyclic",
            "e",
            Some(Semantics::Acyclic),
        ),
        constraint_case(SettledDeclaration::Unsupported, "registered", "x", None),
        constraint_case(
            SettledDeclaration::MalformedLength,
            "badLength",
            "x",
            Some(Semantics::MaxLength(true)),
        ),
    ]
}

fn constraint_declaration_cases() -> Vec<DeclarationCase> {
    let mut cases = scalar_constraint_cases();
    cases.extend(other_constraint_cases());
    cases
}

/// One exemplar for each settled declaration facet.
///
/// This is not a Cartesian declaration carrier: argument/name/map alphabets
/// and their bounds remain among [`ScopeCertificate::missing`].
#[must_use]
pub fn declaration_cases() -> Vec<DeclarationCase> {
    let mut cases = constraint_declaration_cases();
    let edge = SEdge::new("v", "v", "prop", Some("e"));
    let mut required = Schema::default()
        .vertex("v", "object")
        .edge("v", "v", "prop", Some("e"));
    required.required.insert("v".to_owned(), vec![edge.clone()]);
    cases.push(DeclarationCase {
        declaration: SettledDeclaration::RequiredEdge,
        schema: required,
        semantics: BTreeMap::new(),
    });
    let mut ordering = Schema::default()
        .vertex("v", "object")
        .edge("v", "v", "prop", Some("e"));
    ordering.orderings.insert(edge, 0);
    cases.push(DeclarationCase {
        declaration: SettledDeclaration::Ordering,
        schema: ordering,
        semantics: BTreeMap::new(),
    });
    cases.extend(settled_key_cases());
    cases
}

/// Expected outcome family of an invalid/native-boundary case.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BoundaryExpectation {
    /// Unknown graph anchor finding.
    UnknownAnchor,
    /// Undeclared graph edge finding.
    UndeclaredEdge,
    /// Dangling graph endpoint finding.
    DanglingEndpoint,
    /// Duplicate graph position finding.
    DuplicatePosition,
    /// Malformed graph fan finding.
    MalformedFan,
    /// Malformed native table refusal.
    MalformedTable,
    /// Dangling native tree arc refusal.
    DanglingArc,
    /// Native tree fan with a missing parent diagnostic.
    MissingFanParent,
    /// Native tree fan with a missing child diagnostic.
    MissingFanChild,
    /// Native tree second-parent finding.
    SecondParent,
    /// Reserved native tree vocabulary finding.
    ReservedVocabulary,
    /// Missing native tree root finding.
    MissingRoot,
}

/// Native instance of a boundary case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BoundaryInstance {
    /// Graph.
    Graph(Graph),
    /// Table.
    Table(Table),
    /// Tree.
    Tree(Tree),
}

/// One invalid/native-boundary occurrence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundaryCase {
    /// Expected outcome family.
    pub expectation: BoundaryExpectation,
    /// Validation schema.
    pub schema: Schema,
    /// Native instance.
    pub instance: BoundaryInstance,
}

fn graph_boundary_cases(schema: &Schema, edge: &SEdge) -> Vec<BoundaryCase> {
    let node = || [(0, Node::new(0, "v"))].into();
    let graph_case = |expectation, graph| BoundaryCase {
        expectation,
        schema: schema.clone(),
        instance: BoundaryInstance::Graph(graph),
    };
    let undeclared = SEdge::new("v", "v", "other", Some("x"));
    let positioned = |id| GEdge {
        src: 0,
        tgt: 0,
        label: edge.clone(),
        position: Some(id),
    };
    let mut fan = Node::new(0, "h");
    fan.shape = NodeShape::HyperEdge;
    vec![
        graph_case(
            BoundaryExpectation::UnknownAnchor,
            Graph {
                nodes: [(0, Node::new(0, "ghost"))].into(),
                ..Graph::default()
            },
        ),
        graph_case(
            BoundaryExpectation::UndeclaredEdge,
            Graph {
                nodes: node(),
                edges: [(
                    0,
                    GEdge {
                        label: undeclared,
                        ..positioned(0)
                    },
                )]
                .into(),
            },
        ),
        graph_case(
            BoundaryExpectation::DanglingEndpoint,
            Graph {
                nodes: node(),
                edges: [(
                    0,
                    GEdge {
                        tgt: 9,
                        position: None,
                        ..positioned(0)
                    },
                )]
                .into(),
            },
        ),
        graph_case(
            BoundaryExpectation::DuplicatePosition,
            Graph {
                nodes: node(),
                edges: [(0, positioned(0)), (1, positioned(0))].into(),
            },
        ),
        graph_case(
            BoundaryExpectation::MalformedFan,
            Graph {
                nodes: [(0, fan)].into(),
                ..Graph::default()
            },
        ),
    ]
}

fn tree_boundary_cases(schema: &Schema, edge: &SEdge) -> Vec<BoundaryCase> {
    let tree_case = |expectation, tree| BoundaryCase {
        expectation,
        schema: schema.clone(),
        instance: BoundaryInstance::Tree(tree),
    };
    let nodes = || [(0, Node::new(0, "v")), (1, Node::new(1, "v"))].into();
    let mut reserved = Node::new(0, "v");
    reserved.shape = NodeShape::HyperEdge;
    vec![
        tree_case(
            BoundaryExpectation::DanglingArc,
            Tree {
                nodes: [(0, Node::new(0, "v"))].into(),
                arcs: vec![(0, 9, edge.clone())],
                fans: Vec::new(),
                root: 0,
            },
        ),
        tree_case(
            BoundaryExpectation::MissingFanParent,
            Tree {
                nodes: [(0, Node::new(0, "v"))].into(),
                arcs: Vec::new(),
                fans: vec![Fan {
                    parent: 9,
                    hyper_edge: "h".to_owned(),
                    children: BTreeMap::new(),
                }],
                root: 0,
            },
        ),
        tree_case(
            BoundaryExpectation::MissingFanChild,
            Tree {
                nodes: [(0, Node::new(0, "v"))].into(),
                arcs: Vec::new(),
                fans: vec![Fan {
                    parent: 0,
                    hyper_edge: "h".to_owned(),
                    children: BTreeMap::from([("child".to_owned(), 9)]),
                }],
                root: 0,
            },
        ),
        tree_case(
            BoundaryExpectation::SecondParent,
            Tree {
                nodes: [
                    (0, Node::new(0, "v")),
                    (1, Node::new(1, "v")),
                    (2, Node::new(2, "v")),
                ]
                .into(),
                arcs: vec![(0, 2, edge.clone()), (1, 2, edge.clone())],
                fans: Vec::new(),
                root: 0,
            },
        ),
        tree_case(
            BoundaryExpectation::ReservedVocabulary,
            Tree {
                nodes: [(0, reserved)].into(),
                arcs: Vec::new(),
                fans: Vec::new(),
                root: 0,
            },
        ),
        tree_case(
            BoundaryExpectation::MissingRoot,
            Tree {
                nodes: nodes(),
                arcs: vec![(0, 1, edge.clone())],
                fans: Vec::new(),
                root: 9,
            },
        ),
    ]
}

/// The settled invalid/native boundary carrier. Every case has a distinct
/// expected finding or shape-refusal family.
#[must_use]
pub fn boundary_cases() -> Vec<BoundaryCase> {
    let edge = SEdge::new("v", "v", "prop", Some("e"));
    let mut schema = Schema::default()
        .vertex("v", "object")
        .edge("v", "v", "prop", Some("e"));
    schema.orderings.insert(edge.clone(), 0);
    let mut cases = graph_boundary_cases(&schema, &edge);
    cases.push(BoundaryCase {
        expectation: BoundaryExpectation::MalformedTable,
        schema: schema.clone(),
        instance: BoundaryInstance::Table(Table {
            rows: [(0, ("v".to_owned(), BTreeMap::new()))].into(),
            pairs: [(edge.clone(), 0, 9)].into(),
        }),
    });
    cases.extend(tree_boundary_cases(&schema, &edge));
    cases
}

/// A valid native table occurrence whose relation has a stable `Pair`
/// identity in the shared graph presentation.
#[must_use]
pub fn valid_table_pair_case() -> ShapeCase<Table> {
    let relation = SEdge::new("a", "b", "rel", Some("r"));
    ShapeCase {
        schema: Schema::default()
            .vertex("a", "object")
            .vertex("b", "object")
            .edge("a", "b", "rel", Some("r")),
        instance: Table {
            rows: [
                (0, ("a".to_owned(), BTreeMap::new())),
                (1, ("b".to_owned(), BTreeMap::new())),
            ]
            .into(),
            pairs: [(relation, 0, 1)].into(),
        },
    }
}

/// A valid native tree fan occurrence whose node, parent incidence and child
/// incidence all have stable native identities.
#[must_use]
pub fn valid_tree_fan_case() -> ShapeCase<Tree> {
    let mut schema = Schema::default()
        .vertex("parent", "object")
        .vertex("child", "object");
    schema.hyper_edges.insert(
        "h".to_owned(),
        crate::schema::HyperEdge {
            kind: "fan".to_owned(),
            signature: BTreeMap::from([
                ("parent".to_owned(), "parent".to_owned()),
                ("child".to_owned(), "child".to_owned()),
            ]),
            parent_label: "parent".to_owned(),
        },
    );
    ShapeCase {
        schema,
        instance: Tree {
            nodes: [(0, Node::new(0, "parent")), (1, Node::new(1, "child"))].into(),
            arcs: Vec::new(),
            fans: vec![Fan {
                parent: 0,
                hyper_edge: "h".to_owned(),
                children: BTreeMap::from([("child".to_owned(), 1)]),
            }],
            root: 0,
        },
    }
}

/// One schema-labelled instance occurrence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShapeCase<T> {
    /// The schema against which the instance is validated.
    pub schema: Schema,
    /// Native instance.
    pub instance: T,
}

fn words<T: Clone>(alphabet: &[T], length: usize) -> Vec<Vec<T>> {
    let mut level = vec![Vec::new()];
    for _ in 0..length {
        level = level
            .into_iter()
            .flat_map(|prefix| {
                alphabet.iter().map(move |item| {
                    let mut word = prefix.clone();
                    word.push(item.clone());
                    word
                })
            })
            .collect();
    }
    level
}

fn graph_cases_for_schema(schema: &Schema, visit: &mut impl FnMut(ShapeCase<Graph>)) {
    let vertices: Vec<_> = schema.vertices.keys().cloned().collect();
    let values = [None, Some(Value::Int(0)), Some(Value::Int(1))];
    let node_alphabet: Vec<_> = vertices
        .iter()
        .flat_map(|vertex| {
            values
                .iter()
                .map(move |value| (vertex.clone(), value.clone()))
        })
        .collect();
    for node_count in 0..=MAX_ELEMENTS {
        for node_word in words(&node_alphabet, node_count) {
            let nodes: BTreeMap<_, _> = node_word
                .into_iter()
                .enumerate()
                .map(|(index, (anchor, value))| {
                    let node_id = id(index);
                    let mut node = Node::new(node_id, &anchor);
                    if let Some(value) = value {
                        node = node.with_value(value);
                    }
                    (node_id, node)
                })
                .collect();
            let edge_alphabet: Vec<_> = schema
                .edges
                .iter()
                .flat_map(|label| {
                    (0..node_count).flat_map(move |source| {
                        (0..node_count).map(move |target| (label.clone(), source, target))
                    })
                })
                .collect();
            for edge_count in 0..=MAX_ELEMENTS - node_count {
                for edge_word in words(&edge_alphabet, edge_count) {
                    let edges = edge_word
                        .into_iter()
                        .enumerate()
                        .map(|(index, (label, source, target))| {
                            (
                                id(index),
                                GEdge {
                                    src: id(source),
                                    tgt: id(target),
                                    label,
                                    position: None,
                                },
                            )
                        })
                        .collect();
                    visit(ShapeCase {
                        schema: schema.clone(),
                        instance: Graph {
                            nodes: nodes.clone(),
                            edges,
                        },
                    });
                }
            }
        }
    }
}

/// Stream every graph occurrence. For a schema with `v` vertices and `e`
/// edges the summand is `sum_(n+m<=3) (3v)^n (n^2 e)^m`.
pub fn for_each_graph_case(mut visit: impl FnMut(ShapeCase<Graph>)) {
    for schema in schemas() {
        graph_cases_for_schema(&schema, &mut visit);
    }
}

/// Closed count for [`for_each_graph_case`], independent of its loops.
#[must_use]
pub fn graph_cases_count() -> u64 {
    schemas()
        .iter()
        .map(|schema| {
            let vertices = count(schema.vertices.len());
            let labels = count(schema.edges.len());
            (0..=3u32)
                .map(|nodes| {
                    (0..=(3u32 - nodes))
                        .map(|edges| {
                            (3 * vertices).pow(nodes)
                                * (u64::from(nodes).pow(2) * labels).pow(edges)
                        })
                        .sum::<u64>()
                })
                .sum::<u64>()
        })
        .sum()
}

/// Compatibility collector for the settled graph scope.
#[must_use]
pub fn graphs() -> Vec<Graph> {
    let mut out = Vec::with_capacity(GRAPH_CASE_CAPACITY);
    for_each_graph_case(|case| out.push(case.instance));
    out
}

fn attribute_table_cases(out: &mut Vec<ShapeCase<Table>>) {
    let schema = Schema::default()
        .vertex("t", "object")
        .vertex("v", "scalar")
        .edge("t", "v", "prop", Some("x"));
    let values = [None, Some(Value::Int(0)), Some(Value::Int(1))];
    for rows in 0..=MAX_ELEMENTS {
        for assignment in words(&values, rows) {
            let instance_rows = assignment
                .into_iter()
                .enumerate()
                .map(|(index, value)| {
                    let mut cells = BTreeMap::new();
                    if let Some(value) = value {
                        cells.insert("x".to_owned(), value);
                    }
                    (id(index), ("t".to_owned(), cells))
                })
                .collect();
            out.push(ShapeCase {
                schema: schema.clone(),
                instance: Table {
                    rows: instance_rows,
                    pairs: BTreeSet::new(),
                },
            });
        }
    }
}

fn relation_table_cases(out: &mut Vec<ShapeCase<Table>>) {
    let relation = SEdge::new("t", "t", "rel", Some("r"));
    let schema = Schema::default()
        .vertex("t", "object")
        .edge("t", "t", "rel", Some("r"));
    for rows in 0..=MAX_ELEMENTS {
        let candidates: Vec<_> = (0..rows)
            .flat_map(|source| {
                let relation = relation.clone();
                (0..rows).map(move |target| (relation.clone(), id(source), id(target)))
            })
            .collect();
        for pairs in super::subsets(&candidates, candidates.len()) {
            let instance_rows = (0..rows)
                .map(|index| (id(index), ("t".to_owned(), BTreeMap::new())))
                .collect();
            out.push(ShapeCase {
                schema: schema.clone(),
                instance: Table {
                    rows: instance_rows,
                    pairs: pairs.into_iter().collect(),
                },
            });
        }
    }
}

/// Every settled native table occurrence: one optional attribute column or
/// one self-relation, up to three rows.
#[must_use]
pub fn table_cases() -> Vec<ShapeCase<Table>> {
    let mut out = Vec::with_capacity(TABLE_CASE_CAPACITY);
    attribute_table_cases(&mut out);
    relation_table_cases(&mut out);
    out
}

/// Closed count for [`table_cases`]:
/// `sum_(r=0)^3 3^r + sum_(r=0)^3 2^(r^2) = 40 + 531 = 571`.
#[must_use]
pub fn table_cases_count() -> u64 {
    (0..=3u32).map(|rows| 3u64.pow(rows)).sum::<u64>()
        + (0..=3u32).map(|rows| 2u64.pow(rows * rows)).sum::<u64>()
}

/// Stream every settled rooted ordered tree occurrence. For a schema with
/// `v` vertices the one-, two-, and three-node summands are respectively
/// `3v`, `18v^2`, and `648v^3`.
pub fn for_each_tree_case(mut visit: impl FnMut(ShapeCase<Tree>)) {
    for schema in schemas() {
        let anchors: Vec<_> = schema.vertices.keys().map(String::as_str).collect();
        if anchors.is_empty() {
            continue;
        }
        for nodes in 1..=MAX_ELEMENTS {
            for instance in super::trees(&super::TreeScope {
                nodes,
                anchors: &anchors,
                values: &[Value::Int(0), Value::Int(1)],
                arc_names: &["e"],
                hyper_edges: &[],
                max_fans: 0,
            }) {
                visit(ShapeCase {
                    schema: schema.clone(),
                    instance,
                });
            }
        }
    }
}

/// Closed count for [`for_each_tree_case`], independent of its loops.
#[must_use]
pub fn tree_cases_count() -> u64 {
    schemas()
        .iter()
        .map(|schema| {
            let vertices = count(schema.vertices.len());
            3 * vertices + 18 * vertices.pow(2) + 648 * vertices.pow(3)
        })
        .sum()
}

/// Every validation-outcome word of length at most three.
#[must_use]
pub fn sequences<T: Clone>(alphabet: &[T]) -> Vec<Vec<T>> {
    super::sequences(alphabet, MAX_ELEMENTS)
}
