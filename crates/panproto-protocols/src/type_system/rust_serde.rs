//! Rust struct/enum type system protocol definition.
//!
//! Uses typed graph + W-type with interfaces (Group D).
//! Schema: `colimit(ThGraph, ThConstraint, ThMulti, ThInterface)`.
//! Instance: `ThWType`.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::manual_strip,
    clippy::option_if_let_else,
    clippy::map_unwrap_or,
    clippy::unnecessary_wraps,
    clippy::items_after_statements,
    clippy::redundant_closure_for_method_calls,
    clippy::match_same_arms,
    clippy::if_same_then_else,
    clippy::single_char_pattern,
    clippy::needless_pass_by_value
)]

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{IndentWriter, children_by_edge, constraint_value, find_roots, resolve_type};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the Rust serde protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "rust_serde".into(),
        schema_theory: "ThRustSerdeSchema".into(),
        instance_theory: "ThRustSerdeInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "struct".into(),
            "field".into(),
            "enum".into(),
            "variant".into(),
            "tuple-struct".into(),
            "string".into(),
            "i8".into(),
            "i16".into(),
            "i32".into(),
            "i64".into(),
            "i128".into(),
            "u8".into(),
            "u16".into(),
            "u32".into(),
            "u64".into(),
            "u128".into(),
            "f32".into(),
            "f64".into(),
            "bool".into(),
            "vec".into(),
            "hashmap".into(),
            "option".into(),
            "box-type".into(),
        ],
        constraint_sorts: vec![
            "optional".into(),
            "default".into(),
            "rename".into(),
            "skip".into(),
            "flatten".into(),
        ],
    }
}

/// Register the component GATs for Rust serde with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_typed_graph_wtype(registry, "ThRustSerdeSchema", "ThRustSerdeInstance");
}

