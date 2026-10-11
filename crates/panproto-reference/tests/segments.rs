//! Independent SG.1--SG.3, SG.6--SG.7, SG.9, S10 and S10a witnesses.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::convert::Infallible;
use std::mem::discriminant;

use panproto_reference::canonical::{
    Reader, Writer, graph_bytes, read_graph_edge, table_bytes, tree_bytes, write_graph_edge,
};
use panproto_reference::enumerate::segments as bounded;
use panproto_reference::migration::Migration;
use panproto_reference::model::{Fan, GEdge, Graph, Node, SEdge, Tree};
use panproto_reference::pages::lenses::table::{DroppedRow, TableComplement};
use panproto_reference::pages::lenses::{
    GraphShape, LensError, LensShape, SynthIndex, TableShape, TreeOperationalError,
};
use panproto_reference::restrict::{Complement, NoValueLayer, RestrictError, SynthKey};
use panproto_reference::schema::Schema;
use panproto_reference::segment::{
    self, AnyComplementSegment, AnyInstance, AnySegment, ArtifactKey, ArtifactKind, Boundary,
    Budget, Digest, ErrorAt, ErrorDetail, ErrorKey, Ghost, GraphComplementSegment, IdKind, IdRange,
    IdentityError, InstanceShapeKind, ManifestRole, MemoryStore, Partition, PartitionDefect, Phase,
    PutDomainError, Residual, Resource, RunJournal, RunSpec, RunStatus, SegmentError,
    SegmentFloors, SegmentId, SegmentOperationResult, SegmentSpec, SegmentStore,
    TableComplementSegment, TaskRecord, TreeComplementSegment, ViewRef,
};
use panproto_reference::value::{Presence, Value};

trait TestUnwrap<T> {
    #[track_caller]
    fn test_unwrap(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> TestUnwrap<T> for Result<T, E> {
    fn test_unwrap(self, context: &str) -> T {
        self.unwrap_or_else(|error| panic!("{context}: {error:?}"))
    }
}

impl<T> TestUnwrap<T> for Option<T> {
    fn test_unwrap(self, context: &str) -> T {
        self.unwrap_or_else(|| panic!("{context}"))
    }
}

#[derive(Debug)]
struct CommitError;

impl std::fmt::Display for CommitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("injected commit failure")
    }
}

impl std::error::Error for CommitError {}

#[derive(Default)]
struct FailFirstCommitStore {
    inner: MemoryStore,
    failed: bool,
}

impl SegmentStore for FailFirstCommitStore {
    type Error = CommitError;

    fn read(&self, key: &ArtifactKey) -> Result<Option<Vec<u8>>, Self::Error> {
        self.inner.read(key).map_err(|never| match never {})
    }

    fn stage(&mut self, key: &ArtifactKey, bytes: Vec<u8>) -> Result<(), Self::Error> {
        self.inner.stage(key, bytes).map_err(|never| match never {})
    }

    fn commit(&mut self, key: &ArtifactKey, record: &TaskRecord) -> Result<(), Self::Error> {
        if self.failed {
            return self
                .inner
                .commit(key, record)
                .map_err(|never| match never {});
        }
        self.failed = true;
        Err(CommitError)
    }

    fn journal(&self, run: &Digest) -> Result<RunJournal, Self::Error> {
        self.inner.journal(run).map_err(|never| match never {})
    }

    fn staged_uncommitted(&self, run: &Digest) -> Result<Vec<ArtifactKey>, Self::Error> {
        self.inner
            .staged_uncommitted(run)
            .map_err(|never| match never {})
    }
}

#[derive(Default)]
struct MutableStore {
    staged: BTreeMap<ArtifactKey, Vec<u8>>,
    committed: BTreeMap<ArtifactKey, Vec<u8>>,
    journals: BTreeMap<Digest, RunJournal>,
}

impl MutableStore {
    fn corrupt(&mut self, phase: Phase) {
        let bytes = self
            .committed
            .iter_mut()
            .find(|(key, _)| key.task.phase == phase)
            .map(|(_, bytes)| bytes)
            .test_unwrap("phase artifact exists");
        bytes[0] ^= 0x80;
    }
}

impl SegmentStore for MutableStore {
    type Error = Infallible;

    fn read(&self, key: &ArtifactKey) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self.committed.get(key).cloned())
    }

    fn stage(&mut self, key: &ArtifactKey, bytes: Vec<u8>) -> Result<(), Self::Error> {
        self.staged.insert(*key, bytes);
        Ok(())
    }

    fn commit(&mut self, key: &ArtifactKey, record: &TaskRecord) -> Result<(), Self::Error> {
        if let Some(bytes) = self.staged.remove(key) {
            self.committed.insert(*key, bytes);
            self.journals
                .entry(key.run)
                .or_insert_with(|| RunJournal {
                    run: key.run,
                    tasks: BTreeMap::new(),
                })
                .tasks
                .insert(key.task, *record);
        }
        Ok(())
    }

    fn journal(&self, run: &Digest) -> Result<RunJournal, Self::Error> {
        Ok(self.journals.get(run).cloned().unwrap_or(RunJournal {
            run: *run,
            tasks: BTreeMap::new(),
        }))
    }

    fn staged_uncommitted(&self, run: &Digest) -> Result<Vec<ArtifactKey>, Self::Error> {
        Ok(self
            .staged
            .keys()
            .filter(|key| key.run == *run)
            .copied()
            .collect())
    }
}

fn spec(id: u32, ranges: &[(u32, u32)]) -> SegmentSpec {
    SegmentSpec {
        id: SegmentId(id),
        nodes: ranges
            .iter()
            .map(|(lo, hi)| IdRange { lo: *lo, hi: *hi })
            .collect(),
        alloc_nodes: None,
        alloc_edges: None,
    }
}

fn graph_partition() -> Partition {
    Partition {
        shape: InstanceShapeKind::Graph,
        specs: vec![spec(0, &[(0, 0)]), spec(1, &[(1, 1)])],
    }
}

fn crossing_graph() -> Graph {
    let mut graph = Graph::default();
    graph.nodes.insert(0, Node::new(0, "a"));
    graph.nodes.insert(1, Node::new(1, "b"));
    graph.edges.insert(
        7,
        GEdge {
            src: 0,
            tgt: 1,
            label: SEdge::new("a", "b", "e", None),
            position: None,
        },
    );
    graph
}

fn two_fan_tree() -> (Tree, panproto_reference::encode::TreeEmbedding) {
    (
        Tree {
            nodes: BTreeMap::from([
                (0, Node::new(0, "root")),
                (1, Node::new(1, "left")),
                (2, Node::new(2, "right")),
            ]),
            arcs: vec![
                (0, 1, SEdge::new("root", "left", "left", None)),
                (0, 2, SEdge::new("root", "right", "right", None)),
            ],
            fans: vec![
                Fan {
                    parent: 0,
                    hyper_edge: "first".to_owned(),
                    children: BTreeMap::from([("arg".to_owned(), 1)]),
                },
                Fan {
                    parent: 0,
                    hyper_edge: "second".to_owned(),
                    children: BTreeMap::from([("arg".to_owned(), 2)]),
                },
            ],
            root: 0,
        },
        panproto_reference::encode::TreeEmbedding {
            arcs: BTreeMap::from([(1, 0), (2, 1)]),
            fans: vec![
                (5, 2, BTreeMap::from([("arg".to_owned(), 3)])),
                (4, 4, BTreeMap::from([("arg".to_owned(), 5)])),
            ],
            next_node: 6,
            next_edge: 6,
        },
    )
}

#[test]
fn exact_graph_partition_carrier_is_independently_counted() {
    let graphs = bounded::graphs();
    let cases = bounded::graph_partition_cases();
    let mut graphs_by_nodes = [0_u64; 3];
    for graph in &graphs {
        graphs_by_nodes[graph.nodes.len()] += 1;
    }
    assert_eq!(graphs_by_nodes, [1, 3, 21]);
    assert_eq!(bounded::graph_count(), 25);
    assert_eq!(u64::try_from(graphs.len()), Ok(bounded::graph_count()));

    let mut cases_by_nodes = [0_u64; 3];
    for case in &cases {
        cases_by_nodes[case.graph.nodes.len()] += 1;
    }
    assert_eq!(cases_by_nodes, [2, 15, 273]);
    assert_eq!(bounded::graph_partition_case_count(), 290);
    assert_eq!(
        u64::try_from(cases.len()),
        Ok(bounded::graph_partition_case_count())
    );
    for (index, left) in cases.iter().enumerate() {
        assert_eq!(
            cases[index + 1..].iter().position(|right| right == left),
            None,
            "case {index} is duplicated"
        );
    }
}

#[test]
fn every_bounded_graph_partition_round_trips_through_the_identity_pipeline() {
    for (case_index, case) in bounded::graph_partition_cases().iter().enumerate() {
        let whole = AnyInstance::Graph(case.graph.clone());
        let segments = segment::partition(&whole, &case.partition)
            .unwrap_or_else(|error| panic!("case {case_index} partition failed: {error:?}"));
        assert_eq!(
            segment::glue(&segments, &case.partition),
            Ok(whole.clone()),
            "case {case_index} glue differs"
        );
        assert_eq!(
            segment::segmented(&segments, &case.partition, &|value: &AnyInstance| {
                value.clone()
            }),
            Ok(whole),
            "case {case_index} segmented observation differs"
        );
    }
}

#[test]
fn graph_table_and_tree_cross_boundaries_round_trip() {
    let witnesses = bounded::shape_witnesses();
    assert_eq!(witnesses.len(), 3);
    for (index, (whole, partition)) in witnesses.iter().enumerate() {
        let segments = segment::partition(whole, partition)
            .unwrap_or_else(|error| panic!("shape {index} partition failed: {error:?}"));
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].boundary().ghosts.len(), 1);
        assert_eq!(segments[1].boundary().in_stubs.len(), 1);
        assert_eq!(segment::glue(&segments, partition), Ok(whole.clone()));
        assert_eq!(
            segment::segmented(segments.as_slice(), partition, &|value: &AnyInstance| {
                value.clone()
            }),
            Ok(whole.clone())
        );
    }
}

