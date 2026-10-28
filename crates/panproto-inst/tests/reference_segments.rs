#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
//! Release-carrier differentials for segmented instances.
//!
//! `panproto-reference` supplies the dependency-free construction. This test
//! file only converts the same labelled inputs into production types, invokes
//! both implementations independently, and compares their exact observable
//! bytes or typed refusal fields.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_inst::segment::{
    AnySegment, ArtifactKind, Boundary, EdgeIdRun, GraphSegment, IdAllocator, IdRange,
    ManifestHeader, ManifestRole, Partition, Phase, RunJournal, SegmentId, SegmentRecord,
    SegmentSpec, TaskKey, TaskRecord, assemble_graph, assemble_table, assemble_tree, check_segment,
    partition_graph, partition_table, partition_tree, seal_full, verify_manifest, verify_segment,
};
use panproto_inst::{
    Digest, EdgeId, FInstance, GEdge, GInstance, Node, NodeShape, RowId, TreeEmbedding, WInstance,
    canonical_finstance_bytes, canonical_ginstance_bytes,
};
use panproto_reference::enumerate::segments as carrier;
use panproto_reference::model as reference_model;
use panproto_reference::segment as reference;
use panproto_schema::{Edge, InstanceShapeKind};

const GRAPH_PARTITION_CASES: usize = 290;
const SMALL_ALLOCATION_CASES: usize = 1_812;
const FIRST_MANIFEST_RECORD_OFFSET: usize = 2 + 1 + 32 + 1 + 1 + 8;

type ProductionRowGroups = BTreeMap<String, Vec<(RowId, HashMap<String, panproto_inst::Value>)>>;

fn edge_to_production(edge: &reference_model::SEdge) -> Edge {
    Edge {
        src: edge.src.as_str().into(),
        tgt: edge.tgt.as_str().into(),
        kind: edge.kind.as_str().into(),
        name: edge.name.as_deref().map(Into::into),
    }
}

fn node_to_production(node: &reference_model::Node) -> Node {
    assert!(node.value.is_none(), "segment carrier has no node values");
    assert!(
        node.discriminator.is_none(),
        "segment carrier has no discriminator"
    );
    assert!(
        node.fields.is_empty(),
        "segment carrier has no extra fields"
    );
    assert!(
        node.annotations.is_empty(),
        "segment carrier has no annotations"
    );
    let shape = match &node.shape {
        reference_model::NodeShape::Plain => NodeShape::Plain,
        reference_model::NodeShape::List => NodeShape::List,
        reference_model::NodeShape::XmlElement(tag) => NodeShape::XmlElement {
            tag: tag.as_str().into(),
        },
        reference_model::NodeShape::XmlTextSegment => NodeShape::XmlTextSegment,
        reference_model::NodeShape::HyperEdge => NodeShape::HyperEdge,
    };
    Node {
        id: node.id,
        anchor: node.anchor.as_str().into(),
        value: None,
        discriminator: None,
        extra_fields: HashMap::new(),
        position: node.position,
        shape,
        annotations: HashMap::new(),
    }
}

fn graph_to_production(graph: &reference_model::Graph) -> GInstance {
    let mut output = GInstance::new();
    for node in graph.nodes.values() {
        output = output.with_node(node_to_production(node));
    }
    for (id, edge) in &graph.edges {
        output
            .insert_edge(
                EdgeId(*id),
                GEdge {
                    src: edge.src,
                    tgt: edge.tgt,
                    edge: edge_to_production(&edge.label),
                    position: edge.position,
                },
            )
            .expect("the labelled reference graph is well formed");
    }
    output
}

fn table_to_production(table: &reference_model::Table) -> FInstance {
    let mut grouped = ProductionRowGroups::new();
    for (id, (name, cells)) in &table.rows {
        assert!(cells.is_empty(), "segment carrier has no table cells");
        grouped
            .entry(name.clone())
            .or_default()
            .push((RowId(*id), HashMap::new()));
    }
    FInstance::from_parts(
        grouped.into_iter().map(|(name, rows)| (name.into(), rows)),
        table.pairs.iter().map(|(edge, source, target)| {
            (
                edge_to_production(edge),
                vec![(RowId(*source), RowId(*target))],
            )
        }),
    )
    .expect("the labelled reference table is well formed")
}

