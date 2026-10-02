#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The cost of compiling a contraction plan grows with the schema and the
//! regions it returns, never with the planned pairs times the edges.

use std::time::{Duration, Instant};

use panproto_gat::Name;
use panproto_inst::{CompiledMigration, ContractionPlan};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

fn e(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

/// A dropped hub `h` with `n` surviving spokes `s0 .. s(n-1)`, each with an
/// edge into the hub and an edge out of it, and the migration that keeps
/// every spoke: `n * n` planned pairs, each with a region of two edges, in
/// a schema of `n + 1` vertices and `2 n` edges.
fn hub(n: usize) -> (Schema, Schema, CompiledMigration) {
    let spokes: Vec<String> = (0..n).map(|i| format!("s{i}")).collect();
    let mut src = SchemaBuilder::new(&Protocol::default())
        .vertex("h", "record", None)
        .unwrap();
    let mut tgt = SchemaBuilder::new(&Protocol::default());
    for s in &spokes {
        src = src.vertex(s, "record", None).unwrap();
        tgt = tgt.vertex(s, "record", None).unwrap();
    }
    for s in &spokes {
        src = src
            .edge(s, "h", "prop", Some("in"))
            .unwrap()
            .edge("h", s, "prop", Some("out"))
            .unwrap();
    }
    let migration = CompiledMigration {
        surviving_verts: spokes.iter().map(|s| Name::from(s.as_str())).collect(),
        ..CompiledMigration::default()
    };
    (src.build().unwrap(), tgt.build().unwrap(), migration)
}

/// The compile time of the hub, after checking the plan it compiles.
fn compile_time(hub: &(Schema, Schema, CompiledMigration), n: usize) -> Duration {
    let (src, tgt, migration) = hub;
    let start = Instant::now();
    let plan = ContractionPlan::compile(src, tgt, migration).unwrap();
    let elapsed = start.elapsed();
    assert_eq!(plan.pairs.len(), n * n);
    let region = &plan.pairs[&(Name::from("s0"), Name::from("s1"))].region;
    assert_eq!(
        region.iter().cloned().collect::<Vec<_>>(),
        vec![e("h", "s1", "out"), e("s0", "h", "in")]
    );
    assert_eq!(
        plan.pairs.values().map(|p| p.region.len()).sum::<usize>(),
        2 * n * n
    );
    elapsed
}

#[test]
fn compiling_a_plan_is_not_charged_per_pair_and_edge() {
    // Doubling the spokes multiplies the pairs, and the size of the regions
    // the plan returns, by four, and the edges by two. A compilation that
    // scans every edge for every pair grows eightfold; one bounded by the
    // schema searches and the regions it returns grows about fourfold (up to
    // a logarithmic factor from the ordered sets). The two sizes alternate
    // over five rounds and each keeps its least time, so a burst of load
    // from other work slows no single measurement into the ratio.
    let (small_hub, large_hub) = (hub(150), hub(300));
    let (mut small, mut large) = (Duration::MAX, Duration::MAX);
    for _ in 0..5 {
        small = small.min(compile_time(&small_hub, 150));
        large = large.min(compile_time(&large_hub, 300));
    }
    let ratio = large.as_secs_f64() / small.as_secs_f64();
    eprintln!("150 spokes: {small:?}; 300 spokes: {large:?}; ratio {ratio:.2}");
    assert!(
        ratio < 6.0,
        "doubling the spokes multiplied the compile time by {ratio:.2} ({small:?} to {large:?})"
    );
}
