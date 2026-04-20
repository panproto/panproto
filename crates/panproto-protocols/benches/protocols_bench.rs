//! Protocol-level benchmarks on real AT Proto and Avro schemas.

#![allow(clippy::expect_used)]

use panproto_protocols::web_document::atproto;

fn main() {
    divan::main();
}

const FEED_POST: &str = include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json");
const ACTOR_PROFILE: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.actor.profile.json");
const FEED_LIKE: &str = include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.like.json");
const FEED_REPOST: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.repost.json");
const GRAPH_FOLLOW: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.graph.follow.json");
const CREATE_RECORD: &str =
    include_str!("../../../fixtures/atproto/lexicons/com.atproto.repo.createRecord.json");

#[divan::bench]
fn build_atproto_protocol(bencher: divan::Bencher) {
    bencher.bench(atproto::protocol);
}

#[divan::bench(args = [
    ("feed.post", FEED_POST),
    ("actor.profile", ACTOR_PROFILE),
    ("feed.like", FEED_LIKE),
    ("feed.repost", FEED_REPOST),
    ("graph.follow", GRAPH_FOLLOW),
    ("createRecord", CREATE_RECORD),
])]
fn parse_lexicon(bencher: divan::Bencher, (_name, src): (&str, &str)) {
    let v: serde_json::Value = serde_json::from_str(src).expect("json");
    bencher.bench(|| atproto::parse_lexicon(&v));
}

#[divan::bench]
fn register_atproto_theories(bencher: divan::Bencher) {
    bencher.bench(|| {
        let mut registry = std::collections::HashMap::new();
        atproto::register_theories(&mut registry);
        registry.len()
    });
}

// ---------------------------------------------------------------------------
// Colimit benchmarks on real panproto theories
// ---------------------------------------------------------------------------

use panproto_gat::{Sort, Theory, colimit_by_name};

/// Colimit of `ThGraph` and `ThConstraint` over a shared `Vertex` sort —
/// the exact construction `atproto::register_theories` performs to build
/// the schema theory for the AT Protocol.
#[divan::bench]
fn colimit_thgraph_thconstraint_real(bencher: divan::Bencher) {
    use panproto_protocols::theories;
    let th_graph = theories::th_graph();
    let th_constraint = theories::th_constraint();
    let shared_vertex = Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![]);
    bencher.bench(|| colimit_by_name(&th_graph, &th_constraint, &shared_vertex));
}

/// Colimit of `ThWType` and `ThMeta` over a shared `Node` sort — the
/// AT Proto instance theory.
#[divan::bench]
fn colimit_thwtype_thmeta_real(bencher: divan::Bencher) {
    use panproto_protocols::theories;
    let th_wtype = theories::th_wtype();
    let th_meta = theories::th_meta();
    let shared_node = Theory::new("ThNode", vec![Sort::simple("Node")], vec![], vec![]);
    bencher.bench(|| colimit_by_name(&th_wtype, &th_meta, &shared_node));
}

/// End-to-end: compose the full AT Protocol schema theory from its three
/// component theories (`ThGraph` + `ThConstraint` + `ThMulti`), the same two-step
/// colimit used at runtime.
#[divan::bench]
fn build_atproto_schema_theory(bencher: divan::Bencher) {
    use panproto_protocols::theories;
    let th_graph = theories::th_graph();
    let th_constraint = theories::th_constraint();
    let th_multi = theories::th_multi();
    let shared_vertex = Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![]);
    let shared_ve = Theory::new(
        "ThVertexEdge",
        vec![Sort::simple("Vertex"), Sort::simple("Edge")],
        vec![],
        vec![],
    );
    bencher.bench(|| {
        let gc = colimit_by_name(&th_graph, &th_constraint, &shared_vertex).expect("gc");
        colimit_by_name(&gc, &th_multi, &shared_ve)
    });
}