fn tree_to_production(
    tree: &reference_model::Tree,
    embedding: &panproto_reference::encode::TreeEmbedding,
) -> (WInstance, TreeEmbedding) {
    assert!(
        tree.fans.is_empty(),
        "the release shape witness has no fans"
    );
    let tree = WInstance::new(
        tree.nodes
            .iter()
            .map(|(id, node)| (*id, node_to_production(node)))
            .collect(),
        tree.arcs
            .iter()
            .map(|(parent, child, edge)| (*parent, *child, edge_to_production(edge)))
            .collect(),
        Vec::new(),
        tree.root,
        tree.nodes[&tree.root].anchor.as_str().into(),
    );
    let embedding = TreeEmbedding {
        arcs: embedding
            .arcs
            .iter()
            .map(|(child, edge)| (*child, EdgeId(*edge)))
            .collect(),
        fans: Vec::new(),
        next_node: embedding.next_node,
        next_edge: embedding.next_edge,
    };
    (tree, embedding)
}

const fn shape_to_production(shape: reference::InstanceShapeKind) -> InstanceShapeKind {
    match shape {
        reference::InstanceShapeKind::Tree => InstanceShapeKind::Tree,
        reference::InstanceShapeKind::Table => InstanceShapeKind::Table,
        reference::InstanceShapeKind::Graph => InstanceShapeKind::Graph,
    }
}

const fn phase_to_reference(phase: Phase) -> reference::Phase {
    match phase {
        Phase::Validate => reference::Phase::Validate,
        Phase::Export => reference::Phase::Export,
        Phase::Summarize => reference::Phase::Summarize,
        Phase::Assemble => reference::Phase::Assemble,
        Phase::Emit => reference::Phase::Emit,
        Phase::Seal => reference::Phase::Seal,
    }
}

const fn task_key_to_reference(task: TaskKey) -> reference::TaskKey {
    reference::TaskKey {
        phase: phase_to_reference(task.phase),
        segment: match task.segment {
            Some(segment) => Some(reference::SegmentId(segment.0)),
            None => None,
        },
    }
}

fn partition_to_production(partition: &reference::Partition) -> Partition {
    Partition::new(
        shape_to_production(partition.shape),
        partition
            .specs
            .iter()
            .map(|spec| SegmentSpec {
                id: SegmentId(spec.id.0),
                nodes: spec
                    .nodes
                    .iter()
                    .map(|range| IdRange {
                        lo: range.lo,
                        hi: range.hi,
                    })
                    .collect(),
                alloc_nodes: spec.alloc_nodes.map(|range| IdRange {
                    lo: range.lo,
                    hi: range.hi,
                }),
                alloc_edges: spec.alloc_edges.map(|range| IdRange {
                    lo: range.lo,
                    hi: range.hi,
                }),
            })
            .collect(),
    )
}

const fn schema_digest() -> (reference::Digest, Digest) {
    (reference::Digest([23; 32]), Digest([23; 32]))
}

fn assert_segment_bytes_agree(
    reference_segments: &[reference::AnySegment],
    production_segments: &[AnySegment],
    reference_partition: &reference::Partition,
    production_partition: &Partition,
) {
    assert_eq!(reference_segments.len(), production_segments.len());
    let (reference_schema, production_schema) = schema_digest();
    for (reference_segment, production_segment) in
        reference_segments.iter().zip(production_segments)
    {
        let reference_bytes = reference::canonical_segment_bytes(
            reference_segment,
            reference_schema,
            reference_partition,
        )
        .expect("reference segment is canonical");
        let production_bytes = production_segment
            .canonical_bytes(&production_schema, production_partition)
            .expect("production segment is canonical");
        assert_eq!(production_bytes, reference_bytes);
        assert_eq!(
            production_segment
                .digest(&production_schema, production_partition)
                .expect("production segment digest")
                .0,
            reference_segment
                .digest(&reference_schema, reference_partition)
                .expect("reference segment digest")
                .0,
        );
    }
}