#[test]
fn a_tree_fan_crosses_by_edge_id_and_round_trips() {
    let label = SEdge::new("root", "child", "arc", None);
    let tree = Tree {
        nodes: BTreeMap::from([(0, Node::new(0, "root")), (1, Node::new(1, "child"))]),
        arcs: vec![(0, 1, label)],
        fans: vec![Fan {
            parent: 0,
            hyper_edge: "rel".to_owned(),
            children: BTreeMap::from([("arg".to_owned(), 1)]),
        }],
        root: 0,
    };
    let embedding = panproto_reference::encode::TreeEmbedding {
        arcs: BTreeMap::from([(1, 0)]),
        fans: vec![(2, 1, BTreeMap::from([("arg".to_owned(), 2)]))],
        next_node: 3,
        next_edge: 3,
    };
    let partition = Partition {
        shape: InstanceShapeKind::Tree,
        specs: vec![spec(0, &[(0, 0), (2, 2)]), spec(1, &[(1, 1)])],
    };
    let whole = AnyInstance::Tree(tree, embedding);
    let segments = segment::partition(&whole, &partition).test_unwrap("fan fixture must partition");
    let AnySegment::Tree(first) = &segments[0] else {
        panic!("expected tree segment")
    };
    assert_eq!(first.fans.len(), 1);
    assert_eq!(first.boundary.ghosts.len(), 1);
    assert_eq!(segments[1].boundary().in_stubs.len(), 2);
    assert_eq!(segment::glue(&segments, &partition), Ok(whole));
}

#[test]
fn tree_fan_order_is_carried_by_dense_ordinals_not_fan_node_ids() {
    let (tree, embedding) = two_fan_tree();
    let partition = Partition {
        shape: InstanceShapeKind::Tree,
        specs: vec![spec(0, &[(0, 5)])],
    };
    let whole = AnyInstance::Tree(tree, embedding);
    let segments = segment::partition(&whole, &partition).test_unwrap("tree partitions");
    let AnySegment::Tree(part) = &segments[0] else {
        panic!("expected tree segment")
    };
    assert_eq!(
        part.fans
            .iter()
            .map(|fan| (fan.node, fan.ordinal))
            .collect::<Vec<_>>(),
        [(5, 0), (4, 1)]
    );
    let bytes = AnySegment::Tree(part.clone())
        .canonical_bytes(&Digest([3; 32]), &partition)
        .test_unwrap("segment encodes");
    let (decoded, schema) =
        AnySegment::from_canonical_bytes(&bytes, &partition).test_unwrap("segment decodes");
    assert_eq!(schema, Digest([3; 32]));
    assert_eq!(segment::glue(&[decoded], &partition), Ok(whole));

    let mut nondense = segments;
    let AnySegment::Tree(part) = &mut nondense[0] else {
        panic!("expected tree segment")
    };
    part.fans[0].ordinal = 1;
    part.fans[1].ordinal = 2;
    assert!(matches!(
        segment::glue(&nondense, &partition),
        Err(SegmentError::NotEmbedded { .. })
    ));
}

#[test]
fn tree_fan_ordinals_are_dense_and_global_across_segments() {
    let (tree, embedding) = two_fan_tree();
    let partition = Partition {
        shape: InstanceShapeKind::Tree,
        specs: vec![spec(0, &[(0, 0), (5, 5)]), spec(1, &[(1, 4)])],
    };
    let whole = AnyInstance::Tree(tree, embedding);
    let mut segments = segment::partition(&whole, &partition).test_unwrap("tree partitions");
    segments.reverse();
    assert_eq!(segment::glue(&segments, &partition), Ok(whole));

    let mut duplicate = segments.clone();
    for segment in &mut duplicate {
        let AnySegment::Tree(tree) = segment else {
            panic!("expected tree segment")
        };
        if tree.fans.first().is_some_and(|fan| fan.ordinal == 1) {
            tree.fans[0].ordinal = 0;
        }
    }
    assert!(matches!(
        segment::glue(&duplicate, &partition),
        Err(SegmentError::NotEmbedded { .. })
    ));

    let mut gap = segments;
    for segment in &mut gap {
        let AnySegment::Tree(tree) = segment else {
            panic!("expected tree segment")
        };
        if tree.fans.first().is_some_and(|fan| fan.ordinal == 1) {
            tree.fans[0].ordinal = 2;
        }
    }
    assert!(matches!(
        segment::glue(&gap, &partition),
        Err(SegmentError::NotEmbedded { .. })
    ));
}

#[test]
fn invalid_first_fan_child_does_not_hide_later_child_edge_membership() {
    let (tree, embedding) = two_fan_tree();
    let partition = Partition {
        shape: InstanceShapeKind::Tree,
        specs: vec![spec(0, &[(0, 5)])],
    };
    let mut segments = segment::partition(&AnyInstance::Tree(tree, embedding), &partition)
        .test_unwrap("tree partitions");
    let AnySegment::Tree(tree) = &mut segments[0] else {
        panic!("expected tree segment")
    };
    tree.fans[0].child_edges.insert("aaa".to_owned(), 99);
    assert_eq!(
        segment::validate_segment_contract(&segments[0], &partition),
        Err(SegmentError::Invalid {
            violations: vec![SegmentError::NotEmbedded {
                elem: ViewRef::Node(5),
            }],
        })
    );
}

#[test]
fn shape_dangling_ghost_stub_and_duplicate_failures_are_precise() {
    let graph = crossing_graph();
    let partition = graph_partition();
    let whole = AnyInstance::Graph(graph.clone());
    assert_eq!(
        segment::partition(
            &whole,
            &Partition {
                shape: InstanceShapeKind::Table,
                specs: partition.specs.clone(),
            }
        ),
        Err(SegmentError::ShapeMismatch {
            expected: InstanceShapeKind::Graph,
            found: InstanceShapeKind::Table,
        })
    );

    let mut dangling = graph;
    dangling.edges.get_mut(&7).test_unwrap("fixture edge").tgt = 9;
    assert_eq!(
        segment::partition(&AnyInstance::Graph(dangling), &partition),
        Err(SegmentError::DanglingEndpoint {
            elem: ViewRef::Edge(7),
            node: 9,
        })
    );

    let mut segments = segment::partition(&whole, &partition).test_unwrap("fixture partitions");
    let AnySegment::Graph(source) = &mut segments[0] else {
        panic!("expected graph")
    };
    source
        .boundary
        .ghosts
        .get_mut(&1)
        .test_unwrap("ghost")
        .anchor = "wrong".to_owned();
    assert_eq!(
        segment::glue(&segments, &partition),
        Err(SegmentError::GhostMismatch { node: 1 })
    );

    let mut segments = segment::partition(&whole, &partition).test_unwrap("fixture partitions");
    let AnySegment::Graph(target) = &mut segments[1] else {
        panic!("expected graph")
    };
    target.boundary.in_stubs.remove(&ViewRef::Edge(7));
    assert_eq!(
        segment::glue(&segments, &partition),
        Err(SegmentError::InStubMismatch {
            elem: ViewRef::Edge(7)
        })
    );

    let mut segments = segment::partition(&whole, &partition).test_unwrap("fixture partitions");
    let AnySegment::Graph(target) = &mut segments[1] else {
        panic!("expected graph")
    };
    target.edges.insert(
        7,
        GEdge {
            src: 1,
            tgt: 1,
            label: SEdge::new("b", "b", "e", None),
            position: None,
        },
    );
    assert_eq!(
        segment::glue(&segments, &partition),
        Err(SegmentError::DuplicateEdgeId {
            edge: 7,
            segments: vec![SegmentId(0), SegmentId(1)],
        })
    );
}

#[test]
fn local_and_manifest_validators_emit_their_exact_error_variants() {
    let partition = graph_partition();

    let mut mismatched = Graph::default();
    mismatched.nodes.insert(0, Node::new(1, "wrong-id"));
    assert_eq!(
        segment::partition(&AnyInstance::Graph(mismatched), &partition),
        Err(SegmentError::NodeIdMismatch { key: 0, found: 1 })
    );

    let mut unowned = Graph::default();
    unowned.nodes.insert(2, Node::new(2, "outside"));
    assert_eq!(
        segment::partition(&AnyInstance::Graph(unowned), &partition),
        Err(SegmentError::Unowned {
            elem: ViewRef::Node(2),
        })
    );

    let whole = AnyInstance::Graph(crossing_graph());
    let mut wrong_owner = segment::partition(&whole, &partition).test_unwrap("fixture partitions");
    let node = match &wrong_owner[1] {
        AnySegment::Graph(target) => target.nodes[&1].clone(),
        _ => panic!("expected graph segment"),
    };
    let AnySegment::Graph(source) = &mut wrong_owner[0] else {
        panic!("expected graph segment")
    };
    source.nodes.insert(1, node);
    assert_eq!(
        segment::glue(&wrong_owner, &partition),
        Err(SegmentError::WrongOwner {
            elem: ViewRef::Node(1),
            expected: SegmentId(1),
            found: SegmentId(0),
        })
    );

    let segments = segment::partition(&whole, &partition).test_unwrap("fixture partitions");
    let manifest = segment::seal_manifest(
        &segments,
        &partition,
        Digest([1; 32]),
        ManifestRole::Source,
        None,
    )
    .test_unwrap("manifest seals");
    let mut crossed = manifest.clone();
    crossed.segments[0]
        .cross_out
        .insert(SegmentId(1), Digest([8; 32]));
    assert_eq!(
        segment::verify_manifest(&crossed),
        Err(SegmentError::CrossMismatch {
            from: SegmentId(0),
            to: SegmentId(1),
        })
    );

    let wrong_schema = segments[0]
        .canonical_bytes(&Digest([9; 32]), &partition)
        .test_unwrap("segment encodes");
    assert_eq!(
        segment::verify_segment_bytes(&manifest, &wrong_schema),
        Err(SegmentError::SchemaMismatch {
            expected: Digest([1; 32]),
            found: Digest([9; 32]),
        })
    );
}

