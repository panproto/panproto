//! Segment bodies, boundaries, the segment contract, the cross-segment
//! invariants, assembly and the streaming builder.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_schema::InstanceShapeKind;
use proptest::prelude::*;

use super::{TestResult, arb_ginstance, arb_winstance, interleaved, label, next_free};
use crate::functor::FInstance;
use crate::ginstance::{EdgeId, GEdge, GInstance};
use crate::metadata::Node;
use crate::segment::{
    AnySegment, BuilderEdge, BuilderNode, Ghost, GraphSegment, InStub, Partition, PartitionDefect,
    SegmentBuilder, SegmentError, SegmentId, TreeSegment, assemble_graph, assemble_table,
    assemble_tree, check_segment, partition_graph, partition_table, partition_tree,
    verify_partition,
};
use crate::view_ref::{RowId, ViewRef};
use crate::wtype::WInstance;

/// Nodes 0..6 anchored `a`, three segments of two ids each, and edges
/// 0 -> 2 (twice, in parallel), 1 -> 2, 0 -> 4, 2 -> 0 and 4 -> 4.
fn three_segment_graph() -> Result<(GInstance, Partition), SegmentError> {
    let mut graph = GInstance::new();
    for id in 0..6 {
        graph = graph.with_node(Node::new(id, "a"));
    }
    let x = label("a", "a", "prop", Some("x"));
    for (id, (s, t)) in [(0, 2), (0, 2), (1, 2), (0, 4), (2, 0), (4, 4)]
        .into_iter()
        .enumerate()
    {
        graph = graph.with_edge_id(EdgeId(u32::try_from(id).unwrap_or(0)), s, t, x.clone());
    }
    let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[2, 4], 4, 6)?;
    Ok((graph, partition))
}

#[test]
fn each_remote_target_has_exactly_one_ghost() -> TestResult {
    let (graph, partition) = three_segment_graph()?;
    let segments = partition_graph(&graph, &partition)?;
    let s0 = &segments[0];
    // Edges 0 -> 2 (twice) and 1 -> 2 share the ghost of node 2; 0 -> 4 has
    // the ghost of node 4.
    assert_eq!(
        s0.boundary.ghosts.keys().copied().collect::<Vec<_>>(),
        vec![2, 4]
    );
    assert_eq!(
        s0.boundary.ghosts[&2],
        Ghost {
            node: 2,
            owner: SegmentId(1),
            anchor: "a".into(),
        }
    );
    assert_eq!(
        segments[1]
            .boundary
            .ghosts
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![0]
    );
    // The self-loop on 4 crosses nothing.
    assert!(segments[2].boundary.ghosts.is_empty());
    // A ghost the edges do not need, or one naming the wrong owner, breaks
    // the segment's contract.
    for ghost in [
        Ghost {
            node: 5,
            owner: SegmentId(2),
            anchor: "a".into(),
        },
        Ghost {
            node: 2,
            owner: SegmentId(2),
            anchor: "a".into(),
        },
    ] {
        let mut s = s0.clone();
        s.boundary.ghosts.insert(ghost.node, ghost.clone());
        assert_eq!(
            check_segment(&AnySegment::Graph(s), &partition),
            Err(SegmentError::Invalid {
                violations: vec![SegmentError::GhostMismatch { node: ghost.node }],
            })
        );
    }
    Ok(())
}

