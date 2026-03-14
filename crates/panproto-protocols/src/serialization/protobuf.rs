//! Protobuf protocol definition.
//!
//! Protobuf uses a simple constrained graph schema theory
//! (`ThSimpleGraph + ThConstraint`) and a flat instance theory (`ThFlat`).
//!
//! Vertex kinds: message, field, enum, enum-value, oneof, service, rpc, map.
//! Edge kinds: field-of, type-of, variant-of, input-of, output-of.

use std::collections::HashMap;

use panproto_gat::{Sort, Theory, colimit};
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::error::ProtocolError;
use crate::theories;

/// Returns the Protobuf protocol definition.
///
/// Schema theory: `ThSimpleGraph + ThConstraint`.
/// Instance theory: `ThFlat`.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "protobuf".into(),
        schema_theory: "ThProtobufSchema".into(),
        instance_theory: "ThProtobufInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "message".into(),
            "field".into(),
            "enum".into(),
            "enum-value".into(),
            "oneof".into(),
            "service".into(),
            "rpc".into(),
            "map".into(),
            "string".into(),
            "integer".into(),
            "float".into(),
            "boolean".into(),
        ],
        constraint_sorts: vec!["field_number".into(), "label".into(), "packed".into()],
        has_order: true,
        has_coproducts: true,
        has_recursion: true,
        nominal_identity: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for Protobuf with a theory registry.
pub fn register_theories<S: ::std::hash::BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    let th_simple_graph = theories::th_simple_graph();
    let th_constraint = theories::th_constraint();
    let th_flat = theories::th_flat();

    registry.insert("ThSimpleGraph".into(), th_simple_graph.clone());
    registry.insert("ThFlat".into(), th_flat.clone());

    // Schema theory: colimit(ThSimpleGraph, ThConstraint) over shared Vertex.
    let shared_vertex = Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![]);

    if let Ok(mut schema_theory) = colimit(&th_simple_graph, &th_constraint, &shared_vertex) {
        schema_theory.name = "ThProtobufSchema".into();
        registry.insert("ThProtobufSchema".into(), schema_theory);
    }

    // Instance theory is ThFlat.
    let mut inst = th_flat;
    inst.name = "ThProtobufInstance".into();
    registry.insert("ThProtobufInstance".into(), inst);
}

/// Intermediate representation of a parsed field, used for two-pass resolution.
struct FieldInfo {
    field_id: String,
    type_name: String,
}

