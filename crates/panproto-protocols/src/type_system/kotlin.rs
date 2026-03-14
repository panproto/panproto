//! Kotlin data class type system protocol definition.
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

/// Returns the Kotlin protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "kotlin".into(),
        schema_theory: "ThKotlinSchema".into(),
        instance_theory: "ThKotlinInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "data-class".into(),
            "field".into(),
            "enum".into(),
            "enum-entry".into(),
            "sealed-class".into(),
            "interface".into(),
            "string".into(),
            "int".into(),
            "long".into(),
            "float".into(),
            "double".into(),
            "boolean".into(),
            "byte".into(),
            "short".into(),
            "char".into(),
            "list".into(),
            "map".into(),
            "set".into(),
            "array".into(),
        ],
        constraint_sorts: vec![
            "optional".into(),
            "default".into(),
            "val".into(),
            "var".into(),
        ],
        has_order: true,
        has_coproducts: true,
        has_recursion: true,
        nominal_identity: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for Kotlin with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_typed_graph_wtype(registry, "ThKotlinSchema", "ThKotlinInstance");
}

/// Parse Kotlin data class/enum definitions into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing or schema construction fails.
#[allow(clippy::too_many_lines)]
pub fn parse_kotlin_types(input: &str) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;
    let mut builder = SchemaBuilder::new(&proto);
    let mut vertex_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut deferred: Vec<KotlinFieldDeferred> = Vec::new();
    let mut deferred_impls: Vec<KotlinImplDeferred> = Vec::new();

    while i < lines.len() {
        let trimmed = lines[i].trim();

        if trimmed.starts_with("data class ") {
            let rest = &trimmed["data class ".len()..];
            let name = rest
                .split(|c: char| c == '(' || c == '<' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .trim();
            if !name.is_empty() {
                builder = builder.vertex(name, "data-class", None)?;
                vertex_ids.insert(name.to_owned());

                // Check for : interface conformances
                if let Some(colon_idx) = rest.find(')') {
                    let after_paren = rest[colon_idx + 1..].trim();
                    if let Some(stripped) = after_paren.strip_prefix(':') {
                        let before_brace = stripped.split('{').next().unwrap_or("");
                        for part in before_brace.split(',') {
                            let iface = part.split('(').next().unwrap_or("").trim();
                            if !iface.is_empty() {
                                deferred_impls.push(KotlinImplDeferred {
                                    type_name: name.to_owned(),
                                    iface: iface.to_owned(),
                                });
                            }
                        }
                    }
                }

                // Parse constructor params: data class Foo(val x: Int, var y: String = "")
                if let Some(paren_start) = rest.find('(') {
                    if let Some(paren_end) = rest.find(')') {
                        let params = &rest[paren_start + 1..paren_end];
                        for param in params.split(',') {
                            let param = param.trim();
                            if param.is_empty() {
                                continue;
                            }
                            let (field_name, type_expr, is_val, default_val) =
                                parse_kotlin_param(param);
                            if !field_name.is_empty() {
                                let field_id = format!("{name}.{field_name}");
                                builder = builder.vertex(&field_id, "field", None)?;
                                vertex_ids.insert(field_id.clone());
                                builder =
                                    builder.edge(name, &field_id, "field-of", Some(field_name))?;
                                if is_val {
                                    builder = builder.constraint(&field_id, "val", "true");
                                } else {
                                    builder = builder.constraint(&field_id, "var", "true");
                                }
                                if type_expr.ends_with('?') {
                                    builder = builder.constraint(&field_id, "optional", "true");
                                }
                                if let Some(dv) = default_val {
                                    builder = builder.constraint(&field_id, "default", dv);
                                }
                                let base = extract_kotlin_base_type(type_expr);
                                if !base.is_empty() {
                                    deferred.push(KotlinFieldDeferred {
                                        field_id,
                                        type_name: base.to_owned(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
            i += 1;
        } else if trimmed.starts_with("enum class ") {
            let rest = &trimmed["enum class ".len()..];
            let name = rest
                .split(|c: char| c == '{' || c == '(' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .trim();
            if !name.is_empty() {
                builder = builder.vertex(name, "enum", None)?;
                vertex_ids.insert(name.to_owned());
                i += 1;
                let (b, new_i) =
                    parse_kotlin_enum_entries(builder, &lines, i, name, &mut vertex_ids)?;
                builder = b;
                i = new_i;
                continue;
            }
            i += 1;
        } else if trimmed.starts_with("sealed class ") || trimmed.starts_with("sealed interface ") {
            let rest = if let Some(r) = trimmed.strip_prefix("sealed class ") {
                r
            } else {
                &trimmed["sealed interface ".len()..]
            };
            let name = rest
                .split(|c: char| c == '{' || c == '(' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .trim();
            if !name.is_empty() {
                builder = builder.vertex(name, "sealed-class", None)?;
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
            i += 1;
        } else if trimmed.starts_with("interface ") {
            let rest = &trimmed["interface ".len()..];
            let name = rest
                .split(|c: char| c == '{' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .trim();
            if !name.is_empty() {
                builder = builder.vertex(name, "interface", None)?;
                vertex_ids.insert(name.to_owned());
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
    for d in deferred_impls {
        if vertex_ids.contains(&d.iface) {
            builder = builder.edge(&d.type_name, &d.iface, "implements", None)?;
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

struct KotlinFieldDeferred {
    field_id: String,
    type_name: String,
}

struct KotlinImplDeferred {
    type_name: String,
    iface: String,
}

fn parse_kotlin_param(param: &str) -> (&str, &str, bool, Option<&str>) {
    let param = param.trim();
    let (is_val, rest) = if let Some(r) = param.strip_prefix("val ") {
        (true, r)
    } else if let Some(r) = param.strip_prefix("var ") {
        (false, r)
    } else {
        (true, param)
    };
    let (before_default, default) = if let Some(eq_idx) = rest.find('=') {
        let dv = rest[eq_idx + 1..].trim();
        (rest[..eq_idx].trim(), Some(dv))
    } else {
        (rest, None)
    };
    if let Some(colon_idx) = before_default.find(':') {
        let name = before_default[..colon_idx].trim();
        let ty = before_default[colon_idx + 1..].trim();
        (name, ty, is_val, default)
    } else {
        (before_default, "", is_val, default)
    }
}

fn parse_kotlin_enum_entries(
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
        if line.is_empty() || line.starts_with("//") || line == ";" {
            i += 1;
            continue;
        }
        // Stop at semicolon (enum body follows).
        if line.contains(';') {
            // Parse entries before semicolon.
            let before = line.split(';').next().unwrap_or("");
            for part in before.split(',') {
                let entry_name = part
                    .split(|c: char| c == '(' || c.is_whitespace())
                    .next()
                    .unwrap_or("")
                    .trim();
                if !entry_name.is_empty() && entry_name != "}" {
                    let entry_id = format!("{parent}.{entry_name}");
                    if !vertex_ids.contains(&entry_id) {
                        builder = builder.vertex(&entry_id, "enum-entry", None)?;
                        vertex_ids.insert(entry_id.clone());
                        builder = builder.edge(parent, &entry_id, "member-of", Some(entry_name))?;
                    }
                }
            }
            // Skip remaining enum body.
            i += 1;
            while i < lines.len() {
                let l = lines[i].trim();
                if l == "}" || l.starts_with('}') {
                    return Ok((builder, i + 1));
                }
                i += 1;
            }
            return Ok((builder, i));
        }
        for part in line.split(',') {
            let entry_name = part
                .split(|c: char| c == '(' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .trim();
            if !entry_name.is_empty() && entry_name != "}" {
                let entry_id = format!("{parent}.{entry_name}");
                if !vertex_ids.contains(&entry_id) {
                    builder = builder.vertex(&entry_id, "enum-entry", None)?;
                    vertex_ids.insert(entry_id.clone());
                    builder = builder.edge(parent, &entry_id, "member-of", Some(entry_name))?;
                }
            }
        }
        i += 1;
    }
    Ok((builder, i))
}

fn extract_kotlin_base_type(type_expr: &str) -> &str {
    let s = type_expr.trim().trim_end_matches('?');
    s.split('<').next().unwrap_or(s).trim()
}

/// Emit a [`Schema`] as Kotlin data class/enum definitions.
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema cannot be serialized.
pub fn emit_kotlin_types(schema: &Schema) -> Result<String, ProtocolError> {
    let structural = &["field-of", "member-of", "type-of", "implements"];
    let roots = find_roots(schema, structural);
    let mut w = IndentWriter::new("    ");

    for root in &roots {
        match root.kind.as_str() {
            "data-class" => emit_kotlin_data_class(schema, root, &mut w)?,
            "enum" => emit_kotlin_enum(schema, root, &mut w)?,
            "sealed-class" => {
                w.line(&format!("sealed class {}", root.id));
                w.blank();
            }
            "interface" => {
                w.line(&format!("interface {}", root.id));
                w.blank();
            }
            _ => {}
        }
    }
    Ok(w.finish())
}

fn emit_kotlin_data_class(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
    w: &mut IndentWriter,
) -> Result<(), ProtocolError> {
    let fields = children_by_edge(schema, &vertex.id, "field-of");
    let params: Vec<String> = fields
        .iter()
        .map(|(edge, field_vertex)| {
            let field_name = edge.name.as_deref().unwrap_or(&field_vertex.id);
            let type_name = resolve_type(schema, &field_vertex.id)
                .map(|v| v.id.clone())
                .unwrap_or_else(|| "Any".to_string());
            let is_optional =
                constraint_value(schema, &field_vertex.id, "optional").is_some_and(|v| v == "true");
            let is_val =
                constraint_value(schema, &field_vertex.id, "val").is_some_and(|v| v == "true");
            let keyword = if is_val { "val" } else { "var" };
            let ty = if is_optional {
                format!("{type_name}?")
            } else {
                type_name
            };
            let default = constraint_value(schema, &field_vertex.id, "default");
            let suffix = match default {
                Some(val) => format!(" = {val}"),
                None => String::new(),
            };
            format!("{keyword} {field_name}: {ty}{suffix}")
        })
        .collect();

    let impls: Vec<String> = schema
        .outgoing_edges(&vertex.id)
        .iter()
        .filter(|e| e.kind == "implements")
        .map(|e| e.tgt.clone())
        .collect();
    let impl_str = if impls.is_empty() {
        String::new()
    } else {
        format!(" : {}", impls.join(", "))
    };

    w.line(&format!(
        "data class {}({}){impl_str}",
        vertex.id,
        params.join(", ")
    ));
    w.blank();
    Ok(())
}

fn emit_kotlin_enum(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
    w: &mut IndentWriter,
) -> Result<(), ProtocolError> {
    w.line(&format!("enum class {} {{", vertex.id));
    w.indent();
    let entries = children_by_edge(schema, &vertex.id, "member-of");
    let names: Vec<&str> = entries
        .iter()
        .filter_map(|(edge, _)| edge.name.as_deref())
        .collect();
    if !names.is_empty() {
        w.line(&names.join(", "));
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
            src_kinds: vec!["data-class".into()],
            tgt_kinds: vec!["field".into()],
        },
        EdgeRule {
            edge_kind: "type-of".into(),
            src_kinds: vec!["field".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "implements".into(),
            src_kinds: vec!["data-class".into()],
            tgt_kinds: vec!["interface".into()],
        },
        EdgeRule {
            edge_kind: "member-of".into(),
            src_kinds: vec!["enum".into()],
            tgt_kinds: vec!["enum-entry".into()],
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
        assert_eq!(p.name, "kotlin");
        assert_eq!(p.schema_theory, "ThKotlinSchema");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThKotlinSchema"));
        assert!(registry.contains_key("ThKotlinInstance"));
    }

    #[test]
    fn parse_minimal() {
        let input = r"
data class User(val name: String, val age: Int = 0)
";
        let schema = parse_kotlin_types(input).expect("should parse");
        assert!(schema.has_vertex("User"));
        assert!(schema.has_vertex("User.name"));
        assert!(schema.has_vertex("User.age"));
    }

    #[test]
    fn roundtrip() {
        let input = r"
data class User(val name: String, val age: Int)

enum class Color {
    RED, GREEN
}
";
        let schema = parse_kotlin_types(input).expect("should parse");
        let output = emit_kotlin_types(&schema).expect("should emit");
        assert!(output.contains("data class User"));
        assert!(output.contains("enum class Color"));
    }
}