fn partition_and_assemble_shape(
    whole: &reference::AnyInstance,
    partition: &Partition,
) -> Vec<AnySegment> {
    match whole {
        reference::AnyInstance::Graph(graph) => {
            let graph = graph_to_production(graph);
            let segments = partition_graph(&graph, partition).expect("production graph partition");
            let assembled =
                assemble_graph(segments.clone(), partition).expect("production graph assembly");
            assert_eq!(
                canonical_ginstance_bytes(&assembled),
                canonical_ginstance_bytes(&graph)
            );
            segments.into_iter().map(AnySegment::Graph).collect()
        }
        reference::AnyInstance::Table(table) => {
            let table = table_to_production(table);
            let segments = partition_table(&table, partition).expect("production table partition");
            let assembled =
                assemble_table(segments.clone(), partition).expect("production table assembly");
            assert_eq!(
                canonical_finstance_bytes(&assembled),
                canonical_finstance_bytes(&table)
            );
            segments.into_iter().map(AnySegment::Table).collect()
        }
        reference::AnyInstance::Tree(tree, embedding) => {
            let (tree, embedding) = tree_to_production(tree, embedding);
            let segments =
                partition_tree(&tree, &embedding, partition).expect("production tree partition");
            let (assembled, assembled_embedding) =
                assemble_tree(segments.clone(), partition).expect("production tree assembly");
            assert_eq!(
                panproto_inst::segment::canonical_winstance_bytes(&assembled),
                panproto_inst::segment::canonical_winstance_bytes(&tree)
            );
            assert_eq!(assembled_embedding, embedding);
            segments.into_iter().map(AnySegment::Tree).collect()
        }
    }
}

fn seal_production_manifest(
    segments: &[AnySegment],
    partition: &Partition,
    schema: Digest,
) -> panproto_inst::segment::SegmentManifest {
    let records: Vec<_> = segments
        .iter()
        .zip(partition.specs())
        .map(|(segment, spec)| SegmentRecord::of(segment, spec, &schema))
        .collect();
    let runs: Vec<EdgeIdRun<'_>> = segments
        .iter()
        .map(|segment| {
            (
                segment.segment(),
                Box::new(segment.edge_id_run().into_iter()) as Box<dyn Iterator<Item = EdgeId>>,
            )
        })
        .collect();
    let sources = segments
        .iter()
        .map(|segment| {
            Box::new(segment.records()) as Box<dyn panproto_inst::segment::RecordSource + '_>
        })
        .collect();
    seal_full(
        records,
        ManifestHeader {
            shape: partition.shape(),
            schema,
            role: ManifestRole::Source,
            binding: None,
        },
        runs,
        sources,
    )
    .expect("production manifest")
}

fn subsets(values: &[u32]) -> Vec<BTreeSet<u32>> {
    (0..(1_usize << values.len()))
        .map(|mask| {
            values
                .iter()
                .enumerate()
                .filter_map(|(bit, value)| ((mask >> bit) & 1 == 1).then_some(*value))
                .collect()
        })
        .collect()
}

#[test]
fn bounded_graph_partition_glue_and_segment_bytes_agree_exhaustively() {
    let cases = carrier::graph_partition_cases();
    assert_eq!(cases.len(), GRAPH_PARTITION_CASES);
    assert_eq!(
        carrier::graph_partition_case_count(),
        u64::try_from(GRAPH_PARTITION_CASES).unwrap()
    );

    for case in cases {
        let reference_whole = reference::AnyInstance::Graph(case.graph.clone());
        let production_graph = graph_to_production(&case.graph);
        let production_partition = partition_to_production(&case.partition);
        case.partition.validate().expect("reference partition");
        production_partition
            .validate()
            .expect("production partition");

        for id in 0..=2 {
            assert_eq!(
                production_partition.owner_of_node(id).map(|owner| owner.0),
                case.partition.owner_of_node(id).map(|owner| owner.0)
            );
        }

        let reference_segments = reference::partition(&reference_whole, &case.partition)
            .expect("reference graph partition");
        let production_segments: Vec<AnySegment> =
            partition_graph(&production_graph, &production_partition)
                .expect("production graph partition")
                .into_iter()
                .map(AnySegment::Graph)
                .collect();
        assert_segment_bytes_agree(
            &reference_segments,
            &production_segments,
            &case.partition,
            &production_partition,
        );

        assert_eq!(
            reference::glue(&reference_segments, &case.partition),
            Ok(reference_whole.clone())
        );
        assert_eq!(
            reference::segmented(
                &reference_segments,
                &case.partition,
                &|value: &reference::AnyInstance| value.clone(),
            ),
            Ok(reference_whole)
        );
        let production_graph_segments =
            production_segments
                .into_iter()
                .map(|segment| match segment {
                    AnySegment::Graph(segment) => segment,
                    _ => unreachable!("graph partition returned graph segments"),
                });
        let assembled = assemble_graph(production_graph_segments, &production_partition)
            .expect("production graph assembly");
        assert_eq!(
            canonical_ginstance_bytes(&assembled),
            canonical_ginstance_bytes(&production_graph)
        );
    }
}