#[test]
fn partition_validation_covers_ranges_dense_ids_and_table_allocation() {
    let inverted = Partition {
        shape: InstanceShapeKind::Graph,
        specs: vec![spec(0, &[(2, 1)])],
    };
    assert_eq!(
        inverted.validate(),
        Err(SegmentError::InvertedRange {
            segment: SegmentId(0),
            range: IdRange { lo: 2, hi: 1 },
        })
    );
    let overlap = Partition {
        shape: InstanceShapeKind::Graph,
        specs: vec![spec(0, &[(0, 2)]), spec(1, &[(2, 4)])],
    };
    assert_eq!(
        overlap.validate(),
        Err(SegmentError::OverlappingRanges {
            a: SegmentId(0),
            b: SegmentId(1),
        })
    );
    let not_dense = Partition {
        shape: InstanceShapeKind::Graph,
        specs: vec![spec(1, &[])],
    };
    assert_eq!(
        not_dense.validate(),
        Err(SegmentError::MalformedPartition {
            segment: SegmentId(0),
            defect: PartitionDefect::NotDense {
                found: SegmentId(1)
            },
        })
    );
    let mut table_spec = spec(0, &[(0, 2)]);
    table_spec.alloc_edges = Some(IdRange { lo: 8, hi: 9 });
    assert_eq!(
        Partition {
            shape: InstanceShapeKind::Table,
            specs: vec![table_spec],
        }
        .validate(),
        Err(SegmentError::MalformedPartition {
            segment: SegmentId(0),
            defect: PartitionDefect::EdgeAllocationOnTable,
        })
    );
}

#[test]
fn s10a_capacity_admits_exactly_the_u32_domain() {
    assert_eq!(
        segment::validation_view_preflight(SegmentId(3), u64::from(u32::MAX), 1),
        Ok(u64::from(u32::MAX) + 1)
    );
    assert_eq!(
        segment::validation_view_preflight(SegmentId(3), u64::from(u32::MAX), 2),
        Err(SegmentError::ValidationViewUnrepresentable {
            segment: SegmentId(3),
            edges: u64::from(u32::MAX) + 2,
        })
    );
    assert_eq!(
        segment::validation_view_preflight(SegmentId(3), u64::MAX, 1),
        Err(SegmentError::ValidationViewUnrepresentable {
            segment: SegmentId(3),
            edges: u64::MAX,
        })
    );
}

fn exact_instance_encoding(whole: &AnyInstance) -> (&'static str, Vec<u8>) {
    match whole {
        AnyInstance::Tree(tree, _) => (
            "panproto 2026-09 tree instance v1",
            tree_bytes(tree).test_unwrap("tree encodes"),
        ),
        AnyInstance::Table(table) => (
            "panproto 2026-09 table instance v1",
            table_bytes(table).test_unwrap("table encodes"),
        ),
        AnyInstance::Graph(graph) => (
            "panproto 2026-09 graph instance v1",
            graph_bytes(graph).test_unwrap("graph encodes"),
        ),
    }
}

fn assert_segment_artifact_proofs(
    manifest: &segment::SegmentManifest,
    segments: &[AnySegment],
    partition: &Partition,
) {
    for part in segments {
        let bytes = part
            .canonical_bytes(&Digest([1; 32]), partition)
            .test_unwrap("segment encodes");
        assert_eq!(
            AnySegment::from_canonical_bytes(&bytes, partition),
            Ok((part.clone(), Digest([1; 32])))
        );
        assert_eq!(segment::verify_segment(manifest, part), Ok(()));
        assert_eq!(
            segment::verify_segment_bytes(manifest, &bytes),
            Ok(part.clone())
        );
        let index = usize::try_from(part.segment().0).test_unwrap("small id");
        let record = &manifest.segments[index];
        assert!(segment::verify_audit_path(
            manifest.root,
            segment::manifest_leaf(record),
            index,
            manifest.segments.len(),
            &segment::audit_path(manifest, part.segment()),
        ));
        let mut mutated = bytes;
        *mutated.last_mut().test_unwrap("nonempty segment bytes") ^= 1;
        assert!(segment::verify_segment_bytes(manifest, &mutated).is_err());
    }
}

#[test]
fn manifests_are_canonical_bound_and_verifiable() {
    for (whole, partition) in bounded::shape_witnesses() {
        let (context, instance_bytes) = exact_instance_encoding(&whole);
        assert_eq!(
            segment::whole_instance_digest(&whole),
            Ok(Digest::derive(context, &instance_bytes))
        );
        let segments = segment::partition(&whole, &partition).test_unwrap("fixture partitions");
        let first = segment::seal_manifest(
            &segments,
            &partition,
            Digest([1; 32]),
            ManifestRole::Source,
            Some(Digest([2; 32])),
        )
        .test_unwrap("manifest seals");
        let second = segment::seal_manifest(
            &segments,
            &partition,
            Digest([1; 32]),
            ManifestRole::Source,
            Some(Digest([2; 32])),
        )
        .test_unwrap("manifest seals twice");
        assert_eq!(first, second);
        let mut reversed = segments.clone();
        reversed.reverse();
        let reordered = segment::seal_manifest(
            &reversed,
            &partition,
            Digest([1; 32]),
            ManifestRole::Source,
            Some(Digest([2; 32])),
        )
        .test_unwrap("caller segment order is presentation only");
        assert_eq!(first, reordered);
        assert_eq!(segment::verify_manifest(&first), Ok(()));
        assert_segment_artifact_proofs(&first, &segments, &partition);
        assert_eq!(
            segment::canonical_manifest_bytes(&first),
            segment::canonical_manifest_bytes(&second)
        );
        let manifest_bytes = segment::canonical_manifest_bytes(&first);
        assert_eq!(
            segment::decode_manifest_bytes(&manifest_bytes),
            Ok(first.clone())
        );
        let mut truncated = manifest_bytes;
        truncated.pop();
        assert!(matches!(
            segment::decode_manifest_bytes(&truncated),
            Err(SegmentError::Decode {
                artifact: ArtifactKind::Manifest,
                ..
            })
        ));
        let changed = segment::seal_manifest(
            &segments,
            &partition,
            Digest([1; 32]),
            ManifestRole::Source,
            Some(Digest([3; 32])),
        )
        .test_unwrap("changed binding seals");
        assert_ne!(
            segment::canonical_manifest_bytes(&first),
            segment::canonical_manifest_bytes(&changed)
        );
        let mut tampered = first;
        tampered.root = Digest([9; 32]);
        assert!(matches!(
            segment::verify_manifest(&tampered),
            Err(SegmentError::DigestMismatch { segment: None, .. })
        ));
    }
}

#[test]
fn journals_have_canonical_task_order_and_domain_separated_mutation_sensitive_digests() {
    let journal = RunJournal {
        run: Digest([1; 32]),
        tasks: BTreeMap::from([
            (
                segment::TaskKey {
                    phase: Phase::Validate,
                    segment: Some(SegmentId(0)),
                },
                TaskRecord {
                    inputs: Digest([2; 32]),
                    output: Digest([3; 32]),
                    bytes: 5,
                },
            ),
            (
                segment::TaskKey {
                    phase: Phase::Seal,
                    segment: None,
                },
                TaskRecord {
                    inputs: Digest([4; 32]),
                    output: Digest([5; 32]),
                    bytes: 7,
                },
            ),
        ]),
    };
    let bytes = segment::canonical_journal_bytes(&journal);
    assert_eq!(segment::decode_journal_bytes(&bytes), Some(journal.clone()));
    assert_eq!(
        segment::journal_digest(&journal),
        Digest::derive(segment::RUN_JOURNAL_CONTEXT, &bytes)
    );
    let mut changed = journal.clone();
    changed
        .tasks
        .get_mut(&segment::TaskKey {
            phase: Phase::Seal,
            segment: None,
        })
        .test_unwrap("seal record")
        .bytes = 8;
    assert_ne!(
        segment::journal_digest(&journal),
        segment::journal_digest(&changed)
    );
    let mut truncated = bytes;
    truncated.pop();
    assert_eq!(segment::decode_journal_bytes(&truncated), None);
}

#[test]
fn direct_edge_records_and_reader_slices_are_left_inverses_and_truncate() {
    let edge = GEdge {
        src: 7,
        tgt: 9,
        label: SEdge::new("s", "t", "kind", Some("name")),
        position: Some(3),
    };
    let mut writer = Writer::default();
    write_graph_edge(&mut writer, 11, &edge);
    let mut reader = Reader::new(&writer.bytes);
    assert_eq!(reader.position(), 0);
    assert_eq!(read_graph_edge(&mut reader), Some((11, edge)));
    assert!(reader.done());
    assert!(reader.remaining().is_empty());

    let mut short = Reader::new(&writer.bytes[..writer.bytes.len() - 1]);
    assert_eq!(read_graph_edge(&mut short), None);
    let mut slice = Reader::new(&[1, 2, 3]);
    assert_eq!(slice.bytes(2), Some(&[1, 2][..]));
    assert_eq!(slice.position(), 2);
    assert_eq!(slice.remaining(), &[3]);
    assert_eq!(slice.bytes(2), None);
    assert_eq!(slice.position(), 2, "a short read does not advance");
}

