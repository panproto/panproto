//! The acceptance fixtures in `tests/fixtures/segments/`.
//!
//! Each fixture holds a graph instance, a partition of it and a schema
//! digest, and the digests a segmented run must reproduce: the streaming
//! instance digest, each segment's content digest, and the sealed
//! manifest's root and digest. The test splits the instance, checks that
//! assembling the parts gives it back, recomputes every digest and compares
//! it with the file. Regenerate the files after a deliberate encoding
//! change with
//!
//! ```sh
//! PP_SEGMENT_FIXTURES=bless cargo nextest run -p panproto-inst -E 'test(segment_acceptance_fixtures)'
//! ```

use std::path::PathBuf;

use panproto_schema::{Edge, InstanceShapeKind};
use serde::{Deserialize, Serialize};

use super::{TestResult, label};
use crate::ginstance::{EdgeId, GEdge, GInstance};
use crate::identity::Digest;
use crate::metadata::{Node, NodeShape};
use crate::segment::{
    AnySegment, IdRange, ManifestHeader, ManifestRole, Partition, RecordSource, SegmentId,
    SegmentRecord, SegmentSpec, assemble_graph, instance_digest, partition_graph, seal,
    verify_manifest, verify_segment,
};
use crate::value::{FieldPresence, Value};
use crate::view_ref::ViewRef;

/// One acceptance fixture as it is stored.
#[derive(Debug, Serialize, Deserialize)]
struct Fixture {
    /// What the fixture exercises.
    description: String,
    /// The schema digest the segments are written under.
    schema: Digest,
    /// The partition.
    partition: Partition,
    /// The whole instance.
    instance: GInstance,
    /// The digests a segmented run reproduces.
    expected: Expected,
}

/// The digests of a fixture.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Expected {
    /// The streaming instance digest, equal to the whole instance's.
    instance_digest: Digest,
    /// Each segment's content digest, by segment id.
    segments: Vec<Digest>,
    /// The sealed manifest's Merkle root.
    manifest_root: Digest,
    /// The sealed manifest's digest.
    manifest_digest: Digest,
}

/// A partition of node ranges, with no allocation ranges.
fn ranges(specs: &[&[(u32, u32)]]) -> Partition {
    Partition::new(
        InstanceShapeKind::Graph,
        specs
            .iter()
            .zip(0u32..)
            .map(|(nodes, id)| SegmentSpec {
                id: SegmentId(id),
                nodes: nodes.iter().map(|&(lo, hi)| IdRange { lo, hi }).collect(),
                alloc_nodes: None,
                alloc_edges: None,
            })
            .collect(),
    )
}

/// A graph from nodes and `(id, src, tgt, edge, position)` edges.
fn graph(
    nodes: Vec<Node>,
    edges: Vec<(u32, u32, u32, panproto_schema::Edge, Option<u32>)>,
) -> GInstance {
    let mut g = GInstance::new();
    for n in nodes {
        g = g.with_node(n);
    }
    for (id, src, tgt, edge, position) in edges {
        g.insert_edge(
            EdgeId(id),
            GEdge {
                src,
                tgt,
                edge,
                position,
            },
        )
        .unwrap_or_else(|_| unreachable!("fixture edges are well formed"));
    }
    g
}

