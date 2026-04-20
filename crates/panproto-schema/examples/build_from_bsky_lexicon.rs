//! Build a Schema from a real AT Proto Lexicon and print its structure.

use panproto_schema::{normalize, validate};

const FEED_POST: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let lexicon: serde_json::Value = serde_json::from_str(FEED_POST)?;
    let schema = panproto_protocols::web_document::atproto::parse_lexicon(&lexicon)?;
    println!(
        "app.bsky.feed.post: {} vertices, {} edges, {} entry points",
        schema.vertices.len(),
        schema.edges.len(),
        schema.entries.len()
    );

    let normalized = normalize(&schema);
    println!("after normalize: {} vertices", normalized.vertices.len());

    let protocol = panproto_protocols::web_document::atproto::protocol();
    let errors = validate(&schema, &protocol);
    println!("validate: {} errors", errors.len());

    Ok(())
}