#[test]
fn typed_complement_segments_have_exact_empty_layouts_and_strict_left_inverses() {
    let graph = AnyComplementSegment::Graph(GraphComplementSegment::default());
    let table = AnyComplementSegment::Table(TableComplementSegment::default());
    let tree = AnyComplementSegment::Tree(TreeComplementSegment::default());
    let empty = [
        (graph, 2_u8, 8_usize, 75_usize),
        (table, 1_u8, 7_usize, 67_usize),
        (tree, 0_u8, 9_usize, 83_usize),
    ];
    for (value, tag, zero_counts, length) in empty {
        let bytes = value
            .canonical_bytes()
            .test_unwrap("empty complement encodes");
        let mut expected = vec![1, 0, tag];
        expected.extend(vec![0; zero_counts * 8 + 8]);
        assert_eq!(expected.len(), length);
        assert_eq!(bytes, expected);
        assert_eq!(
            AnyComplementSegment::from_canonical_bytes(&bytes),
            Ok(value)
        );
    }

    let mut graph = GraphComplementSegment::default();
    graph.dropped_nodes.insert(3, Node::new(3, "gone"));
    graph
        .original_values
        .insert(4, Some(Presence::Present(Value::Str("source".to_owned()))));
    graph.synthesized_nodes.insert(
        8,
        SynthKey::Contract {
            src: 4,
            tgt: 5,
            edge: SEdge::new("a", "b", "k", None),
        },
    );
    graph.floors = SegmentFloors { node: 9, edge: 12 };
    let graph = AnyComplementSegment::Graph(graph);

    let mut table = TableComplementSegment::default();
    table.dropped_rows.insert(
        2,
        DroppedRow {
            table: "users".to_owned(),
            row: BTreeMap::from([("name".to_owned(), Value::Str("old".to_owned()))]),
        },
    );
    table
        .synthesized_fk_pairs
        .insert((SEdge::new("users", "teams", "team", None), 2, 7));
    table.floors = SegmentFloors { node: 8, edge: 0 };
    let table = AnyComplementSegment::Table(table);

    let mut tree = TreeComplementSegment::default();
    tree.original_anchors.insert(1, "old".to_owned());
    tree.arc_order = vec![2, 7, 11];
    tree.floors = SegmentFloors { node: 4, edge: 12 };
    let tree = AnyComplementSegment::Tree(tree);

    for value in [graph, table, tree] {
        let bytes = value.canonical_bytes().test_unwrap("complement encodes");
        assert_eq!(
            AnyComplementSegment::from_canonical_bytes(&bytes),
            Ok(value.clone())
        );
        assert_eq!(
            value.digest(),
            Ok(segment::complement_segment_digest(&bytes))
        );
        assert!(!value.is_empty());
        assert!(value.element_count() > 0);
        let mut truncated = bytes;
        truncated.pop();
        assert!(matches!(
            AnyComplementSegment::from_canonical_bytes(&truncated),
            Err(SegmentError::Decode {
                artifact: ArtifactKind::ComplementSegment,
                ..
            })
        ));
    }
}

#[test]
fn complement_encoders_reject_invalid_keys_orders_and_table_edge_floors() {
    let mismatched_key = AnyComplementSegment::Graph(GraphComplementSegment {
        dropped_nodes: BTreeMap::from([(4, Node::new(7, "wrong-key"))]),
        ..GraphComplementSegment::default()
    });
    assert_eq!(
        mismatched_key.canonical_bytes(),
        Err(SegmentError::NodeIdMismatch { key: 4, found: 7 })
    );

    for arc_order in [vec![2, 1], vec![1, 1]] {
        let invalid = AnyComplementSegment::Tree(TreeComplementSegment {
            arc_order,
            ..TreeComplementSegment::default()
        });
        assert!(matches!(
            invalid.canonical_bytes(),
            Err(SegmentError::Decode {
                artifact: ArtifactKind::ComplementSegment,
                ..
            })
        ));
    }

    let invalid_table = AnyComplementSegment::Table(TableComplementSegment {
        floors: SegmentFloors { node: 9, edge: 1 },
        ..TableComplementSegment::default()
    });
    assert_eq!(
        invalid_table.canonical_bytes(),
        Err(SegmentError::Decode {
            artifact: ArtifactKind::ComplementSegment,
            message: "table complement has a nonzero edge floor".to_owned(),
        })
    );

    let valid_table = AnyComplementSegment::Table(TableComplementSegment::default());
    let mut invalid_bytes = valid_table
        .canonical_bytes()
        .test_unwrap("empty table complement encodes");
    let edge_floor = invalid_bytes.len() - 4;
    invalid_bytes[edge_floor] = 1;
    assert_eq!(
        AnyComplementSegment::from_canonical_bytes(&invalid_bytes),
        Err(SegmentError::Decode {
            artifact: ArtifactKind::ComplementSegment,
            message: "malformed table complement".to_owned(),
        })
    );
}

#[test]
fn graph_complement_union_coherently_rekeys_and_preserves_residue_laws() {
    let first_key = SynthKey::Contract {
        src: 0,
        tgt: 1,
        edge: SEdge::new("a", "b", "first", None),
    };
    let second_key = SynthKey::Expand {
        source_edge: 7,
        hop: 0,
    };
    let left = GraphComplementSegment {
        dropped_nodes: BTreeMap::from([(0, Node::new(0, "a"))]),
        synthesized_nodes: BTreeMap::from([(80, first_key.clone())]),
        floors: SegmentFloors { node: 4, edge: 6 },
        ..GraphComplementSegment::default()
    };
    let right = GraphComplementSegment {
        dropped_nodes: BTreeMap::from([(3, Node::new(3, "b"))]),
        synthesized_nodes: BTreeMap::from([(90, second_key.clone())]),
        floors: SegmentFloors { node: 8, edge: 5 },
        ..GraphComplementSegment::default()
    };
    let assembled_index = SynthIndex {
        nodes: BTreeMap::from([(8, first_key), (9, second_key)]),
        edges: BTreeMap::new(),
    };
    let whole = segment::assemble_graph_complement(&[left.clone(), right], &assembled_index)
        .test_unwrap("disjoint complement union");
    assert_eq!(
        whole.dropped_nodes.keys().copied().collect::<Vec<_>>(),
        [0, 3]
    );
    assert_eq!(whole.synthesized_nodes, assembled_index.nodes);
    assert_eq!(whole.floor, (8, 6));
    let expected = panproto_reference::restrict::Complement {
        dropped_nodes: BTreeMap::from([(0, Node::new(0, "a")), (3, Node::new(3, "b"))]),
        synthesized_nodes: assembled_index.nodes.clone(),
        floor: (8, 6),
        ..panproto_reference::restrict::Complement::default()
    };
    assert!(GraphShape::complement_divergence(&whole, &expected).is_none());
    let mut divergent = expected;
    divergent.dropped_nodes.remove(&3);
    assert!(GraphShape::complement_divergence(&whole, &divergent).is_some());
    assert!(!left.residue_is_trivial());
    assert!(!left.is_empty());
    let synth_only = GraphComplementSegment {
        synthesized_nodes: BTreeMap::from([(80, assembled_index.nodes[&8].clone())]),
        ..GraphComplementSegment::default()
    };
    assert!(synth_only.residue_is_trivial());
    assert!(!synth_only.is_empty());
    assert!(GraphComplementSegment::default().is_empty());
    assert!(matches!(
        segment::assemble_graph_complement(&[left.clone(), left], &assembled_index),
        Err(SegmentError::Decode {
            artifact: ArtifactKind::ComplementSegment,
            ..
        })
    ));
}

#[test]
fn table_complement_union_preserves_residue_and_empty_laws() {
    let foreign_key = SEdge::new("a", "b", "fk", None);
    let table_left = TableComplementSegment {
        dropped_rows: BTreeMap::from([(
            0,
            DroppedRow {
                table: "a".to_owned(),
                row: BTreeMap::new(),
            },
        )]),
        dropped_fk_pairs: BTreeMap::from([(foreign_key.clone(), BTreeSet::from([(0, 3)]))]),
        floors: SegmentFloors { node: 4, edge: 0 },
        ..TableComplementSegment::default()
    };
    let table_right = TableComplementSegment {
        dropped_fk_pairs: BTreeMap::from([(foreign_key.clone(), BTreeSet::from([(4, 7)]))]),
        synthesized_fk_pairs: BTreeSet::from([(foreign_key.clone(), 0, 3)]),
        floors: SegmentFloors { node: 8, edge: 0 },
        ..TableComplementSegment::default()
    };
    let table_whole =
        segment::assemble_table_complement(&[table_left.clone(), table_right.clone()])
            .test_unwrap("table complement union");
    let mut table_expected = TableComplement::empty();
    table_expected.dropped_rows = table_left.dropped_rows.clone();
    table_expected.dropped_fk_pairs =
        BTreeMap::from([(foreign_key, BTreeSet::from([(0, 3), (4, 7)]))]);
    table_expected.synthesized_fk_pairs = table_right.synthesized_fk_pairs.clone();
    table_expected.id_floor = 8;
    assert_eq!(table_whole, table_expected);
    assert!(TableShape::complement_divergence(&table_whole, &table_expected).is_none());
    assert!(!table_left.residue_is_trivial());
    assert!(!table_right.residue_is_trivial());
    assert!(!table_right.is_empty());
    let synth_only = TableComplementSegment {
        synthesized_fk_pairs: BTreeSet::from([(SEdge::new("a", "b", "fk", None), 0, 3)]),
        ..TableComplementSegment::default()
    };
    assert!(synth_only.residue_is_trivial());
    assert!(!synth_only.is_empty());
    assert!(TableComplementSegment::default().is_empty());
    assert_eq!(
        segment::assemble_table_complement(&[
            TableComplementSegment::default(),
            TableComplementSegment::default(),
        ]),
        Ok(TableComplement::empty())
    );
}

#[test]
fn table_complement_union_refuses_duplicate_pairs_and_edge_floors() {
    let foreign_key = SEdge::new("a", "b", "fk", None);
    let pair = TableComplementSegment {
        dropped_fk_pairs: BTreeMap::from([(foreign_key, BTreeSet::from([(0, 3)]))]),
        ..TableComplementSegment::default()
    };
    assert_eq!(
        segment::assemble_table_complement(&[pair.clone(), pair]),
        Err(SegmentError::Decode {
            artifact: ArtifactKind::ComplementSegment,
            message: "complement pair occurs in more than one segment".to_owned(),
        })
    );
    let invalid_floor = TableComplementSegment {
        floors: SegmentFloors { node: 4, edge: 1 },
        ..TableComplementSegment::default()
    };
    assert_eq!(
        segment::assemble_table_complement(&[invalid_floor]),
        Err(SegmentError::Decode {
            artifact: ArtifactKind::ComplementSegment,
            message: "table complement has a nonzero edge floor".to_owned(),
        })
    );
}