/// Parse a `.proto` file into a [`Schema`].
///
/// Uses a two-pass approach:
/// - **Pass 1**: Parse all messages, enums, and services, collecting vertex IDs.
/// - **Pass 2**: For each field that references another message/enum type, add a "type-of" edge.
///
/// # Errors
///
/// Returns [`ProtocolError`] if the proto file cannot be parsed.
pub fn parse_proto(proto: &str) -> Result<Schema, ProtocolError> {
    let proto_def = protocol();
    let mut builder = SchemaBuilder::new(&proto_def);
    let mut vertex_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut field_infos: Vec<FieldInfo> = Vec::new();

    let lines: Vec<&str> = proto.lines().collect();
    let mut i = 0;

    // Pass 1: Parse all declarations.
    while i < lines.len() {
        let trimmed = lines[i].trim();

        if trimmed.starts_with("syntax") || trimmed.starts_with("package") {
            // Metadata declarations; parsed and skipped.
            i += 1;
        } else if trimmed.starts_with("import") {
            // Cross-file resolution is inherently limited; skip gracefully.
            i += 1;
        } else if trimmed.starts_with("message ") {
            let (new_builder, new_i, new_fields) =
                parse_message(builder, &lines, i, "", &mut vertex_ids)?;
            builder = new_builder;
            field_infos.extend(new_fields);
            i = new_i;
        } else if trimmed.starts_with("enum ") {
            let (new_builder, new_i) = parse_enum(builder, &lines, i, "", &mut vertex_ids)?;
            builder = new_builder;
            i = new_i;
        } else if trimmed.starts_with("service ") {
            let (new_builder, new_i, new_fields) =
                parse_service(builder, &lines, i, &mut vertex_ids)?;
            builder = new_builder;
            field_infos.extend(new_fields);
            i = new_i;
        } else {
            i += 1;
        }
    }

    // Pass 2: Resolve type-of edges for fields referencing other messages/enums.
    for info in &field_infos {
        if vertex_ids.contains(&info.type_name) {
            builder = builder.edge(&info.field_id, &info.type_name, "type-of", None)?;
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Parse a message declaration.
fn parse_message(
    mut builder: SchemaBuilder,
    lines: &[&str],
    start: usize,
    prefix: &str,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<(SchemaBuilder, usize, Vec<FieldInfo>), ProtocolError> {
    let trimmed = lines[start].trim();
    let name = trimmed
        .strip_prefix("message ")
        .and_then(|s| s.split('{').next())
        .map(str::trim)
        .ok_or_else(|| ProtocolError::Parse("invalid message declaration".into()))?;

    let msg_id = if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    };

    builder = builder.vertex(&msg_id, "message", None)?;
    vertex_ids.insert(msg_id.clone());

    let mut field_infos = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i].trim();

        if line == "}" || line.starts_with('}') {
            return Ok((builder, i + 1, field_infos));
        }

        if line.starts_with("message ") {
            let (new_builder, new_i, new_fields) =
                parse_message(builder, lines, i, &msg_id, vertex_ids)?;
            builder = new_builder;
            field_infos.extend(new_fields);
            i = new_i;
            continue;
        }

        if line.starts_with("enum ") {
            let (new_builder, new_i) = parse_enum(builder, lines, i, &msg_id, vertex_ids)?;
            builder = new_builder;
            i = new_i;
            continue;
        }

        if line.starts_with("oneof ") {
            let (new_builder, new_i, new_fields) =
                parse_oneof(builder, lines, i, &msg_id, vertex_ids)?;
            builder = new_builder;
            field_infos.extend(new_fields);
            i = new_i;
            continue;
        }

        // Parse field: [label] type name = number;
        if !line.is_empty() && !line.starts_with("//") && !line.starts_with("option ") {
            let (new_builder, new_field) = parse_field(builder, line, &msg_id, vertex_ids)?;
            builder = new_builder;
            if let Some(info) = new_field {
                field_infos.push(info);
            }
        }

        i += 1;
    }

    Ok((builder, i, field_infos))
}

/// Parse an enum declaration.
fn parse_enum(
    mut builder: SchemaBuilder,
    lines: &[&str],
    start: usize,
    prefix: &str,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<(SchemaBuilder, usize), ProtocolError> {
    let trimmed = lines[start].trim();
    let name = trimmed
        .strip_prefix("enum ")
        .and_then(|s| s.split('{').next())
        .map(str::trim)
        .ok_or_else(|| ProtocolError::Parse("invalid enum declaration".into()))?;

    let enum_id = if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    };

    builder = builder.vertex(&enum_id, "enum", None)?;
    vertex_ids.insert(enum_id.clone());

    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i].trim();
        if line == "}" || line.starts_with('}') {
            return Ok((builder, i + 1));
        }

        // Parse enum value: NAME = NUMBER;
        if !line.is_empty() && !line.starts_with("//") && !line.starts_with("option ") {
            if let Some((val_name, _)) = line.split_once('=') {
                let val_name = val_name.trim();
                let val_id = format!("{enum_id}.{val_name}");
                builder = builder.vertex(&val_id, "enum-value", None)?;
                vertex_ids.insert(val_id.clone());
                builder = builder.edge(&enum_id, &val_id, "variant-of", Some(val_name))?;
            }
        }

        i += 1;
    }

    Ok((builder, i))
}

