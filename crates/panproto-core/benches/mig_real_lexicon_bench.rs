#![allow(missing_docs, clippy::expect_used)]
//! Migration engine benchmarks against real AT Protocol Lexicon schemas.
//!
//! Fixtures are pinned under `fixtures/atproto/lexicons/` — see
//! `fixtures/FIXTURES.md` for sources and licenses. All schemas are real
//! published Lexicons from `bluesky-social/atproto`; benches measure
//! `compile`, `check_existence`, `compose`, and `lift_wtype` against the
//! actual shapes Bluesky clients see in production.

use std::collections::HashMap;

use divan::Bencher;
use panproto_core::gat::Name;
use panproto_core::mig::{Migration, check_existence, compile, compose, lift_wtype};
use panproto_core::protocols::web_document::atproto;
use panproto_core::schema::{Edge, Schema};

fn main() {
    divan::main();
}

// ---------------------------------------------------------------------------
// Fixtures: real AT Protocol Lexicons
// ---------------------------------------------------------------------------

const FEED_POST: &str = include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json");
const ACTOR_PROFILE: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.actor.profile.json");
const FEED_LIKE: &str = include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.like.json");
const GRAPH_FOLLOW: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.graph.follow.json");

fn load_lexicon(src: &str) -> Schema {
    let value: serde_json::Value = serde_json::from_str(src).expect("lexicon JSON parses");
    atproto::parse_lexicon(&value).expect("lexicon builds a schema")
}

fn identity_migration(schema: &Schema) -> Migration {
    let vertex_ids: Vec<Name> = schema.vertices.keys().cloned().collect();
    let edges: Vec<Edge> = schema.edges.keys().cloned().collect();
    Migration::identity(&vertex_ids, &edges)
}

fn make_protocol() -> panproto_schema::Protocol {
    atproto::protocol()
}

// ---------------------------------------------------------------------------
// compile: builds a CompiledMigration for a real Lexicon
// ---------------------------------------------------------------------------

#[divan::bench]
fn compile_feed_post(bencher: Bencher) {
    let schema = load_lexicon(FEED_POST);
    let migration = identity_migration(&schema);
    bencher.bench(|| compile(&schema, &schema, &migration));
}

#[divan::bench]
fn compile_actor_profile(bencher: Bencher) {
    let schema = load_lexicon(ACTOR_PROFILE);
    let migration = identity_migration(&schema);
    bencher.bench(|| compile(&schema, &schema, &migration));
}

#[divan::bench]
fn compile_feed_like(bencher: Bencher) {
    let schema = load_lexicon(FEED_LIKE);
    let migration = identity_migration(&schema);
    bencher.bench(|| compile(&schema, &schema, &migration));
}

#[divan::bench]
fn compile_graph_follow(bencher: Bencher) {
    let schema = load_lexicon(GRAPH_FOLLOW);
    let migration = identity_migration(&schema);
    bencher.bench(|| compile(&schema, &schema, &migration));
}

// ---------------------------------------------------------------------------
// check_existence: validates migration well-formedness against a real schema
// ---------------------------------------------------------------------------

#[divan::bench]
fn check_existence_feed_post(bencher: Bencher) {
    let schema = load_lexicon(FEED_POST);
    let migration = identity_migration(&schema);
    let protocol = make_protocol();
    let registry = HashMap::new();
    bencher.bench(|| check_existence(&protocol, &schema, &schema, &migration, &registry));
}

#[divan::bench]
fn check_existence_actor_profile(bencher: Bencher) {
    let schema = load_lexicon(ACTOR_PROFILE);
    let migration = identity_migration(&schema);
    let protocol = make_protocol();
    let registry = HashMap::new();
    bencher.bench(|| check_existence(&protocol, &schema, &schema, &migration, &registry));
}

// ---------------------------------------------------------------------------
// compose: composing two identity migrations on a real schema
// ---------------------------------------------------------------------------

#[divan::bench]
fn compose_identity_feed_post(bencher: Bencher) {
    let schema = load_lexicon(FEED_POST);
    let m = identity_migration(&schema);
    bencher.bench(|| compose(&m, &m));
}

#[divan::bench]
fn compose_chain_actor_profile(bencher: Bencher) {
    let schema = load_lexicon(ACTOR_PROFILE);
    let m = identity_migration(&schema);
    bencher.bench(|| {
        let m2 = compose(&m, &m).expect("compose 1-2");
        let m3 = compose(&m2, &m).expect("compose 1-3");
        compose(&m3, &m).expect("compose 1-4")
    });
}

// ---------------------------------------------------------------------------
// compose with renaming: simulate a realistic field-rename migration
// ---------------------------------------------------------------------------

/// Build a migration that renames every vertex from `x` to `renamed_x`,
/// preserving edges. Models a protocol-wide refactor applied to a real
/// Lexicon's vertex graph.
fn rename_all_migration(schema: &Schema) -> Migration {
    let mut vertex_map: HashMap<Name, Name> = HashMap::new();
    for v in schema.vertices.keys() {
        vertex_map.insert(v.clone(), Name::from(format!("renamed_{v}")));
    }

    let mut edge_map = HashMap::new();
    for edge in schema.edges.keys() {
        let new_src = vertex_map
            .get(&edge.src)
            .cloned()
            .unwrap_or_else(|| edge.src.clone());
        let new_tgt = vertex_map
            .get(&edge.tgt)
            .cloned()
            .unwrap_or_else(|| edge.tgt.clone());
        edge_map.insert(
            edge.clone(),
            Edge {
                src: new_src,
                tgt: new_tgt,
                kind: edge.kind.clone(),
                name: edge.name.clone(),
            },
        );
    }

    Migration {
        vertex_map,
        edge_map,
        hyper_edge_map: HashMap::new(),
        label_map: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        expr_resolvers: HashMap::new(),
        domain: None,
        codomain: None,
    }
}

#[divan::bench]
fn compose_rename_feed_post(bencher: Bencher) {
    let schema = load_lexicon(FEED_POST);
    let m1 = identity_migration(&schema);
    let m2 = rename_all_migration(&schema);
    bencher.bench(|| compose(&m1, &m2));
}

// ---------------------------------------------------------------------------
// lift_wtype: apply a compiled migration to a real Bluesky post record
// ---------------------------------------------------------------------------

const POST_RECORD_0: &[u8] = include_bytes!("../../../fixtures/atproto/records/post-0.json");
const POST_RECORD_3: &[u8] = include_bytes!("../../../fixtures/atproto/records/post-3.json");

fn load_post_instance(bytes: &[u8], schema: &Schema) -> panproto_inst::WInstance {
    let registry = panproto_core::io::default_registry();
    registry
        .parse_wtype("atproto", schema, bytes)
        .expect("parse real post record")
}

#[divan::bench]
fn lift_wtype_identity_real_post(bencher: Bencher) {
    let schema = load_lexicon(FEED_POST);
    let migration = identity_migration(&schema);
    let compiled = compile(&schema, &schema, &migration).expect("compile");
    let instance = load_post_instance(POST_RECORD_0, &schema);
    bencher.bench(|| lift_wtype(&compiled, &schema, &schema, &instance));
}

#[divan::bench]
fn lift_wtype_identity_real_post_with_reply(bencher: Bencher) {
    let schema = load_lexicon(FEED_POST);
    let migration = identity_migration(&schema);
    let compiled = compile(&schema, &schema, &migration).expect("compile");
    // post-3 includes a reply edge — exercises more of the graph than post-0.
    let instance = load_post_instance(POST_RECORD_3, &schema);
    bencher.bench(|| lift_wtype(&compiled, &schema, &schema, &instance));
}
