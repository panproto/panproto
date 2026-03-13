//! `FlatBuffers` protocol definition.
//!
//! `FlatBuffers` uses a simple constrained graph schema theory
//! (`colimit(ThSimpleGraph, ThConstraint)` + `ThFlat`).
//!
//! Vertex kinds: table, struct, field, enum, enum-value, union, rpc-service,
//!               rpc-method, string, bool, int8, int16, int32, int64, uint8,
//!               uint16, uint32, uint64, float32, float64.
//! Edge kinds: field-of, type-of, variant-of.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{IndentWriter, children_by_edge, constraint_value, find_roots};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the `FlatBuffers` protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "flatbuffers".into(),
        schema_theory: "ThFlatBuffersSchema".into(),
        instance_theory: "ThFlatBuffersInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "table".into(),
            "struct".into(),
            "field".into(),
            "enum".into(),
            "enum-value".into(),
            "union".into(),
            "rpc-service".into(),
            "rpc-method".into(),
            "string".into(),
            "bool".into(),
            "int8".into(),
            "int16".into(),
            "int32".into(),
            "int64".into(),
            "uint8".into(),
            "uint16".into(),
            "uint32".into(),
            "uint64".into(),
            "float32".into(),
            "float64".into(),
        ],
        constraint_sorts: vec!["field_id".into(), "default".into(), "deprecated".into()],
    }
}

/// Register the component GATs for `FlatBuffers` with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_simple_graph_flat(registry, "ThFlatBuffersSchema", "ThFlatBuffersInstance");
}

/// Intermediate representation of a parsed field for two-pass resolution.
struct FieldInfo {
    field_id: String,
    type_name: String,
}

/// Parse a `.fbs` file into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if the `FlatBuffers` schema cannot be parsed.
pub fn parse_fbs(input: &str) -> Result<Schema, ProtocolError> {
    let proto_def = protocol();
    let mut builder = SchemaBuilder::new(&proto_def);
    let mut vertex_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut field_infos: Vec<FieldInfo> = Vec::new();

    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        if trimmed.starts_with("namespace")
            || trimmed.starts_with("include")
            || trimmed.starts_with("attribute")
            || trimmed.starts_with("file_identifier")
            || trimmed.starts_with("file_extension")
            || trimmed.starts_with("root_type")
            || trimmed.starts_with("//")
            || trimmed.is_empty()
        {
            i += 1;
        } else if trimmed.starts_with("table ") {
            let (new_i, new_fields) =
                parse_table_or_struct(&mut builder, &lines, i, "table", &mut vertex_ids)?;
            field_infos.extend(new_fields);
            i = new_i;
        } else if trimmed.starts_with("struct ") {
            let (new_i, new_fields) =
                parse_table_or_struct(&mut builder, &lines, i, "struct", &mut vertex_ids)?;
            field_infos.extend(new_fields);
            i = new_i;
        } else if trimmed.starts_with("enum ") {
            i = parse_enum(&mut builder, &lines, i, &mut vertex_ids)?;
        } else if trimmed.starts_with("union ") {
            i = parse_union(&mut builder, &lines, i, &mut vertex_ids)?;
        } else if trimmed.starts_with("rpc_service ") {
            let (new_i, new_fields) = parse_rpc_service(&mut builder, &lines, i, &mut vertex_ids)?;
            field_infos.extend(new_fields);
            i = new_i;
        } else {
            i += 1;
        }
    }

    // Pass 2: Resolve type-of edges.
    for info in &field_infos {
        if vertex_ids.contains(&info.type_name) {
            builder = builder.edge(&info.field_id, &info.type_name, "type-of", None)?;
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Parse a table or struct declaration.
fn parse_table_or_struct(
    builder: &mut SchemaBuilder,
    lines: &[&str],
    start: usize,
    kind: &str,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<(usize, Vec<FieldInfo>), ProtocolError> {
    let trimmed = lines[start].trim();
    let keyword = if kind == "table" { "table " } else { "struct " };
    let name = trimmed
        .strip_prefix(keyword)
        .and_then(|s| s.split('{').next())
        .map(str::trim)
        .ok_or_else(|| ProtocolError::Parse(format!("invalid {kind} declaration")))?;

    let taken = std::mem::replace(builder, SchemaBuilder::new(&protocol()));
    let mut b = taken.vertex(name, kind, None)?;
    vertex_ids.insert(name.to_string());

    let mut field_infos = Vec::new();
    let mut field_idx = 0u32;
    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i].trim();
        if line == "}" || line.starts_with('}') {
            *builder = b;
            return Ok((i + 1, field_infos));
        }

        if !line.is_empty() && !line.starts_with("//") {
            // Parse field: name:Type [= default];
            let clean = line.trim_end_matches(';').trim();
            if let Some((field_name, rest)) = clean.split_once(':') {
                let field_name = field_name.trim();
                let (type_name, default) = if let Some((t, d)) = rest.split_once('=') {
                    (t.trim(), Some(d.trim()))
                } else {
                    (rest.trim(), None)
                };

                let field_id = format!("{name}.{field_name}");
                b = b.vertex(&field_id, "field", None)?;
                vertex_ids.insert(field_id.clone());
                b = b.edge(name, &field_id, "field-of", Some(field_name))?;

                b = b.constraint(&field_id, "field_id", &field_idx.to_string());
                field_idx += 1;

                if let Some(d) = default {
                    b = b.constraint(&field_id, "default", d);
                }

                // Check for deprecated attribute.
                if clean.contains("deprecated") {
                    b = b.constraint(&field_id, "deprecated", "true");
                }

                let fbs_type = fbs_type_to_kind(type_name);
                field_infos.push(FieldInfo {
                    field_id,
                    type_name: fbs_type.to_string(),
                });
            }
        }

        i += 1;
    }

    *builder = b;
    Ok((i, field_infos))
}

/// Parse an enum declaration.
fn parse_enum(
    builder: &mut SchemaBuilder,
    lines: &[&str],
    start: usize,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<usize, ProtocolError> {
    let trimmed = lines[start].trim();
    // enum Name : Type { ... }
    let after_enum = trimmed
        .strip_prefix("enum ")
        .ok_or_else(|| ProtocolError::Parse("invalid enum declaration".into()))?;
    let name = after_enum
        .split([':', '{'])
        .next()
        .map(str::trim)
        .ok_or_else(|| ProtocolError::Parse("invalid enum declaration".into()))?;

    let taken = std::mem::replace(builder, SchemaBuilder::new(&protocol()));
    let mut b = taken.vertex(name, "enum", None)?;
    vertex_ids.insert(name.to_string());

    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i].trim();
        if line == "}" || line.starts_with('}') {
            *builder = b;
            return Ok(i + 1);
        }

        if !line.is_empty() && !line.starts_with("//") {
            let clean = line.trim_end_matches(',').trim();
            let val_name = if let Some((vn, _)) = clean.split_once('=') {
                vn.trim()
            } else {
                clean
            };

            if !val_name.is_empty() {
                let val_id = format!("{name}.{val_name}");
                b = b.vertex(&val_id, "enum-value", None)?;
                vertex_ids.insert(val_id.clone());
                b = b.edge(name, &val_id, "variant-of", Some(val_name))?;
            }
        }

        i += 1;
    }

    *builder = b;
    Ok(i)
}

