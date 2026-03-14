//! Redis RediSearch protocol definition.
//!
//! Uses Group C theory: simple graph + flat.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{IndentWriter, children_by_edge, find_roots};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the Redis protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "redis".into(),
        schema_theory: "ThRedisSchema".into(),
        instance_theory: "ThRedisInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "index".into(),
            "field".into(),
            "text".into(),
            "tag".into(),
            "numeric".into(),
            "geo".into(),
            "vector".into(),
        ],
        constraint_sorts: vec![],
        has_order: true,
        nominal_identity: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for Redis.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_simple_graph_flat(registry, "ThRedisSchema", "ThRedisInstance");
}

/// Parse FT.CREATE syntax into a [`Schema`].
///
/// Expects syntax like:
/// ```text
/// FT.CREATE idx ON HASH PREFIX 1 doc: SCHEMA title TEXT name TAG age NUMERIC
/// ```
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_redis_schema(input: &str) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.starts_with("FT.CREATE") {
            builder = parse_ft_create(builder, trimmed)?;
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Parse a single FT.CREATE statement.
fn parse_ft_create(mut builder: SchemaBuilder, line: &str) -> Result<SchemaBuilder, ProtocolError> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(ProtocolError::Parse("invalid FT.CREATE".into()));
    }

    let index_name = parts[1];
    builder = builder.vertex(index_name, "index", None)?;

    // Find SCHEMA keyword and parse field definitions after it.
    let schema_idx = parts.iter().position(|p| p.eq_ignore_ascii_case("SCHEMA"));
    if let Some(idx) = schema_idx {
        let mut i = idx + 1;
        while i < parts.len() {
            let field_name = parts[i];
            let field_type = parts.get(i + 1).copied().unwrap_or("TEXT");
            let field_id = format!("{index_name}.{field_name}");
            let kind = redis_type_to_kind(field_type);
            builder = builder.vertex(&field_id, kind, None)?;
            builder = builder.edge(index_name, &field_id, "prop", Some(field_name))?;
            i += 2;
        }
    }

    Ok(builder)
}

/// Map Redis field type to vertex kind.
fn redis_type_to_kind(type_str: &str) -> &'static str {
    match type_str.to_uppercase().as_str() {
        "TEXT" => "text",
        "TAG" => "tag",
        "NUMERIC" => "numeric",
        "GEO" => "geo",
        "VECTOR" => "vector",
        _ => "field",
    }
}

/// Map vertex kind to Redis field type.
fn kind_to_redis_type(kind: &str) -> &'static str {
    match kind {
        "text" => "TEXT",
        "tag" => "TAG",
        "numeric" => "NUMERIC",
        "geo" => "GEO",
        "vector" => "VECTOR",
        _ => "TEXT",
    }
}

/// Emit a [`Schema`] as FT.CREATE syntax.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_redis_schema(schema: &Schema) -> Result<String, ProtocolError> {
    let structural = &["prop"];
    let roots = find_roots(schema, structural);

    let mut w = IndentWriter::new("  ");

    for root in &roots {
        if root.kind != "index" {
            continue;
        }
        let fields = children_by_edge(schema, &root.id, "prop");
        let field_strs: Vec<String> = fields
            .iter()
            .map(|(edge, child)| {
                let name = edge.name.as_deref().unwrap_or(&child.id);
                let type_str = kind_to_redis_type(&child.kind);
                format!("{name} {type_str}")
            })
            .collect();
        w.line(&format!(
            "FT.CREATE {} ON HASH SCHEMA {}",
            root.id,
            field_strs.join(" ")
        ));
    }

    Ok(w.finish())
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![EdgeRule {
        edge_kind: "prop".into(),
        src_kinds: vec!["index".into()],
        tgt_kinds: vec![
            "text".into(),
            "tag".into(),
            "numeric".into(),
            "geo".into(),
            "vector".into(),
            "field".into(),
        ],
    }]
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn protocol_def() {
        let p = protocol();
        assert_eq!(p.name, "redis");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThRedisSchema"));
        assert!(registry.contains_key("ThRedisInstance"));
    }

    #[test]
    fn parse_and_emit() {
        let input = "FT.CREATE myidx ON HASH SCHEMA title TEXT name TAG age NUMERIC\n";
        let schema = parse_redis_schema(input).expect("should parse");
        assert!(schema.has_vertex("myidx"));
        assert!(schema.has_vertex("myidx.title"));
        assert!(schema.has_vertex("myidx.name"));
        assert!(schema.has_vertex("myidx.age"));

        let emitted = emit_redis_schema(&schema).expect("should emit");
        assert!(emitted.contains("FT.CREATE myidx"));
        assert!(emitted.contains("title TEXT"));
    }

    #[test]
    fn roundtrip() {
        let input = "FT.CREATE idx ON HASH SCHEMA f1 TEXT f2 NUMERIC\n";
        let s1 = parse_redis_schema(input).expect("parse");
        let emitted = emit_redis_schema(&s1).expect("emit");
        let s2 = parse_redis_schema(&emitted).expect("re-parse");
        assert_eq!(s1.vertex_count(), s2.vertex_count());
    }
}