#[test]
fn graph_table_and_tree_artifacts_match_the_reference_exactly() {
    for (whole, reference_partition) in carrier::shape_witnesses() {
        let production_partition = partition_to_production(&reference_partition);
        let reference_segments =
            reference::partition(&whole, &reference_partition).expect("reference shape partition");
        assert_eq!(
            reference::glue(&reference_segments, &reference_partition),
            Ok(whole.clone())
        );
        assert_eq!(
            reference::segmented(
                &reference_segments,
                &reference_partition,
                &|value: &reference::AnyInstance| value.clone(),
            ),
            Ok(whole.clone())
        );

        let production_segments = partition_and_assemble_shape(&whole, &production_partition);
        assert_segment_bytes_agree(
            &reference_segments,
            &production_segments,
            &reference_partition,
            &production_partition,
        );

        let (reference_schema, production_schema) = schema_digest();
        let reference_manifest = reference::seal_manifest(
            &reference_segments,
            &reference_partition,
            reference_schema,
            reference::ManifestRole::Source,
            None,
        )
        .expect("reference manifest");
        reference::verify_manifest(&reference_manifest).expect("reference manifest verifies");
        let production_manifest = seal_production_manifest(
            &production_segments,
            &production_partition,
            production_schema,
        );
        verify_manifest(&production_manifest).expect("production manifest verifies");
        for segment in &production_segments {
            verify_segment(&production_manifest, segment).expect("production segment verifies");
        }
        let expected_payload_bytes = match &whole {
            reference::AnyInstance::Table(_) => [8_u64, 8],
            reference::AnyInstance::Graph(_) | reference::AnyInstance::Tree(_, _) => [16_u64, 16],
        };
        assert_eq!(
            production_manifest
                .segments
                .iter()
                .map(|record| record.stats.payload_bytes)
                .collect::<Vec<_>>(),
            expected_payload_bytes
        );
        assert_eq!(
            reference_manifest
                .segments
                .iter()
                .map(|record| record.stats.payload_bytes)
                .collect::<Vec<_>>(),
            expected_payload_bytes
        );

        let production_manifest_bytes = production_manifest.canonical_bytes();
        let reference_manifest_bytes = reference::canonical_manifest_bytes(&reference_manifest);
        let first_record_id = 0_u32.to_le_bytes();
        assert_eq!(
            production_manifest_bytes
                .get(FIRST_MANIFEST_RECORD_OFFSET..FIRST_MANIFEST_RECORD_OFFSET + 4),
            Some(first_record_id.as_slice())
        );
        assert_eq!(
            reference_manifest_bytes
                .get(FIRST_MANIFEST_RECORD_OFFSET..FIRST_MANIFEST_RECORD_OFFSET + 4),
            Some(first_record_id.as_slice())
        );
        assert_eq!(production_manifest_bytes, reference_manifest_bytes);
        assert_eq!(production_manifest.root.0, reference_manifest.root.0);
        assert_eq!(
            production_manifest.instance_digest.map(|digest| digest.0),
            reference_manifest.instance_digest.map(|digest| digest.0)
        );
    }
}

