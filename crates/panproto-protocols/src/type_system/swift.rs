//! Swift Codable type system protocol definition.
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

/// Returns the Swift protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "swift".into(),
        schema_theory: "ThSwiftSchema".into(),
        instance_theory: "ThSwiftInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "struct".into(),
            "class".into(),
            "field".into(),
            "enum".into(),
            "case".into(),
            "protocol".into(),
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
            "float".into(),
            "double".into(),
            "bool".into(),
            "data".into(),
            "date".into(),
            "array".into(),
            "dictionary".into(),
            "set".into(),
            "optional".into(),
        ],
        constraint_sorts: vec!["optional".into(), "codingKey".into(), "default".into()],
    }
}

/// Register the component GATs for Swift with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_typed_graph_wtype(registry, "ThSwiftSchema", "ThSwiftInstance");
}

/// Parse Swift Codable type declarations into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing or schema construction fails.
#[allow(clippy::too_many_lines)]
pub fn parse_swift_types(input: &str) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;
    let mut builder = SchemaBuilder::new(&proto);
    let mut vertex_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut deferred: Vec<SwiftFieldDeferred> = Vec::new();
    let mut deferred_impls: Vec<SwiftImplDeferred> = Vec::new();

    while i < lines.len() {
        let trimmed = lines[i].trim();

        if trimmed.starts_with("struct ") || trimmed.starts_with("public struct ") {
            let rest = if let Some(r) = trimmed.strip_prefix("public struct ") {
                r
            } else {
                &trimmed["struct ".len()..]
            };
            let (name, protocols) = parse_swift_conformances(rest);
            if !name.is_empty() {
                builder = builder.vertex(name, "struct", None)?;
                vertex_ids.insert(name.to_owned());
                for p in protocols {
                    deferred_impls.push(SwiftImplDeferred {
                        type_name: name.to_owned(),
                        iface: p,
                    });
                }
                i += 1;
                let (b, new_i, new_deferred) =
                    parse_swift_fields(builder, &lines, i, name, &mut vertex_ids)?;
                builder = b;
                deferred.extend(new_deferred);
                i = new_i;
                continue;
            }
        } else if trimmed.starts_with("class ") || trimmed.starts_with("public class ") {
            let rest = if let Some(r) = trimmed.strip_prefix("public class ") {
                r
            } else {
                &trimmed["class ".len()..]
            };
            let (name, protocols) = parse_swift_conformances(rest);
            if !name.is_empty() {
                builder = builder.vertex(name, "class", None)?;
                vertex_ids.insert(name.to_owned());
                for p in protocols {
                    deferred_impls.push(SwiftImplDeferred {
                        type_name: name.to_owned(),
                        iface: p,
                    });
                }
                i += 1;
                let (b, new_i, new_deferred) =
                    parse_swift_fields(builder, &lines, i, name, &mut vertex_ids)?;
                builder = b;
                deferred.extend(new_deferred);
                i = new_i;
                continue;
            }
        } else if trimmed.starts_with("enum ") || trimmed.starts_with("public enum ") {
            let rest = if let Some(r) = trimmed.strip_prefix("public enum ") {
                r
            } else {
                &trimmed["enum ".len()..]
            };
            let (name, _protocols) = parse_swift_conformances(rest);
            if !name.is_empty() {
                builder = builder.vertex(name, "enum", None)?;
                vertex_ids.insert(name.to_owned());
                i += 1;
                let (b, new_i) = parse_swift_cases(builder, &lines, i, name, &mut vertex_ids)?;
                builder = b;
                i = new_i;
                continue;
            }
        } else if trimmed.starts_with("protocol ") || trimmed.starts_with("public protocol ") {
            let rest = if let Some(r) = trimmed.strip_prefix("public protocol ") {
                r
            } else {
                &trimmed["protocol ".len()..]
            };
            let name = rest
                .split(|c: char| c == '{' || c == ':' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .trim();
            if !name.is_empty() {
                builder = builder.vertex(name, "protocol", None)?;
                vertex_ids.insert(name.to_owned());
                // Skip body.
                if trimmed.contains('{') {
                    let mut depth = 1i32;
                    i += 1;
                    while i < lines.len() && depth > 0 {
                        let l = lines[i].trim();
                        depth += l.matches('{').count() as i32;
                        depth -= l.matches('}').count() as i32;
                        i += 1;
                    }
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
    for d in deferred_impls {
        if vertex_ids.contains(&d.iface) {
            builder = builder.edge(&d.type_name, &d.iface, "implements", None)?;
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

struct SwiftFieldDeferred {
    field_id: String,
    type_name: String,
}

struct SwiftImplDeferred {
    type_name: String,
    iface: String,
}

fn parse_swift_conformances(rest: &str) -> (&str, Vec<String>) {
    let name = rest
        .split(|c: char| c == '{' || c == ':' || c.is_whitespace())
        .next()
        .unwrap_or("")
        .trim();
    let mut protocols = Vec::new();
    if let Some(colon_idx) = rest.find(':') {
        let after = rest[colon_idx + 1..].split('{').next().unwrap_or("");
        for part in after.split(',') {
            let p = part.trim();
            if !p.is_empty() {
                protocols.push(p.to_owned());
            }
        }
    }
    (name, protocols)
}

fn parse_swift_fields(
    mut builder: SchemaBuilder,
    lines: &[&str],
    start: usize,
    parent: &str,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<(SchemaBuilder, usize, Vec<SwiftFieldDeferred>), ProtocolError> {
    let mut i = start;
    let mut deferred = Vec::new();
    let mut depth = 1i32;

    while i < lines.len() && depth > 0 {
        let line = lines[i].trim();
        depth += line.matches('{').count() as i32;
        depth -= line.matches('}').count() as i32;
        if depth <= 0 {
            return Ok((builder, i + 1, deferred));
        }
        if line.is_empty() || line.starts_with("//") {
            i += 1;
            continue;
        }

        // Parse: let/var name: Type or let/var name: Type?
        let clean = line.trim_start_matches("public ").trim();
        if (clean.starts_with("let ") || clean.starts_with("var ")) && clean.contains(':') {
            let after_keyword = if clean.starts_with("let ") {
                &clean["let ".len()..]
            } else {
                &clean["var ".len()..]
            };
            if let Some(colon_idx) = after_keyword.find(':') {
                let field_name = after_keyword[..colon_idx].trim();
                let type_expr = after_keyword[colon_idx + 1..]
                    .split('=')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_end_matches(';')
                    .trim();

                if !field_name.is_empty() && !field_name.contains(' ') {
                    let field_id = format!("{parent}.{field_name}");
                    builder = builder.vertex(&field_id, "field", None)?;
                    vertex_ids.insert(field_id.clone());
                    builder = builder.edge(parent, &field_id, "field-of", Some(field_name))?;

                    if type_expr.ends_with('?') {
                        builder = builder.constraint(&field_id, "optional", "true");
                    }

                    let base = extract_swift_base_type(type_expr);
                    if !base.is_empty() {
                        deferred.push(SwiftFieldDeferred {
                            field_id,
                            type_name: base.to_owned(),
                        });
                    }
                }
            }
        }
        i += 1;
    }
    Ok((builder, i, deferred))
}

fn parse_swift_cases(
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
        if let Some(rest) = line.strip_prefix("case ") {
            for part in rest.split(',') {
                let case_name = part
                    .split(|c: char| c == '=' || c == '(' || c.is_whitespace())
                    .next()
                    .unwrap_or("")
                    .trim();
                if !case_name.is_empty() {
                    let case_id = format!("{parent}.{case_name}");
                    if !vertex_ids.contains(&case_id) {
                        builder = builder.vertex(&case_id, "case", None)?;
                        vertex_ids.insert(case_id.clone());
                        builder = builder.edge(parent, &case_id, "member-of", Some(case_name))?;
                    }
                }
            }
        }
        i += 1;
    }
    Ok((builder, i))
}

fn extract_swift_base_type(type_expr: &str) -> &str {
    let s = type_expr.trim().trim_end_matches('?');
    let s = s.trim_start_matches('[').trim_end_matches(']');
    s.split('<').next().unwrap_or(s).trim()
}

/// Emit a [`Schema`] as Swift Codable declarations.
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema cannot be serialized.
pub fn emit_swift_types(schema: &Schema) -> Result<String, ProtocolError> {
    let structural = &["field-of", "member-of", "type-of", "implements"];
    let roots = find_roots(schema, structural);
    let mut w = IndentWriter::new("    ");

    for root in &roots {
        match root.kind.as_str() {
            "struct" => emit_swift_struct(schema, root, &mut w)?,
            "class" => emit_swift_class(schema, root, &mut w)?,
            "enum" => emit_swift_enum(schema, root, &mut w)?,
            "protocol" => {
                w.line(&format!("protocol {} {{}}", root.id));
                w.blank();
            }
            _ => {}
        }
    }
    Ok(w.finish())
}

fn conformances_str(schema: &Schema, vertex_id: &str) -> String {
    let impls: Vec<String> = schema
        .outgoing_edges(vertex_id)
        .iter()
        .filter(|e| e.kind == "implements")
        .map(|e| e.tgt.clone())
        .collect();
    if impls.is_empty() {
        String::new()
    } else {
        format!(": {}", impls.join(", "))
    }
}

fn emit_swift_struct(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
    w: &mut IndentWriter,
) -> Result<(), ProtocolError> {
    let conf = conformances_str(schema, &vertex.id);
    w.line(&format!("struct {}{conf} {{", vertex.id));
    w.indent();
    emit_swift_field_list(schema, &vertex.id, w);
    w.dedent();
    w.line("}");
    w.blank();
    Ok(())
}

fn emit_swift_class(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
    w: &mut IndentWriter,
) -> Result<(), ProtocolError> {
    let conf = conformances_str(schema, &vertex.id);
    w.line(&format!("class {}{conf} {{", vertex.id));
    w.indent();
    emit_swift_field_list(schema, &vertex.id, w);
    w.dedent();
    w.line("}");
    w.blank();
    Ok(())
}

fn emit_swift_field_list(schema: &Schema, parent_id: &str, w: &mut IndentWriter) {
    let fields = children_by_edge(schema, parent_id, "field-of");
    for (edge, field_vertex) in &fields {
        let field_name = edge.name.as_deref().unwrap_or(&field_vertex.id);
        let type_name = resolve_type(schema, &field_vertex.id)
            .map(|v| v.id.clone())
            .unwrap_or_else(|| "Any".to_string());
        let is_optional =
            constraint_value(schema, &field_vertex.id, "optional").is_some_and(|v| v == "true");
        let ty = if is_optional {
            format!("{type_name}?")
        } else {
            type_name
        };
        w.line(&format!("let {field_name}: {ty}"));
    }
}

fn emit_swift_enum(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
    w: &mut IndentWriter,
) -> Result<(), ProtocolError> {
    w.line(&format!("enum {} {{", vertex.id));
    w.indent();
    let cases = children_by_edge(schema, &vertex.id, "member-of");
    for (edge, _) in &cases {
        let name = edge.name.as_deref().unwrap_or("unknown");
        w.line(&format!("case {name}"));
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
            src_kinds: vec!["struct".into(), "class".into()],
            tgt_kinds: vec!["field".into()],
        },
        EdgeRule {
            edge_kind: "type-of".into(),
            src_kinds: vec!["field".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "implements".into(),
            src_kinds: vec!["struct".into(), "class".into()],
            tgt_kinds: vec!["protocol".into()],
        },
        EdgeRule {
            edge_kind: "member-of".into(),
            src_kinds: vec!["enum".into()],
            tgt_kinds: vec!["case".into()],
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
        assert_eq!(p.name, "swift");
        assert_eq!(p.schema_theory, "ThSwiftSchema");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThSwiftSchema"));
        assert!(registry.contains_key("ThSwiftInstance"));
    }

    #[test]
    fn parse_minimal() {
        let input = r"
struct User: Codable {
    let name: String
    var age: Int?
}
";
        let schema = parse_swift_types(input).expect("should parse");
        assert!(schema.has_vertex("User"));
        assert!(schema.has_vertex("User.name"));
        assert!(schema.has_vertex("User.age"));
    }

    #[test]
    fn roundtrip() {
        let input = r"
struct User {
    let name: String
    let age: Int
}

enum Color {
    case red
    case green
}
";
        let schema = parse_swift_types(input).expect("should parse");
        let output = emit_swift_types(&schema).expect("should emit");
        assert!(output.contains("struct User"));
        assert!(output.contains("enum Color"));
    }
}
