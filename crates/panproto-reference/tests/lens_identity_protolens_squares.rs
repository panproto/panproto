#![allow(clippy::unwrap_used)]
//! Identity, applicability, and nonvacuity witnesses for the O16 lens paths.

use std::collections::{BTreeMap, BTreeSet};

use panproto_reference::canonical::ComplementShape;
use panproto_reference::lens::{
    GraphLens, GraphShape, GraphSquareObservation, IdentityError, LawGateStatus, LensError,
    Mutation, Protolens, ProtolensError, SchemaPrecondition, SquareError, SquareResult,
    TreeOperationalError, TreeShape, ViewRef, check_graph_laws, check_locality_report,
    check_tree_laws, graph_put_square, graph_square, protolens_instance, protolens_schema,
    table_put_square, table_square,
};
use panproto_reference::migration::Migration;
use panproto_reference::model::{Fan, Graph, Node, NodeShape, SEdge, Table, Tree};
use panproto_reference::rel;
use panproto_reference::restrict::{self, NoValueLayer, SynthKey, compile};
use panproto_reference::schema::{HyperEdge, Schema};
use panproto_reference::table::{self, TableBoundaries, TableLens};
use panproto_reference::value::Value;

fn identity_migration(schema: &Schema) -> Migration {
    Migration {
        surviving_verts: schema.vertices.keys().cloned().collect(),
        surviving_edges: schema.edges.clone(),
        ..Migration::default()
    }
}

fn graph_lens<'a>(
    source: &'a Schema,
    target: &'a Schema,
    migration: &'a Migration,
    plan: &'a panproto_reference::restrict::Plan,
) -> GraphLens<'a> {
    GraphLens::new(
        source,
        target,
        migration,
        plan,
        &BTreeMap::new(),
        &NoValueLayer,
    )
    .unwrap()
}

#[test]
fn graph_put_checks_identity_but_the_domain_predicate_is_identity_free() {
    let source = Schema::default()
        .vertex("kept", "obj")
        .vertex("hidden", "obj");
    let target = Schema::default().vertex("kept", "obj");
    let migration = Migration {
        surviving_verts: BTreeSet::from(["kept".to_owned()]),
        ..Migration::default()
    };
    let plan = compile(&source, &target, &migration).unwrap();
    let lens = graph_lens(&source, &target, &migration, &plan);
    let graph = Graph {
        nodes: [(0, Node::new(0, "kept")), (1, Node::new(1, "hidden"))].into(),
        ..Graph::default()
    };
    let (view, complement) = panproto_reference::lens::get(&lens, &graph).unwrap();
    assert_eq!(lens.put(&view, &complement).unwrap(), graph);

    let (raw_view, raw_complement) =
        restrict::get_with_plan(&source, &target, &migration, &plan, &NoValueLayer, &graph)
            .unwrap();
    assert_eq!(lens.check_put_domain(&raw_view, &raw_complement), Ok(()));
    assert_eq!(
        lens.put(&raw_view, &raw_complement),
        Err(LensError::Identity(IdentityError::UnboundComplement))
    );

    let other_target = Schema::default().vertex("renamed", "obj");
    let other_migration = Migration {
        surviving_verts: BTreeSet::from(["renamed".to_owned()]),
        vertex_remap: [("kept".to_owned(), "renamed".to_owned())].into(),
        ..Migration::default()
    };
    let other_plan = compile(&source, &other_target, &other_migration).unwrap();
    let other = graph_lens(&source, &other_target, &other_migration, &other_plan);
    assert!(matches!(
        other.put(&view, &complement),
        Err(LensError::Identity(IdentityError::Mismatch { .. }))
    ));
}

#[test]
fn graph_identity_matches_the_external_derive_key_golden() {
    // Captured from blake3 1.8.7 before replacing that dependency. The lens
    // material is the two 32-byte endpoint digests followed by 520 variable
    // migration bytes, so this pins the three-update streaming boundary as
    // well as both independently derived endpoint identities.
    const SOURCE_DIGEST: [u8; 32] = [
        0x10, 0x4a, 0x57, 0xbd, 0xd0, 0x75, 0x6c, 0xf0, 0x36, 0xc5, 0x42, 0x3d, 0xd2, 0xf6, 0x8b,
        0x8d, 0x11, 0x97, 0xae, 0x8a, 0x25, 0xf4, 0x7d, 0x59, 0xa8, 0x60, 0xf7, 0x54, 0xf0, 0xe1,
        0xbf, 0x65,
    ];
    const TARGET_DIGEST: [u8; 32] = [
        0x58, 0x2a, 0x5e, 0xa9, 0xe4, 0xc0, 0x01, 0xfa, 0x54, 0xe8, 0xab, 0x62, 0x83, 0x0c, 0x4f,
        0x3b, 0x7b, 0xeb, 0xfa, 0x7c, 0xb1, 0xce, 0x8a, 0x59, 0x60, 0x15, 0x8a, 0x6e, 0xd2, 0xb0,
        0x01, 0x1a,
    ];
    const LENS_DIGEST: [u8; 32] = [
        0x9c, 0x27, 0xd7, 0xf0, 0x55, 0x8b, 0xc6, 0xcf, 0x03, 0x73, 0x26, 0x3d, 0xb4, 0x66, 0x7f,
        0xe4, 0xcc, 0x20, 0xd0, 0x03, 0x54, 0x91, 0x6c, 0x3b, 0xa7, 0xe0, 0x02, 0x0b, 0x4c, 0x36,
        0x98, 0xb0,
    ];

    let source = Schema::default()
        .vertex("source-record", "record")
        .vertex("source-value", "int")
        .edge(
            "source-record",
            "source-value",
            "prop",
            Some("source-field"),
        );
    let target = Schema::default()
        .vertex("target-record", "record")
        .vertex("target-value", "int")
        .edge(
            "target-record",
            "target-value",
            "prop",
            Some("target-field"),
        );
    let source_edge = SEdge::new(
        "source-record",
        "source-value",
        "prop",
        Some("source-field"),
    );
    let target_edge = SEdge::new(
        "target-record",
        "target-value",
        "prop",
        Some("target-field"),
    );
    let migration = Migration {
        surviving_verts: BTreeSet::from(["target-record".to_owned(), "target-value".to_owned()]),
        surviving_edges: BTreeSet::from([source_edge.clone()]),
        vertex_remap: BTreeMap::from([
            ("source-record".to_owned(), "target-record".to_owned()),
            ("source-value".to_owned(), "target-value".to_owned()),
        ]),
        edge_remap: BTreeMap::from([(source_edge, target_edge)]),
        conditional: BTreeSet::from(["source-record".to_owned()]),
        transformed: BTreeSet::from(["source-value".to_owned()]),
        ..Migration::default()
    };
    assert_eq!(table::canonical_migration_bytes(&migration).len(), 520);

    let identity = panproto_reference::lens::graph_lens_identity(
        &source,
        &target,
        &migration,
        ComplementShape::Graph,
    );
    assert_eq!(identity.source_schema, SOURCE_DIGEST);
    assert_eq!(identity.target_schema, TARGET_DIGEST);
    assert_eq!(identity.lens, LENS_DIGEST);
    assert_eq!(identity.shape, ComplementShape::Graph);
    assert_eq!(identity.format_version, 1);
}

#[test]
fn tree_put_checks_identity_but_tree_domain_predicate_is_identity_free() {
    let source = Schema::default()
        .vertex("kept", "obj")
        .vertex("hidden", "obj")
        .edge("kept", "hidden", "child", Some("hidden"));
    let target = Schema::default().vertex("kept", "obj");
    let migration = Migration {
        surviving_verts: BTreeSet::from(["kept".to_owned()]),
        ..Migration::default()
    };
    let plan = compile(&source, &target, &migration).unwrap();
    let lens = graph_lens(&source, &target, &migration, &plan);
    let tree = Tree {
        nodes: [(0, Node::new(0, "kept")), (1, Node::new(1, "hidden"))].into(),
        arcs: vec![(0, 1, SEdge::new("kept", "hidden", "child", Some("hidden")))],
        fans: Vec::new(),
        root: 0,
    };
    let (view, complement, embedding) = panproto_reference::lens::tree_get(&lens, &tree)
        .unwrap()
        .unwrap();
    assert!(!complement.dropped_nodes.is_empty());

    let mut unbound = complement.clone();
    unbound.identity = None;
    assert_eq!(
        panproto_reference::lens::tree_check_put_domain(&lens, &view, &embedding, &unbound),
        Ok(())
    );
    assert_eq!(
        panproto_reference::lens::tree_put(&lens, &view, &embedding, &unbound),
        Err(LensError::Identity(IdentityError::UnboundComplement))
    );

    let expected = panproto_reference::lens::graph_lens_identity(
        &source,
        &target,
        &migration,
        panproto_reference::canonical::ComplementShape::WType,
    );
    let mut found = expected;
    found.format_version += 1;
    let mut mismatched = complement;
    mismatched.identity = Some(found);
    assert_eq!(
        panproto_reference::lens::tree_check_put_domain(&lens, &view, &embedding, &mismatched),
        Ok(())
    );
    assert_eq!(
        panproto_reference::lens::tree_put(&lens, &view, &embedding, &mismatched),
        Err(LensError::Identity(IdentityError::Mismatch {
            expected: Box::new(expected),
            found: Box::new(found),
        }))
    );
}