#[test]
fn tree_complement_union_rekeys_embedded_edge_order() {
    let tree_key = SynthKey::Expand {
        source_edge: 4,
        hop: 1,
    };
    let first = TreeComplementSegment {
        synthesized_edges: BTreeMap::from([(101, tree_key.clone())]),
        arc_order: vec![1, 101],
        ..TreeComplementSegment::default()
    };
    let second = TreeComplementSegment {
        arc_order: vec![3, 7],
        ..TreeComplementSegment::default()
    };
    let tree_index = SynthIndex {
        nodes: BTreeMap::new(),
        edges: BTreeMap::from([(9, tree_key)]),
    };
    let parts = [first, second];
    let tree_whole = segment::assemble_tree_complement_value(&parts, &tree_index)
        .test_unwrap("tree arc ids merge in embedded-id order");
    let expected = segment::AssembledTreeComplement {
        graph: Complement {
            synthesized_edges: tree_index.edges.clone(),
            ..Complement::default()
        },
        arc_order: vec![1, 3, 7, 9],
    };
    assert_eq!(tree_whole, expected);
    assert!(GraphShape::complement_divergence(&tree_whole.graph, &expected.graph).is_none());
    assert!(tree_whole.residue_is_trivial());
    assert!(!tree_whole.is_empty());
    assert_eq!(
        segment::assemble_tree_complement(&parts, &tree_index),
        Ok((expected.graph, expected.arc_order))
    );
}

#[test]
fn whole_tree_complement_distinguishes_order_bookkeeping_from_residue() {
    let order_only = TreeComplementSegment {
        arc_order: vec![1, 3],
        ..TreeComplementSegment::default()
    };
    assert!(
        order_only.residue_is_trivial(),
        "embedded ids make source order reconstruction bookkeeping, not hidden value residue"
    );
    assert!(!order_only.is_empty());
    assert!(TreeComplementSegment::default().is_empty());

    let order_whole =
        segment::assemble_tree_complement_value(&[order_only], &SynthIndex::default())
            .test_unwrap("order-only tree complement assembles");
    assert_eq!(
        order_whole,
        segment::AssembledTreeComplement {
            graph: Complement::default(),
            arc_order: vec![1, 3],
        }
    );
    assert!(order_whole.residue_is_trivial());
    assert!(!order_whole.is_empty());

    let empty = segment::assemble_tree_complement_value(
        &[TreeComplementSegment::default()],
        &SynthIndex::default(),
    )
    .test_unwrap("empty tree complement assembles");
    assert!(empty.residue_is_trivial());
    assert!(empty.is_empty());

    let residual = TreeComplementSegment {
        dropped_nodes: BTreeMap::from([(4, Node::new(4, "hidden"))]),
        ..TreeComplementSegment::default()
    };
    let residual_whole =
        segment::assemble_tree_complement_value(&[residual], &SynthIndex::default())
            .test_unwrap("residual tree complement assembles");
    let expected = segment::AssembledTreeComplement {
        graph: Complement {
            dropped_nodes: BTreeMap::from([(4, Node::new(4, "hidden"))]),
            ..Complement::default()
        },
        arc_order: Vec::new(),
    };
    assert_eq!(residual_whole, expected);
    assert!(!residual_whole.residue_is_trivial());
    assert!(!residual_whole.is_empty());
}

#[test]
fn seal_refuses_an_edge_allocation_range_below_the_global_floor() {
    let whole = AnyInstance::Graph(crossing_graph());
    let mut partition = graph_partition();
    partition.specs[1].alloc_edges = Some(IdRange { lo: 7, hi: 9 });
    let segments = segment::partition(&whole, &partition).test_unwrap("fixture partitions");
    assert_eq!(
        segment::seal_manifest(
            &segments,
            &partition,
            Digest::default(),
            ManifestRole::Source,
            None,
        ),
        Err(SegmentError::AllocationBelowFloor {
            segment: SegmentId(1),
            kind: IdKind::Edge,
        })
    );
}

const fn run_spec(cancel_after_tasks: Option<u32>) -> RunSpec {
    RunSpec {
        operation: Digest([4; 32]),
        schema: Digest([5; 32]),
        binding: Some(Digest([6; 32])),
        cancel_after_tasks,
    }
}

fn run_with_complement(
    segments: &[AnySegment],
    partition: &Partition,
    operation: u8,
    owner: SegmentId,
    complement: &AnyComplementSegment,
    budget: Budget,
) -> segment::SegmentedRun<AnyInstance> {
    let mut store = MemoryStore::default();
    segment::segmented_operation_with_progress(
        segments,
        partition,
        RunSpec {
            operation: Digest([operation; 32]),
            ..run_spec(None)
        },
        budget,
        &mut store,
        &|input| {
            Ok(SegmentOperationResult {
                instance: input.clone(),
                complements: BTreeMap::from([(owner, complement.clone())]),
            })
        },
        &mut |_| {},
    )
}

#[test]
fn table_edge_floor_is_refused_before_the_operation_result_commits() {
    let (whole, partition) = bounded::shape_witnesses()
        .into_iter()
        .find(|(whole, _)| matches!(whole, AnyInstance::Table(_)))
        .test_unwrap("bounded table witness");
    let segments = segment::partition(&whole, &partition).test_unwrap("table partitions");
    let invalid = TableComplementSegment {
        floors: SegmentFloors { node: 5, edge: 1 },
        ..TableComplementSegment::default()
    };
    let result = run_with_complement(
        &segments,
        &partition,
        13,
        SegmentId(0),
        &AnyComplementSegment::Table(invalid),
        Budget::unlimited(),
    );
    assert_eq!(
        result.report.status,
        RunStatus::Failed(SegmentError::Decode {
            artifact: ArtifactKind::ComplementSegment,
            message: "table complement has a nonzero edge floor".to_owned(),
        })
    );
    assert_eq!(
        result.report.tasks_run, 6,
        "the invalid Assemble result is never committed"
    );
}

#[test]
fn every_shape_restarts_from_committed_tasks_to_the_same_manifest() {
    for (whole, partition) in bounded::shape_witnesses() {
        let segments = segment::partition(&whole, &partition).test_unwrap("fixture partitions");
        let mut store = MemoryStore::default();
        let cancelled = segment::segmented_with(
            &segments,
            &partition,
            run_spec(Some(2)),
            Budget::unlimited(),
            &mut store,
            &|value: &AnyInstance| value.clone(),
        );
        assert_eq!(
            cancelled.report.status,
            RunStatus::Incomplete {
                reason: segment::RunIncompleteReason::Cancelled
            }
        );
        assert_eq!(cancelled.report.tasks_run, 2);

        let resumed = segment::segmented_with(
            &segments,
            &partition,
            run_spec(None),
            Budget::unlimited(),
            &mut store,
            &|value: &AnyInstance| value.clone(),
        );
        assert!(matches!(resumed.report.status, RunStatus::Complete { .. }));
        assert!(resumed.report.tasks_reused >= 2);
        assert_eq!(resumed.value, Some(whole.clone()));
        let resumed_manifest = resumed.manifest.test_unwrap("complete run has manifest");

        let replay = segment::segmented_with(
            &segments,
            &partition,
            run_spec(None),
            Budget::unlimited(),
            &mut store,
            &|value: &AnyInstance| value.clone(),
        );
        assert!(matches!(replay.report.status, RunStatus::Complete { .. }));
        assert_eq!(replay.report.tasks_run, 0);
        assert_eq!(
            replay.report.tasks_reused,
            replay.report.progress.tasks_total
        );
        assert_eq!(replay.report.phases.len(), 6);
        assert_eq!(
            replay
                .report
                .phases
                .iter()
                .map(|phase| phase.reused)
                .sum::<u32>(),
            replay.report.progress.tasks_total
        );
        assert_eq!(replay.manifest, Some(resumed_manifest));

        for cut in 0..replay.report.progress.tasks_total {
            let mut boundary_store = MemoryStore::default();
            let interrupted = segment::segmented_with(
                &segments,
                &partition,
                run_spec(Some(cut)),
                Budget::unlimited(),
                &mut boundary_store,
                &|value: &AnyInstance| value.clone(),
            );
            assert_eq!(
                interrupted.report.status,
                RunStatus::Incomplete {
                    reason: segment::RunIncompleteReason::Cancelled
                },
                "cut {cut} must stop before its next task"
            );
            let completed = segment::segmented_with(
                &segments,
                &partition,
                run_spec(None),
                Budget::unlimited(),
                &mut boundary_store,
                &|value: &AnyInstance| value.clone(),
            );
            assert!(matches!(
                completed.report.status,
                RunStatus::Complete { .. }
            ));
            assert_eq!(completed.manifest, replay.manifest, "cut {cut} drifted");
        }
    }
}

#[test]
fn nonidentity_execution_matches_direct_whole_state_and_reuses_assembly() {
    let whole = AnyInstance::Graph(crossing_graph());
    let partition = graph_partition();
    let segments = segment::partition(&whole, &partition).test_unwrap("fixture partitions");
    let calls = Cell::new(0_u32);
    let operation = |input: &AnyInstance| {
        calls.set(calls.get() + 1);
        let AnyInstance::Graph(mut graph) = input.clone() else {
            panic!("graph fixture")
        };
        graph.nodes.get_mut(&0).test_unwrap("node zero").anchor = "changed".to_owned();
        Ok(SegmentOperationResult {
            instance: AnyInstance::Graph(graph),
            complements: BTreeMap::new(),
        })
    };
    let mut store = MemoryStore::default();
    let mut first_progress = Vec::new();
    let first = segment::segmented_operation_with_progress(
        &segments,
        &partition,
        run_spec(None),
        Budget::unlimited(),
        &mut store,
        &operation,
        &mut |event| first_progress.push(*event),
    );
    assert!(matches!(first.report.status, RunStatus::Complete { .. }));
    assert_eq!(calls.get(), 1);
    assert_eq!(
        first_progress.len(),
        usize::try_from(first.report.progress.tasks_total).test_unwrap("small task count")
    );
    let Some(AnyInstance::Graph(first_graph)) = first.value else {
        panic!("graph result")
    };
    assert_eq!(first_graph.nodes[&0].anchor, "changed");
    let mut direct = crossing_graph();
    direct
        .nodes
        .get_mut(&0)
        .test_unwrap("direct node zero")
        .anchor = "changed".to_owned();
    assert_eq!(
        AnyInstance::Graph(first_graph.clone()),
        AnyInstance::Graph(direct)
    );
    assert_ne!(
        AnyInstance::Graph(first_graph),
        whole,
        "the operation is not identity"
    );

    let mut replay_progress = Vec::new();
    let replay = segment::segmented_operation_with_progress(
        &segments,
        &partition,
        run_spec(None),
        Budget::unlimited(),
        &mut store,
        &operation,
        &mut |event| replay_progress.push(*event),
    );
    assert!(matches!(replay.report.status, RunStatus::Complete { .. }));
    assert_eq!(
        calls.get(),
        1,
        "a reused Assemble does not invoke the operation"
    );
    assert_eq!(replay.report.tasks_run, 0);
    assert_eq!(
        replay.report.tasks_reused,
        replay.report.progress.tasks_total
    );
    assert_eq!(
        replay_progress.len(),
        usize::try_from(replay.report.progress.tasks_total).test_unwrap("small task count")
    );
}