#[test]
fn in_stubs_are_keyed_by_the_shape_view_ref() -> TestResult {
    let (graph, partition) = three_segment_graph()?;
    let segments = partition_graph(&graph, &partition)?;
    let keys: Vec<ViewRef> = segments[1].boundary.in_stubs.keys().cloned().collect();
    assert_eq!(
        keys,
        vec![
            ViewRef::Edge(EdgeId(0)),
            ViewRef::Edge(EdgeId(1)),
            ViewRef::Edge(EdgeId(2))
        ]
    );
    assert_eq!(
        segments[1].boundary.in_stubs[&ViewRef::Edge(EdgeId(2))],
        InStub {
            src: 1,
            src_owner: SegmentId(0),
            src_anchor: "a".into(),
            tgt: 2,
            label: label("a", "a", "prop", Some("x")),
        }
    );
    // Tables key an in-stub by its pair.
    let fk = label("a", "b", "fk", None);
    let tables = FInstance::from_parts(
        [
            ("a".into(), vec![(RowId(0), HashMap::new())]),
            ("b".into(), vec![(RowId(5), HashMap::new())]),
        ],
        [(fk.clone(), vec![(RowId(0), RowId(5))])],
    )?;
    let p = Partition::by_node_ranges(InstanceShapeKind::Table, &[3], 0, 6)?;
    let t = partition_table(&tables, &p)?;
    let pair = ViewRef::Pair {
        edge: fk,
        src: RowId(0),
        tgt: RowId(5),
    };
    assert_eq!(
        t[1].boundary.in_stubs.keys().cloned().collect::<Vec<_>>(),
        vec![pair]
    );
    assert_eq!(t[0].boundary.ghosts[&5].anchor.as_ref(), "b");
    // Trees key an arc's in-stub by `Arc { parent, child }`.
    let e = label("doc", "para", "prop", Some("p"));
    let tree = WInstance::new(
        HashMap::from([(0, Node::new(0, "doc")), (1, Node::new(1, "para"))]),
        vec![(0, 1, e)],
        vec![],
        0,
        "doc".into(),
    );
    let (_, embedding) = GInstance::from_wtype_embedded(&tree, None)?;
    let p = Partition::by_node_ranges(InstanceShapeKind::Tree, &[1], 0, 2)?;
    let w = partition_tree(&tree, &embedding, &p)?;
    assert_eq!(
        w[1].boundary.in_stubs.keys().cloned().collect::<Vec<_>>(),
        vec![ViewRef::Arc {
            parent: 0,
            child: 1
        }]
    );
    Ok(())
}

proptest! {
    #[test]
    fn tree_segments_merge_to_the_whole_arc_order_and_hold_one_root(
        (tree, embedding) in arb_winstance(),
        k in 1u32..4,
    ) {
        let free = next_free(embedding.next_node.checked_sub(1), embedding.next_edge.checked_sub(1));
        let partition = interleaved(InstanceShapeKind::Tree, free.max(1), k, 0);
        let segments = partition_tree(&tree, &embedding, &partition).map_err(|e| TestCaseError::fail(e.to_string()))?;
        let roots: Vec<(SegmentId, u32)> = segments.iter().filter_map(|s| s.root.map(|r| (s.segment, r))).collect();
        let owner = partition.owner_of_node(tree.root);
        prop_assert_eq!(roots, vec![(owner.unwrap_or(SegmentId(u32::MAX)), tree.root)]);
        // Merging the tree arcs of every segment by edge id gives the arc
        // order.
        let mut merged: BTreeMap<EdgeId, (u32, u32)> = BTreeMap::new();
        for s in &segments {
            let fan_edges = s.fan_edge_ids();
            merged.extend(s.arcs.iter().filter(|(id, _)| !fan_edges.contains(id)).map(|(id, (p, c, _))| (*id, (*p, *c))));
        }
        let order: Vec<(u32, u32)> = merged.into_values().collect();
        let whole: Vec<(u32, u32)> = tree.arcs.iter().map(|(p, c, _)| (*p, *c)).collect();
        prop_assert_eq!(order, whole);
        // Every owned node whose parent arc is an in-stub roots a local
        // subtree.
        for s in &segments {
            let fan_edges = s.fan_edge_ids();
            let local_children: BTreeSet<u32> = s
                .arcs
                .iter()
                .filter(|(id, _)| !fan_edges.contains(id))
                .map(|(_, (_, c, _))| *c)
                .collect();
            for (key, stub) in &s.boundary.in_stubs {
                if let ViewRef::Arc { child, .. } = key {
                    prop_assert!(!local_children.contains(child));
                    prop_assert_eq!(stub.tgt, *child);
                }
            }
        }
    }
}

#[test]
fn any_segment_serde_tags_are_snake_case() -> TestResult {
    let (graph, partition) = three_segment_graph()?;
    let segment = AnySegment::Graph(partition_graph(&graph, &partition)?.remove(2));
    let json = serde_json::to_value(&segment)?;
    assert_eq!(
        json.as_object()
            .map(|o| o.keys().cloned().collect::<Vec<_>>()),
        Some(vec!["graph".to_owned()])
    );
    let back: AnySegment = serde_json::from_value(json)?;
    assert_eq!(
        back.canonical_bytes(&crate::identity::Digest([0; 32]), &partition)?,
        segment.canonical_bytes(&crate::identity::Digest([0; 32]), &partition)?
    );
    let table = AnySegment::Table(crate::segment::TableSegment {
        segment: SegmentId(0),
        rows: BTreeMap::new(),
        pairs: BTreeSet::new(),
        boundary: crate::segment::Boundary::default(),
    });
    assert_eq!(
        serde_json::to_string(&table)?,
        r#"{"table":{"segment":0,"rows":{},"pairs":[],"boundary":{"ghosts":{},"in_stubs":[]}}}"#
    );
    let tree = AnySegment::Tree(TreeSegment {
        segment: SegmentId(1),
        nodes: BTreeMap::new(),
        arcs: BTreeMap::new(),
        fans: vec![],
        root: None,
        next_node: None,
        next_edge: None,
        boundary: crate::segment::Boundary::default(),
    });
    assert_eq!(
        serde_json::to_string(&tree)?,
        r#"{"tree":{"segment":1,"nodes":{},"arcs":{},"fans":[],"root":null,"next_node":null,"next_edge":null,"boundary":{"ghosts":{},"in_stubs":[]}}}"#
    );
    Ok(())
}