#[test]
fn tree_complements_are_bound_before_tree_view_reconstruction() {
    let source = Schema::default().vertex("a", "obj");
    let first_migration = identity_migration(&source);
    let first_plan = compile(&source, &source, &first_migration).unwrap();
    let first = graph_lens(&source, &source, &first_migration, &first_plan);
    let tree = Tree {
        nodes: [(u32::MAX, Node::new(u32::MAX, "a"))].into(),
        arcs: Vec::new(),
        fans: Vec::new(),
        root: u32::MAX,
    };
    let (view, complement, embedding) = panproto_reference::lens::tree_get(&first, &tree)
        .unwrap()
        .unwrap();

    let target = Schema::default().vertex("b", "obj");
    let second_migration = Migration {
        surviving_verts: BTreeSet::from(["b".to_owned()]),
        vertex_remap: [("a".to_owned(), "b".to_owned())].into(),
        ..Migration::default()
    };
    let second_plan = compile(&source, &target, &second_migration).unwrap();
    let second = graph_lens(&source, &target, &second_migration, &second_plan);
    assert!(matches!(
        panproto_reference::lens::tree_put(&second, &view, &embedding, &complement),
        Err(LensError::Identity(IdentityError::Mismatch { .. }))
    ));
}

#[test]
fn both_protolens_instantiations_refuse_a_false_precondition() {
    let expected = Schema::default().vertex("required", "obj");
    let supplied = Schema::default().vertex("other", "obj");
    let protolens = Protolens::new(
        SchemaPrecondition::HasVertices(BTreeSet::from(["required".to_owned()])),
        expected.clone(),
        identity_migration(&expected),
    );
    assert!(matches!(
        protolens_schema(&protolens, &supplied),
        Err(ProtolensError::FalsePrecondition { .. })
    ));
    assert!(matches!(
        protolens_instance(&protolens, &supplied, &Graph::default()),
        Err(ProtolensError::FalsePrecondition { .. })
    ));
}

#[test]
fn protolens_instance_returns_a_lens_and_its_bound_complement() {
    let schema = Schema::default().vertex("r", "obj");
    let migration = identity_migration(&schema);
    let protolens = Protolens::exact(&schema, &schema, &migration);
    let source = Graph {
        nodes: [(7, Node::new(7, "r"))].into(),
        ..Graph::default()
    };
    let application = protolens_instance(&protolens, &schema, &source).unwrap();
    let bound = application.lens.as_graph_lens().unwrap();
    assert_eq!(application.view, source);
    assert_eq!(
        bound
            .put(&application.view, &application.complement)
            .unwrap(),
        source
    );
}

struct RecursiveContractionFixture {
    source_schema: Schema,
    target_schema: Schema,
    migration: Migration,
    source: Tree,
    image: SEdge,
}

fn recursive_contraction_fixture() -> RecursiveContractionFixture {
    let source_schema = Schema::default()
        .vertex("r", "obj")
        .vertex("m", "obj")
        .vertex("leaf", "str")
        .edge("r", "m", "prop", Some("m"))
        .edge("m", "m", "prop", Some("m"))
        .edge("m", "leaf", "prop", Some("leaf"));
    let image = SEdge::new("r", "leaf", "prop", Some("leaf"));
    let target_schema = Schema::default()
        .vertex("r", "obj")
        .vertex("leaf", "str")
        .edge("r", "leaf", "prop", Some("leaf"));
    let migration = Migration {
        surviving_verts: ["r".to_owned(), "leaf".to_owned()].into(),
        resolver: [(("r".to_owned(), "leaf".to_owned()), image.clone())].into(),
        ..Migration::default()
    };
    let source = Tree {
        nodes: [
            (0, Node::new(0, "r")),
            (1, Node::new(1, "m")),
            (2, Node::new(2, "m")),
            (3, Node::new(3, "m")),
            (
                4,
                Node::new(4, "leaf").with_value(Value::Str("payload".to_owned())),
            ),
        ]
        .into(),
        arcs: vec![
            (0, 1, SEdge::new("r", "m", "prop", Some("m"))),
            (1, 2, SEdge::new("m", "m", "prop", Some("m"))),
            (2, 3, SEdge::new("m", "m", "prop", Some("m"))),
            (3, 4, SEdge::new("m", "leaf", "prop", Some("leaf"))),
        ],
        fans: Vec::new(),
        root: 0,
    };
    RecursiveContractionFixture {
        source_schema,
        target_schema,
        migration,
        source,
        image,
    }
}

fn assert_recursive_square_observation(
    observation: &GraphSquareObservation,
    fixture: &RecursiveContractionFixture,
) {
    let (view, complement) = observation.graph.get.as_ref().unwrap();
    assert_eq!(view.nodes.keys().copied().collect::<Vec<_>>(), vec![0, 4]);
    assert_eq!(
        view.edges,
        [(
            4,
            panproto_reference::model::GEdge {
                src: 0,
                tgt: 4,
                label: fixture.image.clone(),
                position: None,
            },
        )]
        .into()
    );
    assert_eq!(
        complement.synthesized_edges,
        [(
            4,
            SynthKey::Contract {
                src: 0,
                tgt: 4,
                edge: fixture.image.clone(),
            },
        )]
        .into()
    );
    let encoded =
        panproto_reference::encode::tree_to_graph(&fixture.source, Some(&fixture.source_schema))
            .unwrap()
            .0;
    assert_eq!(observation.graph.put, Some(Ok(encoded.clone())));
    assert_eq!(observation.fast.put, Some(Ok(encoded)));
}

fn assert_recursive_basis_families(lens: &GraphLens<'_>, fixture: &RecursiveContractionFixture) {
    let graph_source =
        panproto_reference::encode::tree_to_graph(&fixture.source, Some(&fixture.source_schema))
            .unwrap()
            .0;
    let graph_laws = check_graph_laws(lens, &graph_source);
    let tree_laws = check_tree_laws(lens, &fixture.source);
    for report in [&graph_laws, &tree_laws] {
        assert!(
            report
                .basis_families
                .contains_key(&Mutation::AddContractedEdge)
        );
    }
    assert!(
        !graph_laws
            .basis_families
            .contains_key(&Mutation::ReservedIdReuse),
        "incident dropped edges are outside the local reserved-node proof"
    );
    assert!(
        !tree_laws
            .basis_families
            .contains_key(&Mutation::ReservedIdReuse)
    );
}

fn assert_recursive_put_domain_refusals(
    lens: &GraphLens<'_>,
    observation: &GraphSquareObservation,
    fixture: &RecursiveContractionFixture,
) {
    let (view, complement) = observation.graph.get.as_ref().unwrap();
    // The graph checker reports only IdCollision here, but the replacement
    // changes the source anchor at the endpoint of restored dropped edges.
    // Thus this counterfactual is not a structurally valid source witness.
    let mut exact_graph = view.clone();
    let mut colliding = exact_graph.nodes[&0].clone();
    colliding.id = 1;
    exact_graph.nodes.insert(1, colliding);
    assert_eq!(
        lens.check_put_domain(&exact_graph, complement),
        Err(vec![
            panproto_reference::lens::PutDomainError::IdCollision {
                elem: ViewRef::Node(1),
            },
        ])
    );
    let mut stripped = complement.clone();
    assert!(
        stripped.dropped_nodes.remove(&1).is_some(),
        "the counterfactual removes exactly the colliding dropped node"
    );
    assert_eq!(lens.check_put_domain(&exact_graph, &stripped), Ok(()));
    let restored = lens.put(&exact_graph, &stripped).unwrap();
    assert_eq!(
        panproto_reference::validation::validate_graph(
            &fixture.source_schema,
            &BTreeMap::new(),
            &restored,
        )
        .status,
        panproto_reference::validation::CheckStatus::Failed,
        "the checker-admitted replacement must expose its edge-endpoint mismatch"
    );

    // Reusing a dropped edge id also inserts an edge with no unique source
    // preimage. This is a mixed two-refusal mutation, not another exact
    // ReservedIdReuse witness.
    let mut mixed_graph = view.clone();
    mixed_graph
        .edges
        .insert(0, complement.dropped_edges[&0].clone());
    assert_eq!(
        lens.check_put_domain(&mixed_graph, complement),
        Err(vec![
            panproto_reference::lens::PutDomainError::IdCollision {
                elem: ViewRef::Edge(0),
            },
            panproto_reference::lens::PutDomainError::UnrepresentableViewEdge {
                elem: ViewRef::Edge(0),
            },
        ])
    );

    let (mut tree_view, tree_complement, tree_embedding) =
        panproto_reference::lens::tree_get(lens, &fixture.source)
            .unwrap()
            .unwrap();
    tree_view.nodes.insert(1, Node::new(1, "leaf"));
    tree_view.arcs.push((0, 1, fixture.image.clone()));
    assert_eq!(
        panproto_reference::lens::tree_check_put_domain(
            lens,
            &tree_view,
            &tree_embedding,
            &tree_complement,
        ),
        Err(LensError::PutDomainReport(vec![
            panproto_reference::lens::PutDomainError::IdCollision {
                elem: ViewRef::Node(1),
            },
            panproto_reference::lens::PutDomainError::SynthesizedEdited {
                elem: ViewRef::Edge(4),
            },
            panproto_reference::lens::PutDomainError::ContractedEdgeInserted {
                elem: ViewRef::Edge(5),
            },
        ]))
    );
}

