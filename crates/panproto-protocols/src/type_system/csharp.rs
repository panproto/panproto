//! C# record/class type system protocol definition.
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

/// Returns the C# protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "csharp".into(),
        schema_theory: "ThCSharpSchema".into(),
        instance_theory: "ThCSharpInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "record".into(),
            "class".into(),
            "field".into(),
            "enum".into(),
            "enum-member".into(),
            "interface".into(),
            "property".into(),
            "string".into(),
            "int".into(),
            "long".into(),
            "float".into(),
            "double".into(),
            "decimal".into(),
            "bool".into(),
            "byte".into(),
            "char".into(),
            "datetime".into(),
            "list".into(),
            "dictionary".into(),
            "hashset".into(),
        ],
        constraint_sorts: vec!["optional".into(), "required".into(), "default".into()],
        has_order: true,
        has_coproducts: true,
        has_recursion: true,
        nominal_identity: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for C# with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_typed_graph_wtype(registry, "ThCSharpSchema", "ThCSharpInstance");
}

/// Parse C# type definitions into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing or schema construction fails.
#[allow(clippy::too_many_lines)]
pub fn parse_csharp_types(input: &str) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;
    let mut builder = SchemaBuilder::new(&proto);
    let mut vertex_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut deferred: Vec<CsFieldDeferred> = Vec::new();
    let mut deferred_impls: Vec<CsImplDeferred> = Vec::new();

    while i < lines.len() {
        let trimmed = lines[i].trim();
        let clean = trimmed
            .trim_start_matches("public ")
            .trim_start_matches("internal ")
            .trim_start_matches("private ")
            .trim_start_matches("protected ")
            .trim_start_matches("sealed ")
            .trim_start_matches("abstract ")
            .trim_start_matches("partial ")
            .trim();

        if clean.starts_with("record ") {
            let rest = &clean["record ".len()..];
            let name = rest
                .split(|c: char| c == '(' || c == '{' || c == ':' || c == ';' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .trim();
            if !name.is_empty() {
                builder = builder.vertex(name, "record", None)?;
                vertex_ids.insert(name.to_owned());

                // Parse positional record: record User(string Name, int Age);
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
                                builder = builder.vertex(&field_id, "property", None)?;
                                vertex_ids.insert(field_id.clone());
                                builder =
                                    builder.edge(name, &field_id, "field-of", Some(field_name))?;
                                let base = extract_cs_base_type(type_name);
                                if !base.is_empty() {
                                    deferred.push(CsFieldDeferred {
                                        field_id,
                                        type_name: base.to_owned(),
                                    });
                                }
                            }
                        }
                    }
                }
                // Skip brace-enclosed body if present.
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
        } else if clean.starts_with("class ") {
            let rest = &clean["class ".len()..];
            let name = rest
                .split(|c: char| c == '{' || c == ':' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .trim();
            if !name.is_empty() {
                builder = builder.vertex(name, "class", None)?;
                vertex_ids.insert(name.to_owned());

                // Check for : interface conformances.
                if let Some(colon_idx) = rest.find(':') {
                    let after = rest[colon_idx + 1..].split('{').next().unwrap_or("");
                    for part in after.split(',') {
                        let iface = part.trim();
                        if !iface.is_empty() {
                            deferred_impls.push(CsImplDeferred {
                                type_name: name.to_owned(),
                                iface: iface.to_owned(),
                            });
                        }
                    }
                }
                i += 1;
                let (b, new_i, new_deferred) =
                    parse_cs_class_members(builder, &lines, i, name, &mut vertex_ids)?;
                builder = b;
                deferred.extend(new_deferred);
                i = new_i;
                continue;
            }
            i += 1;
        } else if clean.starts_with("interface ") {
            let rest = &clean["interface ".len()..];
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
        } else if clean.starts_with("enum ") {
            let rest = &clean["enum ".len()..];
            let name = rest
                .split(|c: char| c == '{' || c == ':' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .trim();
            if !name.is_empty() {
                builder = builder.vertex(name, "enum", None)?;
                vertex_ids.insert(name.to_owned());
                i += 1;
                let (b, new_i) = parse_cs_enum_members(builder, &lines, i, name, &mut vertex_ids)?;
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

struct CsFieldDeferred {
    field_id: String,
    type_name: String,
}

struct CsImplDeferred {
    type_name: String,
    iface: String,
}

fn parse_cs_class_members(
    mut builder: SchemaBuilder,
    lines: &[&str],
    start: usize,
    parent: &str,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<(SchemaBuilder, usize, Vec<CsFieldDeferred>), ProtocolError> {
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
        if line.is_empty() || line.starts_with("//") || line.starts_with('[') {
            i += 1;
            continue;
        }

        // Detect property: public Type Name { get; set; }
        if line.contains("get;") || line.contains("get ;") {
            let clean = line
                .trim_start_matches("public ")
                .trim_start_matches("private ")
                .trim_start_matches("protected ")
                .trim_start_matches("required ")
                .trim();
            // Type Name { get; set; }
            let before_brace = clean.split('{').next().unwrap_or("").trim();
            let parts: Vec<&str> = before_brace.rsplitn(2, ' ').collect();
            if parts.len() == 2 {
                let prop_name = parts[0].trim();
                let type_name = parts[1].trim();
                if !prop_name.is_empty() && !type_name.is_empty() {
                    let field_id = format!("{parent}.{prop_name}");
                    builder = builder.vertex(&field_id, "property", None)?;
                    vertex_ids.insert(field_id.clone());
                    builder = builder.edge(parent, &field_id, "field-of", Some(prop_name))?;

                    if type_name.ends_with('?') {
                        builder = builder.constraint(&field_id, "optional", "true");
                    }

                    let base = extract_cs_base_type(type_name);
                    if !base.is_empty() {
                        deferred.push(CsFieldDeferred {
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

fn parse_cs_enum_members(
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
        for part in line.split(',') {
            let name = part.split('=').next().unwrap_or("").trim();
            if !name.is_empty() && name != "}" {
                let member_id = format!("{parent}.{name}");
                if !vertex_ids.contains(&member_id) {
                    builder = builder.vertex(&member_id, "enum-member", None)?;
                    vertex_ids.insert(member_id.clone());
                    builder = builder.edge(parent, &member_id, "member-of", Some(name))?;
                }
            }
        }
        i += 1;
    }
    Ok((builder, i))
}

fn extract_cs_base_type(type_expr: &str) -> &str {
    let s = type_expr.trim().trim_end_matches('?');
    s.split('<').next().unwrap_or(s).trim()
}

/// Emit a [`Schema`] as C# type definitions.
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema cannot be serialized.
pub fn emit_csharp_types(schema: &Schema) -> Result<String, ProtocolError> {
    let structural = &["field-of", "member-of", "type-of", "implements"];
    let roots = find_roots(schema, structural);
    let mut w = IndentWriter::new("    ");

    for root in &roots {
        match root.kind.as_str() {
            "record" => emit_cs_record(schema, root, &mut w)?,
            "class" => emit_cs_class(schema, root, &mut w)?,
            "enum" => emit_cs_enum(schema, root, &mut w)?,
            "interface" => {
                w.line(&format!("public interface {} {{}}", root.id));
                w.blank();
            }
            _ => {}
        }
    }
    Ok(w.finish())
}

fn emit_cs_record(
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
                .unwrap_or_else(|| "object".to_string());
            format!("{type_name} {field_name}")
        })
        .collect();
    w.line(&format!(
        "public record {}({});",
        vertex.id,
        params.join(", ")
    ));
    w.blank();
    Ok(())
}

fn emit_cs_class(
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
        format!(" : {}", impls.join(", "))
    };
    w.line(&format!("public class {}{impl_str} {{", vertex.id));
    w.indent();
    let fields = children_by_edge(schema, &vertex.id, "field-of");
    for (edge, field_vertex) in &fields {
        let field_name = edge.name.as_deref().unwrap_or(&field_vertex.id);
        let type_name = resolve_type(schema, &field_vertex.id)
            .map(|v| v.id.clone())
            .unwrap_or_else(|| "object".to_string());
        let is_optional =
            constraint_value(schema, &field_vertex.id, "optional").is_some_and(|v| v == "true");
        let ty = if is_optional {
            format!("{type_name}?")
        } else {
            type_name
        };
        w.line(&format!("public {ty} {field_name} {{ get; set; }}"));
    }
    w.dedent();
    w.line("}");
    w.blank();
    Ok(())
}

fn emit_cs_enum(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
    w: &mut IndentWriter,
) -> Result<(), ProtocolError> {
    w.line(&format!("public enum {} {{", vertex.id));
    w.indent();
    let members = children_by_edge(schema, &vertex.id, "member-of");
    let names: Vec<&str> = members
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
            src_kinds: vec!["class".into(), "record".into()],
            tgt_kinds: vec!["field".into(), "property".into()],
        },
        EdgeRule {
            edge_kind: "type-of".into(),
            src_kinds: vec!["field".into(), "property".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "implements".into(),
            src_kinds: vec!["class".into(), "record".into()],
            tgt_kinds: vec!["interface".into()],
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
        assert_eq!(p.name, "csharp");
        assert_eq!(p.schema_theory, "ThCSharpSchema");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThCSharpSchema"));
        assert!(registry.contains_key("ThCSharpInstance"));
    }

    #[test]
    fn parse_minimal() {
        let input = r"
public record User(string Name, int Age);
";
        let schema = parse_csharp_types(input).expect("should parse");
        assert!(schema.has_vertex("User"));
        assert!(schema.has_vertex("User.Name"));
        assert!(schema.has_vertex("User.Age"));
    }

    #[test]
    fn roundtrip() {
        let input = r"
public record User(string Name, int Age);

public enum Color {
    Red, Green, Blue
}
";
        let schema = parse_csharp_types(input).expect("should parse");
        let output = emit_csharp_types(&schema).expect("should emit");
        assert!(output.contains("public record User"));
        assert!(output.contains("public enum Color"));
    }
}