#[test]
fn invalid_segments_list_every_violation_sorted() -> TestResult {
    let (graph, partition) = three_segment_graph()?;
    let mut s0: GraphSegment = partition_graph(&graph, &partition)?.remove(0);
    // (a) a node owned by segment 2 held here; (b) an edge to node 5 without
    // its ghost; (c) an in-stub naming the wrong source owner. All three are
    // reported, sorted by the element.
    s0.nodes.insert(5, Node::new(5, "a"));
    s0.edges.insert(
        EdgeId(9),
        GEdge::new(1, 5, label("a", "a", "prop", Some("x"))),
    );
    s0.boundary.in_stubs.insert(
        ViewRef::Edge(EdgeId(7)),
        InStub {
            src: 2,
            src_owner: SegmentId(2),
            src_anchor: "a".into(),
            tgt: 0,
            label: label("a", "a", "prop", None),
        },
    );
    let result = check_segment(&AnySegment::Graph(s0), &partition);
    assert_eq!(
        result,
        Err(SegmentError::Invalid {
            violations: vec![
                SegmentError::WrongOwner {
                    elem: ViewRef::Node(5),
                    expected: SegmentId(2),
                    found: SegmentId(0)
                },
                SegmentError::GhostMismatch { node: 5 },
                SegmentError::InStubMismatch {
                    elem: ViewRef::Edge(EdgeId(7))
                },
            ],
        })
    );
    Ok(())
}

/// Three tree segments under `[0], [1], [2]` whose parent relation over the
/// boundary is the cycle 1 -> 2 -> 1, which the root 0 does not reach.
fn cyclic_tree() -> Result<(Partition, Vec<AnySegment>), SegmentError> {
    let p = Partition::by_node_ranges(InstanceShapeKind::Tree, &[1, 2], 0, 3)?;
    let e = label("doc", "doc", "prop", None);
    let node = |id| (id, Node::new(id, "doc"));
    let stub = |src: u32, owner: u32, tgt: u32| InStub {
        src,
        src_owner: SegmentId(owner),
        src_anchor: "doc".into(),
        tgt,
        label: e.clone(),
    };
    let ghost = |node: u32, owner: u32| Ghost {
        node,
        owner: SegmentId(owner),
        anchor: "doc".into(),
    };
    let mut s1 = TreeSegment {
        segment: SegmentId(1),
        nodes: BTreeMap::from([node(1)]),
        arcs: BTreeMap::from([(EdgeId(0), (1, 2, e.clone()))]),
        fans: vec![],
        root: None,
        next_node: None,
        next_edge: None,
        boundary: crate::segment::Boundary::default(),
    };
    s1.boundary.ghosts.insert(2, ghost(2, 2));
    s1.boundary.in_stubs.insert(
        ViewRef::Arc {
            parent: 2,
            child: 1,
        },
        stub(2, 2, 1),
    );
    let mut s2 = TreeSegment {
        segment: SegmentId(2),
        nodes: BTreeMap::from([node(2)]),
        arcs: BTreeMap::from([(EdgeId(1), (2, 1, e.clone()))]),
        fans: vec![],
        root: None,
        next_node: None,
        next_edge: None,
        boundary: crate::segment::Boundary::default(),
    };
    s2.boundary.ghosts.insert(1, ghost(1, 1));
    s2.boundary.in_stubs.insert(
        ViewRef::Arc {
            parent: 1,
            child: 2,
        },
        stub(1, 1, 2),
    );
    let s0 = TreeSegment {
        segment: SegmentId(0),
        nodes: BTreeMap::from([node(0)]),
        arcs: BTreeMap::new(),
        fans: vec![],
        root: Some(0),
        next_node: Some(3),
        next_edge: Some(2),
        boundary: crate::segment::Boundary::default(),
    };
    let cyclic = vec![
        AnySegment::Tree(s0),
        AnySegment::Tree(s1),
        AnySegment::Tree(s2),
    ];
    Ok((p, cyclic))
}