/// Parse a union declaration.
fn parse_union(
    builder: &mut SchemaBuilder,
    lines: &[&str],
    start: usize,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<usize, ProtocolError> {
    let trimmed = lines[start].trim();
    let name = trimmed
        .strip_prefix("union ")
        .and_then(|s| s.split('{').next())
        .map(str::trim)
        .ok_or_else(|| ProtocolError::Parse("invalid union declaration".into()))?;

    let taken = std::mem::replace(builder, SchemaBuilder::new(&protocol()));
    let b = taken.vertex(name, "union", None)?;
    vertex_ids.insert(name.to_string());

    *builder = b;

    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i].trim();
        if line == "}" || line.starts_with('}') {
            return Ok(i + 1);
        }
        i += 1;
    }

    Ok(i)
}

/// Parse an `rpc_service` declaration.
fn parse_rpc_service(
    builder: &mut SchemaBuilder,
    lines: &[&str],
    start: usize,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<(usize, Vec<FieldInfo>), ProtocolError> {
    let trimmed = lines[start].trim();
    let name = trimmed
        .strip_prefix("rpc_service ")
        .and_then(|s| s.split('{').next())
        .map(str::trim)
        .ok_or_else(|| ProtocolError::Parse("invalid rpc_service declaration".into()))?;

    let taken = std::mem::replace(builder, SchemaBuilder::new(&protocol()));
    let mut b = taken.vertex(name, "rpc-service", None)?;
    vertex_ids.insert(name.to_string());

    let field_infos = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i].trim();
        if line == "}" || line.starts_with('}') {
            *builder = b;
            return Ok((i + 1, field_infos));
        }

        // Parse: MethodName(RequestType):ResponseType;
        if !line.is_empty() && !line.starts_with("//") {
            let clean = line.trim_end_matches(';').trim();
            if let Some(paren_pos) = clean.find('(') {
                let method_name = clean[..paren_pos].trim();
                let method_id = format!("{name}.{method_name}");
                b = b.vertex(&method_id, "rpc-method", None)?;
                vertex_ids.insert(method_id.clone());
                b = b.edge(name, &method_id, "field-of", Some(method_name))?;
            }
        }

        i += 1;
    }

    *builder = b;
    Ok((i, field_infos))
}

