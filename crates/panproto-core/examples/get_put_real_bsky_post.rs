//! Round-trip a real Bluesky post through an identity lens: get, then put.

use std::collections::{HashMap, HashSet};

use panproto_core::inst::CompiledMigration;
use panproto_core::lens::{Lens, get, put};
use panproto_core::schema::{Edge, Schema};

const FEED_POST_LEXICON: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json");
const POST_RECORD: &[u8] = include_bytes!("../../../fixtures/atproto/records/post-3.json");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let lexicon: serde_json::Value = serde_json::from_str(FEED_POST_LEXICON)?;
    let schema: Schema = panproto_core::protocols::web_document::atproto::parse_lexicon(&lexicon)?;

    let surviving_verts = schema.vertices.keys().cloned().collect();
    let surviving_edges: HashSet<Edge> = schema.edges.keys().cloned().collect();
    let compiled = CompiledMigration {
        surviving_verts,
        surviving_edges,
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
        op_term_assignments: HashMap::new(),
        expansion_path: HashMap::new(),
    };
    let lens = Lens {
        compiled,
        src_schema: schema.clone(),
        tgt_schema: schema.clone(),
    };

    let registry = panproto_core::io::default_registry();
    let instance = registry.parse_wtype("atproto", &schema, POST_RECORD)?;
    println!("source: {} nodes", instance.nodes.len());

    let (view, complement) = get(&lens, &instance)?;
    println!(
        "view:   {} nodes, complement dropped {} nodes, {} arcs",
        view.nodes.len(),
        complement.dropped_nodes.len(),
        complement.dropped_arcs.len(),
    );

    let restored = put(&lens, &view, &complement)?;
    println!("restored: {} nodes", restored.nodes.len());
    Ok(())
}