#[test]
fn partition_verification_rejects_ghost_mismatch_duplicate_edge_ids_and_tree_cycles() -> TestResult
{
    let (graph, partition) = three_segment_graph()?;
    let segments: Vec<AnySegment> = partition_graph(&graph, &partition)?
        .into_iter()
        .map(AnySegment::Graph)
        .collect();
    assert_eq!(verify_partition(&segments, &partition), Ok(()));
    // A ghost anchor that differs from its owner's node.
    let mut tampered = segments.clone();
    if let AnySegment::Graph(s) = &mut tampered[0] {
        s.boundary.ghosts.insert(
            2,
            Ghost {
                node: 2,
                owner: SegmentId(1),
                anchor: "b".into(),
            },
        );
    }
    assert_eq!(
        verify_partition(&tampered, &partition),
        Err(SegmentError::GhostMismatch { node: 2 })
    );
    // An edge id held by two segments.
    let mut duplicated = segments;
    if let AnySegment::Graph(s) = &mut duplicated[2] {
        s.edges.insert(
            EdgeId(0),
            GEdge::new(4, 5, label("a", "a", "prop", Some("x"))),
        );
    }
    assert_eq!(
        verify_partition(&duplicated, &partition),
        Err(SegmentError::DuplicateEdgeId {
            edge: EdgeId(0),
            segments: vec![SegmentId(0), SegmentId(2)]
        })
    );
    // A tree whose parent relation over the boundary has a cycle 1 -> 2 -> 1
    // that the root does not reach.
    let (p, cyclic) = cyclic_tree()?;
    assert_eq!(
        verify_partition(&cyclic, &p),
        Err(SegmentError::NotEmbedded {
            elem: ViewRef::Node(1)
        })
    );
    Ok(())
}

#[test]
fn assembly_reports_the_first_error_in_segment_then_view_ref_order() -> TestResult {
    let (graph, partition) = three_segment_graph()?;
    let segments = partition_graph(&graph, &partition)?;
    let mut broken = segments.clone();
    // Segment 2 holds a node of segment 0; segment 1 loses the in-stub of
    // edge 2 and holds a ghost of node 0 under the wrong anchor. The first
    // error is segment 1's, and within it the least element: the in-stub
    // of edge 2 sorts after node 0's ghost.
    broken[2].nodes.insert(1, Node::new(1, "a"));
    broken[1]
        .boundary
        .in_stubs
        .remove(&ViewRef::Edge(EdgeId(2)));
    broken[1].boundary.ghosts.insert(
        0,
        Ghost {
            node: 0,
            owner: SegmentId(0),
            anchor: "z".into(),
        },
    );
    assert_eq!(
        assemble_graph(broken, &partition).err(),
        Some(SegmentError::GhostMismatch { node: 0 })
    );
    let mut later = segments.clone();
    later[1].boundary.in_stubs.remove(&ViewRef::Edge(EdgeId(2)));
    later[2].nodes.insert(1, Node::new(1, "a"));
    assert_eq!(
        assemble_graph(later, &partition).err(),
        Some(SegmentError::InStubMismatch {
            elem: ViewRef::Edge(EdgeId(2))
        })
    );
    let only_two = segments[..2].to_vec();
    assert_eq!(
        assemble_graph(only_two, &partition).err(),
        Some(SegmentError::MalformedPartition {
            segment: SegmentId(2),
            defect: crate::segment::PartitionDefect::SegmentMissing
        })
    );
    assert!(GInstance::identical(
        &assemble_graph(segments, &partition)?,
        &graph
    ));
    let tables = FInstance::new();
    let p = Partition::single(InstanceShapeKind::Table, None);
    assert_eq!(assemble_table(partition_table(&tables, &p)?, &p)?, tables);
    let tree_p = Partition::single(InstanceShapeKind::Tree, None);
    assert_eq!(
        assemble_tree(Vec::new(), &tree_p).err(),
        Some(SegmentError::MalformedPartition {
            segment: SegmentId(0),
            defect: crate::segment::PartitionDefect::SegmentMissing
        })
    );
    Ok(())
}