#[test]
fn complement_budget_refuses_before_emit_commit_then_reuses_the_prefix() {
    let whole = AnyInstance::Graph(crossing_graph());
    let partition = graph_partition();
    let segments = segment::partition(&whole, &partition).test_unwrap("fixture partitions");
    let mut complement = GraphComplementSegment::default();
    complement.dropped_nodes.insert(0, Node::new(0, "hidden"));
    complement.floors = SegmentFloors { node: 2, edge: 8 };
    let complement = AnyComplementSegment::Graph(complement);
    let complement_bytes = complement
        .canonical_bytes()
        .test_unwrap("complement has canonical bytes");
    let calls = Cell::new(0_u32);
    let operation = |input: &AnyInstance| {
        calls.set(calls.get() + 1);
        Ok(SegmentOperationResult {
            instance: input.clone(),
            complements: BTreeMap::from([(SegmentId(0), complement.clone())]),
        })
    };
    let mut store = MemoryStore::default();
    let refused = segment::segmented_operation_with_progress(
        &segments,
        &partition,
        run_spec(None),
        Budget {
            complement: Some(u64::try_from(complement_bytes.len()).test_unwrap("small bytes") - 1),
            ..Budget::unlimited()
        },
        &mut store,
        &operation,
        &mut |_| {},
    );
    assert_eq!(calls.get(), 1);
    assert!(matches!(
        refused.report.status,
        RunStatus::Residual(Residual::BudgetExceeded {
            resource: Resource::Complement,
            phase: Phase::Emit,
            segment: Some(SegmentId(0)),
            needed,
            budget,
        }) if needed == u64::try_from(complement_bytes.len()).test_unwrap("small bytes")
            && budget + 1 == needed
    ));
    assert_eq!(
        refused.report.tasks_run, 7,
        "three local phases plus Assemble"
    );

    let completed = segment::segmented_operation_with_progress(
        &segments,
        &partition,
        run_spec(None),
        Budget {
            complement: Some(u64::try_from(complement_bytes.len()).test_unwrap("small bytes")),
            ..Budget::unlimited()
        },
        &mut store,
        &operation,
        &mut |_| {},
    );
    assert!(matches!(
        completed.report.status,
        RunStatus::Complete { .. }
    ));
    assert_eq!(calls.get(), 1, "the larger budget reuses Assemble");
    assert_eq!(completed.report.tasks_reused, 7);
    assert_eq!(completed.report.tasks_run, 3);
    assert_eq!(
        completed.report.complement.bytes,
        u64::try_from(complement_bytes.len()).test_unwrap("small bytes")
    );
    assert_eq!(
        completed.report.complement.elements,
        complement.element_count()
    );
    assert_eq!(
        completed.report.complement.per_segment,
        BTreeMap::from([(
            SegmentId(0),
            (
                complement.element_count(),
                u64::try_from(complement_bytes.len()).test_unwrap("small bytes"),
            ),
        )])
    );
}

#[test]
fn corrupt_artifacts_and_changed_source_segments_are_recomputed() {
    let whole = AnyInstance::Graph(crossing_graph());
    let partition = graph_partition();
    let segments = segment::partition(&whole, &partition).test_unwrap("fixture partitions");
    let calls = Cell::new(0_u32);
    let operation = |input: &AnyInstance| {
        calls.set(calls.get() + 1);
        Ok(SegmentOperationResult::identity(input))
    };
    let mut store = MutableStore::default();
    let first = segment::segmented_operation_with_progress(
        &segments,
        &partition,
        run_spec(None),
        Budget::unlimited(),
        &mut store,
        &operation,
        &mut |_| {},
    );
    assert!(matches!(first.report.status, RunStatus::Complete { .. }));
    assert_eq!(calls.get(), 1);

    store.corrupt(Phase::Assemble);
    let repaired = segment::segmented_operation_with_progress(
        &segments,
        &partition,
        run_spec(None),
        Budget::unlimited(),
        &mut store,
        &operation,
        &mut |_| {},
    );
    assert!(matches!(repaired.report.status, RunStatus::Complete { .. }));
    assert_eq!(calls.get(), 2);
    assert_eq!(repaired.report.tasks_run, 1);
    assert_eq!(repaired.manifest, first.manifest);

    let mut changed_segments = segments;
    let AnySegment::Graph(source) = &mut changed_segments[0] else {
        panic!("expected graph source segment")
    };
    source
        .edges
        .get_mut(&7)
        .test_unwrap("crossing edge exists")
        .position = Some(3);
    assert!(segment::glue(&changed_segments, &partition).is_ok());
    let changed = segment::segmented_operation_with_progress(
        &changed_segments,
        &partition,
        run_spec(None),
        Budget::unlimited(),
        &mut store,
        &operation,
        &mut |_| {},
    );
    assert!(matches!(changed.report.status, RunStatus::Complete { .. }));
    assert_eq!(calls.get(), 3);
    assert_eq!(
        changed.report.tasks_run,
        changed.report.progress.tasks_total
    );
    assert_ne!(changed.manifest, first.manifest);
}

#[test]
fn a_failed_commit_is_an_incomplete_run_with_an_orphan_and_resumes() {
    let (whole, partition) = bounded::shape_witnesses().remove(0);
    let segments = segment::partition(&whole, &partition).test_unwrap("fixture partitions");
    let mut store = FailFirstCommitStore::default();
    let interrupted = segment::segmented_with(
        &segments,
        &partition,
        run_spec(None),
        Budget::unlimited(),
        &mut store,
        &|value: &AnyInstance| value.clone(),
    );
    assert_eq!(
        interrupted.report.status,
        RunStatus::Incomplete {
            reason: segment::RunIncompleteReason::Store {
                message: "injected commit failure".to_owned(),
            },
        }
    );
    assert_eq!(interrupted.report.tasks_run, 0);
    assert_eq!(interrupted.report.orphans.len(), 1);

    let resumed = segment::segmented_with(
        &segments,
        &partition,
        run_spec(None),
        Budget::unlimited(),
        &mut store,
        &|value: &AnyInstance| value.clone(),
    );
    assert!(matches!(resumed.report.status, RunStatus::Complete { .. }));
    assert_eq!(resumed.value, Some(whole));
    assert!(resumed.report.orphans.is_empty());
}

#[test]
fn budget_residuals_precede_artifact_commits() {
    let (whole, partition) = bounded::shape_witnesses().remove(0);
    let segments = segment::partition(&whole, &partition).test_unwrap("fixture partitions");

    let mut affected_store = MemoryStore::default();
    let affected = segment::segmented_with(
        &segments,
        &partition,
        run_spec(None),
        Budget {
            working_set: u64::MAX,
            complement: None,
            summary: u64::MAX,
            affected_segments: 1,
        },
        &mut affected_store,
        &|value: &AnyInstance| value.clone(),
    );
    assert_eq!(
        affected.report.status,
        RunStatus::Residual(Residual::BudgetExceeded {
            resource: Resource::AffectedSegments,
            phase: segment::Phase::Validate,
            segment: None,
            needed: 2,
            budget: 1,
        })
    );
    assert_eq!(affected.report.tasks_run, 0);

    let mut store = MemoryStore::default();
    let result = segment::segmented_with(
        &segments,
        &partition,
        run_spec(None),
        Budget {
            working_set: 0,
            complement: None,
            summary: u64::MAX,
            affected_segments: u32::MAX,
        },
        &mut store,
        &|value: &AnyInstance| value.clone(),
    );
    assert!(matches!(
        result.report.status,
        RunStatus::Residual(Residual::BudgetExceeded {
            resource: Resource::WorkingSet,
            phase: segment::Phase::Validate,
            ..
        })
    ));
    assert_eq!(result.report.tasks_run, 0);
    assert_eq!(result.report.tasks_reused, 0);

    let mut summary_store = MemoryStore::default();
    let summary = segment::segmented_with(
        &segments,
        &partition,
        run_spec(None),
        Budget {
            working_set: u64::MAX,
            complement: None,
            summary: 0,
            affected_segments: u32::MAX,
        },
        &mut summary_store,
        &|value: &AnyInstance| value.clone(),
    );
    assert!(matches!(
        summary.report.status,
        RunStatus::Residual(Residual::BudgetExceeded {
            resource: Resource::Summary,
            phase: segment::Phase::Assemble,
            needed,
            budget: 0,
            ..
        }) if needed > 0
    ));
    assert_eq!(summary.report.tasks_run, 0);
}

