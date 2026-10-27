#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Bounded production/reference differential checks for the representation
//! laws. The enumerated carriers are labelled rather than quotiented by
//! isomorphism: eight graphs, two tables and two trees.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_inst::identity::{IDENTITY_ENVELOPE_VERSION, SEGMENT_MANIFEST_VERSION};
use panproto_inst::segment::{ManifestRole, SegmentManifest};
use panproto_inst::{
    ComplementShape, Digest, EdgeId, FInstance, Fan, FanDomain, FanViolation, GEdge, GInstance,
    IdentityEnvelope, InstError, Node, NodeShape, NotTableReason, NotTreeReason, RowId,
    TableEmbedding, TreeEmbedding, Value, ViewRef, WInstance, canonical_finstance_bytes,
    canonical_ginstance_bytes,
};
use panproto_reference::canonical::{
    ComplementShape as RefComplementShape, IdentityEnvelope as RefIdentityEnvelope, decode_graph,
    decode_table, decode_tree, identity,
};
use panproto_reference::encode::{
    NotATree as RefNotATree, TableEncodeError, TreeEncodeError, graph_to_table, graph_to_tree,
    reify, schema_parent_name, table_to_graph, tree_to_graph,
};
use panproto_reference::fan::{FanViolation as RefFanViolation, violations_at};
use panproto_reference::model::{
    Fan as RefFan, GEdge as RefGEdge, Graph as RefGraph, Node as RefNode,
    NodeShape as RefNodeShape, SEdge as RefEdge, Table as RefTable, Tree as RefTree,
};
use panproto_reference::schema::{
    Cardinality as RefCardinality, HyperEdge as RefHyperEdge, NullMatch as RefNullMatch,
    OnUnresolved as RefOnUnresolved, RefSource as ReferenceSource, Reference as RefReference,
    ResolveMode as RefResolveMode, Schema as RefSchema,
};
use panproto_reference::value::Value as RefValue;
use panproto_schema::{
    Cardinality, Edge, InstanceShapeKind, NullMatch, OnUnresolved, Protocol, RefSource, Reference,
    ResolveMode, Schema, SchemaBuilder,
};

type ProductionTableRows = BTreeMap<String, Vec<(RowId, HashMap<String, Value>)>>;

fn prod_edge(edge: &RefEdge) -> Edge {
    Edge {
        src: edge.src.as_str().into(),
        tgt: edge.tgt.as_str().into(),
        kind: edge.kind.as_str().into(),
        name: edge.name.as_deref().map(Into::into),
    }
}

fn prod_shape(shape: &RefNodeShape) -> NodeShape {
    match shape {
        RefNodeShape::Plain => NodeShape::Plain,
        RefNodeShape::List => NodeShape::List,
        RefNodeShape::XmlElement(tag) => NodeShape::XmlElement {
            tag: tag.as_str().into(),
        },
        RefNodeShape::XmlTextSegment => NodeShape::XmlTextSegment,
        RefNodeShape::HyperEdge => NodeShape::HyperEdge,
    }
}

fn prod_value(value: &RefValue) -> Value {
    match value {
        RefValue::Int(value) => Value::Int(*value),
        RefValue::List(values) => Value::List(values.iter().map(prod_value).collect()),
        other => panic!("value outside the representation carrier: {other:?}"),
    }
}

fn prod_node(node: &RefNode) -> Node {
    assert!(node.value.is_none(), "the named carrier has no node values");
    let mut out = Node::new(node.id, node.anchor.as_str()).with_shape(prod_shape(&node.shape));
    out.discriminator = node.discriminator.as_deref().map(Into::into);
    out.extra_fields = node
        .fields
        .iter()
        .map(|(key, value)| (key.clone(), prod_value(value)))
        .collect();
    out.position = node.position;
    out.annotations = node
        .annotations
        .iter()
        .map(|(key, value)| (key.clone(), prod_value(value)))
        .collect();
    out
}

fn prod_graph(graph: &RefGraph) -> GInstance {
    let mut out = GInstance::new();
    for node in graph.nodes.values() {
        out.insert_node(prod_node(node))
            .expect("reference nodes have distinct ids");
    }
    for (id, edge) in &graph.edges {
        out.insert_edge(
            EdgeId(*id),
            GEdge::new(edge.src, edge.tgt, prod_edge(&edge.label)).with_position(edge.position),
        )
        .expect("reference graph endpoints are present");
    }
    out
}