/// Rebuild every segment of `segments` through a [`SegmentBuilder`],
/// pushing each element of the original, and anchoring ghosts by `anchor`.
fn rebuild(
    segments: &[AnySegment],
    partition: &Partition,
    anchor: &dyn Fn(u32) -> Option<panproto_gat::Name>,
) -> Result<Vec<AnySegment>, SegmentError> {
    segments
        .iter()
        .map(|segment| {
            let mut b = SegmentBuilder::new(partition, segment.segment())?;
            match segment {
                AnySegment::Graph(s) => {
                    for n in s.nodes.values() {
                        b.push_node(BuilderNode::Node(n.clone()))?;
                    }
                    for (id, e) in &s.edges {
                        b.push_edge(BuilderEdge::Graph {
                            id: *id,
                            edge: e.clone(),
                        })?;
                    }
                }
                AnySegment::Table(s) => {
                    for (id, (table, row)) in &s.rows {
                        b.push_node(BuilderNode::Row {
                            id: *id,
                            table: table.clone(),
                            row: row.clone(),
                        })?;
                    }
                    for (edge, src, tgt) in &s.pairs {
                        b.push_edge(BuilderEdge::Pair {
                            edge: edge.clone(),
                            src: *src,
                            tgt: *tgt,
                        })?;
                    }
                }
                AnySegment::Tree(s) => {
                    let fans: BTreeMap<u32, _> = s.fans.iter().map(|f| (f.node, f)).collect();
                    for n in s.nodes.values() {
                        let node = match fans.get(&n.id) {
                            Some(fan) => BuilderNode::Fan {
                                embedding: (*fan).clone(),
                                hyper_edge: n.anchor.clone(),
                            },
                            None if s.root == Some(n.id) => BuilderNode::Root {
                                node: n.clone(),
                                next_node: s.next_node.unwrap_or(0),
                                next_edge: s.next_edge.unwrap_or(0),
                            },
                            None => BuilderNode::Node(n.clone()),
                        };
                        b.push_node(node)?;
                    }
                    let fan_edges = s.fan_edge_ids();
                    for (id, (src, tgt, l)) in &s.arcs {
                        b.push_edge(if fan_edges.contains(id) {
                            BuilderEdge::FanEdge {
                                id: *id,
                                fan: *src,
                                tgt: *tgt,
                                label: l.clone(),
                            }
                        } else {
                            BuilderEdge::Arc {
                                id: *id,
                                src: *src,
                                tgt: *tgt,
                                label: l.clone(),
                            }
                        })?;
                    }
                }
            }
            for (key, stub) in &segment.boundary().in_stubs {
                b.push_in_stub(key.clone(), stub.clone())?;
            }
            b.finish(anchor)
        })
        .collect()
}

proptest! {
    #[test]
    fn segment_builder_output_equals_partition_output(
        graph in arb_ginstance(),
        (tree, embedding) in arb_winstance(),
        k in 1u32..4,
    ) {
        let digest = crate::identity::Digest([0; 32]);
        let fail = |e: SegmentError| TestCaseError::fail(e.to_string());
        let free = next_free(graph.max_node_id(), graph.max_edge_id().map(|e| e.0));
        let p = interleaved(InstanceShapeKind::Graph, free.max(1), k, 2);
        let split: Vec<AnySegment> = partition_graph(&graph, &p).map_err(fail)?.into_iter().map(AnySegment::Graph).collect();
        let built = rebuild(&split, &p, &|n| graph.node(n).map(|n| n.anchor.clone())).map_err(fail)?;
        for (a, b) in split.iter().zip(&built) {
            prop_assert_eq!(a.canonical_bytes(&digest, &p).map_err(fail)?, b.canonical_bytes(&digest, &p).map_err(fail)?);
        }
        let free = next_free(embedding.next_node.checked_sub(1), embedding.next_edge.checked_sub(1));
        let p = interleaved(InstanceShapeKind::Tree, free.max(1), k, 0);
        let split: Vec<AnySegment> = partition_tree(&tree, &embedding, &p).map_err(fail)?.into_iter().map(AnySegment::Tree).collect();
        let anchors: HashMap<u32, panproto_gat::Name> = split.iter().flat_map(|s| s.node_anchors().into_iter().map(|(id, a)| (id, a.clone()))).collect();
        let built = rebuild(&split, &p, &|n| anchors.get(&n).cloned()).map_err(fail)?;
        for (a, b) in split.iter().zip(&built) {
            prop_assert_eq!(a.canonical_bytes(&digest, &p).map_err(fail)?, b.canonical_bytes(&digest, &p).map_err(fail)?);
        }
    }
}

