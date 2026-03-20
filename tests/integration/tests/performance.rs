//! Integration test 7: Performance benchmark.
//!
//! Verifies that projection lift throughput exceeds a baseline threshold.
//! In debug mode, the threshold is 1K records/sec; release builds target
//! significantly higher throughput. Uses `std::time::Instant` for measurement.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use panproto_gat::Name;
use panproto_inst::value::FieldPresence;
use panproto_inst::{CompiledMigration, Node, Value, WInstance};
use panproto_mig::lift_wtype;
use panproto_schema::{Edge, Schema, Vertex};
use smallvec::SmallVec;

/// Build a simple 3-vertex schema.
fn simple_schema() -> Schema {
    let edge_text = Edge {
        src: "body".into(),
        tgt: "body.text".into(),
        kind: "prop".into(),
        name: Some("text".into()),
    };
    let edge_time = Edge {
        src: "body".into(),
        tgt: "body.time".into(),
        kind: "prop".into(),
        name: Some("time".into()),
    };

    let vertices = HashMap::from([
        (
            "body".into(),
            Vertex {
                id: "body".into(),
                kind: "object".into(),
                nsid: None,
            },
        ),
        (
            "body.text".into(),
            Vertex {
                id: "body.text".into(),
                kind: "string".into(),
                nsid: None,
            },
        ),
        (
            "body.time".into(),
            Vertex {
                id: "body.time".into(),
                kind: "string".into(),
                nsid: None,
            },
        ),
    ]);

    let edges_list = vec![edge_text, edge_time];
    let mut edges = HashMap::new();
    let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

    for e in &edges_list {
        edges.insert(e.clone(), e.kind.clone());
        outgoing.entry(e.src.clone()).or_default().push(e.clone());
        incoming.entry(e.tgt.clone()).or_default().push(e.clone());
        between
            .entry((e.src.clone(), e.tgt.clone()))
            .or_default()
            .push(e.clone());
    }

    Schema {
        protocol: "test".into(),
        vertices,
        edges,
        hyper_edges: HashMap::new(),
        constraints: HashMap::new(),
        required: HashMap::new(),
        nsids: HashMap::new(),
        variants: HashMap::new(),
        orderings: HashMap::new(),
        recursion_points: HashMap::new(),
        spans: HashMap::new(),
        usage_modes: HashMap::new(),
        nominal: HashMap::new(),
        coercions: HashMap::new(),
        mergers: HashMap::new(),
        defaults: HashMap::new(),
        policies: HashMap::new(),
        outgoing,
        incoming,
        between,
    }
}

/// Build a simple 3-node instance.
fn simple_instance() -> WInstance {
    let edge_text = Edge {
        src: "body".into(),
        tgt: "body.text".into(),
        kind: "prop".into(),
        name: Some("text".into()),
    };
    let edge_time = Edge {
        src: "body".into(),
        tgt: "body.time".into(),
        kind: "prop".into(),
        name: Some("time".into()),
    };

    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "body"));
    nodes.insert(
        1,
        Node::new(1, "body.text").with_value(FieldPresence::Present(Value::Str("hello".into()))),
    );
    nodes.insert(
        2,
        Node::new(2, "body.time").with_value(FieldPresence::Present(Value::Str("now".into()))),
    );

    WInstance::new(
        nodes,
        vec![(0, 1, edge_text), (0, 2, edge_time)],
        vec![],
        0,
        "body".into(),
    )
}

#[test]
fn simple_projection_throughput() -> Result<(), Box<dyn std::error::Error>> {
    let schema = simple_schema();
    let instance = simple_instance();

    // Identity migration: keep all vertices and edges.
    let surviving_verts = schema.vertices.keys().cloned().collect();
    let surviving_edges: HashSet<Edge> = schema.edges.keys().cloned().collect();

    let compiled = CompiledMigration {
        surviving_verts,
        surviving_edges,
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
    };

    // Warm up.
    for _ in 0..100 {
        let _ = lift_wtype(&compiled, &schema, &schema, &instance)?;
    }

    // Benchmark.
    let iterations = 100_000;
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = lift_wtype(&compiled, &schema, &schema, &instance)?;
    }
    let elapsed = start.elapsed();

    let records_per_sec = f64::from(iterations) / elapsed.as_secs_f64();

    // Report throughput (informational).
    eprintln!(
        "Simple projection: {records_per_sec:.0} records/sec ({iterations} iterations in {:.3}s)",
        elapsed.as_secs_f64()
    );

    // The target is 1M records/sec. We use a generous margin since
    // this runs in CI and debug mode. In debug mode, 10K/sec is
    // acceptable; the 1M target is for release builds.
    assert!(
        records_per_sec > 1_000.0,
        "throughput {records_per_sec:.0} records/sec is below minimum threshold (1K/sec debug mode)"
    );

    Ok(())
}

#[test]
fn projection_with_drop_throughput() -> Result<(), Box<dyn std::error::Error>> {
    let _schema = simple_schema();
    let instance = simple_instance();

    // Projection: drop body.time.
    let edge_text = Edge {
        src: "body".into(),
        tgt: "body.text".into(),
        kind: "prop".into(),
        name: Some("text".into()),
    };

    let tgt_schema = Schema {
        protocol: "test".into(),
        vertices: HashMap::from([
            (
                "body".into(),
                Vertex {
                    id: "body".into(),
                    kind: "object".into(),
                    nsid: None,
                },
            ),
            (
                "body.text".into(),
                Vertex {
                    id: "body.text".into(),
                    kind: "string".into(),
                    nsid: None,
                },
            ),
        ]),
        edges: HashMap::from([(edge_text.clone(), "prop".into())]),
        hyper_edges: HashMap::new(),
        constraints: HashMap::new(),
        required: HashMap::new(),
        nsids: HashMap::new(),
        variants: HashMap::new(),
        orderings: HashMap::new(),
        recursion_points: HashMap::new(),
        spans: HashMap::new(),
        usage_modes: HashMap::new(),
        nominal: HashMap::new(),
        coercions: HashMap::new(),
        mergers: HashMap::new(),
        defaults: HashMap::new(),
        policies: HashMap::new(),
        outgoing: HashMap::from([("body".into(), SmallVec::from([edge_text.clone()]))]),
        incoming: HashMap::from([("body.text".into(), SmallVec::from([edge_text.clone()]))]),
        between: HashMap::from([(
            ("body".into(), "body.text".into()),
            SmallVec::from([edge_text.clone()]),
        )]),
    };

    let compiled = CompiledMigration {
        surviving_verts: HashSet::from(["body".into(), "body.text".into()]),
        surviving_edges: HashSet::from([edge_text]),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
    };

    let iterations = 100_000;
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = lift_wtype(&compiled, &tgt_schema, &tgt_schema, &instance)?;
    }
    let elapsed = start.elapsed();

    let records_per_sec = f64::from(iterations) / elapsed.as_secs_f64();
    eprintln!(
        "Projection with drop: {records_per_sec:.0} records/sec ({iterations} iterations in {:.3}s)",
        elapsed.as_secs_f64()
    );

    assert!(
        records_per_sec > 1_000.0,
        "throughput {records_per_sec:.0} records/sec is below minimum threshold"
    );

    Ok(())
}
