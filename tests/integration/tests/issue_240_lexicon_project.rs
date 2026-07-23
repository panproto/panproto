//! Regression test for issue #240.
//!
//! An `ATProto` lexicon set must be reachable as the per-file project tree
//! `panproto-vcs` is built around: `parse_schema_bundle_project` parses
//! each lexicon into its own schema (with in-set refs resolved to typed
//! defs) plus path-prefixed cross-file edges, and `build_project_tree`
//! stores that as a per-file Merkle tree that assembles back to a flat
//! schema with the cross-file ref resolved.

use std::collections::HashMap;
use std::path::PathBuf;

use panproto_project::build_project_tree;
use panproto_protocols::parse_schema_bundle_project;
use panproto_schema::Schema;
use panproto_vcs::{
    FileSchemaObject, MemStore, assemble_schema, project_coproduct_protocol, walk_tree,
};

/// A record lexicon whose one field is a ref into a sibling `defs`
/// lexicon, which in turn refs a def within itself.
fn cross_file_docs() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "lexicon": 1,
            "id": "pub.layers.annotation.annotationLayer",
            "defs": {
                "main": {
                    "type": "record",
                    "record": {
                        "type": "object",
                        "required": ["anchor"],
                        "properties": {
                            "anchor": {
                                "type": "ref",
                                "ref": "pub.layers.defs#spatioTemporalAnchor"
                            }
                        }
                    }
                }
            }
        }),
        serde_json::json!({
            "lexicon": 1,
            "id": "pub.layers.defs",
            "defs": {
                "spatioTemporalAnchor": {
                    "type": "object",
                    "required": ["box"],
                    "properties": { "box": {"type": "ref", "ref": "#boundingBox"} }
                },
                "boundingBox": {
                    "type": "object",
                    "required": ["x", "y"],
                    "properties": { "x": {"type": "integer"}, "y": {"type": "integer"} }
                }
            }
        }),
    ]
}

#[test]
fn lexicon_project_round_trips_through_the_vcs_tree() -> Result<(), Box<dyn std::error::Error>> {
    let docs = cross_file_docs();

    let project = parse_schema_bundle_project(
        "atproto",
        &[
            (
                PathBuf::from("annotation/annotationLayer.json"),
                docs[0].clone(),
            ),
            (PathBuf::from("defs.json"), docs[1].clone()),
        ],
    )?;

    let files: HashMap<PathBuf, Schema> = project.files.iter().cloned().collect();
    let protocols: HashMap<PathBuf, String> = files
        .keys()
        .map(|p| (p.clone(), "atproto".to_string()))
        .collect();

    let mut store = MemStore::new();
    let root = build_project_tree(&mut store, &files, &protocols, &project.cross_file_edges)?;

    // The tree stores exactly one per-file atproto leaf per lexicon.
    let mut leaves: Vec<String> = Vec::new();
    walk_tree(&store, &root, |path, file: &FileSchemaObject| {
        assert_eq!(file.protocol, "atproto", "each leaf is an atproto schema");
        leaves.push(path.to_string_lossy().into_owned());
        Ok(())
    })?;
    leaves.sort();
    assert_eq!(leaves.len(), 2, "one leaf per lexicon file, got {leaves:?}");

    // Assembling the tree resolves the cross-file ref to the typed def.
    let proto = project_coproduct_protocol();
    let assembled = assemble_schema(&store, &root, &proto)?;

    let anchor_key = "defs.json::pub.layers.defs#spatioTemporalAnchor";
    let anchor = assembled.vertices.get(anchor_key).ok_or_else(|| {
        format!(
            "assembled schema is missing {anchor_key}; vertices: {:?}",
            assembled.vertices.keys().collect::<Vec<_>>()
        )
    })?;
    assert_eq!(
        &*anchor.kind, "object",
        "the cross-file ref must resolve to the typed def, not an opaque placeholder"
    );

    // A ref edge crosses from the referencing file to that resolved def.
    assert!(
        assembled.edges.keys().any(|e| &*e.kind == "ref"
            && e.src.starts_with("annotation/annotationLayer.json::")
            && &*e.tgt == anchor_key),
        "expected the resolved cross-file ref edge in the assembled schema"
    );

    Ok(())
}
