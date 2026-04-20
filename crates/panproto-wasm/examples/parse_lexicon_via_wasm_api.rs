//! Drive the WASM boundary API from native code: parse a real AT Proto
//! Lexicon via `parse_atproto_lexicon` and read back its metadata.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = include_bytes!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json");
    let handle = panproto_wasm::parse_atproto_lexicon(bytes)
        .map_err(|e| format!("parse_atproto_lexicon failed: {e:?}"))?;
    println!("got schema handle: {handle}");
    let meta = panproto_wasm::schema_metadata(handle)
        .map_err(|e| format!("schema_metadata failed: {e:?}"))?;
    println!("metadata bytes: {}", meta.len());
    Ok(())
}