/// The inputs of each fixture: name, description, partition and instance.
fn inputs() -> Vec<(&'static str, &'static str, Partition, GInstance)> {
    let x = label("a", "a", "prop", Some("x"));
    let anchored = |id: u32, anchor: &str| Node::new(id, anchor);
    let parallel = graph(
        vec![anchored(0, "a"), anchored(1, "a"), anchored(10, "a")],
        vec![
            (0, 0, 10, x.clone(), None),
            (1, 0, 10, x.clone(), None),
            (2, 0, 10, x.clone(), Some(0)),
            (3, 10, 1, x.clone(), None),
        ],
    );
    let cycle = graph(
        vec![anchored(0, "a"), anchored(10, "a"), anchored(20, "a")],
        vec![
            (0, 0, 10, x.clone(), None),
            (1, 10, 20, x.clone(), None),
            (2, 20, 0, x.clone(), None),
        ],
    );
    let member = |r: &str| label("rel", "a", "hyper_label", Some(r));
    let relation = graph(
        vec![
            Node::new(1, "rel").with_shape(NodeShape::HyperEdge),
            anchored(2, "a"),
            anchored(12, "a"),
            anchored(22, "a"),
        ],
        vec![
            (0, 1, 2, member("agent"), None),
            (1, 1, 12, member("patient"), None),
            (2, 1, 22, member("theme"), None),
        ],
    );
    let values = values_instance(x);
    let anchors = anchors_instance();
    vec![
        (
            "parallel_edges_across_a_boundary",
            "three parallel edges, one positioned, from segment 0 into one node of segment 1, and one edge back",
            ranges(&[&[(0, 9)], &[(10, 19)]]),
            parallel,
        ),
        (
            "cycle_through_three_segments",
            "a directed cycle 0 -> 10 -> 20 -> 0 with one node in each of three segments",
            ranges(&[&[(0, 9)], &[(10, 19)], &[(20, 29)]]),
            cycle,
        ),
        (
            "reified_relation_with_members_in_three_segments",
            "a reified relation node whose labelled member edges reach one member in each of three segments",
            ranges(&[&[(0, 9)], &[(10, 19)], &[(20, 29)]]),
            relation,
        ),
        (
            "values_and_extra_fields_round_trip",
            "present, null and absent values, extra fields, annotations, a discriminator and positions on nodes split across two segments",
            ranges(&[&[(0, 9)], &[(10, 19)]]),
            values,
        ),
        (
            "anchors_round_trip",
            "four anchors and four edge labels, with ghosts and in-stubs carrying the remote anchors, under interleaved ranges",
            ranges(&[&[(0, 0), (20, 20)], &[(1, 19)]]),
            anchors,
        ),
    ]
}

/// The instance of `values_and_extra_fields_round_trip`: present, null and
/// absent values, extra fields, annotations, a discriminator and positions.
fn values_instance(x: Edge) -> GInstance {
    let mut present = Node::new(0, "a");
    present.value = Some(FieldPresence::Present(Value::Str("héllo".into())));
    present.extra_fields.insert("count".into(), Value::Int(-3));
    present
        .extra_fields
        .insert("ratio".into(), Value::Float(0.25));
    present.annotations.insert(
        "note".into(),
        Value::List(vec![Value::Bool(true), Value::Null]),
    );
    present.position = Some(2);
    let mut absent = Node::new(10, "a");
    absent.value = Some(FieldPresence::Absent);
    absent.discriminator = Some("kind".into());
    let mut null = Node::new(11, "a");
    null.value = Some(FieldPresence::Null);
    null.extra_fields
        .insert("bytes".into(), Value::Bytes(vec![0, 255, 7]));
    graph(
        vec![present, absent, null],
        vec![(0, 0, 10, x.clone(), Some(1)), (1, 11, 0, x, None)],
    )
}

/// The instance of `anchors_round_trip`: four anchors and four edge labels.
fn anchors_instance() -> GInstance {
    let anchored = |id: u32, anchor: &str| Node::new(id, anchor);
    graph(
        vec![
            anchored(0, "person"),
            anchored(1, "place"),
            anchored(10, "event"),
            anchored(20, "person"),
        ],
        vec![
            (
                0,
                0,
                10,
                label("person", "event", "prop", Some("attends")),
                None,
            ),
            (1, 10, 1, label("event", "place", "prop", Some("at")), None),
            (
                2,
                10,
                20,
                label("event", "person", "prop", Some("host")),
                None,
            ),
            (
                3,
                20,
                0,
                label("person", "person", "prop", Some("knows")),
                None,
            ),
        ],
    )
}

/// The directory of the fixtures.
fn directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/segments")
}