#[test]
fn graph_square_covers_recursive_dropped_wrapper_contraction_and_put() {
    let fixture = recursive_contraction_fixture();
    let plan = compile(
        &fixture.source_schema,
        &fixture.target_schema,
        &fixture.migration,
    )
    .unwrap();
    let lens = graph_lens(
        &fixture.source_schema,
        &fixture.target_schema,
        &fixture.migration,
        &plan,
    );

    let result = graph_square(&lens, &fixture.source);
    let SquareResult::Commutes(observation) = result else {
        panic!("recursive contraction square did not commute: {result:?}");
    };
    assert_recursive_square_observation(&observation, &fixture);
    assert_recursive_basis_families(&lens, &fixture);
    assert_recursive_put_domain_refusals(&lens, &observation, &fixture);
}

#[test]
fn graph_square_covers_a_contracted_leaf_with_no_surviving_descendant() {
    let edge = SEdge::new("r", "leaf", "prop", Some("leaf"));
    let source_schema = Schema::default()
        .vertex("r", "obj")
        .vertex("leaf", "str")
        .edge("r", "leaf", "prop", Some("leaf"));
    let target_schema = Schema::default().vertex("r", "obj");
    let migration = Migration {
        surviving_verts: ["r".to_owned()].into(),
        ..Migration::default()
    };
    let plan = compile(&source_schema, &target_schema, &migration).unwrap();
    let lens = graph_lens(&source_schema, &target_schema, &migration, &plan);
    let source = Tree {
        nodes: [
            (7, Node::new(7, "r")),
            (
                8,
                Node::new(8, "leaf").with_value(Value::Str("hidden".to_owned())),
            ),
        ]
        .into(),
        arcs: vec![(7, 8, edge)],
        fans: Vec::new(),
        root: 7,
    };

    let result = graph_square(&lens, &source);
    let SquareResult::Commutes(observation) = result else {
        panic!("contracted-leaf square did not commute: {result:?}");
    };
    let (view, complement) = observation.graph.get.as_ref().unwrap();
    assert_eq!(view.nodes.keys().copied().collect::<Vec<_>>(), vec![7]);
    assert!(view.edges.is_empty());
    assert_eq!(
        complement.dropped_nodes.keys().copied().collect::<Vec<_>>(),
        vec![8]
    );
    assert_eq!(
        complement.dropped_edges.keys().copied().collect::<Vec<_>>(),
        vec![0]
    );
    assert!(observation.graph.put.as_ref().unwrap().is_ok());
    assert!(observation.fast.put.as_ref().unwrap().is_ok());
}

fn expansion_fixture(with_fan: bool) -> (Schema, Schema, Migration, Tree) {
    let source_edge = SEdge::new("a", "b", "source", Some("ab"));
    let mut source_schema = Schema::default()
        .vertex("a", "obj")
        .vertex("b", "obj")
        .edge("a", "b", "source", Some("ab"));
    let mut target_schema = Schema::default()
        .vertex("a", "obj")
        .vertex("x", "obj")
        .vertex("b", "obj")
        .edge("a", "x", "hop", Some("ax"))
        .edge("x", "b", "hop", Some("xb"));
    let fans = if with_fan {
        let hyper = HyperEdge {
            kind: "link".to_owned(),
            signature: [("child".to_owned(), "b".to_owned())].into(),
            parent_label: "parent".to_owned(),
        };
        source_schema
            .hyper_edges
            .insert("h".to_owned(), hyper.clone());
        target_schema.hyper_edges.insert("h".to_owned(), hyper);
        vec![Fan {
            parent: 10,
            hyper_edge: "h".to_owned(),
            children: [("child".to_owned(), 20)].into(),
        }]
    } else {
        Vec::new()
    };
    let migration = Migration {
        surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
        expansion_path: [(("a".to_owned(), "b".to_owned()), vec!["x".to_owned()])].into(),
        ..Migration::default()
    };
    let source = Tree {
        nodes: [
            (
                10,
                Node::new(10, "a").with_field("name", Value::Str("left".to_owned())),
            ),
            (
                20,
                Node::new(20, "b").with_value(Value::Str("right".to_owned())),
            ),
        ]
        .into(),
        arcs: vec![(10, 20, source_edge)],
        fans,
        root: 10,
    };
    (source_schema, target_schema, migration, source)
}

#[test]
fn graph_square_covers_direct_expand_with_exact_synthesized_carriers() {
    let (source_schema, target_schema, migration, source) = expansion_fixture(false);
    let plan = compile(&source_schema, &target_schema, &migration).unwrap();
    let lens = graph_lens(&source_schema, &target_schema, &migration, &plan);
    let result = graph_square(&lens, &source);
    let SquareResult::Commutes(observation) = result else {
        panic!("direct-expand square did not commute: {result:?}");
    };
    let (view, complement) = observation.graph.get.as_ref().unwrap();
    assert_eq!(
        view.nodes.keys().copied().collect::<Vec<_>>(),
        vec![10, 20, 21]
    );
    assert_eq!(view.edges.keys().copied().collect::<Vec<_>>(), vec![1, 2]);
    assert_eq!(
        complement.synthesized_nodes,
        [(
            21,
            SynthKey::Expand {
                source_edge: 0,
                hop: 0
            }
        )]
        .into()
    );
    assert_eq!(
        complement.synthesized_edges,
        [
            (
                1,
                SynthKey::Expand {
                    source_edge: 0,
                    hop: 0
                }
            ),
            (
                2,
                SynthKey::Expand {
                    source_edge: 0,
                    hop: 1
                }
            ),
        ]
        .into()
    );
    assert!(observation.graph.put.as_ref().unwrap().is_ok());
    assert!(observation.fast.put.as_ref().unwrap().is_ok());
}