fn prod_tree(tree: &RefTree) -> WInstance {
    WInstance::new(
        tree.nodes
            .iter()
            .map(|(id, node)| (*id, prod_node(node)))
            .collect(),
        tree.arcs
            .iter()
            .map(|(parent, child, edge)| (*parent, *child, prod_edge(edge)))
            .collect(),
        tree.fans
            .iter()
            .map(|fan| Fan {
                hyper_edge_id: fan.hyper_edge.clone(),
                parent: fan.parent,
                children: fan.children.clone().into_iter().collect(),
            })
            .collect(),
        tree.root,
        tree.nodes[&tree.root].anchor.as_str().into(),
    )
}

fn prod_table(table: &RefTable) -> Result<FInstance, InstError> {
    let mut grouped = ProductionTableRows::new();
    for (id, (name, cells)) in &table.rows {
        grouped.entry(name.clone()).or_default().push((
            RowId(*id),
            cells
                .iter()
                .map(|(key, value)| (key.clone(), prod_value(value)))
                .collect(),
        ));
    }
    let pairs: BTreeMap<RefEdge, Vec<(RowId, RowId)>> =
        table
            .pairs
            .iter()
            .fold(BTreeMap::new(), |mut found, (edge, src, tgt)| {
                found
                    .entry(edge.clone())
                    .or_default()
                    .push((RowId(*src), RowId(*tgt)));
                found
            });
    FInstance::from_parts(
        grouped.into_iter().map(|(name, rows)| (name.into(), rows)),
        pairs
            .into_iter()
            .map(|(edge, pairs)| (prod_edge(&edge), pairs)),
    )
}

fn prod_schema(schema: &RefSchema) -> Schema {
    let protocol = Protocol::default();
    let mut builder = SchemaBuilder::new(&protocol);
    for (vertex, kind) in &schema.vertices {
        builder = builder
            .vertex(vertex, kind, None)
            .expect("reference vertex builds");
    }
    for edge in &schema.edges {
        builder = builder
            .edge(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())
            .expect("reference edge builds");
    }
    for (id, hyper) in &schema.hyper_edges {
        builder = builder
            .hyper_edge(
                id,
                &hyper.kind,
                hyper
                    .signature
                    .iter()
                    .map(|(label, vertex)| (label.clone(), vertex.clone()))
                    .collect(),
                &hyper.parent_label,
            )
            .expect("reference hyper-edge builds");
    }
    builder.build().expect("reference schema builds")
}

fn graph_schema() -> Schema {
    let protocol = Protocol::default();
    SchemaBuilder::new(&protocol)
        .vertex("a", "record", None)
        .expect("vertex")
        .edge("a", "a", "e", None)
        .expect("edge")
        .build()
        .expect("schema")
}

fn ref_graph_schema() -> RefSchema {
    RefSchema::default()
        .vertex("a", "record")
        .edge("a", "a", "e", None)
}

fn fan_schema() -> RefSchema {
    RefSchema {
        vertices: BTreeMap::from([("a".into(), "record".into()), ("b".into(), "record".into())]),
        hyper_edges: BTreeMap::from([(
            "h".into(),
            RefHyperEdge {
                kind: "fan".into(),
                signature: BTreeMap::from([("l".into(), "b".into()), ("p".into(), "a".into())]),
                parent_label: "p".into(),
            },
        )]),
        ..RefSchema::default()
    }
}

fn fan_tree() -> RefTree {
    RefTree {
        nodes: BTreeMap::from([(0, RefNode::new(0, "a")), (1, RefNode::new(1, "b"))]),
        arcs: vec![(0, 1, RefEdge::new("a", "b", "prop", Some("child")))],
        fans: vec![RefFan {
            parent: 0,
            hyper_edge: "h".into(),
            children: BTreeMap::from([("l".into(), 1)]),
        }],
        root: 0,
    }
}

fn reserved_tree() -> RefTree {
    let mut node = RefNode::new(0, "h");
    node.shape = RefNodeShape::HyperEdge;
    RefTree {
        nodes: BTreeMap::from([(0, node)]),
        arcs: Vec::new(),
        fans: Vec::new(),
        root: 0,
    }
}

fn assert_graph_bytes(graph: &GInstance, expected: &RefGraph) {
    assert_eq!(
        decode_graph(&canonical_ginstance_bytes(graph)),
        Some(expected.clone())
    );
}

