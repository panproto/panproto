//! SWIFT MT financial messaging protocol definition.
//!
//! Uses Group B theory: hypergraph + functor.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the SWIFT MT protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "swift-mt".into(),
        schema_theory: "ThSwiftMtSchema".into(),
        instance_theory: "ThSwiftMtInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "message-type".into(),
            "block".into(),
            "field".into(),
            "tag".into(),
            "string".into(),
            "numeric".into(),
            "date".into(),
            "amount".into(),
            "currency".into(),
            "bic".into(),
        ],
        constraint_sorts: vec!["required".into(), "format".into(), "length".into()],
        has_order: true,
        nominal_identity: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for SWIFT MT.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_hypergraph_functor(registry, "ThSwiftMtSchema", "ThSwiftMtInstance");
}

/// Parse a JSON-based SWIFT MT message schema into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_swift_mt_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    let messages = json
        .get("messages")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| ProtocolError::MissingField("messages".into()))?;

    for (name, def) in messages {
        builder = builder.vertex(name, "message-type", None)?;

        if let Some(blocks) = def.get("blocks").and_then(serde_json::Value::as_object) {
            for (block_name, block_def) in blocks {
                let block_id = format!("{name}.{block_name}");
                builder = builder.vertex(&block_id, "block", None)?;
                builder = builder.edge(name, &block_id, "prop", Some(block_name))?;

                if let Some(fields) = block_def
                    .get("fields")
                    .and_then(serde_json::Value::as_object)
                {
                    for (field_name, field_def) in fields {
                        let field_id = format!("{block_id}.{field_name}");
                        let kind = field_def
                            .get("type")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("string");
                        builder = builder.vertex(&field_id, kind, None)?;
                        builder = builder.edge(&block_id, &field_id, "prop", Some(field_name))?;

                        if let Some(tag) = field_def.get("tag").and_then(serde_json::Value::as_str)
                        {
                            builder = builder.constraint(&field_id, "format", tag);
                        }
                        if field_def
                            .get("required")
                            .and_then(serde_json::Value::as_bool)
                            == Some(true)
                        {
                            builder = builder.constraint(&field_id, "required", "true");
                        }
                    }
                }
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Emit a [`Schema`] as a JSON SWIFT MT schema.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_swift_mt_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let structural = &["prop"];
    let roots = find_roots(schema, structural);

    let mut messages = serde_json::Map::new();
    for root in &roots {
        if root.kind != "message-type" {
            continue;
        }
        let mut msg_obj = serde_json::Map::new();
        let blocks = children_by_edge(schema, &root.id, "prop");

        let mut blocks_obj = serde_json::Map::new();
        for (edge, block) in &blocks {
            let block_name = edge.name.as_deref().unwrap_or(&block.id);
            let mut single_block = serde_json::Map::new();

            let fields = children_by_edge(schema, &block.id, "prop");
            if !fields.is_empty() {
                let mut fields_obj = serde_json::Map::new();
                for (fe, fv) in &fields {
                    let fname = fe.name.as_deref().unwrap_or(&fv.id);
                    let mut fobj = serde_json::Map::new();
                    fobj.insert("type".into(), serde_json::json!(fv.kind));
                    for c in vertex_constraints(schema, &fv.id) {
                        if c.sort == "required" {
                            fobj.insert("required".into(), serde_json::json!(true));
                        } else {
                            fobj.insert(c.sort.clone(), serde_json::json!(c.value));
                        }
                    }
                    fields_obj.insert(fname.to_string(), serde_json::Value::Object(fobj));
                }
                single_block.insert("fields".into(), serde_json::Value::Object(fields_obj));
            }
            blocks_obj.insert(
                block_name.to_string(),
                serde_json::Value::Object(single_block),
            );
        }

        msg_obj.insert("blocks".into(), serde_json::Value::Object(blocks_obj));
        messages.insert(root.id.clone(), serde_json::Value::Object(msg_obj));
    }

    Ok(serde_json::json!({ "messages": messages }))
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![EdgeRule {
        edge_kind: "prop".into(),
        src_kinds: vec!["message-type".into(), "block".into()],
        tgt_kinds: vec![],
    }]
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn protocol_def() {
        let p = protocol();
        assert_eq!(p.name, "swift-mt");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThSwiftMtSchema"));
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "messages": {
                "MT103": {
                    "blocks": {
                        "block4": {
                            "fields": {
                                "20": {"type": "string", "tag": ":20:", "required": true},
                                "32A": {"type": "amount", "tag": ":32A:"}
                            }
                        }
                    }
                }
            }
        });
        let schema = parse_swift_mt_schema(&json).expect("should parse");
        assert!(schema.has_vertex("MT103"));
        let emitted = emit_swift_mt_schema(&schema).expect("emit");
        let s2 = parse_swift_mt_schema(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }
}
