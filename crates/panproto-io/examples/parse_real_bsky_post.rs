//! Parse a real Bluesky post record via the panproto-io codec registry.

use panproto_protocols::web_document::atproto;

const FEED_POST_LEXICON: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json");
const POST_RECORD: &[u8] =
    include_bytes!("../../../fixtures/atproto/records/post-3.json");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let lexicon: serde_json::Value = serde_json::from_str(FEED_POST_LEXICON)?;
    let schema = atproto::parse_lexicon(&lexicon)?;

    let registry = panproto_io::default_registry();
    let instance = registry.parse_wtype("atproto", &schema, POST_RECORD)?;
    println!("parsed post: {} nodes, root={}", instance.nodes.len(), instance.root);

    let bytes = registry.emit_wtype("atproto", &schema, &instance)?;
    println!("emitted {} bytes", bytes.len());
    Ok(())
}
