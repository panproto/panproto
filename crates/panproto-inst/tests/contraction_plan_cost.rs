#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The cost of compiling a contraction plan grows with the schema and the
//! regions it returns, never with the planned pairs times the edges.

mod common;

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
    // Multiplying the spokes by eight multiplies the pairs, and the size of
    // the regions the plan returns, by 64, and the edges by eight. A
    // compilation that scans every edge for every pair grows 512-fold,
    // eightfold against the growth of the plan it returns; one bounded by
    // the schema searches and the regions it returns grows by about 1
    // against it, up to a logarithmic factor from the ordered sets. The
    // bound 2.8 is about the geometric mean of the two growths.
    let (small_hub, large_hub) = (hub(50), hub(400));
    let measured = common::growth(
        15,
        64,
        || compile_time(&small_hub, 50),
        || compile_time(&large_hub, 400),
    );
    let growth = measured.growth;
    eprintln!(
        "a 400-spoke compile takes {:.1} 50-spoke compiles; growth {growth:.2}",
        measured.small_runs
    );
    assert!(
        growth < 2.8,
        "multiplying the spokes by eight multiplied the compile time \
         per planned pair by {growth:.2} (a 400-spoke compile took {:.1} \
         50-spoke compiles)",
        measured.small_runs
    );
}