#[test]
fn structural_failures_precede_affected_and_complement_budget_residuals() {
    let whole = AnyInstance::Graph(crossing_graph());
    let partition = graph_partition();
    let mut invalid = segment::partition(&whole, &partition).test_unwrap("fixture partitions");
    let AnySegment::Graph(source) = &mut invalid[0] else {
        panic!("expected graph segment")
    };
    source
        .boundary
        .ghosts
        .get_mut(&1)
        .test_unwrap("crossing ghost")
        .anchor = "wrong".to_owned();
    let mut invalid_store = MemoryStore::default();
    let crossing = segment::segmented_with(
        &invalid,
        &partition,
        run_spec(None),
        Budget {
            affected_segments: 0,
            ..Budget::unlimited()
        },
        &mut invalid_store,
        &|value: &AnyInstance| value.clone(),
    );
    assert_eq!(
        crossing.report.status,
        RunStatus::Failed(SegmentError::GhostMismatch { node: 1 })
    );
    assert_eq!(crossing.report.tasks_run, 0);

    let segments = segment::partition(&whole, &partition).test_unwrap("fixture partitions");
    let misplaced = GraphComplementSegment {
        dropped_nodes: BTreeMap::from([(1, Node::new(1, "b"))]),
        ..GraphComplementSegment::default()
    };
    let complement = run_with_complement(
        &segments,
        &partition,
        4,
        SegmentId(0),
        &AnyComplementSegment::Graph(misplaced),
        Budget {
            complement: Some(0),
            ..Budget::unlimited()
        },
    );
    assert_eq!(
        complement.report.status,
        RunStatus::Failed(SegmentError::WrongOwner {
            elem: ViewRef::Node(1),
            expected: SegmentId(1),
            found: SegmentId(0),
        })
    );
    assert_eq!(
        complement.report.tasks_run, 6,
        "the invalid Assemble result is never committed"
    );

    let bad_key = GraphComplementSegment {
        dropped_nodes: BTreeMap::from([(0, Node::new(9, "bad-key"))]),
        ..GraphComplementSegment::default()
    };
    let key_mismatch = run_with_complement(
        &segments,
        &partition,
        11,
        SegmentId(0),
        &AnyComplementSegment::Graph(bad_key),
        Budget {
            complement: Some(0),
            ..Budget::unlimited()
        },
    );
    assert_eq!(
        key_mismatch.report.status,
        RunStatus::Failed(SegmentError::NodeIdMismatch { key: 0, found: 9 })
    );
    assert_eq!(key_mismatch.report.tasks_run, 6);
}