/// The digests of `instance` split by `partition` under `schema`, after
/// checking that the parts assemble back to it and that the sealed manifest
/// verifies with every part.
fn compute(
    schema: Digest,
    partition: &Partition,
    instance: &GInstance,
) -> Result<(Expected, Vec<AnySegment>), Box<dyn std::error::Error>> {
    let parts: Vec<AnySegment> = partition_graph(instance, partition)?
        .into_iter()
        .map(AnySegment::Graph)
        .collect();
    let graphs = parts.iter().filter_map(|p| match p {
        AnySegment::Graph(g) => Some(g.clone()),
        _ => None,
    });
    assert!(GInstance::identical(
        &assemble_graph(graphs, partition)?,
        instance
    ));
    let sources = parts
        .iter()
        .map(|p| Box::new(p.records()) as Box<dyn RecordSource>)
        .collect();
    let streamed = instance_digest(InstanceShapeKind::Graph, sources)?;
    assert_eq!(streamed, instance.digest());
    let records: Vec<SegmentRecord> = parts
        .iter()
        .map(|p| SegmentRecord::of(p, &partition.specs()[p.segment().0 as usize], &schema))
        .collect();
    let runs = parts
        .iter()
        .map(|p| {
            (
                p.segment(),
                Box::new(p.edge_id_run().into_iter()) as Box<dyn Iterator<Item = EdgeId>>,
            )
        })
        .collect();
    let header = ManifestHeader {
        shape: InstanceShapeKind::Graph,
        schema,
        role: ManifestRole::Source,
        binding: None,
    };
    let manifest = seal(records, header, runs)?;
    verify_manifest(&manifest)?;
    for part in &parts {
        verify_segment(&manifest, part)?;
    }
    let expected = Expected {
        instance_digest: streamed,
        segments: manifest.segments.iter().map(|r| r.content).collect(),
        manifest_root: manifest.root,
        manifest_digest: manifest.digest(),
    };
    Ok((expected, parts))
}

#[test]
fn segment_acceptance_fixtures() -> TestResult {
    let bless = std::env::var("PP_SEGMENT_FIXTURES").is_ok_and(|v| v == "bless");
    let schema = Digest::derive(
        crate::identity::SEGMENT_CONTEXT,
        b"acceptance fixture schema",
    );
    for (name, description, partition, instance) in inputs() {
        let path = directory().join(format!("{name}.json"));
        let (expected, parts) = compute(schema, &partition, &instance)?;
        if bless {
            let fixture = Fixture {
                description: description.to_owned(),
                schema,
                partition,
                instance,
                expected,
            };
            std::fs::write(&path, serde_json::to_string_pretty(&fixture)? + "\n")?;
            continue;
        }
        let stored: Fixture = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
        assert_eq!(stored.description, description, "{name}");
        assert_eq!(stored.schema, schema, "{name}");
        assert_eq!(stored.partition, partition, "{name}");
        assert!(
            GInstance::identical(&stored.instance, &instance),
            "{name}: the stored instance"
        );
        let (recomputed, _) = compute(stored.schema, &stored.partition, &stored.instance)?;
        assert_eq!(recomputed, stored.expected, "{name}");
        assert_eq!(expected, stored.expected, "{name}");
        check_property(name, &parts);
    }
    Ok(())
}

/// The property each fixture is named for.
fn check_property(name: &str, parts: &[AnySegment]) {
    let stubs = |i: usize| {
        parts[i]
            .boundary()
            .in_stubs
            .keys()
            .cloned()
            .collect::<Vec<_>>()
    };
    match name {
        "parallel_edges_across_a_boundary" => {
            // Three parallel edges, one ghost, three in-stubs.
            assert_eq!(parts[0].boundary().ghosts.len(), 1);
            assert_eq!(
                stubs(1),
                vec![
                    ViewRef::Edge(EdgeId(0)),
                    ViewRef::Edge(EdgeId(1)),
                    ViewRef::Edge(EdgeId(2))
                ]
            );
        }
        "cycle_through_three_segments" => {
            assert_eq!(parts.len(), 3);
            for (i, part) in parts.iter().enumerate() {
                let record = part.cross_out();
                assert_eq!(
                    record.keys().copied().collect::<Vec<_>>(),
                    vec![SegmentId(u32::try_from((i + 1) % 3).unwrap_or(0))]
                );
            }
        }
        "reified_relation_with_members_in_three_segments" => {
            assert_eq!(
                parts[0]
                    .boundary()
                    .ghosts
                    .keys()
                    .copied()
                    .collect::<Vec<_>>(),
                vec![12, 22]
            );
            assert_eq!(stubs(1), vec![ViewRef::Edge(EdgeId(1))]);
            assert_eq!(stubs(2), vec![ViewRef::Edge(EdgeId(2))]);
        }
        "anchors_round_trip" => {
            let ghosts = &parts[0].boundary().ghosts;
            assert_eq!(ghosts[&10].anchor.as_ref(), "event");
            let stub = &parts[0].boundary().in_stubs[&ViewRef::Edge(EdgeId(2))];
            assert_eq!(stub.src_anchor.as_ref(), "event");
        }
        _ => {}
    }
}
