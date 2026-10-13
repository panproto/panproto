//! No floor-governed allocation hands out `u32::MAX`.
//!
//! A recorded floor is `min(max + 1, u32::MAX)`, and 0 for an empty source,
//! so a source holding `u32::MAX` records the floor `u32::MAX`. Allocation
//! from a floor `f` hands out the free ids of `[f, u32::MAX - 1]` and then
//! fails with the typed exhaustion error of its kind
//! ([`InstError::NodeIdsExhausted`] for nodes and rows,
//! [`InstError::EdgeIdsExhausted`] for edges). Nothing is lost by this: the
//! one id withheld is the one after which no floor could be stored.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_inst::complement::tree_id_floor;
use panproto_inst::fan::Fan;
use panproto_inst::functor::FInstance;
use panproto_inst::ginstance::{EdgeId, GInstance, reify_fans};
use panproto_inst::metadata::Node;
use panproto_inst::poly::{SectionEnrichment, section};
use panproto_inst::segment::{IdAllocator, IdRange, RangeExhausted};
use panproto_inst::wtype::{CompiledMigration, WInstance};
use panproto_inst::{InstError, RowId, TreeEmbedding};
use panproto_schema::{Edge, Schema};

const TOP: u32 = u32::MAX;

fn label(kind: &str) -> Edge {
    Edge {
        src: "a".into(),
        tgt: "a".into(),
        kind: kind.into(),
        name: None,
    }
}

/// A tree whose root 0 has the one child `child`, with `fans` fans.
fn tree_with_child(child: u32, fans: Vec<Fan>) -> WInstance {
    let nodes = HashMap::from([(0, Node::new(0, "a")), (child, Node::new(child, "a"))]);
    WInstance::new(
        nodes,
        vec![(0, child, label("p"))],
        fans,
        0,
        Name::from("a"),
    )
}

/// A graph whose node 0 carries one self-loop with the edge id `edge`.
fn graph_with_edge(edge: u32) -> GInstance {
    GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_edge_id(EdgeId(edge), 0, 0, label("x"))
}

/// A table with one row, under the id `row`.
fn table_with_row(row: u32) -> FInstance {
    let mut table = FInstance::new();
    table
        .insert_row("t", RowId(row), HashMap::new())
        .expect("a fresh row id");
    table
}

fn enrichment() -> SectionEnrichment {
    SectionEnrichment {
        base_node_id: 0,
        anchor: "b".into(),
        edge: label("q"),
        value: None,
        extra_fields: Default::default(),
    }
}

/// The ids `section` adds to a base whose nodes are 0 and `child`.
fn section_ids(child: u32, count: usize) -> Result<Vec<u32>, InstError> {
    let base = tree_with_child(child, Vec::new());
    let enrichments = (0..count).map(|_| enrichment()).collect();
    let out = section(&base, &CompiledMigration::default(), enrichments)?;
    let mut added: Vec<u32> = out
        .nodes
        .keys()
        .copied()
        .filter(|id| *id != 0 && *id != child)
        .collect();
    added.sort_unstable();
    Ok(added)
}

#[test]
fn a_whole_range_allocator_withholds_the_top_id() {
    let exhausted = Err(RangeExhausted {
        range: IdRange::FULL,
    });
    let mut ids = IdAllocator::from_floor(IdRange::FULL, TOP - 1);
    assert_eq!(ids.allocate(), Ok(TOP - 1));
    assert_eq!(ids.allocate(), exhausted);
    assert_eq!(
        IdAllocator::from_floor(IdRange::FULL, TOP).allocate(),
        exhausted
    );
    assert_eq!(IdAllocator::new(IdRange::FULL).fresh([TOP - 1]), exhausted);
    assert_eq!(
        IdAllocator::new(IdRange::FULL).fresh([TOP - 2]),
        Ok(TOP - 1)
    );
}

