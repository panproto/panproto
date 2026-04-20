//! Parse a real Bluesky post record into a `WInstance` and inspect its structure.

use panproto_inst::WInstance;

const FEED_POST_LEXICON: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json");
const POST_RECORD: &[u8] = include_bytes!("../../../fixtures/atproto/records/post-3.json");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let lexicon: serde_json::Value = serde_json::from_str(FEED_POST_LEXICON)?;
    let schema = panproto_protocols::web_document::atproto::parse_lexicon(&lexicon)?;

    let registry = panproto_io::default_registry();
    let instance: WInstance = registry.parse_wtype("atproto", &schema, POST_RECORD)?;

    println!("root node id: {}", instance.root);
    println!("node count: {}", instance.nodes.len());
    println!("arc count: {}", instance.arcs.len());
    for (id, node) in &instance.nodes {
        println!("  node {} anchored at {}", id, node.anchor);
    }
    Ok(())
}