#[test]
fn a_builder_refuses_elements_its_segment_does_not_own() -> TestResult {
    let (graph, partition) = three_segment_graph()?;
    let mut b = SegmentBuilder::new(&partition, SegmentId(0))?;
    assert_eq!(
        b.push_node(BuilderNode::Node(Node::new(3, "a"))),
        Err(SegmentError::WrongOwner {
            elem: ViewRef::Node(3),
            expected: SegmentId(1),
            found: SegmentId(0)
        })
    );
    let x = label("a", "a", "prop", Some("x"));
    assert_eq!(
        b.push_edge(BuilderEdge::Graph {
            id: EdgeId(4),
            edge: GEdge::new(2, 0, x.clone())
        }),
        Err(SegmentError::WrongOwner {
            elem: ViewRef::Edge(EdgeId(4)),
            expected: SegmentId(1),
            found: SegmentId(0)
        })
    );
    assert_eq!(
        b.push_node(BuilderNode::Row {
            id: RowId(0),
            table: "a".into(),
            row: HashMap::new()
        }),
        Err(SegmentError::ShapeMismatch {
            expected: InstanceShapeKind::Graph,
            found: InstanceShapeKind::Table
        })
    );
    b.push_node(BuilderNode::Node(Node::new(0, "a")))?;
    b.push_edge(BuilderEdge::Graph {
        id: EdgeId(3),
        edge: GEdge::new(0, 4, x),
    })?;
    assert_eq!(
        b.finish(&|_| None).err(),
        Some(SegmentError::GhostMismatch { node: 4 })
    );
    assert!(graph.node(4).is_some());
    Ok(())
}

/// Nodes 0 (`doc`), 1 and 2 (`para`) with arcs 0 -> 1 and 0 -> 2, and its
/// embedding after deleting node 2: the embedding's counters stay above
/// the deleted node and its arc.
fn edited_tree() -> Result<(WInstance, crate::ginstance::TreeEmbedding), Box<dyn std::error::Error>>
{
    let e = |child: u32| label("doc", "para", "prop", Some(&format!("c{child}")));
    let mut tree = WInstance::new(
        HashMap::from([
            (0, Node::new(0, "doc")),
            (1, Node::new(1, "para")),
            (2, Node::new(2, "para")),
        ]),
        vec![(0, 1, e(1)), (0, 2, e(2))],
        vec![],
        0,
        "doc".into(),
    );
    let (_, mut embedding) = GInstance::from_wtype_embedded(&tree, None)?;
    embedding.apply_edit(&mut tree, &crate::tree_edit::TreeEdit::DeleteNode { id: 2 })?;
    Ok((tree, embedding))
}

#[test]
fn assembling_a_tree_keeps_the_embedding_counters_of_an_edited_tree() -> TestResult {
    let (tree, embedding) = edited_tree()?;
    assert_eq!((embedding.next_node, embedding.next_edge), (3, 2));
    for partition in [
        Partition::single(InstanceShapeKind::Tree, Some(100)),
        Partition::by_node_ranges(InstanceShapeKind::Tree, &[1], 0, 3)?,
        Partition::by_node_ranges(InstanceShapeKind::Tree, &[1, 2], 4, 3)?,
    ] {
        let parts = partition_tree(&tree, &embedding, &partition)?;
        let (_, back) = assemble_tree(parts, &partition)?;
        assert_eq!(back, embedding);
    }
    // Counters raised further, as a valid embedding may carry them, also
    // come back.
    let mut raised = embedding.clone();
    raised.next_node += 5;
    raised.next_edge += 5;
    let partition = Partition::single(InstanceShapeKind::Tree, Some(100));
    let parts = partition_tree(&tree, &raised, &partition)?;
    assert_eq!(assemble_tree(parts, &partition)?.1, raised);
    // An embedding whose counters do not lie above every id it uses is
    // refused.
    let mut low = embedding.clone();
    low.next_node = 1;
    assert_eq!(
        partition_tree(&tree, &low, &partition).err(),
        Some(SegmentError::NotEmbedded {
            elem: ViewRef::Node(1)
        })
    );
    let mut low = embedding;
    low.next_edge = 0;
    assert_eq!(
        partition_tree(&tree, &low, &partition).err(),
        Some(SegmentError::NotEmbedded {
            elem: ViewRef::Edge(EdgeId(0))
        })
    );
    Ok(())
}