#[test]
fn a_segment_range_ending_at_the_top_withholds_the_top_id() {
    let range = IdRange {
        lo: TOP - 2,
        hi: TOP,
    };
    let mut ids = IdAllocator::new(range);
    assert_eq!(ids.allocate(), Ok(TOP - 2));
    assert_eq!(ids.allocate(), Ok(TOP - 1));
    assert_eq!(ids.allocate(), Err(RangeExhausted { range }));
    let top_only = IdRange { lo: TOP, hi: TOP };
    assert_eq!(
        IdAllocator::new(top_only).allocate(),
        Err(RangeExhausted { range: top_only })
    );
    // A range ending below the top hands out its own last id.
    let below = IdRange {
        lo: TOP - 3,
        hi: TOP - 1,
    };
    assert_eq!(
        IdAllocator::from_floor(below, TOP - 1).allocate(),
        Ok(TOP - 1)
    );
}

#[test]
fn add_edge_withholds_the_top_edge_id() {
    let mut graph = graph_with_edge(TOP - 1);
    assert!(matches!(
        graph.add_edge(0, 0, label("y")),
        Err(InstError::EdgeIdsExhausted)
    ));
    assert_eq!(graph.edge_count(), 1);
    let mut graph = graph_with_edge(TOP - 2);
    assert_eq!(graph.add_edge(0, 0, label("y")).ok(), Some(EdgeId(TOP - 1)));
    assert!(matches!(
        graph.add_edge(0, 0, label("y")),
        Err(InstError::EdgeIdsExhausted)
    ));
}

#[test]
fn push_row_withholds_the_top_row_id() {
    let mut table = table_with_row(TOP - 1);
    assert!(matches!(
        table.fresh_row_id(),
        Err(InstError::NodeIdsExhausted)
    ));
    assert!(matches!(
        table.push_row("t", HashMap::new()),
        Err(InstError::NodeIdsExhausted)
    ));
    assert_eq!(table.row_ids(), vec![RowId(TOP - 1)]);
    let mut table = table_with_row(TOP - 2);
    assert_eq!(
        table.push_row("t", HashMap::new()).ok(),
        Some(RowId(TOP - 1))
    );
}

#[test]
fn reify_fans_withholds_the_top_fan_node_id() {
    let graph = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(TOP - 1, "a"));
    let fan = Fan::new("h", 0).with_child("c", TOP - 1);
    assert!(matches!(
        reify_fans(&graph, &[fan], None),
        Err(InstError::NodeIdsExhausted)
    ));
}

#[test]
fn section_withholds_the_top_node_id() {
    assert_eq!(section_ids(TOP - 2, 1).ok(), Some(vec![TOP - 1]));
    assert!(matches!(
        section_ids(TOP - 2, 2),
        Err(InstError::NodeIdsExhausted)
    ));
    assert!(matches!(
        section_ids(TOP - 1, 1),
        Err(InstError::NodeIdsExhausted)
    ));
    assert!(matches!(
        section_ids(TOP, 1),
        Err(InstError::NodeIdsExhausted)
    ));
}

#[test]
fn a_tree_holding_the_top_id_records_the_floor_top() {
    let tree = tree_with_child(TOP, Vec::new());
    assert_eq!(tree_id_floor(&tree).ok(), Some(TOP));
    let (graph, embedding) =
        GInstance::from_wtype_embedded(&tree, None).expect("a tree holding the top id embeds");
    assert_eq!(embedding.next_node, TOP);
    let schema = Schema {
        entries: vec![Name::from("a")],
        ..Schema::default()
    };
    let (_, read) = graph
        .to_wtype_embedded(&schema)
        .expect("a graph holding the top id reads back");
    assert_eq!(read.next_node, TOP);
    // An edit that introduces the top id raises the counter to it.
    let mut counters = TreeEmbedding::default();
    let _ = counters.observe_node(TOP);
    assert_eq!(counters.next_node, TOP);
    // Allocation against the recorded floor then fails.
    assert!(matches!(
        counters.allocate_node(),
        Err(InstError::NodeIdsExhausted)
    ));
}

#[test]
fn a_tree_graph_holding_the_top_edge_id_records_the_floor_top() {
    let graph = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "a"))
        .with_edge_id(EdgeId(TOP), 0, 1, label("p"));
    let schema = Schema {
        entries: vec![Name::from("a")],
        ..Schema::default()
    };
    let (_, read) = graph
        .to_wtype_embedded(&schema)
        .expect("a graph holding the top edge id reads back");
    assert_eq!(read.next_edge, TOP);
}