#[test]
fn append_floor_and_exhaustion_agree_on_an_independently_counted_carrier() {
    let mut checked = 0_usize;
    for lo in 0..=4 {
        for hi in lo..=4 {
            let values: Vec<u32> = (lo..=hi).collect();
            for source in subsets(&values) {
                for used in subsets(&values) {
                    let reference = reference::fresh(reference::IdRange { lo, hi }, &source, &used);
                    let production =
                        IdAllocator::from_source(IdRange { lo, hi }, source.iter().copied())
                            .fresh(used.iter().copied())
                            .ok();
                    assert_eq!(production, reference);
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, SMALL_ALLOCATION_CASES);

    let top_cases = [
        (u32::MAX - 1, u32::MAX, BTreeSet::new(), BTreeSet::new()),
        (
            u32::MAX - 1,
            u32::MAX,
            BTreeSet::from([u32::MAX - 1]),
            BTreeSet::new(),
        ),
        (u32::MAX, u32::MAX, BTreeSet::new(), BTreeSet::new()),
    ];
    for (lo, hi, source, used) in top_cases {
        assert_eq!(
            IdAllocator::from_source(IdRange { lo, hi }, source.iter().copied())
                .fresh(used.iter().copied())
                .ok(),
            reference::fresh(reference::IdRange { lo, hi }, &source, &used)
        );
    }
}

fn assert_partition_refusals() {
    let reference_overlap = reference::Partition {
        shape: reference::InstanceShapeKind::Graph,
        specs: vec![
            reference::SegmentSpec {
                id: reference::SegmentId(0),
                nodes: vec![reference::IdRange { lo: 0, hi: 2 }],
                alloc_nodes: None,
                alloc_edges: None,
            },
            reference::SegmentSpec {
                id: reference::SegmentId(1),
                nodes: vec![reference::IdRange { lo: 2, hi: 3 }],
                alloc_nodes: None,
                alloc_edges: None,
            },
        ],
    };
    let production_overlap = partition_to_production(&reference_overlap);
    assert_eq!(
        production_overlap.validate().unwrap_err().kind(),
        reference_overlap.validate().unwrap_err().external_name()
    );

    // The malformed allocation payload is reported before the fact that a
    // table must not carry an edge allocation range at all.
    let reference_inverted_table_allocation = reference::Partition {
        shape: reference::InstanceShapeKind::Table,
        specs: vec![reference::SegmentSpec {
            id: reference::SegmentId(0),
            nodes: vec![reference::IdRange { lo: 0, hi: 9 }],
            alloc_nodes: None,
            alloc_edges: Some(reference::IdRange { lo: 29, hi: 20 }),
        }],
    };
    assert_eq!(
        reference_inverted_table_allocation.validate(),
        Err(reference::SegmentError::InvertedRange {
            segment: reference::SegmentId(0),
            range: reference::IdRange { lo: 29, hi: 20 },
        })
    );
    assert_eq!(
        partition_to_production(&reference_inverted_table_allocation).validate(),
        Err(panproto_inst::segment::SegmentError::InvertedRange {
            segment: SegmentId(0),
            range: IdRange { lo: 29, hi: 20 },
        })
    );

    let mut graph = reference_model::Graph::default();
    graph.nodes.insert(4, reference_model::Node::new(4, "a"));
    let reference_partition = reference::Partition {
        shape: reference::InstanceShapeKind::Graph,
        specs: vec![reference::SegmentSpec {
            id: reference::SegmentId(0),
            nodes: vec![reference::IdRange { lo: 0, hi: 3 }],
            alloc_nodes: None,
            alloc_edges: None,
        }],
    };
    let production_partition = partition_to_production(&reference_partition);
    let reference_error = reference::partition(
        &reference::AnyInstance::Graph(graph.clone()),
        &reference_partition,
    )
    .unwrap_err();
    let production_error =
        partition_graph(&graph_to_production(&graph), &production_partition).unwrap_err();
    assert_eq!(production_error.kind(), reference_error.external_name());
    assert_eq!(
        production_error,
        panproto_inst::segment::SegmentError::Unowned {
            elem: panproto_inst::ViewRef::Node(4)
        }
    );
}

fn assert_decoder_and_contract_refusals() {
    let valid_reference = reference::Partition {
        shape: reference::InstanceShapeKind::Graph,
        specs: vec![reference::SegmentSpec {
            id: reference::SegmentId(0),
            nodes: vec![reference::IdRange { lo: 0, hi: 0 }],
            alloc_nodes: None,
            alloc_edges: None,
        }],
    };
    let valid_production = partition_to_production(&valid_reference);
    let reference_segment = reference::partition(
        &reference::AnyInstance::Graph(reference_model::Graph {
            nodes: BTreeMap::from([(0, reference_model::Node::new(0, "a"))]),
            edges: BTreeMap::new(),
        }),
        &valid_reference,
    )
    .unwrap()
    .remove(0);
    let production_segment = partition_graph(
        &graph_to_production(&reference_model::Graph {
            nodes: BTreeMap::from([(0, reference_model::Node::new(0, "a"))]),
            edges: BTreeMap::new(),
        }),
        &valid_production,
    )
    .unwrap()
    .remove(0);
    let (reference_schema, production_schema) = schema_digest();
    let mut reference_bytes =
        reference::canonical_segment_bytes(&reference_segment, reference_schema, &valid_reference)
            .unwrap();
    let mut production_bytes = AnySegment::Graph(production_segment)
        .canonical_bytes(&production_schema, &valid_production)
        .unwrap();
    reference_bytes.push(0);
    production_bytes.push(0);
    assert_eq!(
        AnySegment::from_canonical_bytes(&production_bytes, &valid_production)
            .unwrap_err()
            .kind(),
        reference::decode_segment_bytes(&reference_bytes, &valid_reference)
            .unwrap_err()
            .external_name()
    );

    let reference_invalid = reference::AnySegment::Graph(reference::GraphSegment {
        segment: reference::SegmentId(0),
        nodes: BTreeMap::from([(0, reference_model::Node::new(1, "a"))]),
        edges: BTreeMap::new(),
        boundary: reference::Boundary::default(),
    });
    let production_invalid = AnySegment::Graph(GraphSegment {
        segment: SegmentId(0),
        nodes: BTreeMap::from([(0, node_to_production(&reference_model::Node::new(1, "a")))]),
        edges: BTreeMap::new(),
        boundary: Boundary::default(),
    });
    let reference_error =
        reference::validate_segment_contract(&reference_invalid, &valid_reference).unwrap_err();
    let production_error = check_segment(&production_invalid, &valid_production).unwrap_err();
    assert_eq!(
        reference_error,
        reference::SegmentError::Invalid {
            violations: vec![reference::SegmentError::NodeIdMismatch { key: 0, found: 1 }],
        }
    );
    assert_eq!(
        production_error,
        panproto_inst::segment::SegmentError::Invalid {
            violations: vec![panproto_inst::segment::SegmentError::NodeIdMismatch {
                key: 0,
                found: 1,
            }],
        }
    );
    assert_eq!(
        reference::validation_view_preflight(
            reference::SegmentId(7),
            reference::VALIDATION_VIEW_EDGE_CAPACITY,
            0,
        ),
        Ok(reference::VALIDATION_VIEW_EDGE_CAPACITY)
    );
    assert_eq!(
        reference::validation_view_preflight(
            reference::SegmentId(7),
            reference::VALIDATION_VIEW_EDGE_CAPACITY,
            1,
        ),
        Err(reference::SegmentError::ValidationViewUnrepresentable {
            segment: reference::SegmentId(7),
            edges: reference::VALIDATION_VIEW_EDGE_CAPACITY + 1,
        })
    );
}

#[test]
fn partition_and_decoder_refusals_preserve_typed_error_precedence() {
    assert_partition_refusals();
    assert_decoder_and_contract_refusals();
}

#[test]
fn storage_vocabulary_serde_layout_and_run_identity_are_stable() {
    for (phase, expected) in [
        (Phase::Validate, "validate"),
        (Phase::Export, "export"),
        (Phase::Summarize, "summarize"),
        (Phase::Assemble, "assemble"),
        (Phase::Emit, "emit"),
        (Phase::Seal, "seal"),
    ] {
        assert_eq!(serde_json::to_value(phase).unwrap(), expected);
    }
    for (kind, expected) in [
        (ArtifactKind::Segment, "segment"),
        (ArtifactKind::ComplementSegment, "complement_segment"),
        (ArtifactKind::Export, "export"),
        (ArtifactKind::Summary, "summary"),
        (ArtifactKind::Assembly, "assembly"),
        (ArtifactKind::Manifest, "manifest"),
        (ArtifactKind::EdgeIdRun, "edge_id_run"),
    ] {
        assert_eq!(serde_json::to_value(kind).unwrap(), expected);
    }

    let production_run = Digest([31; 32]);
    let production_tasks = BTreeMap::from([
        (
            TaskKey {
                phase: Phase::Validate,
                segment: None,
            },
            TaskRecord {
                inputs: Digest([41; 32]),
                output: Digest([42; 32]),
                bytes: 43,
            },
        ),
        (
            TaskKey {
                phase: Phase::Emit,
                segment: Some(SegmentId(2)),
            },
            TaskRecord {
                inputs: Digest([51; 32]),
                output: Digest([52; 32]),
                bytes: 53,
            },
        ),
    ]);
    let mut production_journal = RunJournal::new(production_run);
    production_journal.tasks = production_tasks;

    let json = serde_json::to_value(&production_journal).unwrap();
    let tasks = json["tasks"].as_array().unwrap();
    assert_eq!(tasks[0][0]["phase"], "validate");
    assert!(tasks[0][0]["segment"].is_null());
    assert_eq!(tasks[1][0]["phase"], "emit");
    assert_eq!(tasks[1][0]["segment"], 2);

    let base_partition = Partition::new(
        InstanceShapeKind::Graph,
        vec![SegmentSpec {
            id: SegmentId(0),
            nodes: vec![IdRange { lo: 0, hi: 0 }],
            alloc_nodes: None,
            alloc_edges: None,
        }],
    );
    let changed_partition = Partition::new(
        InstanceShapeKind::Graph,
        vec![SegmentSpec {
            id: SegmentId(0),
            nodes: vec![IdRange { lo: 0, hi: 1 }],
            alloc_nodes: None,
            alloc_edges: None,
        }],
    );
    let lens = Digest([61; 32]);
    let input = Digest([62; 32]);
    let run = RunJournal::run_digest(&lens, &input, &base_partition, b"get");
    assert_ne!(
        run,
        RunJournal::run_digest(&Digest([63; 32]), &input, &base_partition, b"get")
    );
    assert_ne!(
        run,
        RunJournal::run_digest(&lens, &Digest([64; 32]), &base_partition, b"get")
    );
    assert_ne!(
        run,
        RunJournal::run_digest(&lens, &input, &changed_partition, b"get")
    );
    assert_ne!(
        run,
        RunJournal::run_digest(&lens, &input, &base_partition, b"put")
    );
}

#[test]
fn bounded_task_input_digest_and_journal_reuse_match_the_reference() {
    let phases = [
        Phase::Validate,
        Phase::Export,
        Phase::Summarize,
        Phase::Assemble,
        Phase::Emit,
        Phase::Seal,
    ];
    let task_carrier: Vec<_> = phases
        .into_iter()
        .flat_map(|phase| {
            [None, Some(SegmentId(0)), Some(SegmentId(u32::MAX))]
                .map(|segment| TaskKey { phase, segment })
        })
        .collect();
    assert_eq!(task_carrier.len(), 18);

    let a = Digest([11; 32]);
    let b = Digest([12; 32]);
    let input_carrier = [Vec::new(), vec![a], vec![a, b], vec![b, a], vec![a, a]];
    let lenses = [Digest([21; 32]), Digest([22; 32])];
    let mut digest_cases = 0;
    let mut digest_observations = Vec::new();
    for lens in lenses {
        for task in &task_carrier {
            for inputs in &input_carrier {
                let reference_inputs: Vec<_> = inputs
                    .iter()
                    .map(|digest| reference::Digest(digest.0))
                    .collect();
                let production = TaskRecord::inputs_digest(&lens, *task, inputs);
                let expected = reference::task_inputs_digest(
                    &reference::Digest(lens.0),
                    task_key_to_reference(*task),
                    &reference_inputs,
                );
                assert_eq!(
                    production.0, expected.0,
                    "task-input digest disagrees for lens {lens:?}, task {task:?}, inputs {inputs:?}"
                );
                digest_observations.push((production, expected));
                digest_cases += 1;
            }
        }
    }
    assert_eq!(digest_cases, 180);
    let mut observation_groups = digest_observations.chunks_exact(input_carrier.len());
    for observations in &mut observation_groups {
        assert_eq!(
            observations[2].0, observations[3].0,
            "production task-input digest must ignore input order"
        );
        assert_eq!(
            observations[2].1, observations[3].1,
            "reference task-input digest must ignore input order"
        );
        assert_ne!(
            observations[1].0, observations[4].0,
            "production task-input digest must preserve duplicate multiplicity"
        );
        assert_ne!(
            observations[1].1, observations[4].1,
            "reference task-input digest must preserve duplicate multiplicity"
        );
    }
    assert!(observation_groups.remainder().is_empty());

    let mut production_tasks = BTreeMap::new();
    let mut reference_tasks = BTreeMap::new();
    for (index, task) in task_carrier.iter().copied().enumerate() {
        let index = u8::try_from(index).unwrap();
        let inputs = Digest([80 + index; 32]);
        let output = Digest([110 + index; 32]);
        let bytes = u64::from(index) * 17;
        production_tasks.insert(
            task,
            TaskRecord {
                inputs,
                output,
                bytes,
            },
        );
        reference_tasks.insert(
            task_key_to_reference(task),
            reference::TaskRecord {
                inputs: reference::Digest(inputs.0),
                output: reference::Digest(output.0),
                bytes,
            },
        );
    }
    let production_journal = RunJournal {
        run: Digest([140; 32]),
        tasks: production_tasks,
    };
    let reference_journal = reference::RunJournal {
        run: reference::Digest([140; 32]),
        tasks: reference_tasks,
    };
    let compare_reuse = |task: TaskKey, inputs: Digest, output: Digest, expected: bool| {
        let production = production_journal.reusable(&task, &inputs, &output);
        let reference = reference::journal_task_reusable(
            &reference_journal,
            &task_key_to_reference(task),
            &reference::Digest(inputs.0),
            &reference::Digest(output.0),
        );
        assert_eq!(production, expected, "unexpected reuse result for {task:?}");
        assert_eq!(production, reference, "reuse disagrees for {task:?}");
    };

    let wrong_inputs = Digest([240; 32]);
    let wrong_output = Digest([241; 32]);
    let mut reuse_cases = 0;
    for task in &task_carrier {
        let record = production_journal.tasks[task];
        compare_reuse(*task, record.inputs, record.output, true);
        compare_reuse(*task, wrong_inputs, record.output, false);
        compare_reuse(*task, record.inputs, wrong_output, false);
        reuse_cases += 3;
    }
    compare_reuse(
        TaskKey {
            phase: Phase::Seal,
            segment: Some(SegmentId(1)),
        },
        wrong_inputs,
        wrong_output,
        false,
    );
    reuse_cases += 1;
    assert_eq!(reuse_cases, 55);
}

#[test]
fn release_carrier_has_no_unstated_scope_expansion() {
    assert_eq!(carrier::FULL_SCOPE_GAPS.len(), 12);
    assert_eq!(carrier::graph_count(), 25);
    assert_eq!(carrier::graph_partition_case_count(), 290);
    assert_eq!(carrier::shape_witnesses().len(), 3);
    assert_eq!(reference::VALIDATION_VIEW_EDGE_CAPACITY, 1_u64 << 32);
    assert_eq!(
        reference::validation_view_preflight(
            reference::SegmentId(7),
            reference::VALIDATION_VIEW_EDGE_CAPACITY,
            0,
        ),
        Ok(reference::VALIDATION_VIEW_EDGE_CAPACITY)
    );
    assert_eq!(
        reference::validation_view_preflight(
            reference::SegmentId(7),
            reference::VALIDATION_VIEW_EDGE_CAPACITY,
            1,
        ),
        Err(reference::SegmentError::ValidationViewUnrepresentable {
            segment: reference::SegmentId(7),
            edges: reference::VALIDATION_VIEW_EDGE_CAPACITY + 1,
        })
    );
    assert_eq!(reference::SEGMENT_FORMAT_VERSION, 1);
    assert_eq!(reference::SEGMENT_MANIFEST_VERSION, 1);
}
