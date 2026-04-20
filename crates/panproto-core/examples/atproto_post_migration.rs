//! End-to-end: load a Bluesky Lexicon, parse a real post record, run an
//! identity migration, and emit the result back — all via panproto-core.

use panproto_core::gat::Name;
use panproto_core::mig::{Migration, compile, lift_wtype};
use panproto_core::protocols::web_document::atproto;
use panproto_core::schema::Edge;

const FEED_POST_LEXICON: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json");
const REAL_POST: &[u8] = include_bytes!("../../../fixtures/atproto/records/post-3.json");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let v: serde_json::Value = serde_json::from_str(FEED_POST_LEXICON)?;
    let schema = atproto::parse_lexicon(&v)?;

    let vertex_ids: Vec<Name> = schema.vertices.keys().cloned().collect();
    let edges: Vec<Edge> = schema.edges.keys().cloned().collect();
    let migration = Migration::identity(&vertex_ids, &edges);
    let compiled = compile(&schema, &schema, &migration)?;

    let registry = panproto_core::io::default_registry();
    let instance = registry.parse_wtype("atproto", &schema, REAL_POST)?;
    let lifted = lift_wtype(&compiled, &schema, &schema, &instance)?;
    let bytes = registry.emit_wtype("atproto", &schema, &lifted)?;

    println!("pipeline OK: in={} bytes → out={} bytes", REAL_POST.len(), bytes.len());
    Ok(())
}