/// The ids `draw` hands out before it fails, and the failure. At most five
/// draws are made, which exhausts every floor of the last four ids.
fn drain<T, E>(mut draw: impl FnMut() -> Result<T, E>) -> (Vec<T>, Option<E>) {
    let mut ids = Vec::new();
    for _ in 0..5 {
        match draw() {
            Ok(id) => ids.push(id),
            Err(error) => return (ids, Some(error)),
        }
    }
    (ids, None)
}

/// The floors of the last four ids, `u32::MAX` included.
fn top_floors() -> std::ops::RangeInclusive<u32> {
    (TOP - 3)..=TOP
}

/// The ids allocation may hand out at floor `floor`: `[floor, u32::MAX - 1]`.
fn below_top(floor: u32) -> Vec<u32> {
    (floor..TOP).collect()
}

#[test]
fn every_id_allocator_is_exhaustive_at_the_top() {
    for floor in top_floors() {
        let expected = below_top(floor);
        // The whole-state allocator, resumed from a recorded floor.
        let mut ids = IdAllocator::from_floor(IdRange::FULL, floor);
        let (drawn, error) = drain(|| ids.allocate());
        assert_eq!(drawn, expected, "whole range at floor {floor}");
        assert_eq!(
            error,
            Some(RangeExhausted {
                range: IdRange::FULL
            })
        );
        assert_eq!(ids.floor(), TOP);
        assert!(ids.is_exhausted());
        // The whole-state allocator over a source whose largest id is
        // `floor - 1`, and its fresh id.
        let ids = IdAllocator::from_source(IdRange::FULL, [0, floor - 1]);
        assert_eq!(ids.floor(), floor);
        assert_eq!(ids.fresh([]).ok(), expected.first().copied());
        assert_eq!(ids.fresh([floor]).ok(), expected.get(1).copied());
        // Segment ranges ending at the top: one starting at the floor, and
        // one starting below it.
        for lo in [floor, TOP - 3] {
            let range = IdRange { lo, hi: TOP };
            let mut ids = IdAllocator::from_floor(range, floor);
            let (drawn, error) = drain(|| ids.allocate());
            assert_eq!(drawn, expected, "range {range} at floor {floor}");
            assert_eq!(error, Some(RangeExhausted { range }));
        }
        // A recorded floor below the range's `lo` resumes at `lo`.
        let high = IdRange {
            lo: TOP - 1,
            hi: TOP,
        };
        let mut ids = IdAllocator::from_floor(high, floor);
        let (drawn, error) = drain(|| ids.allocate());
        assert_eq!(
            drawn,
            below_top(floor.max(TOP - 1)),
            "range {high} at floor {floor}"
        );
        assert_eq!(error, Some(RangeExhausted { range: high }));
        // A range ending one below the top hands out its own `hi`.
        let range = IdRange {
            lo: TOP - 3,
            hi: TOP - 1,
        };
        let mut ids = IdAllocator::from_floor(range, floor);
        let (drawn, _) = drain(|| ids.allocate());
        assert_eq!(drawn, expected);
    }
}

#[test]
fn every_graph_and_table_allocator_is_exhaustive_at_the_top() {
    for floor in top_floors() {
        let expected = below_top(floor);
        let mut graph = graph_with_edge(floor - 1);
        let (drawn, error) = drain(|| graph.add_edge(0, 0, label("y")).map(|id| id.0));
        assert_eq!(drawn, expected, "add_edge at floor {floor}");
        assert!(matches!(error, Some(InstError::EdgeIdsExhausted)));
        let mut table = table_with_row(floor - 1);
        let (drawn, error) = drain(|| table.push_row("t", HashMap::new()).map(|id| id.0));
        assert_eq!(drawn, expected, "push_row at floor {floor}");
        assert!(matches!(error, Some(InstError::NodeIdsExhausted)));
        let mut embedding = panproto_inst::TableEmbedding {
            next_edge: floor,
            ..Default::default()
        };
        let (drawn, error) = drain(|| embedding.allocate_edge().map(|id| id.0));
        assert_eq!(drawn, expected, "table embedding at floor {floor}");
        assert!(matches!(error, Some(InstError::EdgeIdsExhausted)));
        assert_eq!(embedding.next_edge, TOP);
    }
}

