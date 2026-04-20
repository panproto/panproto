//! Initialize a fresh repository and commit a chain of real Bluesky Lexicons.

use panproto_protocols::web_document::atproto;
use panproto_vcs::Repository;

const LEXICONS: &[(&str, &str)] = &[
    ("app.bsky.feed.post", include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json")),
    ("app.bsky.actor.profile", include_str!("../../../fixtures/atproto/lexicons/app.bsky.actor.profile.json")),
    ("app.bsky.feed.like", include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.like.json")),
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir()?;
    let mut repo = Repository::init(tmp.path())?;

    for (name, src) in LEXICONS {
        let v: serde_json::Value = serde_json::from_str(src)?;
        let schema = atproto::parse_lexicon(&v)?;
        repo.add(&schema)?;
        let id = repo.commit(&format!("adopt {name}"), "example")?;
        println!("committed {name}: {id}");
    }

    let log = repo.log(None)?;
    println!("log: {} commits", log.len());
    Ok(())
}