#[test]
fn a_builder_names_a_misplaced_tree_arc_by_its_arc_reference() -> TestResult {
    let partition = Partition::by_node_ranges(InstanceShapeKind::Tree, &[10], 0, 20)?;
    let mut b = SegmentBuilder::new(&partition, SegmentId(0))?;
    assert_eq!(
        b.push_edge(BuilderEdge::Arc {
            id: EdgeId(4),
            src: 12,
            tgt: 13,
            label: label("doc", "para", "prop", None),
        }),
        Err(SegmentError::WrongOwner {
            elem: ViewRef::Arc {
                parent: 12,
                child: 13
            },
            expected: SegmentId(1),
            found: SegmentId(0)
        })
    );
    Ok(())
}

/// Whether `result` is the contract failure listing an in-stub mismatch at
/// `key`.
fn lists_stub_mismatch(result: Result<(), SegmentError>, key: &ViewRef) -> bool {
    matches!(result, Err(SegmentError::Invalid { violations })
        if violations.contains(&SegmentError::InStubMismatch { elem: key.clone() }))
}

/// Move the in-stub at `from` of `segment` to the key `to`.
fn rekey(segment: &mut AnySegment, from: &ViewRef, to: ViewRef) {
    let stubs = match segment {
        AnySegment::Graph(s) => &mut s.boundary.in_stubs,
        AnySegment::Table(s) => &mut s.boundary.in_stubs,
        AnySegment::Tree(s) => &mut s.boundary.in_stubs,
    };
    if let Some(stub) = stubs.remove(from) {
        stubs.insert(to, stub);
    }
}

#[test]
fn an_in_stub_keyed_by_another_shape_s_reference_breaks_the_segment_contract() -> TestResult {
    // A graph in-stub keyed by a tree arc.
    let (graph, partition) = three_segment_graph()?;
    let mut graph_seg = AnySegment::Graph(partition_graph(&graph, &partition)?.remove(1));
    let arc = ViewRef::Arc {
        parent: 1,
        child: 2,
    };
    rekey(&mut graph_seg, &ViewRef::Edge(EdgeId(2)), arc.clone());
    assert!(lists_stub_mismatch(
        check_segment(&graph_seg, &partition),
        &arc
    ));
    // A table in-stub keyed by a graph edge.
    let fk = label("a", "b", "fk", None);
    let tables = FInstance::from_parts(
        [
            ("a".into(), vec![(RowId(0), HashMap::new())]),
            ("b".into(), vec![(RowId(5), HashMap::new())]),
        ],
        [(fk.clone(), vec![(RowId(0), RowId(5))])],
    )?;
    let tp = Partition::by_node_ranges(InstanceShapeKind::Table, &[3], 0, 6)?;
    let mut table_seg = AnySegment::Table(partition_table(&tables, &tp)?.remove(1));
    let pair = ViewRef::Pair {
        edge: fk,
        src: RowId(0),
        tgt: RowId(5),
    };
    rekey(&mut table_seg, &pair, ViewRef::Edge(EdgeId(3)));
    assert!(lists_stub_mismatch(
        check_segment(&table_seg, &tp),
        &ViewRef::Edge(EdgeId(3))
    ));
    // A tree arc's in-stub keyed by a graph edge: only fan edges are keyed
    // by edge id in a tree.
    let e = label("doc", "para", "prop", Some("p"));
    let tree = WInstance::new(
        HashMap::from([(0, Node::new(0, "doc")), (1, Node::new(1, "para"))]),
        vec![(0, 1, e)],
        vec![],
        0,
        "doc".into(),
    );
    let (_, embedding) = GInstance::from_wtype_embedded(&tree, None)?;
    let wp = Partition::by_node_ranges(InstanceShapeKind::Tree, &[1], 0, 2)?;
    let mut tree_seg = AnySegment::Tree(partition_tree(&tree, &embedding, &wp)?.remove(1));
    let arc = ViewRef::Arc {
        parent: 0,
        child: 1,
    };
    rekey(&mut tree_seg, &arc, ViewRef::Edge(EdgeId(0)));
    assert!(lists_stub_mismatch(
        check_segment(&tree_seg, &wp),
        &ViewRef::Edge(EdgeId(0))
    ));
    Ok(())
}