#[test]
fn every_tree_allocator_is_exhaustive_at_the_top() {
    for floor in top_floors() {
        let expected = below_top(floor);
        let mut embedding = TreeEmbedding {
            next_node: floor,
            next_edge: floor,
            ..TreeEmbedding::default()
        };
        let (drawn, error) = drain(|| embedding.allocate_node());
        assert_eq!(drawn, expected, "tree nodes at floor {floor}");
        assert!(matches!(error, Some(InstError::NodeIdsExhausted)));
        assert_eq!(embedding.next_node, TOP);
        let (drawn, error) = drain(|| embedding.allocate_edge().map(|id| id.0));
        assert_eq!(drawn, expected, "tree edges at floor {floor}");
        assert!(matches!(error, Some(InstError::EdgeIdsExhausted)));
        assert_eq!(embedding.next_edge, TOP);
        // A view id against a complement recording the floor: the floor
        // itself below the top, and the typed error at it.
        let complement = panproto_inst::Complement {
            id_floor: floor,
            ..Default::default()
        };
        let empty = WInstance::new(HashMap::new(), Vec::new(), Vec::new(), 0, Name::from("a"));
        let fresh = panproto_inst::complement::fresh_view_node_id(&empty, &complement);
        assert_eq!(fresh.as_ref().ok(), expected.first(), "view id at {floor}");
        if expected.is_empty() {
            assert!(matches!(fresh, Err(InstError::NodeIdsExhausted)));
        }
    }
}

#[test]
fn every_fan_and_section_allocation_is_exhaustive_at_the_top() {
    for floor in top_floors() {
        let room = TOP - floor;
        for count in 0..=4_u32 {
            let fits = count <= room;
            let fans: Vec<Fan> = (0..count)
                .map(|k| Fan::new(format!("h{k}"), 0).with_child("c", floor - 1))
                .collect();
            // The fan nodes of a graph whose largest node is `floor - 1`.
            let graph = GInstance::new()
                .with_node(Node::new(0, "a"))
                .with_node(Node::new(floor - 1, "a"));
            let reified = reify_fans(&graph, &fans, None);
            if fits {
                let mut added: Vec<u32> = reified
                    .expect("room for every fan node")
                    .nodes()
                    .keys()
                    .copied()
                    .filter(|id| *id >= floor)
                    .collect();
                added.sort_unstable();
                assert_eq!(added, (floor..floor + count).collect::<Vec<_>>());
            } else {
                assert!(
                    matches!(reified, Err(InstError::NodeIdsExhausted)),
                    "{count} fans at floor {floor}"
                );
            }
            // The tree floor and the tree embedding's counter.
            let tree = tree_with_child(floor - 1, fans);
            // One above the last fan node when every fan fits.
            let floor_after = floor.saturating_add(count);
            match (fits, tree_id_floor(&tree)) {
                (true, Ok(found)) => assert_eq!(found, floor_after),
                (false, Err(InstError::NodeIdsExhausted)) => {}
                (_, other) => panic!("tree floor of {count} fans at {floor}: {other:?}"),
            }
            match (fits, GInstance::from_wtype_embedded(&tree, None)) {
                (true, Ok((_, embedding))) => assert_eq!(embedding.next_node, floor_after),
                (false, Err(InstError::NodeIdsExhausted)) => {}
                (_, other) => panic!("embedding of {count} fans at {floor}: {other:?}"),
            }
            // The enrichment nodes of a section.
            let added = section_ids(floor - 1, usize::try_from(count).expect("small"));
            if fits {
                assert_eq!(added.ok(), Some((floor..floor + count).collect()));
            } else {
                assert!(matches!(added, Err(InstError::NodeIdsExhausted)));
            }
        }
    }
}