#[test]
fn graph_square_preserves_fan_ids_and_order_while_expanding_an_arc() {
    let (source_schema, target_schema, migration, source) = expansion_fixture(true);
    let plan = compile(&source_schema, &target_schema, &migration).unwrap();
    let lens = graph_lens(&source_schema, &target_schema, &migration, &plan);
    let result = graph_square(&lens, &source);
    let SquareResult::Commutes(observation) = result else {
        panic!("fan-plus-expansion square did not commute: {result:?}");
    };
    let (view, _) = observation.graph.get.as_ref().unwrap();
    assert_eq!(
        view.nodes
            .iter()
            .find_map(|(id, node)| (node.shape == NodeShape::HyperEdge).then_some(*id)),
        Some(21)
    );
    assert_eq!(
        view.edges.keys().copied().collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
    let restored = panproto_reference::encode::tree_to_graph(&source, Some(&source_schema))
        .unwrap()
        .0;
    assert_eq!(observation.graph.put, Some(Ok(restored.clone())));
    assert_eq!(observation.fast.put, Some(Ok(restored)));
    let graph_source = panproto_reference::encode::tree_to_graph(&source, Some(&source_schema))
        .unwrap()
        .0;
    let report = check_graph_laws(&lens, &graph_source);
    assert!(report.basis_families.contains_key(&Mutation::MalformedFan));
}

#[test]
fn fan_parent_labels_use_target_on_get_and_source_on_put() {
    let mut source_schema = Schema::default().vertex("r", "obj").vertex("leaf", "obj");
    let ordinary = SEdge::new("r", "leaf", "child", Some("leaf"));
    source_schema.edges.insert(ordinary.clone());
    source_schema.hyper_edges.insert(
        "h".to_owned(),
        HyperEdge {
            kind: "h".to_owned(),
            signature: [("x".to_owned(), "leaf".to_owned())].into(),
            parent_label: "p_old".to_owned(),
        },
    );
    let mut target_schema = Schema::default().vertex("r", "obj").vertex("leaf", "obj");
    target_schema.edges.insert(ordinary.clone());
    target_schema.hyper_edges.insert(
        "h".to_owned(),
        HyperEdge {
            kind: "h".to_owned(),
            signature: [("x".to_owned(), "leaf".to_owned())].into(),
            parent_label: "p_new".to_owned(),
        },
    );
    let migration = Migration {
        surviving_verts: ["r".to_owned(), "leaf".to_owned()].into(),
        surviving_edges: [ordinary.clone()].into(),
        ..Migration::default()
    };
    let plan = compile(&source_schema, &target_schema, &migration).unwrap();
    let lens = graph_lens(&source_schema, &target_schema, &migration, &plan);
    let source = Tree {
        nodes: [(0, Node::new(0, "r")), (1, Node::new(1, "leaf"))].into(),
        arcs: vec![(0, 1, ordinary)],
        fans: vec![Fan {
            parent: 0,
            hyper_edge: "h".to_owned(),
            children: [("x".to_owned(), 1)].into(),
        }],
        root: 0,
    };
    let result = graph_square(&lens, &source);
    let SquareResult::Commutes(observation) = result else {
        panic!("fan parent-label square did not commute: {result:#?}");
    };
    let target_view = &observation.graph.get.as_ref().unwrap().0;
    let target_parent = target_view
        .edges
        .values()
        .find(|edge| edge.label.kind == restrict::HYPER_PARENT)
        .unwrap();
    assert_eq!(target_parent.label.name.as_deref(), Some("p_new"));
    assert_eq!(observation.graph.get, observation.fast.get);
    let restored = observation.fast.put.as_ref().unwrap().as_ref().unwrap();
    let source_parent = restored
        .edges
        .values()
        .find(|edge| edge.label.kind == restrict::HYPER_PARENT)
        .unwrap();
    assert_eq!(source_parent.label.name.as_deref(), Some("p_old"));
}

#[test]
fn real_graph_and_tree_law_adapters_run_every_gate_nonvacuously() {
    let (source_schema, target_schema, migration, source) = expansion_fixture(false);
    let plan = compile(&source_schema, &target_schema, &migration).unwrap();
    let lens = graph_lens(&source_schema, &target_schema, &migration, &plan);
    let source_graph = panproto_reference::encode::tree_to_graph(&source, Some(&source_schema))
        .unwrap()
        .0;

    let graph = check_graph_laws(&lens, &source_graph);
    let tree = check_tree_laws(&lens, &source);
    for report in [&graph, &tree] {
        assert!(report.basis_cases >= 4);
        assert_eq!(report.gates.len(), 5);
        assert!(
            report.gates.iter().all(|gate| {
                matches!(gate.status, LawGateStatus::Passed { cases } if cases > 0)
            }),
            "nonvacuous real-law report failed: {report:#?}"
        );
    }
    for report in [&graph, &tree] {
        assert!(report.basis_families.contains_key(&Mutation::ValueEdit));
        assert!(report.basis_families.contains_key(&Mutation::FieldEdit));
        assert!(
            report
                .basis_families
                .contains_key(&Mutation::RemoveSynthesized)
        );
        assert!(
            report
                .basis_families
                .contains_key(&Mutation::EditSynthesized)
        );
    }
}

#[test]
fn real_single_step_bases_have_literal_exhaustive_counts_and_family_order() {
    let graph_schema = Schema::default().vertex("a", "obj").vertex("b", "obj");
    let graph_migration = identity_migration(&graph_schema);
    let graph_plan = compile(&graph_schema, &graph_schema, &graph_migration).unwrap();
    let graph_identity_lens =
        graph_lens(&graph_schema, &graph_schema, &graph_migration, &graph_plan);
    let graph_source = Graph {
        nodes: [
            (0, Node::new(0, "a").with_field("f", Value::Bool(true))),
            (1, Node::new(1, "b")),
        ]
        .into(),
        edges: BTreeMap::new(),
    };
    let graph_report = check_graph_laws(&graph_identity_lens, &graph_source);
    assert_eq!(
        graph_report.basis_sequence,
        vec![
            Mutation::ValueEdit,
            Mutation::ValueEdit,
            Mutation::FieldEdit,
            Mutation::AddNode,
            Mutation::AddNode,
            Mutation::RemoveNode,
            Mutation::RemoveNode,
            Mutation::RelabelNode,
            Mutation::RelabelNode,
        ]
    );

    let edge = SEdge::new("a", "b", "child", Some("b"));
    let tree_schema = Schema::default()
        .vertex("a", "obj")
        .vertex("b", "obj")
        .edge("a", "b", "child", Some("b"));
    let tree_migration = identity_migration(&tree_schema);
    let tree_plan = compile(&tree_schema, &tree_schema, &tree_migration).unwrap();
    let tree_identity_lens = graph_lens(&tree_schema, &tree_schema, &tree_migration, &tree_plan);
    let tree_source = Tree {
        nodes: [
            (0, Node::new(0, "a").with_field("f", Value::Bool(true))),
            (1, Node::new(1, "b")),
        ]
        .into(),
        arcs: vec![(0, 1, edge)],
        fans: Vec::new(),
        root: 0,
    };
    let tree_report = check_tree_laws(&tree_identity_lens, &tree_source);
    assert_eq!(
        tree_report.basis_sequence,
        vec![
            Mutation::ValueEdit,
            Mutation::ValueEdit,
            Mutation::FieldEdit,
            Mutation::AddNode,
            Mutation::RemoveNode,
            Mutation::AddEdge,
            Mutation::RemoveEdge,
        ]
    );
}

fn full_mutation_schemas() -> (Schema, Schema) {
    let source_schema = Schema::default()
        .vertex("r", "obj")
        .vertex("leaf", "obj")
        .vertex("spare", "obj")
        .edge("r", "leaf", "child", Some("child"))
        .edge("r", "leaf", "child", Some("alternate"))
        .edge("r", "r", "child", Some("loop"))
        .edge("r", "spare", "child", Some("spare"))
        .edge("spare", "leaf", "source", Some("expanded"));
    let target_schema = Schema::default()
        .vertex("r", "obj")
        .vertex("leaf", "obj")
        .vertex("spare", "obj")
        .vertex("x", "obj")
        .edge("r", "leaf", "child", Some("child"))
        .edge("r", "leaf", "child", Some("alternate"))
        .edge("r", "r", "child", Some("loop"))
        .edge("r", "spare", "child", Some("spare"))
        .edge("spare", "x", "hop", Some("sx"))
        .edge("x", "leaf", "hop", Some("xl"));
    (source_schema, target_schema)
}

fn full_mutation_migration() -> Migration {
    Migration {
        surviving_verts: ["r".to_owned(), "leaf".to_owned(), "spare".to_owned()].into(),
        expansion_path: [(
            ("spare".to_owned(), "leaf".to_owned()),
            vec!["x".to_owned()],
        )]
        .into(),
        ..Migration::default()
    }
}

struct FullMutationEdges {
    child: SEdge,
    alternate: SEdge,
    loop_edge: SEdge,
    spare: SEdge,
    expanded: SEdge,
}

fn full_mutation_edges() -> FullMutationEdges {
    FullMutationEdges {
        child: SEdge::new("r", "leaf", "child", Some("child")),
        alternate: SEdge::new("r", "leaf", "child", Some("alternate")),
        loop_edge: SEdge::new("r", "r", "child", Some("loop")),
        spare: SEdge::new("r", "spare", "child", Some("spare")),
        expanded: SEdge::new("spare", "leaf", "source", Some("expanded")),
    }
}

fn full_mutation_tree(edges: &FullMutationEdges) -> Tree {
    Tree {
        nodes: [
            (
                0,
                Node::new(0, "r").with_field("name", Value::Str("root".to_owned())),
            ),
            (1, Node::new(1, "leaf")),
            (2, Node::new(2, "leaf")),
            (3, Node::new(3, "r")),
            (4, Node::new(4, "spare")),
            (5, Node::new(5, "leaf")),
        ]
        .into(),
        arcs: vec![
            (0, 3, edges.loop_edge.clone()),
            (0, 1, edges.child.clone()),
            (0, 2, edges.child.clone()),
            (0, 4, edges.spare.clone()),
            (4, 5, edges.expanded.clone()),
        ],
        fans: Vec::new(),
        root: 0,
    }
}

fn full_mutation_graph(edges: &FullMutationEdges) -> Graph {
    Graph {
        nodes: [
            (
                0,
                Node::new(0, "r").with_field("name", Value::Str("root".to_owned())),
            ),
            (1, Node::new(1, "leaf")),
            (2, Node::new(2, "leaf")),
            (3, Node::new(3, "r")),
            (4, Node::new(4, "spare")),
            (5, Node::new(5, "leaf")),
            (6, Node::new(6, "spare")),
        ]
        .into(),
        edges: [
            (
                0,
                panproto_reference::model::GEdge {
                    src: 0,
                    tgt: 1,
                    label: edges.child.clone(),
                    position: None,
                },
            ),
            (
                1,
                panproto_reference::model::GEdge {
                    src: 0,
                    tgt: 2,
                    label: edges.child.clone(),
                    position: None,
                },
            ),
            (
                2,
                panproto_reference::model::GEdge {
                    src: 3,
                    tgt: 3,
                    label: edges.loop_edge.clone(),
                    position: None,
                },
            ),
            (
                3,
                panproto_reference::model::GEdge {
                    src: 3,
                    tgt: 1,
                    label: edges.alternate.clone(),
                    position: None,
                },
            ),
            (
                4,
                panproto_reference::model::GEdge {
                    src: 0,
                    tgt: 4,
                    label: edges.spare.clone(),
                    position: None,
                },
            ),
            (
                5,
                panproto_reference::model::GEdge {
                    src: 4,
                    tgt: 5,
                    label: edges.expanded.clone(),
                    position: None,
                },
            ),
        ]
        .into(),
    }
}

fn full_mutation_fixture() -> (Schema, Schema, Migration, Graph, Tree) {
    let (source_schema, target_schema) = full_mutation_schemas();
    let migration = full_mutation_migration();
    let edges = full_mutation_edges();
    let tree = full_mutation_tree(&edges);
    let graph = full_mutation_graph(&edges);
    (source_schema, target_schema, migration, graph, tree)
}

#[test]
fn real_bases_execute_every_applicable_admitted_family_and_locality_oracle() {
    let (source_schema, target_schema, migration, graph_source, tree_source) =
        full_mutation_fixture();
    let plan = compile(&source_schema, &target_schema, &migration).unwrap();
    let lens = graph_lens(&source_schema, &target_schema, &migration, &plan);
    let graph = check_graph_laws(&lens, &graph_source);
    let tree = check_tree_laws(&lens, &tree_source);
    let graph_expected: BTreeSet<_> = [
        Mutation::ValueEdit,
        Mutation::FieldEdit,
        Mutation::AddNode,
        Mutation::RemoveNode,
        Mutation::RelabelNode,
        Mutation::AddEdge,
        Mutation::RemoveEdge,
        Mutation::Reconnect,
        Mutation::RelabelEdge,
        Mutation::Reorder,
        Mutation::AddParallelEdge,
        Mutation::AddSelfLoop,
    ]
    .into();
    let tree_expected: BTreeSet<_> = [
        Mutation::ValueEdit,
        Mutation::FieldEdit,
        Mutation::AddNode,
        Mutation::RemoveNode,
        Mutation::AddEdge,
        Mutation::RemoveEdge,
        Mutation::Reconnect,
        Mutation::RelabelEdge,
        Mutation::Reorder,
    ]
    .into();
    let graph_actual: BTreeSet<_> = graph.basis_families.keys().copied().collect();
    let tree_actual: BTreeSet<_> = tree.basis_families.keys().copied().collect();
    assert!(
        graph_expected.is_subset(&graph_actual),
        "missing graph families: {:?}; actual: {graph_actual:?}",
        graph_expected.difference(&graph_actual).collect::<Vec<_>>()
    );
    assert!(
        tree_expected.is_subset(&tree_actual),
        "missing tree families: {:?}; actual: {tree_actual:?}",
        tree_expected.difference(&tree_actual).collect::<Vec<_>>()
    );
    assert!(
        graph.gates.iter().all(|gate| matches!(
            gate.status,
            LawGateStatus::Passed { cases } if cases > 0
        )),
        "graph SSB report failed: {graph:#?}"
    );
    assert!(
        tree.gates.iter().all(|gate| matches!(
            gate.status,
            LawGateStatus::Passed { cases } if cases > 0
        )),
        "tree SSB report failed: {tree:#?}"
    );

    let graph_locality = check_locality_report::<GraphShape, _>(&lens, &graph_source, &());
    let tree_locality = check_locality_report::<TreeShape, _>(&lens, &tree_source, &());
    assert!(matches!(
        graph_locality.status,
        LawGateStatus::Passed { cases } if cases >= graph_expected.len()
    ));
    assert!(matches!(
        tree_locality.status,
        LawGateStatus::Passed { cases } if cases >= tree_expected.len()
    ));

    let single = Tree {
        nodes: [(0, Node::new(0, "r"))].into(),
        arcs: Vec::new(),
        fans: Vec::new(),
        root: 0,
    };
    let relabel = check_tree_laws(&lens, &single);
    assert!(relabel.basis_families.contains_key(&Mutation::RelabelNode));
}

#[test]
fn edited_graph_tree_put_square_has_an_independent_success_oracle_and_order() {
    let (source_schema, target_schema, migration, _, source) = full_mutation_fixture();
    let plan = compile(&source_schema, &target_schema, &migration).unwrap();
    let lens = graph_lens(&source_schema, &target_schema, &migration, &plan);
    let (mut edited, _, embedding) = panproto_reference::lens::tree_get(&lens, &source)
        .unwrap()
        .unwrap();
    edited
        .nodes
        .get_mut(&1)
        .unwrap()
        .fields
        .insert("edited".to_owned(), Value::Bool(true));
    let mut expected_tree = source.clone();
    expected_tree
        .nodes
        .get_mut(&1)
        .unwrap()
        .fields
        .insert("edited".to_owned(), Value::Bool(true));
    let expected_graph =
        panproto_reference::encode::tree_to_graph(&expected_tree, Some(&source_schema))
            .unwrap()
            .0;
    let result = graph_put_square(&lens, &source, &edited, &embedding);
    let SquareResult::Commutes(observation) = result else {
        panic!("admitted edited-view put square did not commute: {result:?}");
    };
    assert_eq!(observation.graph.put, Some(Ok(expected_graph.clone())));
    assert_eq!(observation.fast.put, Some(Ok(expected_graph)));
    assert_eq!(observation.graph_tree_put, Some(Ok(expected_tree.clone())));
    assert_eq!(observation.fast_tree_put, Some(Ok(expected_tree)));
}

#[test]
fn edited_three_sibling_order_is_outside_the_exact_position_free_graph_square() {
    let (source_schema, target_schema, migration, _, source) = full_mutation_fixture();
    let plan = compile(&source_schema, &target_schema, &migration).unwrap();
    let lens = graph_lens(&source_schema, &target_schema, &migration, &plan);
    let (mut edited, _, embedding) = panproto_reference::lens::tree_get(&lens, &source)
        .unwrap()
        .unwrap();
    let root_children = |tree: &Tree| {
        tree.arcs
            .iter()
            .filter_map(|(parent, child, _)| (*parent == 0).then_some(*child))
            .collect::<Vec<_>>()
    };
    assert_eq!(root_children(&edited), vec![3, 1, 2, 4]);

    let desired = [1, 2, 3, 4];
    edited.arcs.sort_by_key(|(parent, child, _)| {
        if *parent == 0 {
            desired
                .iter()
                .position(|candidate| candidate == child)
                .unwrap()
        } else {
            usize::MAX
        }
    });
    assert_eq!(root_children(&edited), desired);

    let result = graph_put_square(&lens, &source, &edited, &embedding);
    let SquareResult::Refused(observation) = result else {
        panic!("edited three-sibling reorder did not expose the square boundary: {result:#?}");
    };

    assert_eq!(
        observation
            .fast_tree_get
            .as_ref()
            .map(root_children)
            .unwrap(),
        vec![3, 1, 2, 4]
    );
    // F.B compares tree child sequence with position-free graph
    // `ordered_out_edges`. Stable embedded EdgeIds therefore present the
    // source order on the direct graph route, while the unchanged durable
    // embedding cannot represent the caller's sequence-only reorder. The
    // edit is outside the exact common domain; neither route may smuggle
    // order through `GEdge.position` or renumber identity carriers.
    assert_eq!(observation.graph_tree_put, Some(Ok(source)));
    assert!(matches!(
        observation.fast_tree_put,
        Some(Err(SquareError::OutsideExactDomain(
            "the durable embedding does not represent the edited tree presentation"
        )))
    ));
    assert!(matches!(
        observation.fast.put,
        Some(Err(SquareError::OutsideExactDomain(
            "the durable embedding does not represent the edited tree presentation"
        )))
    ));
    assert_eq!(
        observation
            .graph
            .put
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .ordered_out_edges(0),
        vec![0, 1, 2, 3]
    );
}

#[test]
fn graph_put_square_transports_an_exact_synthesized_edit_error() {
    let (source_schema, target_schema, migration, source) = expansion_fixture(false);
    let plan = compile(&source_schema, &target_schema, &migration).unwrap();
    let lens = graph_lens(&source_schema, &target_schema, &migration, &plan);
    let (mut edited, _, embedding) = panproto_reference::lens::tree_get(&lens, &source)
        .unwrap()
        .unwrap();
    edited.arcs[0].2.kind.push_str("-edited");
    let result = graph_put_square(&lens, &source, &edited, &embedding);
    let SquareResult::Commutes(observation) = result else {
        panic!("synthesized-edge put refusal did not commute: {result:?}");
    };
    assert_eq!(
        observation.graph.put,
        Some(Err(SquareError::PutDomain(
            panproto_reference::lens::PutDomainError::SynthesizedEdited {
                elem: ViewRef::Edge(1),
            },
        )))
    );
    assert_eq!(observation.graph.put, observation.fast.put);
}

#[test]
fn tree_put_domain_accepts_cross_parent_synthesized_arc_interleavings() {
    let (source_schema, target_schema, migration, source) = expansion_fixture(false);
    let plan = compile(&source_schema, &target_schema, &migration).unwrap();
    let lens = graph_lens(&source_schema, &target_schema, &migration, &plan);
    let (mut synthesized_only, complement, embedding) =
        panproto_reference::lens::tree_get(&lens, &source)
            .unwrap()
            .unwrap();
    synthesized_only.arcs.reverse();
    assert_eq!(
        panproto_reference::lens::tree_check_put_domain(
            &lens,
            &synthesized_only,
            &embedding,
            &complement,
        ),
        Ok(())
    );

    let (source_schema, target_schema, migration, _, source) = full_mutation_fixture();
    let plan = compile(&source_schema, &target_schema, &migration).unwrap();
    let lens = graph_lens(&source_schema, &target_schema, &migration, &plan);
    let (mut mixed, complement, embedding) = panproto_reference::lens::tree_get(&lens, &source)
        .unwrap()
        .unwrap();
    let synth_at = mixed
        .arcs
        .iter()
        .position(|(_, child, _)| embedding.arcs.get(child) == Some(&5))
        .unwrap();
    let synthesized = mixed.arcs.remove(synth_at);
    mixed.arcs.insert(0, synthesized);
    assert_eq!(
        panproto_reference::lens::tree_check_put_domain(&lens, &mixed, &embedding, &complement,),
        Ok(())
    );
}

#[test]
fn tree_put_domain_rejects_a_synthesized_arc_before_a_kept_sibling() {
    let direct = SEdge::new("r", "leaf", "child", Some("leaf"));
    let source_schema = Schema::default()
        .vertex("r", "obj")
        .vertex("m", "obj")
        .vertex("leaf", "obj")
        .edge("r", "leaf", "child", Some("leaf"))
        .edge("r", "m", "child", Some("m"))
        .edge("m", "leaf", "child", Some("leaf"));
    let target_schema = Schema::default()
        .vertex("r", "obj")
        .vertex("leaf", "obj")
        .edge("r", "leaf", "child", Some("leaf"));
    let migration = Migration {
        surviving_verts: ["r".to_owned(), "leaf".to_owned()].into(),
        surviving_edges: [direct.clone()].into(),
        resolver: [(("r".to_owned(), "leaf".to_owned()), direct.clone())].into(),
        ..Migration::default()
    };
    let source = Tree {
        nodes: [
            (0, Node::new(0, "r")),
            (1, Node::new(1, "leaf")),
            (2, Node::new(2, "m")),
            (3, Node::new(3, "leaf")),
        ]
        .into(),
        arcs: vec![
            (0, 1, direct.clone()),
            (0, 2, SEdge::new("r", "m", "child", Some("m"))),
            (2, 3, SEdge::new("m", "leaf", "child", Some("leaf"))),
        ],
        fans: Vec::new(),
        root: 0,
    };
    let plan = compile(&source_schema, &target_schema, &migration).unwrap();
    let lens = graph_lens(&source_schema, &target_schema, &migration, &plan);
    let (mut view, complement, embedding) = panproto_reference::lens::tree_get(&lens, &source)
        .unwrap()
        .unwrap();
    assert_eq!(view.arcs, vec![(0, 1, direct.clone()), (0, 3, direct)]);
    assert_eq!(
        complement
            .synthesized_edges
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![3]
    );

    view.arcs.swap(0, 1);
    assert_eq!(
        panproto_reference::lens::tree_check_put_domain(&lens, &view, &embedding, &complement,),
        Err(LensError::PutDomainReport(vec![
            panproto_reference::lens::PutDomainError::SynthesizedEdited {
                elem: ViewRef::Edge(3),
            },
        ]))
    );
}

#[test]
fn tree_put_domain_rejects_same_parent_synthesized_key_reordering() {
    let image = SEdge::new("r", "leaf", "child", Some("leaf"));
    let source_schema = Schema::default()
        .vertex("r", "obj")
        .vertex("m", "obj")
        .vertex("leaf", "obj")
        .edge("r", "m", "child", Some("m"))
        .edge("m", "leaf", "child", Some("leaf"));
    let target_schema = Schema::default()
        .vertex("r", "obj")
        .vertex("leaf", "obj")
        .edge("r", "leaf", "child", Some("leaf"));
    let migration = Migration {
        surviving_verts: ["r".to_owned(), "leaf".to_owned()].into(),
        resolver: [(("r".to_owned(), "leaf".to_owned()), image.clone())].into(),
        ..Migration::default()
    };
    let source = Tree {
        nodes: [
            (0, Node::new(0, "r")),
            (1, Node::new(1, "m")),
            (2, Node::new(2, "leaf")),
            (3, Node::new(3, "m")),
            (4, Node::new(4, "leaf")),
        ]
        .into(),
        arcs: vec![
            (0, 1, SEdge::new("r", "m", "child", Some("m"))),
            (1, 2, SEdge::new("m", "leaf", "child", Some("leaf"))),
            (0, 3, SEdge::new("r", "m", "child", Some("m"))),
            (3, 4, SEdge::new("m", "leaf", "child", Some("leaf"))),
        ],
        fans: Vec::new(),
        root: 0,
    };
    let plan = compile(&source_schema, &target_schema, &migration).unwrap();
    let lens = graph_lens(&source_schema, &target_schema, &migration, &plan);
    let (mut view, complement, embedding) = panproto_reference::lens::tree_get(&lens, &source)
        .unwrap()
        .unwrap();
    assert_eq!(view.arcs, vec![(0, 2, image.clone()), (0, 4, image)]);
    assert_eq!(
        complement
            .synthesized_edges
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![4, 5]
    );

    view.arcs.reverse();
    assert_eq!(
        panproto_reference::lens::tree_check_put_domain(&lens, &view, &embedding, &complement,),
        Err(LensError::PutDomainReport(vec![
            panproto_reference::lens::PutDomainError::SynthesizedEdited {
                elem: ViewRef::Edge(4),
            },
        ]))
    );
}

#[test]
fn graph_put_square_transports_an_exact_dropped_edge_id_collision() {
    let (source_schema, target_schema, migration, source) = {
        let source_schema = Schema::default()
            .vertex("r", "obj")
            .vertex("m", "obj")
            .vertex("leaf", "obj")
            .edge("r", "m", "child", Some("m"))
            .edge("m", "leaf", "child", Some("leaf"));
        let image = SEdge::new("r", "leaf", "child", Some("leaf"));
        let target_schema = Schema::default()
            .vertex("r", "obj")
            .vertex("leaf", "obj")
            .edge("r", "leaf", "child", Some("leaf"));
        let migration = Migration {
            surviving_verts: ["r".to_owned(), "leaf".to_owned()].into(),
            resolver: [(("r".to_owned(), "leaf".to_owned()), image)].into(),
            ..Migration::default()
        };
        let source = Tree {
            nodes: [
                (0, Node::new(0, "r")),
                (1, Node::new(1, "m")),
                (2, Node::new(2, "leaf")),
            ]
            .into(),
            arcs: vec![
                (0, 1, SEdge::new("r", "m", "child", Some("m"))),
                (1, 2, SEdge::new("m", "leaf", "child", Some("leaf"))),
            ],
            fans: Vec::new(),
            root: 0,
        };
        (source_schema, target_schema, migration, source)
    };
    let plan = compile(&source_schema, &target_schema, &migration).unwrap();
    let lens = graph_lens(&source_schema, &target_schema, &migration, &plan);
    let (mut edited, _, mut embedding) = panproto_reference::lens::tree_get(&lens, &source)
        .unwrap()
        .unwrap();
    edited.nodes.insert(9, Node::new(9, "leaf"));
    edited
        .arcs
        .push((0, 9, SEdge::new("r", "leaf", "child", Some("leaf"))));
    embedding.arcs.insert(9, 0);
    let collision = graph_put_square(&lens, &source, &edited, &embedding);
    let SquareResult::Commutes(observation) = collision else {
        panic!("dropped-edge-id collision did not commute: {collision:?}");
    };
    assert_eq!(
        observation.graph.put,
        Some(Err(SquareError::PutDomain(
            panproto_reference::lens::PutDomainError::IdCollision {
                elem: ViewRef::Edge(0),
            },
        )))
    );
    assert_eq!(observation.graph.put, observation.fast.put);
}

#[test]
fn graph_put_square_transports_exact_tree_shape_and_exhaustion_errors() {
    let schema = Schema::default()
        .vertex("r", "obj")
        .vertex("leaf", "obj")
        .edge("r", "leaf", "child", Some("leaf"));
    let migration = identity_migration(&schema);
    let plan = compile(&schema, &schema, &migration).unwrap();
    let lens = graph_lens(&schema, &schema, &migration, &plan);
    let source = Tree {
        nodes: [(0, Node::new(0, "r")), (1, Node::new(1, "leaf"))].into(),
        arcs: vec![(0, 1, SEdge::new("r", "leaf", "child", Some("leaf")))],
        fans: Vec::new(),
        root: 0,
    };
    let (mut edited, _, embedding) = panproto_reference::lens::tree_get(&lens, &source)
        .unwrap()
        .unwrap();
    edited.root = 1;
    let not_a_tree = graph_put_square(&lens, &source, &edited, &embedding);
    let SquareResult::Commutes(observation) = not_a_tree else {
        panic!("restored-tree refusal did not commute: {not_a_tree:?}");
    };
    assert_eq!(
        observation.graph.put,
        Some(Err(SquareError::TreeOperational(
            TreeOperationalError::RestoredNotATree {
                reason: panproto_reference::encode::NotATree::RootHasParent { edge: 0 },
            },
        )))
    );
    assert_eq!(observation.graph.put, observation.fast.put);

    let (mut edited, _, mut embedding) = panproto_reference::lens::tree_get(&lens, &source)
        .unwrap()
        .unwrap();
    edited.nodes.insert(2, Node::new(2, "leaf"));
    edited
        .arcs
        .push((0, 2, SEdge::new("r", "leaf", "child", Some("leaf"))));
    embedding.arcs.insert(1, u32::MAX - 1);
    embedding.arcs.remove(&2);
    let exhausted = graph_put_square(&lens, &source, &edited, &embedding);
    let SquareResult::Commutes(observation) = exhausted else {
        panic!("edge-id exhaustion did not commute: {exhausted:?}");
    };
    assert_eq!(
        observation.graph.put,
        Some(Err(SquareError::TreeOperational(
            TreeOperationalError::IdsExhausted { nodes: false },
        )))
    );
    assert_eq!(observation.graph.put, observation.fast.put);
}

#[test]
fn two_contracted_siblings_follow_graph_carrier_order_against_a_manual_oracle() {
    let source_schema = Schema::default()
        .vertex("r", "obj")
        .vertex("m", "obj")
        .vertex("leaf", "obj")
        .edge("r", "m", "child", Some("m"))
        .edge("m", "leaf", "child", Some("leaf"));
    let image = SEdge::new("r", "leaf", "child", Some("leaf"));
    let target_schema = Schema::default()
        .vertex("r", "obj")
        .vertex("leaf", "obj")
        .edge("r", "leaf", "child", Some("leaf"));
    let migration = Migration {
        surviving_verts: ["r".to_owned(), "leaf".to_owned()].into(),
        resolver: [(("r".to_owned(), "leaf".to_owned()), image.clone())].into(),
        ..Migration::default()
    };
    let source = Tree {
        nodes: [
            (0, Node::new(0, "r")),
            (1, Node::new(1, "m")),
            (2, Node::new(2, "leaf")),
            (3, Node::new(3, "m")),
            (4, Node::new(4, "leaf")),
        ]
        .into(),
        arcs: vec![
            (0, 3, SEdge::new("r", "m", "child", Some("m"))),
            (3, 4, SEdge::new("m", "leaf", "child", Some("leaf"))),
            (0, 1, SEdge::new("r", "m", "child", Some("m"))),
            (1, 2, SEdge::new("m", "leaf", "child", Some("leaf"))),
        ],
        fans: Vec::new(),
        root: 0,
    };
    let plan = compile(&source_schema, &target_schema, &migration).unwrap();
    let lens = graph_lens(&source_schema, &target_schema, &migration, &plan);
    let result = graph_square(&lens, &source);
    let SquareResult::Commutes(observation) = result else {
        panic!("contracted-sibling square did not commute: {result:?}");
    };
    let expected = Graph {
        nodes: [
            (0, Node::new(0, "r")),
            (2, Node::new(2, "leaf")),
            (4, Node::new(4, "leaf")),
        ]
        .into(),
        edges: [
            (
                4,
                panproto_reference::model::GEdge {
                    src: 0,
                    tgt: 2,
                    label: image.clone(),
                    position: None,
                },
            ),
            (
                5,
                panproto_reference::model::GEdge {
                    src: 0,
                    tgt: 4,
                    label: image.clone(),
                    position: None,
                },
            ),
        ]
        .into(),
    };
    assert_eq!(observation.graph.get.as_ref().unwrap().0, expected);
    assert_eq!(observation.graph_tree_get, observation.fast_tree_get);
    assert_eq!(
        observation.fast_tree_get.as_ref().unwrap().arcs,
        vec![(0, 2, image.clone()), (0, 4, image.clone())]
    );
    assert_eq!(observation.graph_tree_put, Some(Ok(source.clone())));
    assert_eq!(observation.fast_tree_put, Some(Ok(source.clone())));
    let (tree_view, complement, embedding) = panproto_reference::lens::tree_get(&lens, &source)
        .unwrap()
        .unwrap();
    assert_eq!(tree_view.arcs, vec![(0, 2, image.clone()), (0, 4, image)]);
    assert_eq!(
        panproto_reference::lens::tree_put(&lens, &tree_view, &embedding, &complement).unwrap(),
        source
    );
}

#[test]
fn root_pruning_is_the_exact_documented_tree_domain_refusal() {
    let source_schema = Schema::default()
        .vertex("root", "obj")
        .vertex("leaf", "obj")
        .edge("root", "leaf", "child", Some("leaf"));
    let target_schema = Schema::default().vertex("leaf", "obj");
    let migration = Migration {
        surviving_verts: ["leaf".to_owned()].into(),
        ..Migration::default()
    };
    let source = Tree {
        nodes: [(0, Node::new(0, "root")), (1, Node::new(1, "leaf"))].into(),
        arcs: vec![(0, 1, SEdge::new("root", "leaf", "child", Some("leaf")))],
        fans: Vec::new(),
        root: 0,
    };
    let plan = compile(&source_schema, &target_schema, &migration).unwrap();
    let lens = graph_lens(&source_schema, &target_schema, &migration, &plan);
    let result = graph_square(&lens, &source);
    let SquareResult::Refused(observation) = result else {
        panic!("root pruning must be the typed tree-domain refusal: {result:?}");
    };
    let graph_view = &observation.graph.get.as_ref().unwrap().0;
    assert_eq!(graph_view.nodes, [(1, Node::new(1, "leaf"))].into());
    assert!(graph_view.edges.is_empty());
    assert_eq!(
        observation.fast.get,
        Err(SquareError::OutsideExactDomain(
            "the tree route prunes the distinguished root"
        ))
    );
    assert_eq!(
        observation
            .graph
            .put
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .nodes
            .len(),
        2
    );
    assert_eq!(observation.fast.put, None);
}

#[test]
fn graph_square_compares_equal_restrict_refusals_but_types_a_shape_decline() {
    let edge = SEdge::new("r", "m", "prop", Some("m"));
    let second = SEdge::new("m", "leaf", "prop", Some("leaf"));
    let source_schema = Schema::default()
        .vertex("r", "obj")
        .vertex("m", "obj")
        .vertex("leaf", "obj")
        .edge("r", "m", "prop", Some("m"))
        .edge("m", "leaf", "prop", Some("leaf"));
    let target_schema = Schema::default().vertex("r", "obj").vertex("leaf", "obj");
    let migration = Migration {
        surviving_verts: ["r".to_owned(), "leaf".to_owned()].into(),
        ..Migration::default()
    };
    let plan = compile(&source_schema, &target_schema, &migration).unwrap();
    let lens = graph_lens(&source_schema, &target_schema, &migration, &plan);
    let source = Tree {
        nodes: [
            (0, Node::new(0, "r")),
            (1, Node::new(1, "m")),
            (2, Node::new(2, "leaf")),
        ]
        .into(),
        arcs: vec![(0, 1, edge), (1, 2, second)],
        fans: Vec::new(),
        root: 0,
    };
    let result = graph_square(&lens, &source);
    let SquareResult::Commutes(observation) = result else {
        panic!("equal restrict refusals were not compared: {result:?}");
    };
    assert!(matches!(
        &observation.graph.get,
        Err(SquareError::Restrict(_))
    ));
    assert_eq!(observation.graph.get, observation.fast.get);
    assert_eq!(observation.graph.put, None);
    assert_eq!(observation.fast.put, None);

    let identity = identity_migration(&source_schema);
    let edge_free_target = Schema::default()
        .vertex("r", "obj")
        .vertex("m", "obj")
        .vertex("leaf", "obj");
    let dropping_plan = compile(&source_schema, &edge_free_target, &identity).unwrap();
    let dropping_lens = graph_lens(&source_schema, &edge_free_target, &identity, &dropping_plan);
    assert!(matches!(
        graph_square(&dropping_lens, &source),
        SquareResult::Refused(_)
    ));
}

fn record_boundary(schema: &Schema) -> panproto_reference::rel::RelSchema {
    rel::classify(
        schema,
        &BTreeSet::from(["int".to_owned()]),
        &BTreeSet::from(["record".to_owned()]),
    )
    .unwrap()
}

#[test]
fn table_square_uses_the_source_preimage_for_an_injective_edge_rename() {
    let old = SEdge::new("a", "b", "old", Some("old"));
    let new = SEdge::new("a", "b", "new", Some("new"));
    let source_schema = Schema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .edge("a", "b", "old", Some("old"));
    let target_schema = Schema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .edge("a", "b", "new", Some("new"));
    let migration = Migration {
        surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
        edge_remap: [(old.clone(), new.clone())].into(),
        ..Migration::default()
    };
    let graph_plan = compile(&source_schema, &target_schema, &migration).unwrap();
    let graph = graph_lens(&source_schema, &target_schema, &migration, &graph_plan);
    let source_boundary = record_boundary(&source_schema);
    let target_boundary = record_boundary(&target_schema);
    let table_plan = table::compile_for_tables(
        &source_schema,
        &target_schema,
        &source_boundary,
        &target_boundary,
        &migration,
    )
    .unwrap();
    let table = TableLens::new_at_boundary(
        &source_schema,
        &target_schema,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &table_plan,
        &BTreeMap::new(),
        &NoValueLayer,
    )
    .unwrap();
    let source = Table {
        rows: [
            (4, ("a".to_owned(), BTreeMap::new())),
            (9, ("b".to_owned(), BTreeMap::new())),
        ]
        .into(),
        pairs: [(old, 4, 9)].into(),
    };

    let result = table_square(&graph, &table, &source);
    let SquareResult::Commutes(observation) = result else {
        panic!("injective edge-rename square did not commute: {result:?}");
    };
    let (view, complement) = observation.fast.get.as_ref().unwrap();
    assert_eq!(view.edges[&0].label, new);
    assert!(complement.original_edges.is_empty());
    let source_graph = panproto_reference::encode::table_to_graph(&source)
        .unwrap()
        .0;
    assert_eq!(observation.graph.put, Some(Ok(source_graph.clone())));
    assert_eq!(observation.fast.put, Some(Ok(source_graph)));
}

fn contraction_table_fixture() -> (Schema, Schema, Migration, Table, SEdge) {
    let left = SEdge::new("a", "m", "left", Some("left"));
    let right = SEdge::new("m", "b", "right", Some("right"));
    let image = SEdge::new("a", "b", "joined", Some("joined"));
    let source_schema = Schema::default()
        .vertex("a", "record")
        .vertex("m", "record")
        .vertex("b", "record")
        .edge("a", "m", "left", Some("left"))
        .edge("m", "b", "right", Some("right"));
    let target_schema = Schema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .edge("a", "b", "joined", Some("joined"));
    let migration = Migration {
        surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
        resolver: [(("a".to_owned(), "b".to_owned()), image.clone())].into(),
        ..Migration::default()
    };
    let source = Table {
        rows: [
            (1, ("a".to_owned(), BTreeMap::new())),
            (2, ("m".to_owned(), BTreeMap::new())),
            (3, ("b".to_owned(), BTreeMap::new())),
        ]
        .into(),
        pairs: [(left, 1, 2), (right, 2, 3)].into(),
    };
    (source_schema, target_schema, migration, source, image)
}

#[test]
fn table_square_allocates_a_synthesized_pair_by_its_contract_key_and_puts() {
    let (source_schema, target_schema, migration, source, image) = contraction_table_fixture();
    let graph_plan = compile(&source_schema, &target_schema, &migration).unwrap();
    let graph = graph_lens(&source_schema, &target_schema, &migration, &graph_plan);
    let source_boundary = record_boundary(&source_schema);
    let target_boundary = record_boundary(&target_schema);
    let table_plan = table::compile_for_tables(
        &source_schema,
        &target_schema,
        &source_boundary,
        &target_boundary,
        &migration,
    )
    .unwrap();
    let table = TableLens::new_at_boundary(
        &source_schema,
        &target_schema,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &table_plan,
        &BTreeMap::new(),
        &NoValueLayer,
    )
    .unwrap();

    let result = table_square(&graph, &table, &source);
    let SquareResult::Commutes(observation) = result else {
        panic!("synthesized-pair square did not commute: {result:?}");
    };
    let (view, complement) = observation.fast.get.as_ref().unwrap();
    assert_eq!(view.edges.keys().copied().collect::<Vec<_>>(), vec![2]);
    assert_eq!(view.edges[&2].label, image);
    assert_eq!(
        complement.synthesized_edges,
        [(
            2,
            SynthKey::Contract {
                src: 1,
                tgt: 3,
                edge: image,
            },
        )]
        .into()
    );
    let source_graph = panproto_reference::encode::table_to_graph(&source)
        .unwrap()
        .0;
    assert_eq!(observation.graph.put, Some(Ok(source_graph.clone())));
    assert_eq!(observation.fast.put, Some(Ok(source_graph)));

    let edited = Table {
        rows: [
            (1, ("a".to_owned(), BTreeMap::new())),
            (3, ("b".to_owned(), BTreeMap::new())),
        ]
        .into(),
        pairs: BTreeSet::new(),
    };
    let refusal = table_put_square(&graph, &table, &source, &edited);
    let SquareResult::Commutes(observation) = refusal else {
        panic!("put-error square did not commute: {refusal:?}");
    };
    assert_eq!(
        observation.graph.put,
        Some(Err(SquareError::PutDomain(
            panproto_reference::lens::PutDomainError::SynthesizedEdited {
                elem: ViewRef::Edge(2),
            },
        )))
    );
    assert_eq!(observation.graph.put, observation.fast.put);
}

#[test]
fn table_square_compares_equal_refusals_and_detects_a_cross_lens_mismatch() {
    let (source_schema, mut target_schema, migration, source, _) = contraction_table_fixture();
    target_schema.edges.clear();
    let unresolved = Migration {
        surviving_verts: migration.surviving_verts,
        ..Migration::default()
    };
    let graph_plan = compile(&source_schema, &target_schema, &unresolved).unwrap();
    let graph = graph_lens(&source_schema, &target_schema, &unresolved, &graph_plan);
    let source_boundary = record_boundary(&source_schema);
    let target_boundary = record_boundary(&target_schema);
    let table_plan = table::compile_for_tables(
        &source_schema,
        &target_schema,
        &source_boundary,
        &target_boundary,
        &unresolved,
    )
    .unwrap();
    let table = TableLens::new_at_boundary(
        &source_schema,
        &target_schema,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &unresolved,
        &table_plan,
        &BTreeMap::new(),
        &NoValueLayer,
    )
    .unwrap();
    let result = table_square(&graph, &table, &source);
    let SquareResult::Commutes(observation) = result else {
        panic!("equal table refusals were not compared: {result:?}");
    };
    assert!(matches!(
        &observation.graph.get,
        Err(SquareError::Restrict(_))
    ));
    assert_eq!(observation.graph.get, observation.fast.get);
    assert_eq!(observation.graph.put, None);
    assert_eq!(observation.fast.put, None);

    let one_row_schema = Schema::default().vertex("t", "record");
    let identity = identity_migration(&one_row_schema);
    let identity_graph_plan = compile(&one_row_schema, &one_row_schema, &identity).unwrap();
    let identity_graph = graph_lens(
        &one_row_schema,
        &one_row_schema,
        &identity,
        &identity_graph_plan,
    );
    let one_row_boundary = record_boundary(&one_row_schema);
    let empty_schema = Schema::default();
    let empty_boundary = record_boundary(&empty_schema);
    let dropping = Migration::default();
    let dropping_plan = table::compile_for_tables(
        &one_row_schema,
        &empty_schema,
        &one_row_boundary,
        &empty_boundary,
        &dropping,
    )
    .unwrap();
    let dropping_table = TableLens::new_at_boundary(
        &one_row_schema,
        &empty_schema,
        TableBoundaries::new(&one_row_boundary, &empty_boundary),
        &dropping,
        &dropping_plan,
        &BTreeMap::new(),
        &NoValueLayer,
    )
    .unwrap();
    let one_row = Table {
        rows: [(9, ("t".to_owned(), BTreeMap::new()))].into(),
        ..Table::default()
    };
    assert!(matches!(
        table_square(&identity_graph, &dropping_table, &one_row),
        SquareResult::Mismatch(_)
    ));
}
