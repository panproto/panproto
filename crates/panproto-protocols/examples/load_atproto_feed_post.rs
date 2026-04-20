//! Load every vendored AT Proto Lexicon and report its vertex/edge counts.

use panproto_protocols::web_document::atproto;

const LEXICONS: &[(&str, &str)] = &[
    (
        "app.bsky.feed.post",
        include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json"),
    ),
    (
        "app.bsky.actor.profile",
        include_str!("../../../fixtures/atproto/lexicons/app.bsky.actor.profile.json"),
    ),
    (
        "app.bsky.feed.like",
        include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.like.json"),
    ),
    (
        "app.bsky.feed.repost",
        include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.repost.json"),
    ),
    (
        "app.bsky.graph.follow",
        include_str!("../../../fixtures/atproto/lexicons/app.bsky.graph.follow.json"),
    ),
    (
        "com.atproto.repo.createRecord",
        include_str!("../../../fixtures/atproto/lexicons/com.atproto.repo.createRecord.json"),
    ),
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for (name, src) in LEXICONS {
        let v: serde_json::Value = serde_json::from_str(src)?;
        let schema = atproto::parse_lexicon(&v)?;
        println!(
            "{name}: {} vertices, {} edges, {} entries",
            schema.vertices.len(),
            schema.edges.len(),
            schema.entries.len()
        );
    }
    Ok(())
}