/// Parse a oneof declaration.
fn parse_oneof(
    mut builder: SchemaBuilder,
    lines: &[&str],
    start: usize,
    msg_id: &str,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<(SchemaBuilder, usize, Vec<FieldInfo>), ProtocolError> {
    let trimmed = lines[start].trim();
    let name = trimmed
        .strip_prefix("oneof ")
        .and_then(|s| s.split('{').next())
        .map(str::trim)
        .ok_or_else(|| ProtocolError::Parse("invalid oneof declaration".into()))?;

    let oneof_id = format!("{msg_id}.{name}");
    builder = builder.vertex(&oneof_id, "oneof", None)?;
    vertex_ids.insert(oneof_id.clone());
    builder = builder.edge(msg_id, &oneof_id, "field-of", Some(name))?;

    let mut field_infos = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i].trim();
        if line == "}" || line.starts_with('}') {
            return Ok((builder, i + 1, field_infos));
        }

        if !line.is_empty() && !line.starts_with("//") {
            let (new_builder, new_field) = parse_field(builder, line, &oneof_id, vertex_ids)?;
            builder = new_builder;
            if let Some(info) = new_field {
                field_infos.push(info);
            }
        }

        i += 1;
    }

    Ok((builder, i, field_infos))
}

/// Parse a service declaration.
fn parse_service(
    mut builder: SchemaBuilder,
    lines: &[&str],
    start: usize,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<(SchemaBuilder, usize, Vec<FieldInfo>), ProtocolError> {
    let trimmed = lines[start].trim();
    let name = trimmed
        .strip_prefix("service ")
        .and_then(|s| s.split('{').next())
        .map(str::trim)
        .ok_or_else(|| ProtocolError::Parse("invalid service declaration".into()))?;

    builder = builder.vertex(name, "service", None)?;
    vertex_ids.insert(name.to_string());

    let mut field_infos = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i].trim();
        if line == "}" || line.starts_with('}') {
            return Ok((builder, i + 1, field_infos));
        }

        if line.starts_with("rpc ") {
            // rpc MethodName (InputType) returns (OutputType);
            if let Some(rpc_name) = line
                .strip_prefix("rpc ")
                .and_then(|s| s.split('(').next())
                .map(str::trim)
            {
                let rpc_id = format!("{name}.{rpc_name}");
                builder = builder.vertex(&rpc_id, "rpc", None)?;
                vertex_ids.insert(rpc_id.clone());
                builder = builder.edge(name, &rpc_id, "field-of", Some(rpc_name))?;

                // Extract input type from first parenthesized group.
                if let Some(input_start) = line.find('(') {
                    if let Some(input_end) = line[input_start..].find(')') {
                        let input_type = line[input_start + 1..input_start + input_end].trim();
                        if !input_type.is_empty() {
                            field_infos.push(FieldInfo {
                                field_id: rpc_id.clone(),
                                type_name: input_type.to_string(),
                            });
                        }
                    }
                }

                // Extract output type from "returns (...)" group.
                if let Some(returns_idx) = line.find("returns") {
                    let after_returns = &line[returns_idx + "returns".len()..];
                    if let Some(out_start) = after_returns.find('(') {
                        if let Some(out_end) = after_returns[out_start..].find(')') {
                            let output_type =
                                after_returns[out_start + 1..out_start + out_end].trim();
                            if !output_type.is_empty() {
                                field_infos.push(FieldInfo {
                                    field_id: rpc_id.clone(),
                                    type_name: output_type.to_string(),
                                });
                            }
                        }
                    }
                }
            }

            // Handle multi-line rpc blocks with braces.
            if line.contains('{') && !line.contains('}') {
                i += 1;
                while i < lines.len() {
                    let rpc_line = lines[i].trim();
                    if rpc_line == "}" || rpc_line.starts_with('}') {
                        break;
                    }
                    i += 1;
                }
            }
        }

        i += 1;
    }

    Ok((builder, i, field_infos))
}

