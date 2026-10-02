#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Graph migration coverage: one record per weakly connected component,
//! named by its least node id, failing exactly when the graph restrict of
//! the component errors, with the ids the successful records drop.

use panproto_gat::Name;
use panproto_inst::{BoundaryProtocols, CompiledMigration, EdgeId, GInstance, Node};
use panproto_mig::{PartialReason, check_graph_coverage};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

fn e(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

/// `r -c-> c`, `r -d-> d -k-> k` in the source; the target keeps `r`, `c`
/// and `k` with only `r -c-> c`, so a path `r -> d -> k` contracts to a
/// pair with no target edge.
fn schemas() -> (Schema, Schema, CompiledMigration) {
    let protocol = Protocol::default();
    let src = SchemaBuilder::new(&protocol)
        .vertex("r", "record", None)
        .and_then(|b| b.vertex("c", "record", None))
        .and_then(|b| b.vertex("d", "record", None))
        .and_then(|b| b.vertex("k", "record", None))
        .and_then(|b| b.edge("r", "c", "prop", Some("c")))
        .and_then(|b| b.edge("r", "d", "prop", Some("d")))
        .and_then(|b| b.edge("d", "k", "prop", Some("k")))
        .and_then(SchemaBuilder::build)
        .unwrap();
    let tgt = SchemaBuilder::new(&protocol)
        .vertex("r", "record", None)
        .and_then(|b| b.vertex("c", "record", None))
        .and_then(|b| b.vertex("k", "record", None))
        .and_then(|b| b.edge("r", "c", "prop", Some("c")))
        .and_then(SchemaBuilder::build)
        .unwrap();
    let m = CompiledMigration {
        surviving_verts: ["r", "c", "k"].iter().map(|v| Name::from(*v)).collect(),
        ..CompiledMigration::default()
    };
    (src, tgt, m)
}

/// Four components: `{0, 1}` kept whole, `{2, 3}` losing the dropped `d`,
/// `{4, 5, 6}` whose contracted pair has no target edge, and `{7}` alone.
fn instance() -> GInstance {
    let mut g = GInstance::new();
    for (id, anchor) in [
        (0, "r"),
        (1, "c"),
        (2, "r"),
        (3, "d"),
        (4, "r"),
        (5, "d"),
        (6, "k"),
        (7, "r"),
    ] {
        g = g.with_node(Node::new(id, anchor));
    }
    g.with_edge(0, 1, e("r", "c", "c"))
        .with_edge(2, 3, e("r", "d", "d"))
        .with_edge(4, 5, e("r", "d", "d"))
        .with_edge(5, 6, e("d", "k", "k"))
}

#[test]
fn graph_coverage_reports_one_record_per_component() {
    let (src, tgt, m) = schemas();
    let protocol = Protocol::default();
    let protocols = BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    let report = check_graph_coverage(&m, &[instance()], &src, &tgt, protocols);
    assert_eq!(report.coverage.total_records, 4);
    assert_eq!(report.coverage.successful, 3);
    assert!((report.coverage.coverage_ratio - 0.75).abs() < f64::EPSILON);
    // Dropping data is not a failure: the dropped `d` and its edge are
    // reported beside the coverage.
    assert_eq!(report.dropped_nodes, vec![3]);
    assert_eq!(report.dropped_edges, vec![EdgeId(1)]);
}

#[test]
fn graph_coverage_fails_a_component_only_when_its_restrict_errors() {
    let (src, tgt, m) = schemas();
    let protocol = Protocol::default();
    let protocols = BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    let report = check_graph_coverage(&m, &[instance()], &src, &tgt, protocols);
    let failed: Vec<(u32, &PartialReason)> = report
        .coverage
        .failed
        .iter()
        .map(|f| (f.record_id, &f.reason))
        .collect();
    // The record is named by its least node; its reason is the unlocated
    // `NoEdgeFound` of the contracted pair `(4, 6)`.
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].0, 4);
    assert!(
        matches!(failed[0].1, PartialReason::MissingRequiredField { field } if field == "edge r → k"),
        "{:?}",
        failed[0].1
    );
    // The same component restricted on its own errors with that pair, and
    // the others restrict.
    let whole =
        panproto_inst::graph_restrict_with_complement(&instance(), &src, &tgt, &m, protocols)
            .unwrap_err();
    assert_eq!(whole.key().map(|k| (k.stage, k.id, k.id2)), Some((2, 4, 6)));
}
