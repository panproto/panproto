//! TypeScript type system protocol definition.
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

/// Returns the TypeScript protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "typescript".into(),
        schema_theory: "ThTypeScriptSchema".into(),
        instance_theory: "ThTypeScriptInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "interface".into(),
            "type-alias".into(),
            "field".into(),
            "enum".into(),
            "enum-member".into(),
            "class".into(),
            "union".into(),
            "intersection".into(),
            "tuple".into(),
            "string".into(),
            "number".into(),
            "boolean".into(),
            "null".into(),
            "undefined".into(),
            "void".into(),
            "any".into(),
            "unknown".into(),
            "bigint".into(),
            "symbol".into(),
            "array".into(),
            "record".into(),
        ],
        constraint_sorts: vec!["optional".into(), "readonly".into(), "deprecated".into()],
        has_order: true,
        has_coproducts: true,
        has_recursion: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for TypeScript with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_typed_graph_wtype(registry, "ThTypeScriptSchema", "ThTypeScriptInstance");
}

/// Parse TypeScript type declarations into a [`Schema`].
///
/// Handles `interface`, `type`, `enum`, and `class` declarations.
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing or schema construction fails.
#[allow(clippy::too_many_lines)]
pub fn parse_ts_types(input: &str) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;
    let mut builder = SchemaBuilder::new(&proto);
    let mut vertex_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    struct ImplementsDeferred {
        type_name: String,
        iface: String,
    }
    let mut deferred_types: Vec<TsFieldDeferred> = Vec::new();
    let mut deferred_impls: Vec<ImplementsDeferred> = Vec::new();

    while i < lines.len() {
        let trimmed = lines[i].trim();

        if trimmed.starts_with("interface ") {
            let rest = &trimmed["interface ".len()..];
            let (name, extends) = parse_ts_name_and_extends(rest);
            if name.is_empty() {
                i += 1;
            } else {
                builder = builder.vertex(name, "interface", None)?;
                vertex_ids.insert(name.to_owned());
                for iface in extends {
                    deferred_impls.push(ImplementsDeferred {
                        type_name: name.to_owned(),
                        iface,
                    });
                }
                i += 1;
                let (b, new_i, deferred) =
                    parse_ts_fields(builder, &lines, i, name, &mut vertex_ids)?;
                builder = b;
                deferred_types.extend(deferred);
                i = new_i;
            }
        } else if trimmed.starts_with("type ") {
            let rest = &trimmed["type ".len()..];
            let name = rest
                .split(|c: char| c == '=' || c == '<' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .trim();
            if name.is_empty() {
                i += 1;
            } else {
                // Check if it's an object type alias: type X = { ... }
                if rest.contains('{') {
                    builder = builder.vertex(name, "type-alias", None)?;
                    vertex_ids.insert(name.to_owned());
                    i += 1;
                    let (b, new_i, deferred) =
                        parse_ts_fields(builder, &lines, i, name, &mut vertex_ids)?;
                    builder = b;
                    deferred_types.extend(deferred);
                    i = new_i;
                } else {
                    builder = builder.vertex(name, "type-alias", None)?;
                    vertex_ids.insert(name.to_owned());
                    i += 1;
                }
            }
        } else if trimmed.starts_with("enum ") {
            let rest = &trimmed["enum ".len()..];
            let name = rest
                .split(|c: char| c == '{' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .trim();
            if name.is_empty() {
                i += 1;
            } else {
                builder = builder.vertex(name, "enum", None)?;
                vertex_ids.insert(name.to_owned());
                i += 1;
                let (b, new_i) = parse_ts_enum_members(builder, &lines, i, name, &mut vertex_ids)?;
                builder = b;
                i = new_i;
            }
        } else if trimmed.starts_with("class ")
            || trimmed.starts_with("export class ")
            || trimmed.starts_with("abstract class ")
        {
            let rest = if let Some(r) = trimmed.strip_prefix("export class ") {
                r
            } else if let Some(r) = trimmed.strip_prefix("abstract class ") {
                r
            } else {
                &trimmed["class ".len()..]
            };
            let (name, extends_list) = parse_ts_class_header(rest);
            if name.is_empty() {
                i += 1;
            } else {
                builder = builder.vertex(name, "class", None)?;
                vertex_ids.insert(name.to_owned());
                for iface in extends_list {
                    deferred_impls.push(ImplementsDeferred {
                        type_name: name.to_owned(),
                        iface,
                    });
                }
                i += 1;
                let (b, new_i, deferred) =
                    parse_ts_fields(builder, &lines, i, name, &mut vertex_ids)?;
                builder = b;
                deferred_types.extend(deferred);
                i = new_i;
            }
        } else {
            i += 1;
        }
    }

    // Resolve deferred type-of edges.
    for d in deferred_types {
        if vertex_ids.contains(&d.type_name) {
            builder = builder.edge(&d.field_id, &d.type_name, "type-of", None)?;
        }
    }
    for d in deferred_impls {
        if vertex_ids.contains(&d.iface) {
            builder = builder.edge(&d.type_name, &d.iface, "implements", None)?;
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

fn parse_ts_name_and_extends(rest: &str) -> (&str, Vec<String>) {
    let name = rest
        .split(|c: char| c == '{' || c == '<' || c.is_whitespace())
        .next()
        .unwrap_or("")
        .trim();
    let mut extends = Vec::new();
    if let Some(idx) = rest.find("extends") {
        let after = &rest[idx + "extends".len()..];
        let before_brace = after.split('{').next().unwrap_or("");
        for part in before_brace.split(',') {
            let iface = part.trim();
            if !iface.is_empty() {
                extends.push(iface.to_owned());
            }
        }
    }
    (name, extends)
}

fn parse_ts_class_header(rest: &str) -> (&str, Vec<String>) {
    let name = rest
        .split(|c: char| c == '{' || c == '<' || c.is_whitespace())
        .next()
        .unwrap_or("")
        .trim();
    let mut impls = Vec::new();
    if let Some(idx) = rest.find("implements") {
        let after = &rest[idx + "implements".len()..];
        let before_brace = after.split('{').next().unwrap_or("");
        for part in before_brace.split(',') {
            let iface = part.trim();
            if !iface.is_empty() {
                impls.push(iface.to_owned());
            }
        }
    }
    // Also handle extends for classes
    if let Some(idx) = rest.find("extends") {
        let after = &rest[idx + "extends".len()..];
        let before = after.split(['{', ' ']).next().unwrap_or("").trim();
        if !before.is_empty() && before != "implements" && !before.starts_with("implements") {
            impls.push(before.to_owned());
        }
    }
    (name, impls)
}

struct TsFieldDeferred {
    field_id: String,
    type_name: String,
}

fn parse_ts_fields(
    mut builder: SchemaBuilder,
    lines: &[&str],
    start: usize,
    parent: &str,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<(SchemaBuilder, usize, Vec<TsFieldDeferred>), ProtocolError> {
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
        // Parse field: name: Type; or name?: Type;
        if let Some(colon_idx) = line.find(':') {
            let mut field_part = line[..colon_idx].trim();
            let mut is_optional = false;
            let mut is_readonly = false;
            if field_part.starts_with("readonly ") {
                is_readonly = true;
                field_part = field_part["readonly ".len()..].trim();
            }
            if let Some(stripped) = field_part.strip_suffix('?') {
                is_optional = true;
                field_part = stripped.trim();
            }
            let field_name = field_part;
            if !field_name.is_empty() && !field_name.contains(' ') {
                let field_id = format!("{parent}.{field_name}");
                builder = builder.vertex(&field_id, "field", None)?;
                vertex_ids.insert(field_id.clone());
                builder = builder.edge(parent, &field_id, "field-of", Some(field_name))?;
                if is_optional {
                    builder = builder.constraint(&field_id, "optional", "true");
                }
                if is_readonly {
                    builder = builder.constraint(&field_id, "readonly", "true");
                }

                let type_expr = line[colon_idx + 1..].trim().trim_end_matches(';').trim();
                let base_type = extract_ts_base_type(type_expr);
                if !base_type.is_empty() {
                    deferred.push(TsFieldDeferred {
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

fn parse_ts_enum_members(
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
        let member_name = line.split(['=', ',']).next().unwrap_or("").trim();
        if !member_name.is_empty() {
            let member_id = format!("{parent}.{member_name}");
            builder = builder.vertex(&member_id, "enum-member", None)?;
            vertex_ids.insert(member_id.clone());
            builder = builder.edge(parent, &member_id, "member-of", Some(member_name))?;
        }
        i += 1;
    }
    Ok((builder, i))
}

fn extract_ts_base_type(type_expr: &str) -> &str {
    let s = type_expr.trim();
    let s = s.trim_end_matches("[]");
    let s = s.split('<').next().unwrap_or(s);
    let s = s.split('|').next().unwrap_or(s);
    s.trim()
}

/// Emit a [`Schema`] as TypeScript type declarations.
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema cannot be serialized.
pub fn emit_ts_types(schema: &Schema) -> Result<String, ProtocolError> {
    let structural = &["field-of", "member-of", "type-of", "implements"];
    let roots = find_roots(schema, structural);
    let mut w = IndentWriter::new("  ");

    for root in &roots {
        match root.kind.as_str() {
            "interface" => emit_ts_interface(schema, root, &mut w)?,
            "type-alias" => emit_ts_type_alias(schema, root, &mut w)?,
            "enum" => emit_ts_enum(schema, root, &mut w)?,
            "class" => emit_ts_class(schema, root, &mut w)?,
            _ => {}
        }
    }
    Ok(w.finish())
}

fn emit_ts_interface(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
    w: &mut IndentWriter,
) -> Result<(), ProtocolError> {
    let impls = implements_clause(schema, &vertex.id);
    let ext = if impls.is_empty() {
        String::new()
    } else {
        format!(" extends {}", impls.join(", "))
    };
    w.line(&format!("interface {}{ext} {{", vertex.id));
    w.indent();
    emit_ts_field_list(schema, &vertex.id, w);
    w.dedent();
    w.line("}");
    w.blank();
    Ok(())
}

fn emit_ts_type_alias(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
    w: &mut IndentWriter,
) -> Result<(), ProtocolError> {
    let fields = children_by_edge(schema, &vertex.id, "field-of");
    if fields.is_empty() {
        w.line(&format!("type {} = {{}};", vertex.id));
    } else {
        w.line(&format!("type {} = {{", vertex.id));
        w.indent();
        emit_ts_field_list(schema, &vertex.id, w);
        w.dedent();
        w.line("};");
    }
    w.blank();
    Ok(())
}

fn emit_ts_enum(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
    w: &mut IndentWriter,
) -> Result<(), ProtocolError> {
    w.line(&format!("enum {} {{", vertex.id));
    w.indent();
    let members = children_by_edge(schema, &vertex.id, "member-of");
    for (edge, _) in &members {
        let name = edge.name.as_deref().unwrap_or("UNKNOWN");
        w.line(&format!("{name},"));
    }
    w.dedent();
    w.line("}");
    w.blank();
    Ok(())
}

fn emit_ts_class(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
    w: &mut IndentWriter,
) -> Result<(), ProtocolError> {
    let impls = implements_clause(schema, &vertex.id);
    let impl_str = if impls.is_empty() {
        String::new()
    } else {
        format!(" implements {}", impls.join(", "))
    };
    w.line(&format!("class {}{impl_str} {{", vertex.id));
    w.indent();
    emit_ts_field_list(schema, &vertex.id, w);
    w.dedent();
    w.line("}");
    w.blank();
    Ok(())
}

fn emit_ts_field_list(schema: &Schema, parent_id: &str, w: &mut IndentWriter) {
    let fields = children_by_edge(schema, parent_id, "field-of");
    for (edge, field_vertex) in &fields {
        let field_name = edge.name.as_deref().unwrap_or(&field_vertex.id);
        let is_optional =
            constraint_value(schema, &field_vertex.id, "optional").is_some_and(|v| v == "true");
        let is_readonly =
            constraint_value(schema, &field_vertex.id, "readonly").is_some_and(|v| v == "true");
        let type_name = resolve_type(schema, &field_vertex.id)
            .map(|v| v.id.clone())
            .unwrap_or_else(|| "any".to_string());
        let readonly_prefix = if is_readonly { "readonly " } else { "" };
        let opt = if is_optional { "?" } else { "" };
        w.line(&format!("{readonly_prefix}{field_name}{opt}: {type_name};"));
    }
}

fn implements_clause(schema: &Schema, vertex_id: &str) -> Vec<String> {
    let mut impls: Vec<String> = schema
        .outgoing_edges(vertex_id)
        .iter()
        .filter(|e| e.kind == "implements")
        .map(|e| e.tgt.clone())
        .collect();
    impls.sort();
    impls
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "field-of".into(),
            src_kinds: vec!["interface".into(), "type-alias".into(), "class".into()],
            tgt_kinds: vec!["field".into()],
        },
        EdgeRule {
            edge_kind: "type-of".into(),
            src_kinds: vec!["field".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "implements".into(),
            src_kinds: vec!["interface".into(), "class".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "member-of".into(),
            src_kinds: vec!["enum".into()],
            tgt_kinds: vec!["enum-member".into()],
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
        assert_eq!(p.name, "typescript");
        assert_eq!(p.schema_theory, "ThTypeScriptSchema");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThTypeScriptSchema"));
        assert!(registry.contains_key("ThTypeScriptInstance"));
    }

    #[test]
    fn parse_minimal() {
        let input = r"
interface User {
  name: string;
  age: number;
}
";
        let schema = parse_ts_types(input).expect("should parse");
        assert!(schema.has_vertex("User"));
        assert!(schema.has_vertex("User.name"));
        assert!(schema.has_vertex("User.age"));
    }

    #[test]
    fn roundtrip() {
        let input = r"
interface User {
  name: string;
}

enum Color {
  Red,
  Green,
}
";
        let schema = parse_ts_types(input).expect("should parse");
        let output = emit_ts_types(&schema).expect("should emit");
        assert!(output.contains("interface User"));
        assert!(output.contains("enum Color"));
    }
}