/// Parse a single field declaration.
///
/// Returns the updated builder and optionally a `FieldInfo` for deferred type-of resolution.
fn parse_field(
    mut builder: SchemaBuilder,
    line: &str,
    parent_id: &str,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<(SchemaBuilder, Option<FieldInfo>), ProtocolError> {
    let parts: Vec<&str> = line
        .split(';')
        .next()
        .unwrap_or(line)
        .split_whitespace()
        .collect();

    if parts.len() < 3 {
        return Ok((builder, None));
    }

    // Check for map<K, V> fields.
    let joined = parts.join(" ");
    if let Some(map_start) = joined.find("map<") {
        return parse_map_field(builder, &joined, map_start, parent_id, vertex_ids);
    }

    // Determine if there is a label (required/optional/repeated).
    let (label, type_name, field_name, field_number) =
        if parts[0] == "required" || parts[0] == "optional" || parts[0] == "repeated" {
            if parts.len() < 5 {
                return Ok((builder, None));
            }
            (
                Some(parts[0]),
                parts[1],
                parts[2],
                parts.get(4).copied().unwrap_or("0"),
            )
        } else {
            (
                None,
                parts[0],
                parts[1],
                parts.get(3).copied().unwrap_or("0"),
            )
        };

    let field_id = format!("{parent_id}.{field_name}");
    builder = builder.vertex(&field_id, "field", None)?;
    vertex_ids.insert(field_id.clone());
    builder = builder.edge(parent_id, &field_id, "field-of", Some(field_name))?;

    // Add field number constraint.
    let num = field_number.trim_matches(';').trim();
    if !num.is_empty() {
        builder = builder.constraint(&field_id, "field_number", num);
    }

    // Add label constraint.
    if let Some(l) = label {
        builder = builder.constraint(&field_id, "label", l);
    }

    // Collect type reference for pass 2 resolution.
    let field_info = FieldInfo {
        field_id,
        type_name: type_name.to_string(),
    };

    Ok((builder, Some(field_info)))
}

/// Parse a `map<K, V>` field declaration.
fn parse_map_field(
    mut builder: SchemaBuilder,
    line: &str,
    map_start: usize,
    parent_id: &str,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<(SchemaBuilder, Option<FieldInfo>), ProtocolError> {
    // Extract the content between < and >.
    let after_map = &line[map_start + 4..];
    let close = after_map.find('>').unwrap_or(after_map.len());
    let inner = &after_map[..close];
    let rest = after_map[close + 1..].trim();

    // Parse key and value types.
    let kv: Vec<&str> = inner.splitn(2, ',').map(str::trim).collect();
    let key_type = kv.first().copied().unwrap_or("string");
    let val_type = kv.get(1).copied().unwrap_or("string");

    // Parse field name and number from the rest.
    let rest_parts: Vec<&str> = rest.split_whitespace().collect();
    let field_name = rest_parts.first().copied().unwrap_or("unknown");
    let field_number = rest_parts.get(2).copied().unwrap_or("0").trim_matches(';');

    let field_id = format!("{parent_id}.{field_name}");
    builder = builder.vertex(&field_id, "map", None)?;
    vertex_ids.insert(field_id.clone());
    builder = builder.edge(parent_id, &field_id, "field-of", Some(field_name))?;

    if !field_number.is_empty() {
        builder = builder.constraint(&field_id, "field_number", field_number);
    }

    // Create key type vertex.
    let key_id = format!("{field_id}:key");
    let key_kind = proto_scalar_kind(key_type);
    builder = builder.vertex(&key_id, key_kind, None)?;
    vertex_ids.insert(key_id.clone());
    builder = builder.edge(&field_id, &key_id, "type-of", Some("key"))?;

    // Create value type vertex.
    let val_id = format!("{field_id}:value");
    let val_kind = proto_scalar_kind(val_type);
    builder = builder.vertex(&val_id, val_kind, None)?;
    vertex_ids.insert(val_id.clone());
    builder = builder.edge(&field_id, &val_id, "type-of", Some("value"))?;

    Ok((builder, None))
}

/// Map a protobuf scalar type to a vertex kind.
///
/// Maps well-known protobuf scalar types to semantic vertex kinds:
/// string/bytes to `"string"`, integer types to `"integer"`, floating
/// point types to `"float"`, bool to `"boolean"`. User-defined
/// message/enum types fall back to `"field"`.
fn proto_scalar_kind(type_name: &str) -> &'static str {
    match type_name {
        "string" | "bytes" => "string",
        "int32" | "int64" | "uint32" | "uint64" | "sint32" | "sint64" | "fixed32" | "fixed64"
        | "sfixed32" | "sfixed64" => "integer",
        "float" | "double" => "float",
        "bool" => "boolean",
        _ => "field",
    }
}

/// Map a vertex kind back to a protobuf scalar type name.
fn kind_to_proto_scalar(kind: &str) -> &'static str {
    match kind {
        "integer" => "int32",
        "float" => "double",
        "boolean" => "bool",
        _ => "string",
    }
}