#[test]
fn allocation_floor_is_append_only_on_the_exact_small_carrier() {
    fn subsets() -> Vec<BTreeSet<u32>> {
        (0_u32..16)
            .map(|bits| (0_u32..4).filter(|item| bits & (1 << item) != 0).collect())
            .collect()
    }
    let sets = subsets();
    let mut cases = 0_u64;
    for lo in 0_u32..=3 {
        for hi in lo..=3 {
            let range = IdRange { lo, hi };
            for source in &sets {
                for used in &sets {
                    let expected = source
                        .iter()
                        .chain(used)
                        .copied()
                        .filter(|id| range.contains(*id))
                        .max()
                        .map_or(Some(lo), |id| id.checked_add(1))
                        .filter(|id| *id <= hi && *id != u32::MAX);
                    assert_eq!(segment::fresh(range, source, used), expected);
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 2_560);
    assert_eq!(
        segment::fresh(
            IdRange { lo: 0, hi: 3 },
            &BTreeSet::from([0, 2]),
            &BTreeSet::new()
        ),
        Some(3),
        "fresh is append-floor, not least-absent"
    );
}

fn structural_error_vocabulary() -> Vec<SegmentError> {
    let segment = SegmentId(0);
    let digest = Digest::default();
    let elem = ViewRef::Node(0);
    vec![
        SegmentError::InvertedRange {
            segment,
            range: IdRange { lo: 1, hi: 0 },
        },
        SegmentError::OverlappingRanges {
            a: segment,
            b: segment,
        },
        SegmentError::Unowned { elem: elem.clone() },
        SegmentError::WrongOwner {
            elem: elem.clone(),
            expected: segment,
            found: segment,
        },
        SegmentError::NodeIdMismatch { key: 0, found: 1 },
        SegmentError::GhostMismatch { node: 0 },
        SegmentError::InStubMismatch { elem },
        SegmentError::DuplicateEdgeId {
            edge: 0,
            segments: vec![segment],
        },
        SegmentError::CrossMismatch {
            from: segment,
            to: segment,
        },
        SegmentError::DigestMismatch {
            segment: None,
            expected: digest,
            found: digest,
        },
        SegmentError::SchemaMismatch {
            expected: digest,
            found: digest,
        },
        SegmentError::ShapeMismatch {
            expected: InstanceShapeKind::Graph,
            found: InstanceShapeKind::Table,
        },
        SegmentError::NoAllocationRange {
            segment,
            kind: IdKind::Node,
        },
        SegmentError::AllocationExhausted {
            segment,
            kind: IdKind::Node,
        },
        SegmentError::AllocationBelowFloor {
            segment,
            kind: IdKind::Edge,
        },
        SegmentError::EdgeIdOutsideAllocation { edge: 0, segment },
        SegmentError::Invalid {
            violations: Vec::new(),
        },
    ]
}

fn pipeline_error_vocabulary() -> Vec<SegmentError> {
    let segment = SegmentId(0);
    let elem = ViewRef::Node(0);
    vec![
        SegmentError::Restrict {
            at: ErrorAt {
                segment,
                key: ErrorKey {
                    stage: 0,
                    id: 0,
                    id2: 0,
                    kind: 0,
                },
            },
            detail: Box::new(ErrorDetail("x".to_owned())),
        },
        SegmentError::PutDomain(vec![PutDomainError("x".to_owned())]),
        SegmentError::Edit {
            segment,
            detail: Box::new(ErrorDetail("x".to_owned())),
        },
        SegmentError::Identity(IdentityError("x".to_owned())),
        SegmentError::Decode {
            artifact: ArtifactKind::Segment,
            message: "x".to_owned(),
        },
        SegmentError::MalformedPartition {
            segment,
            defect: PartitionDefect::SegmentMissing,
        },
        SegmentError::DanglingEndpoint {
            elem: elem.clone(),
            node: 0,
        },
        SegmentError::NotEmbedded { elem },
        SegmentError::ValidationViewUnrepresentable { segment, edges: 0 },
    ]
}

#[test]
fn all_twenty_six_external_error_names_are_stable_and_distinct() {
    let mut errors = structural_error_vocabulary();
    errors.extend(pipeline_error_vocabulary());
    assert_eq!(errors.len(), 26);
    let variants: HashSet<_> = errors.iter().map(discriminant).collect();
    assert_eq!(variants.len(), 26);
    let names = [
        "inverted_range",
        "overlapping_ranges",
        "unowned",
        "wrong_owner",
        "node_id_mismatch",
        "ghost_mismatch",
        "in_stub_mismatch",
        "duplicate_edge_id",
        "cross_mismatch",
        "digest_mismatch",
        "schema_mismatch",
        "shape_mismatch",
        "no_allocation_range",
        "allocation_exhausted",
        "allocation_below_floor",
        "edge_id_outside_allocation",
        "invalid",
        "restrict",
        "put_domain",
        "edit",
        "identity",
        "decode",
        "malformed_partition",
        "dangling_endpoint",
        "not_embedded",
        "validation_view_unrepresentable",
    ];
    for (index, (error, name)) in errors.iter().zip(names).enumerate() {
        assert_eq!(usize::from(error.variant_index()), index);
        assert_eq!(error.external_name(), name);
    }
}

#[test]
fn allocation_refusals_are_reached_through_the_typed_allocator_contract() {
    let no_ranges = graph_partition();
    assert_eq!(
        segment::allocate_fresh(
            &no_ranges,
            SegmentId(0),
            IdKind::Node,
            &BTreeSet::new(),
            &BTreeSet::new(),
        ),
        Err(SegmentError::NoAllocationRange {
            segment: SegmentId(0),
            kind: IdKind::Node,
        })
    );

    let mut allocated = graph_partition();
    allocated.specs[0].alloc_nodes = Some(IdRange { lo: 0, hi: 0 });
    allocated.specs[0].alloc_edges = Some(IdRange { lo: 8, hi: 9 });
    assert_eq!(
        segment::allocate_fresh(
            &allocated,
            SegmentId(0),
            IdKind::Node,
            &BTreeSet::from([0]),
            &BTreeSet::new(),
        ),
        Err(SegmentError::AllocationExhausted {
            segment: SegmentId(0),
            kind: IdKind::Node,
        })
    );
    assert_eq!(
        segment::check_new_edge_id(&allocated, SegmentId(0), 7, false),
        Err(SegmentError::EdgeIdOutsideAllocation {
            edge: 7,
            segment: SegmentId(0),
        })
    );
    assert_eq!(
        segment::check_new_edge_id(&allocated, SegmentId(0), 7, true),
        Ok(())
    );
    assert_eq!(
        segment::check_new_edge_id(&allocated, SegmentId(0), 8, false),
        Ok(())
    );
}

fn produced_restrict_failure() -> SegmentError {
    let schema = Schema::default().vertex("a", "obj").vertex("b", "obj");
    let migration = Migration {
        surviving_verts: BTreeSet::from(["a".to_owned(), "b".to_owned()]),
        ..Migration::default()
    };
    let invalid_graph = Graph {
        nodes: BTreeMap::from([(0, Node::new(0, "a")), (1, Node::new(1, "b"))]),
        edges: BTreeMap::from([(
            8,
            GEdge {
                src: 0,
                tgt: 1,
                label: SEdge::new("a", "b", "undeclared", None),
                position: None,
            },
        )]),
    };
    let Err(restriction) = panproto_reference::restrict::get(
        &schema,
        &schema,
        &migration,
        &NoValueLayer,
        &invalid_graph,
    ) else {
        panic!("the undeclared edge must reach the restriction engine")
    };
    let RestrictError::At {
        key: restrict_key,
        error: restrict_error,
    } = restriction
    else {
        panic!("expected an element-local restriction failure")
    };
    let restrict = segment::adapt_restrict_at(SegmentId(0), restrict_key, &restrict_error);
    assert_eq!(
        restrict,
        SegmentError::Restrict {
            at: ErrorAt {
                segment: SegmentId(0),
                key: ErrorKey {
                    stage: 1,
                    id: 8,
                    id2: 0,
                    kind: 2,
                },
            },
            detail: Box::new(ErrorDetail(format!("{restrict_error:?}"))),
        }
    );
    restrict
}

fn produced_edit_failure() -> SegmentError {
    let mut delta = panproto_reference::edit::GraphDelta::default();
    delta.del.insert(
        panproto_reference::edit::GElem::Node(99),
        panproto_reference::edit::GPayload::Node(Node::new(99, "a")),
    );
    let Err(edit_error) = panproto_reference::edit::apply(&delta, &Graph::default()) else {
        panic!("deleting an absent node must refuse")
    };
    let edit = segment::adapt_edit_error(SegmentId(1), &edit_error);
    assert_eq!(
        edit,
        SegmentError::Edit {
            segment: SegmentId(1),
            detail: Box::new(ErrorDetail(format!("{edit_error:?}"))),
        }
    );
    edit
}

fn produced_lens_failures() -> (SegmentError, SegmentError) {
    let lens_schema = Schema::default().vertex("a", "obj");
    let lens_migration = Migration {
        surviving_verts: BTreeSet::from(["a".to_owned()]),
        ..Migration::default()
    };
    let plan = panproto_reference::restrict::compile(&lens_schema, &lens_schema, &lens_migration)
        .test_unwrap("identity plan compiles");
    let semantics = BTreeMap::new();
    let lens = panproto_reference::lens::GraphLens::new(
        &lens_schema,
        &lens_schema,
        &lens_migration,
        &plan,
        &semantics,
        &NoValueLayer,
    )
    .test_unwrap("identity lens constructs");
    let mut view = Graph {
        nodes: BTreeMap::from([(0, Node::new(0, "a"))]),
        ..Graph::default()
    };
    let mut complement = Complement {
        dropped_nodes: BTreeMap::from([(1, Node::new(1, "a"))]),
        ..Complement::default()
    };
    let Err(identity_error) = lens.put(&view, &complement) else {
        panic!("unstamped residue must refuse")
    };
    let identity =
        segment::adapt_lens_error(&identity_error).test_unwrap("identity error projects into SG.9");
    complement.identity = Some(*lens.identity());
    view.nodes.insert(1, Node::new(1, "a"));
    let Err(put_error) = lens.put(&view, &complement) else {
        panic!("reusing a dropped id must refuse")
    };
    let put = segment::adapt_lens_error(&put_error).test_unwrap("put error projects into SG.9");
    assert_eq!(
        identity,
        SegmentError::Identity(IdentityError("UnboundComplement".to_owned()))
    );
    assert_eq!(
        put,
        SegmentError::PutDomain(vec![PutDomainError(
            "IdCollision { elem: Node(1) }".to_owned(),
        )])
    );
    (put, identity)
}

#[test]
fn tree_operational_errors_remain_outside_the_exact_sg9_vocabulary() {
    let errors = [
        TreeOperationalError::RestoredNotATree {
            reason: panproto_reference::encode::NotATree::MissingRoot,
        },
        TreeOperationalError::IdsExhausted { nodes: true },
        TreeOperationalError::IdsExhausted { nodes: false },
    ];
    for operational in errors {
        assert_eq!(
            segment::adapt_lens_error(&LensError::TreeOperational(operational.clone())),
            Err(operational)
        );
    }
    assert_eq!(
        structural_error_vocabulary().len() + pipeline_error_vocabulary().len(),
        26
    );
}

#[test]
fn operation_failures_preserve_restrict_put_edit_and_identity_payloads() {
    let whole = AnyInstance::Graph(crossing_graph());
    let partition = graph_partition();
    let segments = segment::partition(&whole, &partition).test_unwrap("fixture partitions");
    let restrict = produced_restrict_failure();
    let edit = produced_edit_failure();
    let (put, identity) = produced_lens_failures();
    let errors = [restrict, put, edit, identity];
    for error in errors {
        let mut store = MemoryStore::default();
        let result = segment::segmented_operation_with_progress(
            &segments,
            &partition,
            run_spec(None),
            Budget::unlimited(),
            &mut store,
            &|_| Err(error.clone()),
            &mut |_| {},
        );
        assert_eq!(result.report.status, RunStatus::Failed(error));
        assert_eq!(result.report.tasks_run, 6, "failure occurs at Assemble");
    }
}

#[test]
fn emit_refuses_a_complement_element_assigned_to_the_wrong_segment() {
    let whole = AnyInstance::Graph(crossing_graph());
    let partition = graph_partition();
    let segments = segment::partition(&whole, &partition).test_unwrap("fixture partitions");
    let misplaced = GraphComplementSegment {
        dropped_nodes: BTreeMap::from([(1, Node::new(1, "b"))]),
        ..GraphComplementSegment::default()
    };
    let result = run_with_complement(
        &segments,
        &partition,
        4,
        SegmentId(0),
        &AnyComplementSegment::Graph(misplaced),
        Budget::unlimited(),
    );
    assert_eq!(
        result.report.status,
        RunStatus::Failed(SegmentError::WrongOwner {
            elem: ViewRef::Node(1),
            expected: SegmentId(1),
            found: SegmentId(0),
        })
    );
    assert_eq!(
        result.report.tasks_run, 6,
        "the invalid Assemble result never commits"
    );
}

#[test]
fn synthesized_complement_ownership_follows_structural_source_not_presentation_id() {
    let whole = AnyInstance::Graph(crossing_graph());
    let partition = graph_partition();
    let segments = segment::partition(&whole, &partition).test_unwrap("fixture partitions");
    let from_node_one = SynthKey::Contract {
        src: 1,
        tgt: 0,
        edge: SEdge::new("b", "a", "contract", None),
    };
    let presentation_in_zero = AnyComplementSegment::Graph(GraphComplementSegment {
        synthesized_nodes: BTreeMap::from([(0, from_node_one)]),
        ..GraphComplementSegment::default()
    });

    let accepted = run_with_complement(
        &segments,
        &partition,
        8,
        SegmentId(1),
        &presentation_in_zero,
        Budget::unlimited(),
    );
    assert!(matches!(accepted.report.status, RunStatus::Complete { .. }));

    let rejected = run_with_complement(
        &segments,
        &partition,
        9,
        SegmentId(0),
        &presentation_in_zero,
        Budget::unlimited(),
    );
    assert_eq!(
        rejected.report.status,
        RunStatus::Failed(SegmentError::WrongOwner {
            elem: ViewRef::Node(0),
            expected: SegmentId(1),
            found: SegmentId(0),
        })
    );

    let from_edge_seven = GraphComplementSegment {
        synthesized_edges: BTreeMap::from([(
            100,
            SynthKey::Expand {
                source_edge: 7,
                hop: 0,
            },
        )]),
        ..GraphComplementSegment::default()
    };
    let edge_rejected = run_with_complement(
        &segments,
        &partition,
        10,
        SegmentId(1),
        &AnyComplementSegment::Graph(from_edge_seven),
        Budget::unlimited(),
    );
    assert_eq!(
        edge_rejected.report.status,
        RunStatus::Failed(SegmentError::WrongOwner {
            elem: ViewRef::Edge(100),
            expected: SegmentId(0),
            found: SegmentId(1),
        })
    );

    let contract_expand = GraphComplementSegment {
        synthesized_nodes: BTreeMap::from([(
            101,
            SynthKey::ContractExpand {
                src: 1,
                tgt: 0,
                hop: 0,
            },
        )]),
        ..GraphComplementSegment::default()
    };
    let contract_expand_rejected = run_with_complement(
        &segments,
        &partition,
        12,
        SegmentId(0),
        &AnyComplementSegment::Graph(contract_expand),
        Budget::unlimited(),
    );
    assert_eq!(
        contract_expand_rejected.report.status,
        RunStatus::Failed(SegmentError::WrongOwner {
            elem: ViewRef::Node(101),
            expected: SegmentId(1),
            found: SegmentId(0),
        })
    );
}

#[test]
fn restriction_error_keys_order_by_element_before_kind_and_encode_in_field_order() {
    let earlier_element = ErrorKey {
        stage: 0,
        id: 3,
        id2: 0,
        kind: 2,
    };
    let later_element = ErrorKey {
        stage: 0,
        id: 9,
        id2: 0,
        kind: 1,
    };
    assert!(earlier_element < later_element);
    assert_eq!(
        earlier_element.canonical_bytes(),
        [0, 3, 0, 0, 0, 0, 0, 0, 0, 2]
    );
}

#[test]
fn unresolved_full_c10_parameters_remain_explicit() {
    use bounded::ScopeGap::{
        AllocationRanges, EdgeMultiplicity, EdgePositions, ExecutionCarrierBounds, GraphNodeBound,
        NodeMetadataAlphabet, NodeShapesAndFans, NodeValues, SchemaAlphabet,
        StateIdsAndRangeShapes, TableCarrierBounds, TreeCarrierBounds,
    };
    assert_eq!(
        bounded::FULL_SCOPE_GAPS,
        [
            SchemaAlphabet,
            GraphNodeBound,
            EdgeMultiplicity,
            NodeValues,
            NodeShapesAndFans,
            NodeMetadataAlphabet,
            EdgePositions,
            AllocationRanges,
            StateIdsAndRangeShapes,
            TableCarrierBounds,
            TreeCarrierBounds,
            ExecutionCarrierBounds,
        ]
    );
}

#[test]
fn local_invalid_wraps_all_sorted_violations() {
    let graph = crossing_graph();
    let partition = graph_partition();
    let mut segments = segment::partition(&AnyInstance::Graph(graph), &partition)
        .test_unwrap("fixture partitions");
    let AnySegment::Graph(source) = &mut segments[0] else {
        panic!("expected graph")
    };
    source.boundary = Boundary {
        ghosts: BTreeMap::from([(
            9,
            Ghost {
                node: 9,
                owner: SegmentId(1),
                anchor: "x".to_owned(),
            },
        )]),
        in_stubs: BTreeMap::new(),
    };
    let error = segment::validate_segment_contract(&segments[0], &partition);
    assert!(matches!(
        error,
        Err(SegmentError::Invalid { ref violations }) if violations.len() == 2
    ));
}