#[test]
fn two_fan_labels_into_one_remote_node_leave_two_in_stubs_keyed_by_edge_id() -> TestResult {
    // The fan over parent 1 names node 0 under two labels. Its fan node is
    // owned with node 1, so both labelled edges cross into node 0's
    // segment: as `Arc { parent: fan node, child: 0 }` they would share one
    // key, so a tree keys fan edges by their edge ids.
    let tree = WInstance::new(
        HashMap::from([(0, Node::new(0, "doc")), (1, Node::new(1, "para"))]),
        vec![(0, 1, label("doc", "para", "prop", Some("p")))],
        vec![
            crate::fan::Fan::new("rel", 1)
                .with_child("a".to_owned(), 0)
                .with_child("b".to_owned(), 0),
        ],
        0,
        "doc".into(),
    );
    let (_, embedding) = GInstance::from_wtype_embedded(&tree, None)?;
    let fan = embedding.fans[0].clone();
    let free = next_free(
        embedding.next_node.checked_sub(1),
        embedding.next_edge.checked_sub(1),
    );
    let p = Partition::by_node_ranges(InstanceShapeKind::Tree, &[1], 0, free)?;
    let parts = partition_tree(&tree, &embedding, &p)?;
    let stubs = &parts[0].boundary.in_stubs;
    let keys: Vec<ViewRef> = stubs.keys().cloned().collect();
    assert_eq!(
        keys,
        vec![
            ViewRef::Edge(fan.child_edges["a"]),
            ViewRef::Edge(fan.child_edges["b"])
        ]
    );
    assert!(stubs.values().all(|s| s.src == fan.node && s.tgt == 0));
    let (back, back_embedding) = assemble_tree(parts, &p)?;
    assert!(super::same_tree(&back, &tree));
    assert_eq!(back_embedding, embedding);
    Ok(())
}

#[test]
fn tree_segment_counters_are_checked_in_the_root_segment_and_across_segments() -> TestResult {
    let (tree, embedding) = edited_tree()?;
    let p = Partition::by_node_ranges(InstanceShapeKind::Tree, &[1], 0, 3)?;
    let parts = partition_tree(&tree, &embedding, &p)?;
    assert_eq!(
        (parts[0].next_node, parts[0].next_edge, parts[1].next_node),
        (Some(3), Some(2), None)
    );
    let not_embedded = |elem: ViewRef| SegmentError::NotEmbedded { elem };
    let lists = |result: Result<(), SegmentError>, elem: ViewRef| {
        matches!(result, Err(SegmentError::Invalid { violations })
            if violations.contains(&not_embedded(elem)))
    };
    // The root's segment without its counters.
    let mut bare = parts[0].clone();
    bare.next_edge = None;
    assert!(lists(
        check_segment(&AnySegment::Tree(bare), &p),
        ViewRef::Node(0)
    ));
    // Counters in a segment without the root: a defect of the segment, not
    // of any element, since a counter's value need not be any id held.
    let stray_counters = SegmentError::MalformedPartition {
        segment: SegmentId(1),
        defect: PartitionDefect::CountersWithoutRoot,
    };
    for (node, edge) in [(Some(3), Some(2)), (Some(3), None), (None, Some(9))] {
        let mut stray = parts[1].clone();
        stray.next_node = node;
        stray.next_edge = edge;
        let stray = AnySegment::Tree(stray);
        assert_eq!(
            check_segment(&stray, &p).err(),
            Some(stray_counters.clone())
        );
        let laid_out = vec![AnySegment::Tree(parts[0].clone()), stray];
        assert_eq!(
            verify_partition(&laid_out, &p).err(),
            Some(stray_counters.clone())
        );
    }
    // An empty segment without the root holding only the edge counter: no
    // node 9 exists to be named.
    let empty = TreeSegment {
        segment: SegmentId(1),
        nodes: BTreeMap::new(),
        arcs: BTreeMap::new(),
        fans: Vec::new(),
        root: None,
        next_node: None,
        next_edge: Some(9),
        boundary: crate::segment::Boundary::default(),
    };
    assert_eq!(
        check_segment(&AnySegment::Tree(empty), &p).err(),
        Some(stray_counters)
    );
    // A counter at or below an id another segment holds: node 1 is in
    // segment 1, so only the check across segments sees it.
    let mut low = parts;
    low[0].next_node = Some(1);
    check_segment(&AnySegment::Tree(low[0].clone()), &p)?;
    let any: Vec<AnySegment> = low.iter().cloned().map(AnySegment::Tree).collect();
    assert_eq!(
        verify_partition(&any, &p).err(),
        Some(not_embedded(ViewRef::Node(1)))
    );
    assert_eq!(
        assemble_tree(low, &p).err(),
        Some(not_embedded(ViewRef::Node(1)))
    );
    Ok(())
}
