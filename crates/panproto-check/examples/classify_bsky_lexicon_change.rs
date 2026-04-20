//! Classify a synthetic breaking change to `app.bsky.feed.post`.

use panproto_check::{classify, diff};

const FEED_POST: &str = include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let v: serde_json::Value = serde_json::from_str(FEED_POST)?;
    let old = panproto_protocols::web_document::atproto::parse_lexicon(&v)?;

    // Synthesize a change: drop the first non-entry vertex.
    let mut new = old.clone();
    let entries: std::collections::HashSet<_> = new.entries.iter().cloned().collect();
    let victim = new
        .vertices
        .keys()
        .find(|k| !entries.contains(*k))
        .cloned()
        .ok_or("no droppable vertex")?;
    new.vertices.remove(&victim);
    new.edges.retain(|e, _| e.src != victim && e.tgt != victim);

    println!("dropped vertex: {victim}");

    let d = diff(&old, &new);
    println!(
        "diff: {} added, {} removed vertices",
        d.added_vertices.len(),
        d.removed_vertices.len()
    );

    let protocol = panproto_protocols::web_document::atproto::protocol();
    let report = classify(&d, &protocol);
    println!(
        "compat report: {} breaking, {} non-breaking, compatible={}",
        report.breaking.len(),
        report.non_breaking.len(),
        report.compatible
    );
    Ok(())
}