/// Map `FlatBuffers` type name to vertex kind.
fn fbs_type_to_kind(type_name: &str) -> &str {
    match type_name {
        "string" => "string",
        "bool" => "bool",
        "byte" | "int8" => "int8",
        "short" | "int16" => "int16",
        "int" | "int32" => "int32",
        "long" | "int64" => "int64",
        "ubyte" | "uint8" => "uint8",
        "ushort" | "uint16" => "uint16",
        "uint" | "uint32" => "uint32",
        "ulong" | "uint64" => "uint64",
        "float" | "float32" => "float32",
        "double" | "float64" => "float64",
        other => other,
    }
}

/// Emit a `.fbs` file from a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if the schema cannot be serialized.
pub fn emit_fbs(schema: &Schema) -> Result<String, ProtocolError> {
    let mut w = IndentWriter::new("  ");
    let roots = find_roots(schema, &["field-of", "variant-of", "type-of"]);

    for root in &roots {
        match root.kind.as_str() {
            "table" | "struct" => {
                w.line(&format!("{} {} {{", root.kind, root.id));
                w.indent();

                let fields = children_by_edge(schema, &root.id, "field-of");
                for (edge, field_vertex) in &fields {
                    let name = edge.name.as_deref().unwrap_or(&field_vertex.id);

                    let type_children = children_by_edge(schema, &field_vertex.id, "type-of");
                    let type_name = type_children
                        .first()
                        .map_or("string", |(_, tv)| tv.kind.as_str());

                    let default_part = constraint_value(schema, &field_vertex.id, "default")
                        .map_or_else(String::new, |d| format!(" = {d}"));

                    w.line(&format!("{name}:{type_name}{default_part};"));
                }

                w.dedent();
                w.line("}");
                w.blank();
            }
            "enum" => {
                w.line(&format!("enum {} : int {{", root.id));
                w.indent();

                let variants = children_by_edge(schema, &root.id, "variant-of");
                for (edge, _) in &variants {
                    let name = edge.name.as_deref().unwrap_or("UNKNOWN");
                    w.line(&format!("{name},"));
                }

                w.dedent();
                w.line("}");
                w.blank();
            }
            _ => {}
        }
    }

    Ok(w.finish())
}

/// Well-formedness rules for `FlatBuffers` edges.
fn edge_rules() -> Vec<EdgeRule> {
    let all_types: Vec<String> = vec![
        "field", "table", "struct", "enum", "union", "string", "bool", "int8", "int16", "int32",
        "int64", "uint8", "uint16", "uint32", "uint64", "float32", "float64",
    ]
    .into_iter()
    .map(Into::into)
    .collect();

    vec![
        EdgeRule {
            edge_kind: "field-of".into(),
            src_kinds: vec!["table".into(), "struct".into(), "rpc-service".into()],
            tgt_kinds: vec!["field".into(), "rpc-method".into()],
        },
        EdgeRule {
            edge_kind: "type-of".into(),
            src_kinds: vec!["field".into(), "rpc-method".into()],
            tgt_kinds: all_types,
        },
        EdgeRule {
            edge_kind: "variant-of".into(),
            src_kinds: vec!["enum".into()],
            tgt_kinds: vec!["enum-value".into()],
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
        assert_eq!(p.name, "flatbuffers");
        assert_eq!(p.schema_theory, "ThFlatBuffersSchema");
        assert_eq!(p.instance_theory, "ThFlatBuffersInstance");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThFlatBuffersSchema"));
        assert!(registry.contains_key("ThFlatBuffersInstance"));
    }

    #[test]
    fn parse_minimal() {
        let input = r"
table Person {
  name:string;
  age:int32;
}
";
        let schema = parse_fbs(input).expect("should parse");
        assert!(schema.has_vertex("Person"));
        assert!(schema.has_vertex("Person.name"));
        assert!(schema.has_vertex("Person.age"));
    }

    #[test]
    fn emit_minimal() {
        let input = r"
table Person {
  name:string;
  age:int32;
}
";
        let schema = parse_fbs(input).expect("should parse");
        let emitted = emit_fbs(&schema).expect("should emit");
        assert!(emitted.contains("table Person"));
        assert!(emitted.contains("name"));
        assert!(emitted.contains("age"));
    }

    #[test]
    fn roundtrip() {
        let input = r"
table Event {
  id:string;
  count:int32;
}
";
        let schema1 = parse_fbs(input).expect("parse 1");
        let emitted = emit_fbs(&schema1).expect("emit");
        let schema2 = parse_fbs(&emitted).expect("parse 2");

        assert_eq!(schema1.vertex_count(), schema2.vertex_count());
        assert!(schema2.has_vertex("Event"));
        assert!(schema2.has_vertex("Event.id"));
        assert!(schema2.has_vertex("Event.count"));
    }
}
