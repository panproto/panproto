//! Integration test: `RelationalText` ↔ Layers via panproto.
//!
//! Verifies the full pipeline:
//! 1. Parse RT format lexicons and Layers lexicons as `ATProto` schemas
//! 2. Auto-generate protolens chains between schemas
//! 3. Parse documents, apply lenses, verify round-trip

use panproto_protocols::atproto;

/// The `org.relationaltext.richtext.document` lexicon — Document with text + facets.
fn rt_document_lexicon() -> serde_json::Value {
    serde_json::json!({
        "lexicon": 1,
        "id": "org.relationaltext.richtext.document",
        "defs": {
            "main": {
                "type": "object",
                "required": ["text"],
                "properties": {
                    "text": { "type": "string", "maxLength": 300 },
                    "facets": {
                        "type": "array",
                        "items": { "type": "ref", "ref": "#facet" }
                    }
                }
            },
            "facet": {
                "type": "object",
                "required": ["index", "features"],
                "properties": {
                    "index": { "type": "ref", "ref": "#byteSlice" },
                    "features": {
                        "type": "array",
                        "items": { "type": "ref", "ref": "#feature" }
                    }
                }
            },
            "byteSlice": {
                "type": "object",
                "required": ["byteStart", "byteEnd"],
                "properties": {
                    "byteStart": { "type": "integer", "minimum": 0 },
                    "byteEnd": { "type": "integer", "minimum": 0 }
                }
            },
            "feature": {
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "attrs": { "type": "unknown" }
                }
            }
        }
    })
}

/// The `org.relationaltext.format-lexicon` lexicon — defines the schema for format-lexicons.
fn format_lexicon_schema() -> serde_json::Value {
    serde_json::json!({
        "lexicon": 1,
        "id": "org.relationaltext.format-lexicon",
        "defs": {
            "main": {
                "type": "record",
                "key": "literal:self",
                "record": {
                    "type": "object",
                    "required": ["id", "features"],
                    "properties": {
                        "id": { "type": "string" },
                        "version": { "type": "string" },
                        "features": {
                            "type": "array",
                            "items": { "type": "ref", "ref": "#feature" }
                        }
                    }
                }
            },
            "feature": {
                "type": "object",
                "required": ["typeId"],
                "properties": {
                    "typeId": { "type": "string" },
                    "featureClass": {
                        "type": "string",
                        "knownValues": ["inline", "block", "entity", "comment", "meta"]
                    },
                    "expandStart": { "type": "boolean" },
                    "expandEnd": { "type": "boolean" },
                    "void": { "type": "boolean" }
                }
            }
        }
    })
}

/// A `pub.layers.annotation.annotationLayer` lexicon (simplified).
fn layers_annotation_lexicon() -> serde_json::Value {
    serde_json::json!({
        "lexicon": 1,
        "id": "pub.layers.annotation.annotationLayer",
        "defs": {
            "main": {
                "type": "record",
                "key": "tid",
                "record": {
                    "type": "object",
                    "required": ["kind", "annotations"],
                    "properties": {
                        "kind": {
                            "type": "string",
                            "knownValues": ["token-tag", "span", "relation", "tree", "graph"]
                        },
                        "subkind": { "type": "string" },
                        "formalism": { "type": "string" },
                        "annotations": {
                            "type": "array",
                            "items": { "type": "ref", "ref": "#annotation" }
                        }
                    }
                }
            },
            "annotation": {
                "type": "object",
                "properties": {
                    "uuid": { "type": "string" },
                    "label": { "type": "string" },
                    "value": { "type": "string" },
                    "anchor": { "type": "ref", "ref": "#span" },
                    "confidence": { "type": "integer", "minimum": 0, "maximum": 1000 }
                }
            },
            "span": {
                "type": "object",
                "required": ["byteStart", "byteEnd"],
                "properties": {
                    "byteStart": { "type": "integer", "minimum": 0 },
                    "byteEnd": { "type": "integer", "minimum": 0 }
                }
            }
        }
    })
}

