//! Compile and apply an identity migration on a real `app.bsky.feed.post` Lexicon.
//!
//! Loads the Lexicon from `fixtures/atproto/lexicons/`, builds an identity
//! migration over its vertex graph, compiles it, and lifts a real Bluesky
//! post record through the migration to demonstrate a round-trip.

use panproto_gat::Name;
use panproto_mig::{Migration, compile, lift_wtype};
use panproto_protocols::web_document::atproto;
use panproto_schema::{Edge, Schema};

const FEED_POST: &str = include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json");
const POST_RECORD: &[u8] = include_bytes!("../../../fixtures/atproto/records/post-3.json");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let lexicon: serde_json::Value = serde_json::from_str(FEED_POST)?;
    let schema: Schema = atproto::parse_lexicon(&lexicon)?;
    println!(
        "loaded app.bsky.feed.post schema: {} vertices, {} edges",
        schema.vertices.len(),
        schema.edges.len()
    );

    let vertex_ids: Vec<Name> = schema.vertices.keys().cloned().collect();
    let edges: Vec<Edge> = schema.edges.keys().cloned().collect();
    let migration = Migration::identity(&vertex_ids, &edges);

    let compiled = compile(&schema, &schema, &migration)?;
    println!(
        "compiled: {} surviving vertices, {} surviving edges",
        compiled.surviving_verts.len(),
        compiled.surviving_edges.len()
    );

    let registry = panproto_io::default_registry();
    let instance = registry.parse_wtype("atproto", &schema, POST_RECORD)?;
    println!("parsed post record: {} nodes", instance.nodes.len());

    let lifted = lift_wtype(&compiled, &schema, &schema, &instance)?;
    println!(
        "lifted through identity migration: {} nodes",
        lifted.nodes.len()
    );

    Ok(())
}