/// Emit a [`Schema`] as a `.proto` format string.
///
/// Reconstructs messages, enums, and services from the schema graph,
/// producing a valid proto3 file with `syntax = "proto3";` header.
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema cannot be serialized.
pub fn emit_proto(schema: &Schema) -> Result<String, ProtocolError> {
    use crate::emit::{IndentWriter, find_roots};

    let structural = &["field-of", "variant-of"];
    let roots = find_roots(schema, structural);

    let mut w = IndentWriter::new("  ");
    w.line("syntax = \"proto3\";");
    w.blank();

    for root in &roots {
        match root.kind.as_str() {
            "message" => emit_proto_message(schema, root, &mut w),
            "enum" => emit_proto_enum(schema, root, &mut w),
            "service" => emit_proto_service(schema, root, &mut w),
            _ => {}
        }
    }

    Ok(w.finish())
}

/// Emit a single message declaration.
fn emit_proto_message(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
    w: &mut crate::emit::IndentWriter,
) {
    use crate::emit::{children_by_edge, constraint_value};

    // Use the short name (last segment after dot).
    let name = vertex.id.rsplit('.').next().unwrap_or(&vertex.id);
    w.line(&format!("message {name} {{"));
    w.indent();

    let fields = children_by_edge(schema, &vertex.id, "field-of");
    for (edge, field_vertex) in &fields {
        let field_name = edge.name.as_deref().unwrap_or(&field_vertex.id);

        match field_vertex.kind.as_str() {
            "oneof" => {
                w.line(&format!("oneof {field_name} {{"));
                w.indent();
                let oneof_fields = children_by_edge(schema, &field_vertex.id, "field-of");
                for (oe, ov) in &oneof_fields {
                    let of_name = oe.name.as_deref().unwrap_or(&ov.id);
                    let type_name = resolve_proto_field_type(schema, ov);
                    let num = constraint_value(schema, &ov.id, "field_number").unwrap_or("0");
                    w.line(&format!("{type_name} {of_name} = {num};"));
                }
                w.dedent();
                w.line("}");
            }
            "map" => {
                let num = constraint_value(schema, &field_vertex.id, "field_number").unwrap_or("0");
                let type_of_edges = children_by_edge(schema, &field_vertex.id, "type-of");
                let mut key_type = "string";
                let mut val_type = "string";
                for (te, tv) in &type_of_edges {
                    let edge_name = te.name.as_deref().unwrap_or("");
                    if edge_name == "key" {
                        key_type = kind_to_proto_scalar(&tv.kind);
                    } else if edge_name == "value" {
                        val_type = kind_to_proto_scalar(&tv.kind);
                    }
                }
                w.line(&format!(
                    "map<{key_type}, {val_type}> {field_name} = {num};"
                ));
            }
            "field" => {
                let label = constraint_value(schema, &field_vertex.id, "label").unwrap_or("");
                let num = constraint_value(schema, &field_vertex.id, "field_number").unwrap_or("0");
                let type_name = resolve_proto_field_type(schema, field_vertex);
                let label_prefix = if label.is_empty() {
                    String::new()
                } else {
                    format!("{label} ")
                };
                w.line(&format!("{label_prefix}{type_name} {field_name} = {num};"));
            }
            _ => {
                // Nested message or enum handled as child vertex.
                let num = constraint_value(schema, &field_vertex.id, "field_number").unwrap_or("0");
                let type_name = resolve_proto_field_type(schema, field_vertex);
                w.line(&format!("{type_name} {field_name} = {num};"));
            }
        }
    }

    // Check for nested messages/enums.
    for (_, child) in children_by_edge(schema, &vertex.id, "field-of") {
        if child.kind == "message" {
            emit_proto_message(schema, child, w);
        } else if child.kind == "enum" {
            emit_proto_enum(schema, child, w);
        }
    }

    w.dedent();
    w.line("}");
    w.blank();
}