/// Parse Rust struct/enum definitions into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing or schema construction fails.
#[allow(clippy::too_many_lines)]
pub fn parse_rust_types(input: &str) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;
    let mut builder = SchemaBuilder::new(&proto);
    let mut vertex_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut deferred: Vec<RustFieldDeferred> = Vec::new();

    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Skip derive/attribute lines.
        if trimmed.starts_with("#[") || trimmed.starts_with("//") || trimmed.starts_with("pub(") {
            i += 1;
            continue;
        }

        let clean = trimmed.trim_start_matches("pub ").trim();

        if clean.starts_with("struct ") {
            let rest = &clean["struct ".len()..];
            let name = rest
                .split(|c: char| c == '{' || c == '(' || c == '<' || c == ';' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .trim();
            if !name.is_empty() {
                if rest.contains('(') && !rest.contains('{') {
                    builder = builder.vertex(name, "tuple-struct", None)?;
                    vertex_ids.insert(name.to_owned());
                } else {
                    builder = builder.vertex(name, "struct", None)?;
                    vertex_ids.insert(name.to_owned());
                    i += 1;
                    let (b, new_i, new_deferred) =
                        parse_rust_fields(builder, &lines, i, name, &mut vertex_ids)?;
                    builder = b;
                    deferred.extend(new_deferred);
                    i = new_i;
                    continue;
                }
            }
            i += 1;
        } else if clean.starts_with("enum ") {
            let rest = &clean["enum ".len()..];
            let name = rest
                .split(|c: char| c == '{' || c == '<' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .trim();
            if !name.is_empty() {
                builder = builder.vertex(name, "enum", None)?;
                vertex_ids.insert(name.to_owned());
                i += 1;
                let (b, new_i) = parse_rust_variants(builder, &lines, i, name, &mut vertex_ids)?;
                builder = b;
                i = new_i;
                continue;
            }
            i += 1;
        } else {
            i += 1;
        }
    }

    for d in deferred {
        if vertex_ids.contains(&d.type_name) {
            builder = builder.edge(&d.field_id, &d.type_name, "type-of", None)?;
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

struct RustFieldDeferred {
    field_id: String,
    type_name: String,
}

fn parse_rust_fields(
    mut builder: SchemaBuilder,
    lines: &[&str],
    start: usize,
    parent: &str,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<(SchemaBuilder, usize, Vec<RustFieldDeferred>), ProtocolError> {
    let mut i = start;
    let mut deferred = Vec::new();
    let mut pending_serde_attrs: Vec<(String, String)> = Vec::new();

    while i < lines.len() {
        let line = lines[i].trim();
        if line == "}" || line.starts_with('}') {
            return Ok((builder, i + 1, deferred));
        }
        if line.is_empty() || line.starts_with("//") {
            i += 1;
            continue;
        }
        if line.starts_with("#[serde(") {
            let attrs = line.trim_start_matches("#[serde(").trim_end_matches(")]");
            for attr in attrs.split(',') {
                let attr = attr.trim();
                if let Some((key, val)) = attr.split_once('=') {
                    pending_serde_attrs.push((
                        key.trim().to_owned(),
                        val.trim().trim_matches('"').to_owned(),
                    ));
                } else if attr == "skip" || attr == "flatten" || attr == "default" {
                    pending_serde_attrs.push((attr.to_owned(), "true".to_owned()));
                }
            }
            i += 1;
            continue;
        }
        if line.starts_with("#[") {
            i += 1;
            continue;
        }

        let clean = line.trim_start_matches("pub ").trim();
        if let Some(colon_idx) = clean.find(':') {
            let field_name = clean[..colon_idx].trim();
            if !field_name.is_empty() && !field_name.contains(' ') {
                let field_id = format!("{parent}.{field_name}");
                builder = builder.vertex(&field_id, "field", None)?;
                vertex_ids.insert(field_id.clone());
                builder = builder.edge(parent, &field_id, "field-of", Some(field_name))?;

                let type_expr = clean[colon_idx + 1..].trim().trim_end_matches(',').trim();
                if type_expr.starts_with("Option<") {
                    builder = builder.constraint(&field_id, "optional", "true");
                }

                for (key, val) in &pending_serde_attrs {
                    builder = builder.constraint(&field_id, key, val);
                }
                pending_serde_attrs.clear();

                let base_type = extract_rust_base_type(type_expr);
                if !base_type.is_empty() {
                    deferred.push(RustFieldDeferred {
                        field_id,
                        type_name: base_type.to_owned(),
                    });
                }
            }
        }
        i += 1;
    }
    Ok((builder, i, deferred))
}

fn parse_rust_variants(
    mut builder: SchemaBuilder,
    lines: &[&str],
    start: usize,
    parent: &str,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<(SchemaBuilder, usize), ProtocolError> {
    let mut i = start;
    while i < lines.len() {
        let line = lines[i].trim();
        if line == "}" || line.starts_with('}') {
            return Ok((builder, i + 1));
        }
        if line.is_empty() || line.starts_with("//") || line.starts_with("#[") {
            i += 1;
            continue;
        }
        let variant_name = line
            .split(|c: char| c == '(' || c == '{' || c == ',' || c.is_whitespace())
            .next()
            .unwrap_or("")
            .trim();
        if !variant_name.is_empty() {
            let variant_id = format!("{parent}.{variant_name}");
            builder = builder.vertex(&variant_id, "variant", None)?;
            vertex_ids.insert(variant_id.clone());
            builder = builder.edge(parent, &variant_id, "variant-of", Some(variant_name))?;

            if line.contains('{') && !line.contains('}') {
                i += 1;
                while i < lines.len() {
                    let inner = lines[i].trim();
                    if inner.starts_with('}') || inner == "}" {
                        break;
                    }
                    i += 1;
                }
            }
        }
        i += 1;
    }
    Ok((builder, i))
}

fn extract_rust_base_type(type_expr: &str) -> &str {
    let s = type_expr.trim();
    let s = if let Some(inner) = s.strip_prefix("Option<") {
        inner.trim_end_matches('>')
    } else if let Some(inner) = s.strip_prefix("Vec<") {
        inner.trim_end_matches('>')
    } else if let Some(inner) = s.strip_prefix("Box<") {
        inner.trim_end_matches('>')
    } else {
        s
    };
    s.split('<').next().unwrap_or(s).trim()
}

/// Emit a [`Schema`] as Rust struct/enum definitions.
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema cannot be serialized.
pub fn emit_rust_types(schema: &Schema) -> Result<String, ProtocolError> {
    let structural = &["field-of", "variant-of", "type-of"];
    let roots = find_roots(schema, structural);
    let mut w = IndentWriter::new("    ");

    for root in &roots {
        match root.kind.as_str() {
            "struct" => emit_rust_struct(schema, root, &mut w)?,
            "enum" => emit_rust_enum(schema, root, &mut w)?,
            "tuple-struct" => {
                w.line("#[derive(Serialize, Deserialize)]");
                w.line(&format!("pub struct {}();", root.id));
                w.blank();
            }
            _ => {}
        }
    }
    Ok(w.finish())
}

fn emit_rust_struct(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
    w: &mut IndentWriter,
) -> Result<(), ProtocolError> {
    w.line("#[derive(Serialize, Deserialize)]");
    w.line(&format!("pub struct {} {{", vertex.id));
    w.indent();
    let fields = children_by_edge(schema, &vertex.id, "field-of");
    for (edge, field_vertex) in &fields {
        let field_name = edge.name.as_deref().unwrap_or(&field_vertex.id);
        let type_name = resolve_type(schema, &field_vertex.id)
            .map(|v| v.id.clone())
            .unwrap_or_else(|| "String".to_string());
        let is_optional =
            constraint_value(schema, &field_vertex.id, "optional").is_some_and(|v| v == "true");

        let rename = constraint_value(schema, &field_vertex.id, "rename");
        let skip = constraint_value(schema, &field_vertex.id, "skip").is_some_and(|v| v == "true");
        let flatten =
            constraint_value(schema, &field_vertex.id, "flatten").is_some_and(|v| v == "true");

        let mut serde_parts = Vec::new();
        if let Some(r) = rename {
            serde_parts.push(format!("rename = \"{r}\""));
        }
        if skip {
            serde_parts.push("skip".to_string());
        }
        if flatten {
            serde_parts.push("flatten".to_string());
        }
        if !serde_parts.is_empty() {
            w.line(&format!("#[serde({})]", serde_parts.join(", ")));
        }

        let ty = if is_optional {
            format!("Option<{type_name}>")
        } else {
            type_name
        };
        w.line(&format!("pub {field_name}: {ty},"));
    }
    w.dedent();
    w.line("}");
    w.blank();
    Ok(())
}

fn emit_rust_enum(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
    w: &mut IndentWriter,
) -> Result<(), ProtocolError> {
    w.line("#[derive(Serialize, Deserialize)]");
    w.line(&format!("pub enum {} {{", vertex.id));
    w.indent();
    let variants = children_by_edge(schema, &vertex.id, "variant-of");
    for (edge, _) in &variants {
        let name = edge.name.as_deref().unwrap_or("Unknown");
        w.line(&format!("{name},"));
    }
    w.dedent();
    w.line("}");
    w.blank();
    Ok(())
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "field-of".into(),
            src_kinds: vec!["struct".into(), "tuple-struct".into()],
            tgt_kinds: vec!["field".into()],
        },
        EdgeRule {
            edge_kind: "type-of".into(),
            src_kinds: vec!["field".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "variant-of".into(),
            src_kinds: vec!["enum".into()],
            tgt_kinds: vec!["variant".into()],
        },
    ]
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn protocol_creates_valid_definition() {
        let p = protocol();
        assert_eq!(p.name, "rust_serde");
        assert_eq!(p.schema_theory, "ThRustSerdeSchema");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThRustSerdeSchema"));
        assert!(registry.contains_key("ThRustSerdeInstance"));
    }

    #[test]
    fn parse_minimal() {
        let input = r"
struct User {
    name: String,
    age: i32,
}
";
        let schema = parse_rust_types(input).expect("should parse");
        assert!(schema.has_vertex("User"));
        assert!(schema.has_vertex("User.name"));
        assert!(schema.has_vertex("User.age"));
    }

    #[test]
    fn roundtrip() {
        let input = r"
struct User {
    name: String,
    age: i32,
}

enum Color {
    Red,
    Green,
    Blue,
}
";
        let schema = parse_rust_types(input).expect("should parse");
        let output = emit_rust_types(&schema).expect("should emit");
        assert!(output.contains("pub struct User"));
        assert!(output.contains("pub enum Color"));
    }
}
