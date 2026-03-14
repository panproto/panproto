//! Go struct type system protocol definition.
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

/// Returns the Go protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "go_struct".into(),
        schema_theory: "ThGoSchema".into(),
        instance_theory: "ThGoInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "struct".into(),
            "field".into(),
            "interface".into(),
            "method".into(),
            "string".into(),
            "int".into(),
            "int8".into(),
            "int16".into(),
            "int32".into(),
            "int64".into(),
            "uint".into(),
            "uint8".into(),
            "uint16".into(),
            "uint32".into(),
            "uint64".into(),
            "float32".into(),
            "float64".into(),
            "bool".into(),
            "byte".into(),
            "rune".into(),
            "slice".into(),
            "map-type".into(),
        ],
        constraint_sorts: vec!["json_tag".into(), "omitempty".into()],
        has_order: true,
        has_recursion: true,
        nominal_identity: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for Go with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_typed_graph_wtype(registry, "ThGoSchema", "ThGoInstance");
}

/// Parse Go struct/interface definitions into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing or schema construction fails.
#[allow(clippy::too_many_lines)]
pub fn parse_go_types(input: &str) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;
    let mut builder = SchemaBuilder::new(&proto);
    let mut vertex_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut deferred: Vec<GoFieldDeferred> = Vec::new();

    while i < lines.len() {
        let trimmed = lines[i].trim();

        if trimmed.starts_with("type ") {
            let rest = &trimmed["type ".len()..];
            // type Name struct { or type Name interface {
            let parts: Vec<&str> = rest.splitn(3, ' ').collect();
            if parts.len() >= 2 {
                let name = parts[0].trim();
                let kind_keyword = parts[1].trim().trim_end_matches('{').trim();

                if kind_keyword == "struct" {
                    builder = builder.vertex(name, "struct", None)?;
                    vertex_ids.insert(name.to_owned());
                    i += 1;
                    let (b, new_i, new_deferred) =
                        parse_go_struct_fields(builder, &lines, i, name, &mut vertex_ids)?;
                    builder = b;
                    deferred.extend(new_deferred);
                    i = new_i;
                    continue;
                } else if kind_keyword == "interface" {
                    builder = builder.vertex(name, "interface", None)?;
                    vertex_ids.insert(name.to_owned());
                    i += 1;
                    let (b, new_i) =
                        parse_go_interface_methods(builder, &lines, i, name, &mut vertex_ids)?;
                    builder = b;
                    i = new_i;
                    continue;
                }
            }
        }
        i += 1;
    }

    for d in deferred {
        if vertex_ids.contains(&d.type_name) {
            builder = builder.edge(&d.field_id, &d.type_name, "type-of", None)?;
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

struct GoFieldDeferred {
    field_id: String,
    type_name: String,
}

fn parse_go_struct_fields(
    mut builder: SchemaBuilder,
    lines: &[&str],
    start: usize,
    parent: &str,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<(SchemaBuilder, usize, Vec<GoFieldDeferred>), ProtocolError> {
    let mut i = start;
    let mut deferred = Vec::new();

    while i < lines.len() {
        let line = lines[i].trim();
        if line == "}" || line.starts_with('}') {
            return Ok((builder, i + 1, deferred));
        }
        if line.is_empty() || line.starts_with("//") {
            i += 1;
            continue;
        }

        // Parse: FieldName Type `json:"tag"`
        let parts: Vec<&str> = line.splitn(3, |c: char| c.is_whitespace()).collect();
        if parts.len() >= 2 {
            let field_name = parts[0].trim();
            let type_expr = parts[1].trim();
            if !field_name.is_empty() && field_name.chars().next().is_some_and(|c| c.is_uppercase())
            {
                let field_id = format!("{parent}.{field_name}");
                builder = builder.vertex(&field_id, "field", None)?;
                vertex_ids.insert(field_id.clone());
                builder = builder.edge(parent, &field_id, "field-of", Some(field_name))?;

                // Extract json tag if present.
                let tag_part = if parts.len() >= 3 { parts[2] } else { "" };
                if let Some(json_val) = extract_go_json_tag(tag_part) {
                    let (tag_name, omitempty) = parse_go_tag_value(&json_val);
                    if !tag_name.is_empty() {
                        builder = builder.constraint(&field_id, "json_tag", &tag_name);
                    }
                    if omitempty {
                        builder = builder.constraint(&field_id, "omitempty", "true");
                    }
                }

                let base_type = extract_go_base_type(type_expr);
                if !base_type.is_empty() {
                    deferred.push(GoFieldDeferred {
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

fn parse_go_interface_methods(
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
        if line.is_empty() || line.starts_with("//") {
            i += 1;
            continue;
        }
        // Method signature: MethodName(params) returnType
        let method_name = line
            .split(|c: char| c == '(' || c.is_whitespace())
            .next()
            .unwrap_or("")
            .trim();
        if !method_name.is_empty() {
            let method_id = format!("{parent}.{method_name}");
            if !vertex_ids.contains(&method_id) {
                builder = builder.vertex(&method_id, "method", None)?;
                vertex_ids.insert(method_id.clone());
                builder = builder.edge(parent, &method_id, "field-of", Some(method_name))?;
            }
        }
        i += 1;
    }
    Ok((builder, i))
}

fn extract_go_json_tag(tag_str: &str) -> Option<String> {
    // `json:"name,omitempty"`
    let start = tag_str.find("json:\"")?;
    let after = &tag_str[start + "json:\"".len()..];
    let end = after.find('"')?;
    Some(after[..end].to_owned())
}

fn parse_go_tag_value(tag: &str) -> (String, bool) {
    let parts: Vec<&str> = tag.split(',').collect();
    let name = parts.first().unwrap_or(&"").to_string();
    let omitempty = parts.contains(&"omitempty");
    (name, omitempty)
}

fn extract_go_base_type(type_expr: &str) -> &str {
    let s = type_expr.trim();
    let s = s.trim_start_matches('*');
    let s = s.trim_start_matches("[]");
    s.split('[').next().unwrap_or(s).trim()
}

/// Emit a [`Schema`] as Go struct/interface definitions.
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema cannot be serialized.
pub fn emit_go_types(schema: &Schema) -> Result<String, ProtocolError> {
    let structural = &["field-of", "type-of", "implements"];
    let roots = find_roots(schema, structural);
    let mut w = IndentWriter::new("\t");

    for root in &roots {
        match root.kind.as_str() {
            "struct" => emit_go_struct(schema, root, &mut w)?,
            "interface" => emit_go_interface(schema, root, &mut w)?,
            _ => {}
        }
    }
    Ok(w.finish())
}

fn emit_go_struct(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
    w: &mut IndentWriter,
) -> Result<(), ProtocolError> {
    w.line(&format!("type {} struct {{", vertex.id));
    w.indent();
    let fields = children_by_edge(schema, &vertex.id, "field-of");
    for (edge, field_vertex) in &fields {
        let field_name = edge.name.as_deref().unwrap_or(&field_vertex.id);
        let type_name = resolve_type(schema, &field_vertex.id)
            .map(|v| v.id.clone())
            .unwrap_or_else(|| "string".to_string());

        let json_tag = constraint_value(schema, &field_vertex.id, "json_tag");
        let omitempty =
            constraint_value(schema, &field_vertex.id, "omitempty").is_some_and(|v| v == "true");

        let tag_str = match (json_tag, omitempty) {
            (Some(tag), true) => format!(" `json:\"{tag},omitempty\"`"),
            (Some(tag), false) => format!(" `json:\"{tag}\"`"),
            (None, true) => format!(" `json:\"{},omitempty\"`", field_name.to_lowercase()),
            (None, false) => String::new(),
        };
        w.line(&format!("{field_name} {type_name}{tag_str}"));
    }
    w.dedent();
    w.line("}");
    w.blank();
    Ok(())
}

fn emit_go_interface(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
    w: &mut IndentWriter,
) -> Result<(), ProtocolError> {
    w.line(&format!("type {} interface {{", vertex.id));
    w.indent();
    let methods = children_by_edge(schema, &vertex.id, "field-of");
    for (edge, _) in &methods {
        let method_name = edge.name.as_deref().unwrap_or("Unknown");
        w.line(method_name);
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
            src_kinds: vec!["struct".into(), "interface".into()],
            tgt_kinds: vec!["field".into(), "method".into()],
        },
        EdgeRule {
            edge_kind: "type-of".into(),
            src_kinds: vec!["field".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "implements".into(),
            src_kinds: vec!["struct".into()],
            tgt_kinds: vec!["interface".into()],
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
        assert_eq!(p.name, "go_struct");
        assert_eq!(p.schema_theory, "ThGoSchema");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThGoSchema"));
        assert!(registry.contains_key("ThGoInstance"));
    }

    #[test]
    fn parse_minimal() {
        let input = r#"
type User struct {
    Name string `json:"name"`
    Age  int    `json:"age"`
}
"#;
        let schema = parse_go_types(input).expect("should parse");
        assert!(schema.has_vertex("User"));
        assert!(schema.has_vertex("User.Name"));
        assert!(schema.has_vertex("User.Age"));
    }

    #[test]
    fn roundtrip() {
        let input = r#"
type User struct {
    Name string `json:"name"`
    Age  int    `json:"age"`
}

type Reader interface {
    Read()
}
"#;
        let schema = parse_go_types(input).expect("should parse");
        let output = emit_go_types(&schema).expect("should emit");
        assert!(output.contains("type User struct"));
        assert!(output.contains("type Reader interface"));
    }
}