/// Resolve the type name for a protobuf field.
fn resolve_proto_field_type(schema: &Schema, field_vertex: &panproto_schema::Vertex) -> String {
    use crate::emit::resolve_type;

    resolve_type(schema, &field_vertex.id).map_or_else(
        || kind_to_proto_scalar(&field_vertex.kind).to_string(),
        |type_vertex| match type_vertex.kind.as_str() {
            "message" | "enum" => type_vertex
                .id
                .rsplit('.')
                .next()
                .unwrap_or(&type_vertex.id)
                .to_string(),
            other => kind_to_proto_scalar(other).to_string(),
        },
    )
}

/// Emit a single enum declaration.
fn emit_proto_enum(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
    w: &mut crate::emit::IndentWriter,
) {
    use crate::emit::children_by_edge;

    let name = vertex.id.rsplit('.').next().unwrap_or(&vertex.id);
    w.line(&format!("enum {name} {{"));
    w.indent();

    let variants = children_by_edge(schema, &vertex.id, "variant-of");
    for (edge, _variant_vertex) in &variants {
        let val_name = edge.name.as_deref().unwrap_or("UNKNOWN");
        w.line(&format!("{val_name} = 0;"));
    }

    w.dedent();
    w.line("}");
    w.blank();
}

/// Emit a single service declaration.
fn emit_proto_service(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
    w: &mut crate::emit::IndentWriter,
) {
    use crate::emit::children_by_edge;

    let name = vertex.id.rsplit('.').next().unwrap_or(&vertex.id);
    w.line(&format!("service {name} {{"));
    w.indent();

    let rpcs = children_by_edge(schema, &vertex.id, "field-of");
    for (edge, rpc_vertex) in &rpcs {
        if rpc_vertex.kind != "rpc" {
            continue;
        }
        let rpc_name = edge.name.as_deref().unwrap_or(&rpc_vertex.id);

        // Find input and output type-of edges.
        let type_of_edges: Vec<_> = schema
            .outgoing_edges(&rpc_vertex.id)
            .iter()
            .filter(|e| e.kind == "type-of")
            .collect();

        // The parser stores both input and output as type-of edges.
        // First one is input, second is output (by insertion order).
        let input_type = type_of_edges
            .first()
            .and_then(|e| schema.vertices.get(&e.tgt))
            .map_or("Empty", |v| v.id.rsplit('.').next().unwrap_or(&v.id));

        let output_type = type_of_edges
            .get(1)
            .and_then(|e| schema.vertices.get(&e.tgt))
            .map_or("Empty", |v| v.id.rsplit('.').next().unwrap_or(&v.id));

        w.line(&format!(
            "rpc {rpc_name} ({input_type}) returns ({output_type});"
        ));
    }

    w.dedent();
    w.line("}");
    w.blank();
}