fn assert_tree_embedding(
    actual: &TreeEmbedding,
    expected: &panproto_reference::encode::TreeEmbedding,
) {
    assert_eq!(
        actual
            .arcs
            .iter()
            .map(|(child, edge)| (*child, edge.0))
            .collect::<BTreeMap<_, _>>(),
        expected.arcs
    );
    let fans = actual
        .fans
        .iter()
        .map(|fan| {
            (
                fan.node,
                fan.parent_edge.0,
                fan.child_edges
                    .iter()
                    .map(|(label, edge)| (label.clone(), edge.0))
                    .collect(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(fans, expected.fans);
    assert_eq!(actual.next_node, expected.next_node);
    assert_eq!(actual.next_edge, expected.next_edge);
}

fn assert_table_embedding(
    actual: &TableEmbedding,
    expected: &panproto_reference::encode::TableEmbedding,
) {
    assert_eq!(
        actual
            .pairs
            .iter()
            .map(|((edge, src, tgt), id)| {
                (
                    (
                        RefEdge::new(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref()),
                        src.0,
                        tgt.0,
                    ),
                    id.0,
                )
            })
            .collect::<BTreeMap<_, _>>(),
        *expected
    );
    assert_eq!(
        actual.next_edge,
        u32::try_from(expected.len()).expect("bounded table embedding")
    );
}

#[test]
fn tree_to_graph_matches_production_on_the_exact_carrier_and_refusals() {
    let schema = fan_schema();
    let prod_schema = prod_schema(&schema);
    let mut trees = panproto_reference::enumerate::representation::trees();
    assert_eq!(trees.len(), 2);
    trees.push(fan_tree());

    for tree in &trees {
        let use_schema = (!tree.fans.is_empty()).then_some(&schema);
        let use_prod_schema = (!tree.fans.is_empty()).then_some(&prod_schema);
        let (expected_graph, expected_embedding) =
            tree_to_graph(tree, use_schema).expect("carrier tree has a graph form");
        let (actual_graph, actual_embedding) =
            GInstance::from_wtype_embedded(&prod_tree(tree), use_prod_schema)
                .expect("production carrier tree has a graph form");
        assert_graph_bytes(&actual_graph, &expected_graph);
        assert_tree_embedding(&actual_embedding, &expected_embedding);
    }

    let saturated = RefTree {
        nodes: BTreeMap::from([(u32::MAX, RefNode::new(u32::MAX, "a"))]),
        arcs: Vec::new(),
        fans: Vec::new(),
        root: u32::MAX,
    };
    let (expected, expected_embedding) =
        tree_to_graph(&saturated, None).expect("fan-free saturated floor does not allocate");
    let (actual, actual_embedding) = GInstance::from_wtype_embedded(&prod_tree(&saturated), None)
        .expect("production fan-free saturated floor does not allocate");
    assert_graph_bytes(&actual, &expected);
    assert_tree_embedding(&actual_embedding, &expected_embedding);

    let exhausting = RefTree {
        fans: vec![RefFan {
            parent: u32::MAX,
            hyper_edge: "h".into(),
            children: BTreeMap::new(),
        }],
        ..saturated
    };
    assert_eq!(
        tree_to_graph(&exhausting, None),
        Err(TreeEncodeError::NodeIdsExhausted)
    );
    assert!(matches!(
        GInstance::from_wtype_embedded(&prod_tree(&exhausting), None),
        Err(InstError::NodeIdsExhausted)
    ));

    let reserved = reserved_tree();
    assert_eq!(
        tree_to_graph(&reserved, None),
        Err(TreeEncodeError::ReservedVocabulary)
    );
    assert!(matches!(
        GInstance::from_wtype_embedded(&prod_tree(&reserved), None),
        Err(InstError::ReservedVocabulary {
            at: ViewRef::Node(0)
        })
    ));

    let reserved_before_dangling = RefTree {
        nodes: reserved.nodes,
        arcs: vec![(0, 1, RefEdge::new("h", "a", "e", None))],
        fans: Vec::new(),
        root: 0,
    };
    assert_eq!(
        tree_to_graph(&reserved_before_dangling, None),
        Err(TreeEncodeError::ReservedVocabulary)
    );
    assert!(matches!(
        GInstance::from_wtype_embedded(&prod_tree(&reserved_before_dangling), None),
        Err(InstError::ReservedVocabulary {
            at: ViewRef::Node(0)
        })
    ));

    let second_parent = RefTree {
        nodes: (0..3).map(|id| (id, RefNode::new(id, "a"))).collect(),
        arcs: vec![
            (0, 2, RefEdge::new("a", "a", "e", None)),
            (1, 2, RefEdge::new("a", "a", "e", None)),
        ],
        fans: Vec::new(),
        root: 0,
    };
    assert_eq!(
        tree_to_graph(&second_parent, None),
        Err(TreeEncodeError::SecondParent { node: 2 })
    );
    assert!(matches!(
        GInstance::from_wtype_embedded(&prod_tree(&second_parent), None),
        Err(InstError::NotATree {
            reason: NotTreeReason::MultipleParents { node: 2, edges }
        }) if edges == vec![EdgeId(0), EdgeId(1)]
    ));
}

#[test]
fn graph_to_tree_decision_matches_production_on_the_exact_carrier() {
    let graphs = panproto_reference::enumerate::representation::graphs();
    assert_eq!(graphs.len(), 8);
    for graph in &graphs {
        for root in 0..3 {
            let expected = graph_to_tree(graph, root, &schema_parent_name(None));
            let actual = prod_graph(graph).to_wtype(root);
            assert_eq!(
                actual.is_ok(),
                expected.is_ok(),
                "graph {graph:?}, root {root}"
            );
            if let (Ok(actual), Ok(expected)) = (actual, expected) {
                assert_eq!(
                    decode_tree(&panproto_inst::segment::canonical_winstance_bytes(&actual)),
                    Some(expected)
                );
            }
        }
    }

    let empty = RefGraph::default();
    assert_eq!(
        graph_to_tree(&empty, 0, &schema_parent_name(None)),
        Err(RefNotATree::MissingRoot)
    );
    assert!(matches!(
        prod_graph(&empty).to_wtype(0),
        Err(InstError::MissingRoot)
    ));

    let disconnected = RefGraph {
        nodes: BTreeMap::from([(0, RefNode::new(0, "a")), (1, RefNode::new(1, "a"))]),
        edges: BTreeMap::new(),
    };
    assert_eq!(
        graph_to_tree(&disconnected, 0, &schema_parent_name(None)),
        Err(RefNotATree::Unreached { node: 1 })
    );
    assert!(matches!(
        prod_graph(&disconnected).to_wtype(0),
        Err(InstError::NotATree {
            reason: NotTreeReason::NoRoot
        })
    ));

    let mut fan = RefNode::new(1, "h");
    fan.shape = RefNodeShape::HyperEdge;
    let schema_free_named_parent = RefGraph {
        nodes: BTreeMap::from([(0, RefNode::new(0, "a")), (1, fan)]),
        edges: BTreeMap::from([(
            0,
            RefGEdge {
                src: 1,
                tgt: 0,
                label: RefEdge::new("h", "a", "hyper_parent", Some("p")),
                position: None,
            },
        )]),
    };
    let expected = graph_to_tree(&schema_free_named_parent, 0, &schema_parent_name(None))
        .expect("without a schema, the parent-edge name is unconstrained");
    let actual = prod_graph(&schema_free_named_parent)
        .to_wtype(0)
        .expect("production accepts a named schema-free fan parent");
    assert_eq!(
        decode_tree(&panproto_inst::segment::canonical_winstance_bytes(&actual)),
        Some(expected)
    );

    let positioned = RefGraph {
        nodes: disconnected.nodes,
        edges: BTreeMap::from([(
            4,
            RefGEdge {
                src: 0,
                tgt: 1,
                label: RefEdge::new("a", "a", "e", None),
                position: Some(0),
            },
        )]),
    };
    assert_eq!(
        graph_to_tree(&positioned, 0, &schema_parent_name(None)),
        Err(RefNotATree::PositionedEdge { edge: 4 })
    );
    assert!(matches!(
        prod_graph(&positioned).to_wtype(0),
        Err(InstError::NotATree {
            reason: NotTreeReason::PositionedEdge { edge: EdgeId(4) }
        })
    ));
}

#[test]
fn reification_round_trip_matches_production_on_the_exact_image() {
    let schema = fan_schema();
    let prod_schema = prod_schema(&schema);
    let mut trees = panproto_reference::enumerate::representation::trees();
    assert_eq!(trees.len(), 2);
    trees.push(fan_tree());
    for tree in &trees {
        let with_schema = (!tree.fans.is_empty()).then_some(&schema);
        let with_prod_schema = (!tree.fans.is_empty()).then_some(&prod_schema);
        let expected = reify(tree, with_schema).expect("carrier tree is in the reification image");
        let (graph, _) = GInstance::from_wtype_embedded(&prod_tree(tree), with_prod_schema)
            .expect("production reification");
        let actual = graph
            .to_wtype(tree.root)
            .expect("production image reads back");
        assert_eq!(
            decode_tree(&panproto_inst::segment::canonical_winstance_bytes(&actual)),
            Some(expected)
        );
    }

    let reserved = reserved_tree();
    assert_eq!(reify(&reserved, None), None);
    assert!(matches!(
        GInstance::from_wtype_embedded(&prod_tree(&reserved), None),
        Err(InstError::ReservedVocabulary {
            at: ViewRef::Node(0)
        })
    ));
}

#[test]
fn canonical_graph_table_and_tree_bytes_decode_to_the_same_values() {
    let graphs = panproto_reference::enumerate::representation::graphs();
    let tables = panproto_reference::enumerate::representation::tables();
    let trees = panproto_reference::enumerate::representation::trees();
    assert_eq!((graphs.len(), tables.len(), trees.len()), (8, 2, 2));

    for graph in &graphs {
        let bytes = canonical_ginstance_bytes(&prod_graph(graph));
        assert_eq!(decode_graph(&bytes), Some(graph.clone()));
    }
    for table in &tables {
        let bytes = canonical_finstance_bytes(&prod_table(table).expect("well-formed table"));
        assert_eq!(decode_table(&bytes), Some(table.clone()));
    }
    for tree in &trees {
        let bytes = panproto_inst::segment::canonical_winstance_bytes(&prod_tree(tree));
        assert_eq!(decode_tree(&bytes), Some(tree.clone()));
    }

    let malformed_left = RefTree {
        nodes: BTreeMap::from([(0, RefNode::new(0, "a"))]),
        arcs: Vec::new(),
        fans: Vec::new(),
        root: 0,
    };
    let malformed_right = RefTree {
        nodes: BTreeMap::from([(1, RefNode::new(0, "a"))]),
        arcs: Vec::new(),
        fans: Vec::new(),
        root: 1,
    };
    assert_eq!(
        panproto_inst::segment::canonical_winstance_bytes(&prod_tree(&malformed_left)),
        panproto_inst::segment::canonical_winstance_bytes(&prod_tree(&malformed_right)),
        "map-key/root collisions are outside the validated-tree byte theorem"
    );

    let mut graph_bytes = canonical_ginstance_bytes(&prod_graph(&graphs[0]));
    graph_bytes.push(0);
    assert_eq!(decode_graph(&graph_bytes), None);
    let mut table_bytes = canonical_finstance_bytes(&prod_table(&tables[0]).expect("table"));
    table_bytes.push(0);
    assert_eq!(decode_table(&table_bytes), None);
    let mut tree_bytes = panproto_inst::segment::canonical_winstance_bytes(&prod_tree(&trees[0]));
    tree_bytes.push(0);
    assert_eq!(decode_tree(&tree_bytes), None);
}

const fn ref_complement_shape(shape: ComplementShape) -> RefComplementShape {
    match shape {
        ComplementShape::WType => RefComplementShape::WType,
        ComplementShape::Graph => RefComplementShape::Graph,
        ComplementShape::Functor => RefComplementShape::Functor,
        ComplementShape::Resolution => RefComplementShape::Resolution,
    }
}

#[test]
fn identity_envelope_layout_matches_the_production_manifest_writer() {
    for (index, shape) in ComplementShape::ALL.into_iter().enumerate() {
        let index = u8::try_from(index).expect("four complement shapes");
        let envelope = IdentityEnvelope {
            version: IDENTITY_ENVELOPE_VERSION,
            source_schema: Digest([index + 1; 32]),
            target_schema: Digest([index + 5; 32]),
            lens: Digest([index + 9; 32]),
            complement_format: shape.current_format(),
        };
        let manifest = SegmentManifest {
            version: SEGMENT_MANIFEST_VERSION,
            shape: InstanceShapeKind::Graph,
            schema: Digest([0; 32]),
            role: ManifestRole::View {
                source: Digest([13; 32]),
                envelope: envelope.clone(),
            },
            binding: None,
            segments: Vec::new(),
            root: Digest([0; 32]),
            instance_digest: None,
        };
        let bytes = manifest.canonical_bytes();
        let actual: [u8; 101] = bytes[68..169].try_into().expect("identity envelope span");
        let expected = identity(&RefIdentityEnvelope {
            source_schema: envelope.source_schema.0,
            target_schema: envelope.target_schema.0,
            lens: envelope.lens.0,
            shape: ref_complement_shape(shape),
            format_version: envelope.complement_format.version,
        });
        assert_eq!(actual, expected);
    }
}

fn relational_table() -> RefTable {
    RefTable {
        rows: BTreeMap::from([
            (
                0,
                ("t".into(), BTreeMap::from([("c".into(), RefValue::Int(0))])),
            ),
            (1, ("t".into(), BTreeMap::new())),
        ]),
        pairs: BTreeSet::from([(RefEdge::new("t", "t", "e", None), 0, 1)]),
    }
}

fn declared_table_schema(nullable: bool) -> (Schema, Edge) {
    let protocol = Protocol::default();
    let edge = Edge {
        src: "post".into(),
        tgt: "user".into(),
        kind: "fk".into(),
        name: Some("author".into()),
    };
    let mut schema = SchemaBuilder::new(&protocol)
        .vertex("post", "record", None)
        .and_then(|builder| builder.vertex("user", "record", None))
        .and_then(|builder| builder.edge("post", "user", "fk", Some("author")))
        .and_then(SchemaBuilder::build)
        .expect("declared table schema");
    schema.declarations.references.insert(
        "author".into(),
        Reference {
            id: "author".into(),
            holder: "post".into(),
            source: RefSource::KeyValue { parts: Vec::new() },
            target_vertex: "user".into(),
            target_key: None,
            edge: Some(edge.clone()),
            cardinality: Cardinality::One,
            nullable,
            on_unresolved: OnUnresolved::Error,
            mode: ResolveMode::AddEdge,
            guard: None,
            null_match: NullMatch::default(),
            null_vertex: None,
            scope_via: None,
        },
    );
    (schema, edge)
}

fn ref_declared_table_schema(nullable: bool) -> (RefSchema, RefEdge) {
    let edge = RefEdge::new("post", "user", "fk", Some("author"));
    let mut schema = RefSchema::default()
        .vertex("post", "record")
        .vertex("user", "record")
        .edge("post", "user", "fk", Some("author"));
    schema.references.insert(
        "author".into(),
        RefReference {
            id: "author".into(),
            holder: "post".into(),
            source: ReferenceSource::KeyValue { parts: Vec::new() },
            target_vertex: "user".into(),
            target_key: None,
            edge: Some(edge.clone()),
            cardinality: RefCardinality::One,
            nullable,
            on_unresolved: RefOnUnresolved::Error,
            mode: RefResolveMode::AddEdge,
            guard: None,
            null_match: RefNullMatch::Simple,
            null_vertex: None,
            scope_via: None,
        },
    );
    (schema, edge)
}

#[test]
fn table_to_graph_matches_production_on_the_exact_carrier_and_refusals() {
    let mut tables = panproto_reference::enumerate::representation::tables();
    assert_eq!(tables.len(), 2);
    tables.push(relational_table());
    for table in &tables {
        let (expected_graph, expected_embedding) =
            table_to_graph(table).expect("carrier table has a graph form");
        let (actual_graph, actual_embedding) =
            GInstance::from_finstance(&prod_table(table).expect("well-formed carrier table"))
                .expect("production table has a graph form");
        assert_graph_bytes(&actual_graph, &expected_graph);
        assert_table_embedding(&actual_embedding, &expected_embedding);
    }

    let pair = (RefEdge::new("t", "t", "e", None), 0, 1);
    let malformed = RefTable {
        rows: BTreeMap::from([(0, ("t".into(), BTreeMap::new()))]),
        pairs: BTreeSet::from([pair.clone()]),
    };
    assert_eq!(
        table_to_graph(&malformed),
        Err(TableEncodeError::MalformedTable {
            pair: Box::new(pair.clone())
        })
    );
    assert!(matches!(
        prod_table(&malformed),
        Err(InstError::DanglingForeignKey { edge, row: RowId(1) })
            if edge == prod_edge(&pair.0)
    ));
    // `prod_table` is the raw-reference adapter. Its refusal occurs before
    // `GInstance::from_finstance` can be called, rather than in that
    // converter.

    let table = prod_table(&relational_table()).expect("well-formed table");
    let mut saturated = TableEmbedding {
        pairs: BTreeMap::new(),
        next_edge: u32::MAX,
    };
    assert!(matches!(
        GInstance::from_finstance_along(&table, &mut saturated),
        Err(InstError::EdgeIdsExhausted)
    ));
}

fn assert_edge_shape_refusals(schema: &Schema, reference_schema: &RefSchema) {
    let nodes = BTreeMap::from([(0, RefNode::new(0, "a")), (1, RefNode::new(1, "a"))]);
    let edge = RefEdge::new("a", "a", "e", None);
    let positioned = RefGraph {
        nodes: nodes.clone(),
        edges: BTreeMap::from([(
            4,
            RefGEdge {
                src: 0,
                tgt: 1,
                label: edge.clone(),
                position: Some(0),
            },
        )]),
    };
    assert_eq!(graph_to_table(&positioned, reference_schema), None);
    assert!(matches!(
        prod_graph(&positioned).try_to_finstance(schema),
        Err(InstError::NotATable {
            reason: NotTableReason::EdgePositioned { edge: EdgeId(4) }
        })
    ));

    let duplicate = RefGraph {
        nodes,
        edges: BTreeMap::from([
            (
                3,
                RefGEdge {
                    src: 0,
                    tgt: 1,
                    label: edge.clone(),
                    position: None,
                },
            ),
            (
                7,
                RefGEdge {
                    src: 0,
                    tgt: 1,
                    label: edge,
                    position: None,
                },
            ),
        ]),
    };
    assert_eq!(graph_to_table(&duplicate, reference_schema), None);
    assert!(matches!(
        prod_graph(&duplicate).try_to_finstance(schema),
        Err(InstError::NotATable {
            reason: NotTableReason::ParallelForeignKey { edges }
        }) if edges == vec![EdgeId(3), EdgeId(7)]
    ));
}

fn assert_cell_and_schema_refusals(schema: &Schema, reference_schema: &RefSchema) {
    let nested = RefTable {
        rows: BTreeMap::from([(
            0,
            (
                "t".into(),
                BTreeMap::from([("c".into(), RefValue::List(vec![RefValue::Int(0)]))]),
            ),
        )]),
        pairs: BTreeSet::new(),
    };
    let nested_tables = prod_table(&nested).expect("nested cells are valid table input");
    let (nested_graph, _) = GInstance::from_finstance(&nested_tables)
        .expect("the forward table embedding preserves nested cells");
    let (nested_ref_graph, _) = table_to_graph(&nested).expect("nested reference table embeds");
    let nested_ref_schema = RefSchema::default().vertex("t", "record");
    assert_eq!(graph_to_table(&nested_ref_graph, &nested_ref_schema), None);
    assert!(matches!(
        nested_graph.try_to_finstance(&Schema::default()),
        Err(InstError::NotATable {
            reason: NotTableReason::NestedCell { node: 0, field }
        }) if field == "c"
    ));

    let unknown = RefGraph {
        nodes: BTreeMap::from([(0, RefNode::new(0, "a")), (1, RefNode::new(1, "a"))]),
        edges: BTreeMap::from([(
            9,
            RefGEdge {
                src: 0,
                tgt: 1,
                label: RefEdge::new("a", "a", "unknown", None),
                position: None,
            },
        )]),
    };
    assert_eq!(graph_to_table(&unknown, reference_schema), None);
    assert!(matches!(
        prod_graph(&unknown).try_to_finstance(schema),
        Err(InstError::NotATable {
            reason: NotTableReason::EdgeNotForeignKey { edge: EdgeId(9) }
        })
    ));
}

fn assert_declared_lift_refusals() {
    let (functional_schema, author) = declared_table_schema(true);
    let (functional_ref_schema, _) = ref_declared_table_schema(true);
    let multiple = RefGraph {
        nodes: BTreeMap::from([
            (0, RefNode::new(0, "post")),
            (1, RefNode::new(1, "user")),
            (2, RefNode::new(2, "user")),
        ]),
        edges: BTreeMap::from([
            (
                3,
                RefGEdge {
                    src: 0,
                    tgt: 1,
                    label: RefEdge::new("post", "user", "fk", Some("author")),
                    position: None,
                },
            ),
            (
                7,
                RefGEdge {
                    src: 0,
                    tgt: 2,
                    label: RefEdge::new("post", "user", "fk", Some("author")),
                    position: None,
                },
            ),
        ]),
    };
    assert_eq!(graph_to_table(&multiple, &functional_ref_schema), None);
    assert!(matches!(
        prod_graph(&multiple).try_to_finstance(&functional_schema),
        Err(InstError::NotATable {
            reason: NotTableReason::MultipleLifts { node: 0, edge, edges }
        }) if edge == author && edges == vec![EdgeId(3), EdgeId(7)]
    ));

    let (required_schema, author) = declared_table_schema(false);
    let (required_ref_schema, _) = ref_declared_table_schema(false);
    let missing = RefGraph {
        nodes: BTreeMap::from([(0, RefNode::new(0, "post")), (1, RefNode::new(1, "user"))]),
        edges: BTreeMap::new(),
    };
    assert_eq!(graph_to_table(&missing, &required_ref_schema), None);
    assert!(matches!(
        prod_graph(&missing).try_to_finstance(&required_schema),
        Err(InstError::NotATable {
            reason: NotTableReason::MissingLift { node: 0, edge }
        }) if edge == author
    ));
}

#[test]
fn graph_to_table_decision_matches_production_on_the_exact_carrier() {
    let schema = graph_schema();
    let reference_schema = ref_graph_schema();
    let graphs = panproto_reference::enumerate::representation::graphs();
    assert_eq!(graphs.len(), 8);
    for graph in &graphs {
        let expected = graph_to_table(graph, &reference_schema);
        let actual = prod_graph(graph).try_to_finstance(&schema);
        assert_eq!(actual.is_ok(), expected.is_some(), "graph {graph:?}");
        if let (Ok((actual, _)), Some(expected)) = (actual, expected) {
            assert_eq!(
                decode_table(&canonical_finstance_bytes(&actual)),
                Some(expected)
            );
        }
    }

    assert_edge_shape_refusals(&schema, &reference_schema);
    assert_cell_and_schema_refusals(&schema, &reference_schema);
    assert_declared_lift_refusals();
}

fn ref_fan_violation(violation: &FanViolation) -> RefFanViolation {
    match violation {
        FanViolation::MissingParent => RefFanViolation::MissingParent,
        FanViolation::MultipleParents { edges } => RefFanViolation::MultipleParents {
            edges: edges.iter().map(|edge| edge.0).collect(),
        },
        FanViolation::DuplicateLabel { label, edges } => RefFanViolation::DuplicateLabel {
            label: label.to_string(),
            edges: edges.iter().map(|edge| edge.0).collect(),
        },
        FanViolation::UnknownLabel { label } => RefFanViolation::UnknownLabel {
            label: label.to_string(),
        },
        FanViolation::ReservedKindOutsideFan { edge } => {
            RefFanViolation::ReservedKindOutsideFan { edge: edge.0 }
        }
        FanViolation::ForeignIncidentEdge { edge } => {
            RefFanViolation::ForeignIncidentEdge { edge: edge.0 }
        }
        FanViolation::UnknownHyperEdge { hyper_edge } => RefFanViolation::UnknownHyperEdge {
            hyper_edge: hyper_edge.to_string(),
        },
        FanViolation::MissingChild { label } => {
            panic!("graph fan violations cannot contain MissingChild({label})")
        }
    }
}

fn complex_fan_graph() -> RefGraph {
    let mut fan = RefNode::new(0, "h");
    fan.shape = RefNodeShape::HyperEdge;
    RefGraph {
        nodes: BTreeMap::from([
            (0, fan),
            (1, RefNode::new(1, "a")),
            (2, RefNode::new(2, "b")),
            (3, RefNode::new(3, "a")),
        ]),
        edges: BTreeMap::from([
            (
                0,
                RefGEdge {
                    src: 1,
                    tgt: 0,
                    label: RefEdge::new("a", "h", "e", None),
                    position: None,
                },
            ),
            (
                1,
                RefGEdge {
                    src: 0,
                    tgt: 1,
                    label: RefEdge::new("h", "a", "e", None),
                    position: None,
                },
            ),
            (
                2,
                RefGEdge {
                    src: 0,
                    tgt: 1,
                    label: RefEdge::new("h", "a", "hyper_parent", Some("p")),
                    position: None,
                },
            ),
            (
                3,
                RefGEdge {
                    src: 0,
                    tgt: 3,
                    label: RefEdge::new("h", "a", "hyper_parent", Some("p")),
                    position: None,
                },
            ),
            (
                4,
                RefGEdge {
                    src: 0,
                    tgt: 2,
                    label: RefEdge::new("h", "b", "hyper_label", Some("x")),
                    position: None,
                },
            ),
            (
                5,
                RefGEdge {
                    src: 0,
                    tgt: 2,
                    label: RefEdge::new("h", "b", "hyper_label", Some("x")),
                    position: None,
                },
            ),
        ]),
    }
}

fn assert_fan_violations(graph: &RefGraph, node: u32, schema: Option<&RefSchema>) {
    let expected = violations_at(graph, node, schema);
    let prod_schema = schema.map(|schema| {
        if schema == &RefSchema::default() {
            Schema::default()
        } else {
            prod_schema(schema)
        }
    });
    let actual = FanDomain::violations_at(&prod_graph(graph), node, prod_schema.as_ref())
        .iter()
        .map(ref_fan_violation)
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

#[test]
fn fan_domain_lists_the_same_ordered_typed_violations() {
    assert_fan_violations(&RefGraph::default(), 7, None);

    let plain_reserved = RefGraph {
        nodes: BTreeMap::from([(0, RefNode::new(0, "a")), (1, RefNode::new(1, "a"))]),
        edges: BTreeMap::from([(
            0,
            RefGEdge {
                src: 0,
                tgt: 1,
                label: RefEdge::new("a", "a", "hyper_parent", None),
                position: None,
            },
        )]),
    };
    assert_fan_violations(&plain_reserved, 0, None);

    let mut lone = RefNode::new(0, "h");
    lone.shape = RefNodeShape::HyperEdge;
    let lone = RefGraph {
        nodes: BTreeMap::from([(0, lone)]),
        edges: BTreeMap::new(),
    };
    assert_fan_violations(&lone, 0, Some(&RefSchema::default()));

    let schema = fan_schema();
    let complex = complex_fan_graph();
    assert_fan_violations(&complex, 0, Some(&schema));
    assert_eq!(
        violations_at(&complex, 0, Some(&schema)),
        vec![
            RefFanViolation::ForeignIncidentEdge { edge: 0 },
            RefFanViolation::ForeignIncidentEdge { edge: 1 },
            RefFanViolation::MultipleParents { edges: vec![2, 3] },
            RefFanViolation::DuplicateLabel {
                label: "x".into(),
                edges: vec![4, 5]
            },
            RefFanViolation::UnknownLabel { label: "x".into() },
        ]
    );
}