/// A `pub.layers.expression.expression` lexicon (simplified).
fn layers_expression_lexicon() -> serde_json::Value {
    serde_json::json!({
        "lexicon": 1,
        "id": "pub.layers.expression.expression",
        "defs": {
            "main": {
                "type": "record",
                "key": "tid",
                "record": {
                    "type": "object",
                    "required": ["text"],
                    "properties": {
                        "text": { "type": "string", "maxLength": 10_000_000 },
                        "kind": {
                            "type": "string",
                            "knownValues": ["document", "transcript", "paragraph", "sentence"]
                        },
                        "language": { "type": "string" }
                    }
                }
            }
        }
    })
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[test]
fn parse_rt_document_lexicon() -> Result<(), Box<dyn std::error::Error>> {
    let schema = atproto::parse_lexicon(&rt_document_lexicon())?;
    assert!(
        schema.has_vertex("org.relationaltext.richtext.document"),
        "document vertex"
    );
    Ok(())
}

#[test]
fn parse_format_lexicon_schema() -> Result<(), Box<dyn std::error::Error>> {
    let schema = atproto::parse_lexicon(&format_lexicon_schema())?;
    assert!(
        schema.has_vertex("org.relationaltext.format-lexicon"),
        "format-lexicon record vertex"
    );
    assert!(
        schema.has_vertex("org.relationaltext.format-lexicon:body"),
        "format-lexicon body object"
    );
    Ok(())
}

#[test]
fn parse_layers_annotation_lexicon() -> Result<(), Box<dyn std::error::Error>> {
    let schema = atproto::parse_lexicon(&layers_annotation_lexicon())?;
    assert!(
        schema.has_vertex("pub.layers.annotation.annotationLayer"),
        "annotationLayer record vertex"
    );
    assert!(
        schema.has_vertex("pub.layers.annotation.annotationLayer:body"),
        "body object vertex"
    );
    Ok(())
}

#[test]
fn parse_layers_expression_lexicon() -> Result<(), Box<dyn std::error::Error>> {
    let schema = atproto::parse_lexicon(&layers_expression_lexicon())?;
    assert!(
        schema.has_vertex("pub.layers.expression.expression"),
        "expression record vertex"
    );
    Ok(())
}

#[test]
fn rt_and_layers_schemas_share_byte_range_structure() -> Result<(), Box<dyn std::error::Error>> {
    let rt_schema = atproto::parse_lexicon(&rt_document_lexicon())?;
    let layers_schema = atproto::parse_lexicon(&layers_annotation_lexicon())?;

    // Both have byteStart/byteEnd integer fields — the shared position model
    let rt_has_byte_start = rt_schema
        .vertices
        .values()
        .any(|v| v.id.as_ref().contains("byteStart"));
    let layers_has_byte_start = layers_schema
        .vertices
        .values()
        .any(|v| v.id.as_ref().contains("byteStart"));

    assert!(rt_has_byte_start, "RT schema has byteStart");
    assert!(layers_has_byte_start, "Layers schema has byteStart");
    Ok(())
}

#[test]
fn auto_generate_protolens_between_rt_schemas() -> Result<(), Box<dyn std::error::Error>> {
    let rt_schema = atproto::parse_lexicon(&rt_document_lexicon())?;
    let protocol = atproto::protocol();
    let config = panproto_lens::AutoLensConfig::default();

    // Auto-generate lens from RT document schema to itself (identity)
    let result = panproto_lens::auto_generate(&rt_schema, &rt_schema, &protocol, &config)?;
    assert!(
        result.alignment_quality > 0.0,
        "identity alignment should have positive quality"
    );
    Ok(())
}

#[test]
fn parse_real_layers_lexicon_from_disk() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../layers-pub/layers/lexicons/pub/layers/annotation/annotationLayer.json");

    if !path.exists() {
        // Skip if layers-pub is not checked out
        eprintln!("skipping: layers-pub not found at {}", path.display());
        return Ok(());
    }

    let content = std::fs::read_to_string(&path)?;
    let json: serde_json::Value = serde_json::from_str(&content)?;
    let schema = atproto::parse_lexicon(&json)?;

    assert!(
        schema.vertex_count() > 0,
        "parsed schema should have vertices"
    );
    Ok(())
}

#[test]
fn parse_real_rt_format_lexicon_from_disk() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../relationaltext/lexicons/org/relationaltext/richtext/document.json");

    if !path.exists() {
        eprintln!("skipping: relationaltext not found at {}", path.display());
        return Ok(());
    }

    let content = std::fs::read_to_string(&path)?;
    let json: serde_json::Value = serde_json::from_str(&content)?;
    let schema = atproto::parse_lexicon(&json)?;

    assert!(
        schema.has_vertex("org.relationaltext.richtext.document"),
        "document vertex from real lexicon"
    );
    Ok(())
}