/// Well-formedness rules for Protobuf edges.
fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "field-of".into(),
            src_kinds: vec!["message".into(), "oneof".into(), "service".into()],
            tgt_kinds: vec![
                "field".into(),
                "oneof".into(),
                "rpc".into(),
                "map".into(),
                "string".into(),
                "integer".into(),
                "float".into(),
                "boolean".into(),
            ],
        },
        EdgeRule {
            edge_kind: "type-of".into(),
            src_kinds: vec!["field".into(), "rpc".into(), "map".into()],
            tgt_kinds: vec![
                "message".into(),
                "enum".into(),
                "field".into(),
                "map".into(),
                "string".into(),
                "integer".into(),
                "float".into(),
                "boolean".into(),
            ],
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
        assert_eq!(p.name, "protobuf");
        assert_eq!(p.schema_theory, "ThProtobufSchema");
        assert_eq!(p.instance_theory, "ThProtobufInstance");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);

        assert!(registry.contains_key("ThSimpleGraph"));
        assert!(registry.contains_key("ThProtobufSchema"));
        assert!(registry.contains_key("ThProtobufInstance"));
    }

    #[test]
    fn parse_simple_proto() {
        let proto = r"
message Person {
  string name = 1;
  int32 age = 2;
  bool active = 3;
}
";

        let schema = parse_proto(proto);
        assert!(schema.is_ok(), "parse_proto should succeed: {schema:?}");
        let schema = schema.ok();
        let schema = schema.as_ref();

        assert!(schema.is_some_and(|s| s.has_vertex("Person")));
        assert!(schema.is_some_and(|s| s.has_vertex("Person.name")));
        assert!(schema.is_some_and(|s| s.has_vertex("Person.age")));
    }

    #[test]
    fn parse_proto_with_type_of_edges() {
        let proto = r"
message Address {
  string street = 1;
}

message Person {
  string name = 1;
  Address address = 2;
}
";
        let schema = parse_proto(proto).expect("should parse");
        // The field Person.address should have a type-of edge to Address.
        let type_of: Vec<_> = schema
            .outgoing_edges("Person.address")
            .iter()
            .filter(|e| e.kind == "type-of")
            .collect();
        assert_eq!(type_of.len(), 1);
        assert_eq!(type_of[0].tgt, "Address");
    }

    #[test]
    fn parse_proto_with_syntax_and_package() {
        let proto = r#"
syntax = "proto3";
package example;

message Foo {
  string bar = 1;
}
"#;
        let schema = parse_proto(proto).expect("should parse");
        assert!(schema.has_vertex("Foo"));
    }

    #[test]
    fn parse_proto_with_service() {
        let proto = r"
message Request {
  string query = 1;
}

message Response {
  string result = 1;
}

service SearchService {
  rpc Search (Request) returns (Response);
}
";
        let schema = parse_proto(proto).expect("should parse");
        assert!(schema.has_vertex("SearchService"));
        assert!(schema.has_vertex("SearchService.Search"));
        assert_eq!(
            schema.vertices.get("SearchService").unwrap().kind,
            "service"
        );
        assert_eq!(
            schema.vertices.get("SearchService.Search").unwrap().kind,
            "rpc"
        );
    }

    #[test]
    fn emit_proto_roundtrip() {
        let proto = r#"
syntax = "proto3";

message Person {
  string name = 1;
  int32 age = 2;
  bool active = 3;
}
"#;
        let schema1 = parse_proto(proto).expect("first parse should succeed");
        let emitted = emit_proto(&schema1).expect("emit should succeed");
        let schema2 = parse_proto(&emitted).expect("re-parse should succeed");

        assert_eq!(
            schema1.vertex_count(),
            schema2.vertex_count(),
            "vertex counts should match after round-trip"
        );
        assert_eq!(
            schema1.edge_count(),
            schema2.edge_count(),
            "edge counts should match after round-trip"
        );
    }

    #[test]
    fn parse_proto_with_map_field() {
        let proto = r"
message Config {
  map<string, string> labels = 1;
}
";
        let schema = parse_proto(proto).expect("should parse");
        assert!(schema.has_vertex("Config"));
        assert!(schema.has_vertex("Config.labels"));
        assert_eq!(schema.vertices.get("Config.labels").unwrap().kind, "map");
        assert!(schema.has_vertex("Config.labels:key"));
        assert!(schema.has_vertex("Config.labels:value"));
    }
}
