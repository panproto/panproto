//! Java type system protocol definition.
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

use crate::emit::{IndentWriter, children_by_edge, find_roots, resolve_type};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the Java protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "java".into(),
        schema_theory: "ThJavaSchema".into(),
        instance_theory: "ThJavaInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "class".into(),
            "record".into(),
            "field".into(),
            "enum".into(),
            "enum-constant".into(),
            "interface".into(),
            "method".into(),
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
            "optional".into(),
        ],
        constraint_sorts: vec![
            "optional".into(),
            "final".into(),
            "static".into(),
            "transient".into(),
        ],
        has_order: true,
        has_coproducts: true,
        has_recursion: true,
        nominal_identity: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for Java with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_typed_graph_wtype(registry, "ThJavaSchema", "ThJavaInstance");
}

/// Parse Java type definitions into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing or schema construction fails.
#[allow(clippy::too_many_lines)]
pub fn parse_java_types(input: &str) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;
    let mut builder = SchemaBuilder::new(&proto);
    let mut vertex_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    struct ImplDeferred {
        type_name: String,
        iface: String,
    }
    let mut deferred: Vec<JavaFieldDeferred> = Vec::new();
    let mut deferred_impls: Vec<ImplDeferred> = Vec::new();

    while i < lines.len() {
        let trimmed = lines[i].trim();
        let clean = trimmed
            .trim_start_matches("public ")
            .trim_start_matches("private ")
            .trim_start_matches("protected ")
            .trim_start_matches("abstract ")
            .trim_start_matches("final ")
            .trim_start_matches("static ")
            .trim();

        if clean.starts_with("record ") {
            let rest = &clean["record ".len()..];
            let name = rest
                .split(|c: char| c == '(' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .trim();
            if !name.is_empty() {
                builder = builder.vertex(name, "record", None)?;
                vertex_ids.insert(name.to_owned());
                if let Some(paren_start) = rest.find('(') {
                    if let Some(paren_end) = rest.find(')') {
                        let params = &rest[paren_start + 1..paren_end];
                        for param in params.split(',') {
                            let param = param.trim();
                            if param.is_empty() {
                                continue;
                            }
                            let parts: Vec<&str> = param.rsplitn(2, ' ').collect();
                            if parts.len() == 2 {
                                let field_name = parts[0].trim();
                                let type_name = parts[1].trim();
                                let field_id = format!("{name}.{field_name}");
                                builder = builder.vertex(&field_id, "field", None)?;
                                vertex_ids.insert(field_id.clone());
                                builder =
                                    builder.edge(name, &field_id, "field-of", Some(field_name))?;
                                let base = extract_java_base_type(type_name);
                                if !base.is_empty() {
                                    deferred.push(JavaFieldDeferred {
                                        field_id,
                                        type_name: base.to_owned(),
                                    });
                                }
                            }
                        }
                    }
                }
                if trimmed.contains('{') {
                    let mut depth =
                        trimmed.matches('{').count() as i32 - trimmed.matches('}').count() as i32;
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
        } else if clean.starts_with("class ") {
            let rest = &clean["class ".len()..];
            let name = rest
                .split(|c: char| c == '{' || c == '<' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .trim();
            if !name.is_empty() {
                builder = builder.vertex(name, "class", None)?;
                vertex_ids.insert(name.to_owned());
                if let Some(idx) = rest.find("implements") {
                    let after = &rest[idx + "implements".len()..];
                    let before_brace = after.split('{').next().unwrap_or("");
                    for iface in before_brace.split(',') {
                        let iface = iface.trim();
                        if !iface.is_empty() {
                            deferred_impls.push(ImplDeferred {
                                type_name: name.to_owned(),
                                iface: iface.to_owned(),
                            });
                        }
                    }
                }
                i += 1;
                let (b, new_i, new_deferred) =
                    parse_java_class_fields(builder, &lines, i, name, &mut vertex_ids)?;
                builder = b;
                deferred.extend(new_deferred);
                i = new_i;
                continue;
            }
            i += 1;
        } else if clean.starts_with("interface ") {
            let rest = &clean["interface ".len()..];
            let name = rest
                .split(|c: char| c == '{' || c == '<' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .trim();
            if !name.is_empty() {
                builder = builder.vertex(name, "interface", None)?;
                vertex_ids.insert(name.to_owned());
                if trimmed.contains('{') {
                    let mut depth =
                        trimmed.matches('{').count() as i32 - trimmed.matches('}').count() as i32;
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
        } else if clean.starts_with("enum ") {
            let rest = &clean["enum ".len()..];
            let name = rest
                .split(|c: char| c == '{' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .trim();
            if !name.is_empty() {
                builder = builder.vertex(name, "enum", None)?;
                vertex_ids.insert(name.to_owned());
                i += 1;
                let (b, new_i) =
                    parse_java_enum_constants(builder, &lines, i, name, &mut vertex_ids)?;
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
    for d in deferred_impls {
        if vertex_ids.contains(&d.iface) {
            builder = builder.edge(&d.type_name, &d.iface, "implements", None)?;
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

struct JavaFieldDeferred {
    field_id: String,
    type_name: String,
}

fn parse_java_class_fields(
    mut builder: SchemaBuilder,
    lines: &[&str],
    start: usize,
    parent: &str,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<(SchemaBuilder, usize, Vec<JavaFieldDeferred>), ProtocolError> {
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
        if line.is_empty() || line.starts_with("//") || line.starts_with('@') {
            i += 1;
            continue;
        }
        if line.ends_with(';') && !line.contains('(') {
            let clean = line
                .trim_start_matches("public ")
                .trim_start_matches("private ")
                .trim_start_matches("protected ")
                .trim_start_matches("final ")
                .trim_start_matches("static ")
                .trim_start_matches("transient ")
                .trim_end_matches(';')
                .trim();
            let parts: Vec<&str> = clean
                .split('=')
                .next()
                .unwrap_or(clean)
                .trim()
                .rsplitn(2, ' ')
                .collect();
            if parts.len() == 2 {
                let field_name = parts[0].trim();
                let type_name = parts[1].trim();
                if !field_name.is_empty() && !type_name.is_empty() {
                    let field_id = format!("{parent}.{field_name}");
                    builder = builder.vertex(&field_id, "field", None)?;
                    vertex_ids.insert(field_id.clone());
                    builder = builder.edge(parent, &field_id, "field-of", Some(field_name))?;
                    let base = extract_java_base_type(type_name);
                    if !base.is_empty() {
                        deferred.push(JavaFieldDeferred {
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

fn parse_java_enum_constants(
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
        for part in line.split([',', ';']) {
            let name = part.split('(').next().unwrap_or("").trim();
            if !name.is_empty() && name != "}" {
                let const_id = format!("{parent}.{name}");
                if !vertex_ids.contains(&const_id) {
                    builder = builder.vertex(&const_id, "enum-constant", None)?;
                    vertex_ids.insert(const_id.clone());
                    builder = builder.edge(parent, &const_id, "field-of", Some(name))?;
                }
            }
        }
        i += 1;
    }
    Ok((builder, i))
}

fn extract_java_base_type(type_expr: &str) -> &str {
    let s = type_expr.trim();
    let s = s.split('<').next().unwrap_or(s);
    s.split('[').next().unwrap_or(s).trim()
}

/// Emit a [`Schema`] as Java type definitions.
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema cannot be serialized.
pub fn emit_java_types(schema: &Schema) -> Result<String, ProtocolError> {
    let structural = &["field-of", "type-of", "implements"];
    let roots = find_roots(schema, structural);
    let mut w = IndentWriter::new("    ");

    for root in &roots {
        match root.kind.as_str() {
            "record" => emit_java_record(schema, root, &mut w)?,
            "class" => emit_java_class(schema, root, &mut w)?,
            "enum" => emit_java_enum(schema, root, &mut w)?,
            "interface" => {
                w.line(&format!("public interface {} {{}}", root.id));
                w.blank();
            }
            _ => {}
        }
    }
    Ok(w.finish())
}

fn emit_java_record(
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
                .unwrap_or_else(|| "Object".to_string());
            format!("{type_name} {field_name}")
        })
        .collect();
    w.line(&format!(
        "public record {}({}) {{}}",
        vertex.id,
        params.join(", ")
    ));
    w.blank();
    Ok(())
}

fn emit_java_class(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
    w: &mut IndentWriter,
) -> Result<(), ProtocolError> {
    let impls: Vec<String> = schema
        .outgoing_edges(&vertex.id)
        .iter()
        .filter(|e| e.kind == "implements")
        .map(|e| e.tgt.clone())
        .collect();
    let impl_str = if impls.is_empty() {
        String::new()
    } else {
        format!(" implements {}", impls.join(", "))
    };
    w.line(&format!("public class {}{impl_str} {{", vertex.id));
    w.indent();
    let fields = children_by_edge(schema, &vertex.id, "field-of");
    for (edge, field_vertex) in &fields {
        let field_name = edge.name.as_deref().unwrap_or(&field_vertex.id);
        let type_name = resolve_type(schema, &field_vertex.id)
            .map(|v| v.id.clone())
            .unwrap_or_else(|| "Object".to_string());
        w.line(&format!("private {type_name} {field_name};"));
    }
    w.dedent();
    w.line("}");
    w.blank();
    Ok(())
}

fn emit_java_enum(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
    w: &mut IndentWriter,
) -> Result<(), ProtocolError> {
    w.line(&format!("public enum {} {{", vertex.id));
    w.indent();
    let constants = children_by_edge(schema, &vertex.id, "field-of");
    let names: Vec<&str> = constants
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
            src_kinds: vec!["class".into(), "record".into(), "enum".into()],
            tgt_kinds: vec!["field".into(), "enum-constant".into()],
        },
        EdgeRule {
            edge_kind: "type-of".into(),
            src_kinds: vec!["field".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "implements".into(),
            src_kinds: vec!["class".into(), "record".into()],
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
        assert_eq!(p.name, "java");
        assert_eq!(p.schema_theory, "ThJavaSchema");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThJavaSchema"));
        assert!(registry.contains_key("ThJavaInstance"));
    }

    #[test]
    fn parse_minimal() {
        let input = r"
public record User(String name, int age) {}
";
        let schema = parse_java_types(input).expect("should parse");
        assert!(schema.has_vertex("User"));
        assert!(schema.has_vertex("User.name"));
        assert!(schema.has_vertex("User.age"));
    }

    #[test]
    fn roundtrip() {
        let input = r"
public record User(String name, int age) {}

public enum Color {
    RED, GREEN, BLUE
}
";
        let schema = parse_java_types(input).expect("should parse");
        let output = emit_java_types(&schema).expect("should emit");
        assert!(output.contains("public record User"));
        assert!(output.contains("public enum Color"));
    }
}
