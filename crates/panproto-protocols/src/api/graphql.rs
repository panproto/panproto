//! GraphQL protocol definition.
//!
//! GraphQL uses a constrained multigraph with interfaces:
//! `colimit(ThGraph, ThConstraint, ThMulti, ThInterface)`.
//! Instance theory: `ThWType`.
//!
//! Vertex kinds: type, field, interface, union, enum, scalar, input, enum-value, subscription.
//! Edge kinds: field-of, implements, member-of, type-of.

use std::collections::HashMap;

use panproto_gat::{Sort, Theory, colimit};
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::error::ProtocolError;
use crate::theories;

/// Returns the GraphQL protocol definition.
///
/// Schema theory: `colimit(ThGraph, ThConstraint, ThMulti, ThInterface)`.
/// Instance theory: `ThWType`.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "graphql".into(),
        schema_theory: "ThGraphQLSchema".into(),
        instance_theory: "ThGraphQLInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "type".into(),
            "interface".into(),
            "input".into(),
            "field".into(),
            "union".into(),
            "enum".into(),
            "scalar".into(),
            "enum-value".into(),
            "subscription".into(),
        ],
        constraint_sorts: vec!["non_null".into(), "list".into(), "deprecated".into()],
        has_order: true,
        has_coproducts: true,
        has_recursion: true,
        nominal_identity: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for GraphQL with a theory registry.
pub fn register_theories<S: ::std::hash::BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    let th_graph = theories::th_graph();
    let th_constraint = theories::th_constraint();
    let th_multi = theories::th_multi();
    let th_interface = theories::th_interface();
    let th_wtype = theories::th_wtype();

    registry.insert("ThInterface".into(), th_interface.clone());

    let shared_vertex = Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![]);

    // Step 1: colimit(ThGraph, ThConstraint)
    if let Ok(gc) = colimit(&th_graph, &th_constraint, &shared_vertex) {
        // Step 2: colimit(gc, ThMulti) over {Vertex, Edge}
        let shared_ve = Theory::new(
            "ThVertexEdge",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![],
            vec![],
        );
        if let Ok(gcm) = colimit(&gc, &th_multi, &shared_ve) {
            // Step 3: colimit(gcm, ThInterface) over {Vertex}
            if let Ok(mut schema_theory) = colimit(&gcm, &th_interface, &shared_vertex) {
                schema_theory.name = "ThGraphQLSchema".into();
                registry.insert("ThGraphQLSchema".into(), schema_theory);
            }
        }
    }

    // Instance theory is ThWType.
    let mut inst = th_wtype;
    inst.name = "ThGraphQLInstance".into();
    registry.insert("ThGraphQLInstance".into(), inst);
}

/// Parse a GraphQL SDL string into a [`Schema`].
///
/// Uses a two-pass approach to resolve forward references:
/// - **Pass 1**: Parse all type, interface, input, enum, union, scalar, and
///   subscription declarations, creating vertices and field-of edges.
/// - **Pass 2**: Resolve cross-type references -- implements edges, type-of
///   edges for fields, and member-of edges for union members.
///
/// Also handles `extend type`, `schema { ... }`, and `directive` declarations.
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing or schema construction fails.
#[allow(clippy::too_many_lines)]
pub fn parse_sdl(sdl: &str) -> Result<Schema, ProtocolError> {
    let proto = protocol();

    // -----------------------------------------------------------------------
    // Pass 1: Collect all declarations (vertices + field-of edges).
    // We also record deferred cross-references to resolve in pass 2.
    // -----------------------------------------------------------------------
    let lines: Vec<&str> = sdl.lines().collect();
    let mut i = 0;

    let mut deferred: Vec<Deferred> = Vec::new();
    // Track all vertex IDs so we can look them up in pass 2.
    let mut vertex_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    let mut builder = SchemaBuilder::new(&proto);

    while i < lines.len() {
        let trimmed = lines[i].trim();

        if trimmed.starts_with("extend type ") {
            let (new_builder, new_i, new_deferred) =
                parse_extend_type(builder, &lines, i, &mut vertex_ids)?;
            builder = new_builder;
            deferred.extend(new_deferred);
            i = new_i;
        } else if trimmed.starts_with("type ") {
            // Determine kind: "type" or "subscription" for type Subscription.
            let name = trimmed
                .strip_prefix("type ")
                .and_then(|s| s.split(|c: char| c == '{' || c.is_whitespace()).next())
                .unwrap_or("")
                .trim();
            let kind = if name == "Subscription" {
                "subscription"
            } else {
                "type"
            };
            let (new_builder, new_i, new_deferred) =
                parse_type_def(builder, &lines, i, kind, &mut vertex_ids)?;
            builder = new_builder;
            deferred.extend(new_deferred);
            i = new_i;
        } else if trimmed.starts_with("interface ") {
            let (new_builder, new_i, new_deferred) =
                parse_type_def(builder, &lines, i, "interface", &mut vertex_ids)?;
            builder = new_builder;
            deferred.extend(new_deferred);
            i = new_i;
        } else if trimmed.starts_with("input ") {
            let (new_builder, new_i, new_deferred) =
                parse_type_def(builder, &lines, i, "input", &mut vertex_ids)?;
            builder = new_builder;
            deferred.extend(new_deferred);
            i = new_i;
        } else if trimmed.starts_with("enum ") {
            let (new_builder, new_i) = parse_enum_def(builder, &lines, i, &mut vertex_ids)?;
            builder = new_builder;
            i = new_i;
        } else if trimmed.starts_with("union ") {
            let (new_builder, new_deferred) = parse_union_def(builder, trimmed, &mut vertex_ids)?;
            builder = new_builder;
            deferred.extend(new_deferred);
            i += 1;
        } else if trimmed.starts_with("scalar ") {
            let name = trimmed.strip_prefix("scalar ").unwrap_or("").trim();
            if !name.is_empty() {
                builder = builder.vertex(name, "scalar", None)?;
                vertex_ids.insert(name.to_owned());
            }
            i += 1;
        } else if trimmed.starts_with("schema ") || trimmed == "schema" || trimmed == "schema {" {
            // Parse schema definition: `schema { query: Query, mutation: Mutation }`.
            // Skip it gracefully (the root types are already parsed as type declarations).
            i += 1;
            while i < lines.len() {
                let line = lines[i].trim();
                if line == "}" || line.starts_with('}') {
                    i += 1;
                    break;
                }
                i += 1;
            }
        } else if trimmed.starts_with("directive ") {
            // Directive definitions: skip gracefully, consuming any multi-line block.
            i += 1;
            if trimmed.contains('{') && !trimmed.contains('}') {
                while i < lines.len() {
                    let line = lines[i].trim();
                    if line == "}" || line.starts_with('}') {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
        } else {
            i += 1;
        }
    }

    // -----------------------------------------------------------------------
    // Pass 2: Resolve deferred cross-references.
    // -----------------------------------------------------------------------
    for d in deferred {
        match d.kind {
            DeferredKind::Implements { type_name, iface } => {
                if vertex_ids.contains(&iface) {
                    builder = builder.edge(&type_name, &iface, "implements", None)?;
                }
            }
            DeferredKind::TypeOf {
                field_id,
                type_name,
            } => {
                if vertex_ids.contains(&type_name) {
                    builder = builder.edge(&field_id, &type_name, "type-of", None)?;
                }
            }
            DeferredKind::MemberOf { union_name, member } => {
                if vertex_ids.contains(&member) {
                    builder = builder.edge(&union_name, &member, "member-of", Some(&member))?;
                }
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Deferred cross-type reference to resolve in pass 2.
struct Deferred {
    kind: DeferredKind,
}

/// The kind of deferred reference.
enum DeferredKind {
    /// `type Foo implements Bar` -- add "implements" edge from Foo to Bar.
    Implements {
        /// The type implementing the interface.
        type_name: String,
        /// The interface being implemented.
        iface: String,
    },
    /// Field `Parent.field` has type `TypeName` -- add "type-of" edge.
    TypeOf {
        /// The field vertex ID (e.g. `"Query.user"`).
        field_id: String,
        /// The referenced type name.
        type_name: String,
    },
    /// `union Foo = A | B | C` -- add "member-of" edge from union to member.
    MemberOf {
        /// The union vertex name.
        union_name: String,
        /// The member type name.
        member: String,
    },
}

/// Extract the base type name from a GraphQL type expression.
///
/// Strips `!` (non-null) and `[`/`]` (list) wrappers to return the
/// underlying named type. For example, `[String!]!` returns `String`.
fn extract_base_type(type_expr: &str) -> &str {
    let s = type_expr.trim();
    let s = s.trim_end_matches('!');
    let s = s.trim_start_matches('[');
    let s = s.trim_end_matches(']');
    let s = s.trim_end_matches('!');
    s.trim()
}

/// Parse a type/interface/input/subscription definition.
///
/// Returns the updated builder, the next line index, and any deferred
/// cross-type references (implements and type-of edges).
fn parse_type_def(
    mut builder: SchemaBuilder,
    lines: &[&str],
    start: usize,
    kind: &str,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<(SchemaBuilder, usize, Vec<Deferred>), ProtocolError> {
    let trimmed = lines[start].trim();

    // Determine the keyword to strip from the line.
    let keyword = match kind {
        "type" | "subscription" => "type ",
        "interface" => "interface ",
        "input" => "input ",
        _ => return Err(ProtocolError::Parse(format!("unknown kind: {kind}"))),
    };

    let after_keyword = trimmed
        .strip_prefix(keyword)
        .ok_or_else(|| ProtocolError::Parse(format!("expected {keyword}")))?;

    // Handle "type Name implements Iface {"
    let name = after_keyword
        .split(|c: char| c == '{' || c.is_whitespace())
        .next()
        .unwrap_or("")
        .trim();

    if name.is_empty() {
        return Err(ProtocolError::Parse("empty type name".into()));
    }

    builder = builder.vertex(name, kind, None)?;
    vertex_ids.insert(name.to_owned());

    let mut deferred = Vec::new();

    // Extract implements interfaces (e.g. "type Foo implements Bar & Baz {")
    if let Some(impl_idx) = after_keyword.find("implements") {
        let after_impl = &after_keyword[impl_idx + "implements".len()..];
        let before_brace = after_impl.split('{').next().unwrap_or("");
        for iface_name in before_brace.split('&') {
            let iface = iface_name.trim();
            if !iface.is_empty() {
                deferred.push(Deferred {
                    kind: DeferredKind::Implements {
                        type_name: name.to_owned(),
                        iface: iface.to_owned(),
                    },
                });
            }
        }
    }

    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i].trim();
        if line == "}" || line.starts_with('}') {
            return Ok((builder, i + 1, deferred));
        }

        // Parse field: name: Type or name(args): Type
        if !line.is_empty() && !line.starts_with('#') {
            if let Some(colon_idx) = line.find(':') {
                let field_name = line[..colon_idx].split('(').next().unwrap_or("").trim();

                if !field_name.is_empty() {
                    let field_id = format!("{name}.{field_name}");
                    builder = builder.vertex(&field_id, "field", None)?;
                    vertex_ids.insert(field_id.clone());
                    builder = builder.edge(name, &field_id, "field-of", Some(field_name))?;

                    // Extract field type for deferred type-of edge
                    let type_expr = line[colon_idx + 1..].trim();
                    let base_type = extract_base_type(type_expr);
                    if !base_type.is_empty() {
                        deferred.push(Deferred {
                            kind: DeferredKind::TypeOf {
                                field_id,
                                type_name: base_type.to_owned(),
                            },
                        });
                    }
                }
            }
        }

        i += 1;
    }

    Ok((builder, i, deferred))
}

/// Parse `extend type Foo { ... }` by adding fields to the existing type.
fn parse_extend_type(
    mut builder: SchemaBuilder,
    lines: &[&str],
    start: usize,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<(SchemaBuilder, usize, Vec<Deferred>), ProtocolError> {
    let trimmed = lines[start].trim();
    let after = trimmed
        .strip_prefix("extend type ")
        .ok_or_else(|| ProtocolError::Parse("expected extend type".into()))?;

    let name = after
        .split(|c: char| c == '{' || c.is_whitespace())
        .next()
        .unwrap_or("")
        .trim();

    if name.is_empty() {
        return Err(ProtocolError::Parse("empty extend type name".into()));
    }

    // If the type doesn't exist yet, create it.
    if !vertex_ids.contains(name) {
        builder = builder.vertex(name, "type", None)?;
        vertex_ids.insert(name.to_owned());
    }

    let mut deferred = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i].trim();
        if line == "}" || line.starts_with('}') {
            return Ok((builder, i + 1, deferred));
        }

        if !line.is_empty() && !line.starts_with('#') {
            if let Some(colon_idx) = line.find(':') {
                let field_name = line[..colon_idx].split('(').next().unwrap_or("").trim();

                if !field_name.is_empty() {
                    let field_id = format!("{name}.{field_name}");
                    builder = builder.vertex(&field_id, "field", None)?;
                    vertex_ids.insert(field_id.clone());
                    builder = builder.edge(name, &field_id, "field-of", Some(field_name))?;

                    let type_expr = line[colon_idx + 1..].trim();
                    let base_type = extract_base_type(type_expr);
                    if !base_type.is_empty() {
                        deferred.push(Deferred {
                            kind: DeferredKind::TypeOf {
                                field_id,
                                type_name: base_type.to_owned(),
                            },
                        });
                    }
                }
            }
        }

        i += 1;
    }

    Ok((builder, i, deferred))
}

/// Parse an enum definition.
fn parse_enum_def(
    mut builder: SchemaBuilder,
    lines: &[&str],
    start: usize,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<(SchemaBuilder, usize), ProtocolError> {
    let trimmed = lines[start].trim();
    let name = trimmed
        .strip_prefix("enum ")
        .and_then(|s| s.split('{').next())
        .map(str::trim)
        .ok_or_else(|| ProtocolError::Parse("invalid enum declaration".into()))?;

    builder = builder.vertex(name, "enum", None)?;
    vertex_ids.insert(name.to_owned());

    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i].trim();
        if line == "}" || line.starts_with('}') {
            return Ok((builder, i + 1));
        }
        // Enum values are simple identifiers.
        if !line.is_empty() && !line.starts_with('#') {
            let val_name = line.trim();
            let val_id = format!("{name}.{val_name}");
            builder = builder.vertex(&val_id, "enum-value", None)?;
            vertex_ids.insert(val_id.clone());
            builder = builder.edge(name, &val_id, "member-of", Some(val_name))?;
        }
        i += 1;
    }

    Ok((builder, i))
}

/// Parse a union definition (single line: `union Foo = A | B | C`).
///
/// Returns the updated builder and deferred member-of edges.
fn parse_union_def(
    mut builder: SchemaBuilder,
    line: &str,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<(SchemaBuilder, Vec<Deferred>), ProtocolError> {
    let after = line
        .strip_prefix("union ")
        .ok_or_else(|| ProtocolError::Parse("expected union".into()))?;

    let parts: Vec<&str> = after.splitn(2, '=').collect();
    if parts.len() < 2 {
        return Err(ProtocolError::Parse("invalid union syntax".into()));
    }

    let name = parts[0].trim();
    builder = builder.vertex(name, "union", None)?;
    vertex_ids.insert(name.to_owned());

    let mut deferred = Vec::new();
    for member in parts[1].split('|') {
        let member_name = member.trim();
        if !member_name.is_empty() {
            deferred.push(Deferred {
                kind: DeferredKind::MemberOf {
                    union_name: name.to_owned(),
                    member: member_name.to_owned(),
                },
            });
        }
    }

    Ok((builder, deferred))
}

/// Emit a [`Schema`] as a GraphQL SDL string.
///
/// Reconstructs type, interface, input, enum, union, scalar, and
/// subscription declarations from the schema graph.
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema cannot be serialized.
pub fn emit_sdl(schema: &Schema) -> Result<String, ProtocolError> {
    use crate::emit::{IndentWriter, find_roots};

    let structural = &["field-of", "member-of", "implements", "type-of"];
    let roots = find_roots(schema, structural);

    let mut w = IndentWriter::new("  ");

    for root in &roots {
        match root.kind.as_str() {
            "scalar" => {
                w.line(&format!("scalar {}", root.id));
                w.blank();
            }
            "type" | "subscription" | "interface" | "input" => {
                emit_graphql_type_def(schema, root, &mut w);
            }
            "enum" => {
                emit_graphql_enum(schema, root, &mut w);
            }
            "union" => {
                emit_graphql_union(schema, root, &mut w);
            }
            _ => {}
        }
    }

    Ok(w.finish())
}

/// Emit a type/interface/input/subscription definition.
fn emit_graphql_type_def(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
    w: &mut crate::emit::IndentWriter,
) {
    use crate::emit::children_by_edge;

    let keyword = match vertex.kind.as_str() {
        "interface" => "interface",
        "input" => "input",
        _ => "type",
    };

    // Check for implements edges.
    let implements_edges: Vec<_> = schema
        .outgoing_edges(&vertex.id)
        .iter()
        .filter(|e| e.kind == "implements")
        .collect();

    let implements_str = if implements_edges.is_empty() {
        String::new()
    } else {
        let ifaces: Vec<&str> = implements_edges.iter().map(|e| e.tgt.as_str()).collect();
        format!(" implements {}", ifaces.join(" & "))
    };

    w.line(&format!("{keyword} {}{implements_str} {{", vertex.id));
    w.indent();

    let fields = children_by_edge(schema, &vertex.id, "field-of");
    for (edge, field_vertex) in &fields {
        let field_name = edge.name.as_deref().unwrap_or(&field_vertex.id);

        // Determine the type expression.
        let type_expr = resolve_graphql_type_expr(schema, field_vertex);
        w.line(&format!("{field_name}: {type_expr}"));
    }

    w.dedent();
    w.line("}");
    w.blank();
}

/// Resolve the GraphQL type expression for a field, including `non_null` and list wrappers.
fn resolve_graphql_type_expr(schema: &Schema, field_vertex: &panproto_schema::Vertex) -> String {
    use crate::emit::{constraint_value, resolve_type};

    let base = resolve_type(schema, &field_vertex.id).map_or_else(
        || {
            match field_vertex.kind.as_str() {
                "integer" => "Int",
                "boolean" => "Boolean",
                "float" => "Float",
                _ => "String",
            }
            .to_string()
        },
        |type_vertex| type_vertex.id.to_string(),
    );

    let is_list = constraint_value(schema, &field_vertex.id, "list").is_some_and(|v| v == "true");
    let is_non_null =
        constraint_value(schema, &field_vertex.id, "non_null").is_some_and(|v| v == "true");

    let mut result = base;
    if is_list {
        result = format!("[{result}]");
    }
    if is_non_null {
        result = format!("{result}!");
    }
    result
}

/// Emit an enum definition.
fn emit_graphql_enum(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
    w: &mut crate::emit::IndentWriter,
) {
    use crate::emit::children_by_edge;

    w.line(&format!("enum {} {{", vertex.id));
    w.indent();

    let members = children_by_edge(schema, &vertex.id, "member-of");
    for (edge, _member_vertex) in &members {
        let val_name = edge.name.as_deref().unwrap_or("UNKNOWN");
        w.line(val_name);
    }

    w.dedent();
    w.line("}");
    w.blank();
}

/// Emit a union definition.
fn emit_graphql_union(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
    w: &mut crate::emit::IndentWriter,
) {
    use crate::emit::children_by_edge;

    let members = children_by_edge(schema, &vertex.id, "member-of");
    let member_names: Vec<&str> = members
        .iter()
        .filter_map(|(edge, _)| edge.name.as_deref())
        .collect();

    w.line(&format!(
        "union {} = {}",
        vertex.id,
        member_names.join(" | ")
    ));
    w.blank();
}

/// Well-formedness rules for GraphQL edges.
fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "field-of".into(),
            src_kinds: vec![
                "type".into(),
                "interface".into(),
                "input".into(),
                "subscription".into(),
            ],
            tgt_kinds: vec!["field".into()],
        },
        EdgeRule {
            edge_kind: "implements".into(),
            src_kinds: vec!["type".into(), "subscription".into()],
            tgt_kinds: vec!["interface".into()],
        },
        EdgeRule {
            edge_kind: "member-of".into(),
            src_kinds: vec!["union".into(), "enum".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "type-of".into(),
            src_kinds: vec!["field".into()],
            tgt_kinds: vec![],
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
        assert_eq!(p.name, "graphql");
        assert_eq!(p.schema_theory, "ThGraphQLSchema");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThGraphQLSchema"));
        assert!(registry.contains_key("ThGraphQLInstance"));
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn parse_sdl_with_forward_references() {
        let sdl = r"
scalar DateTime

interface Node {
  id: ID
}

type User implements Node {
  id: ID
  name: String
  posts: [Post!]!
}

type Post implements Node {
  id: ID
  title: String
  author: User
  createdAt: DateTime
}

input CreatePostInput {
  title: String
  authorId: ID
}

enum Status {
  DRAFT
  PUBLISHED
  ARCHIVED
}

union SearchResult = User | Post
";

        let schema = parse_sdl(sdl).unwrap_or_else(|e| panic!("parse_sdl should succeed: {e}"));

        // Verify vertices
        assert!(
            schema.vertices.contains_key("Node"),
            "should have Node interface"
        );
        assert!(
            schema.vertices.contains_key("User"),
            "should have User type"
        );
        assert!(
            schema.vertices.contains_key("Post"),
            "should have Post type"
        );
        assert!(
            schema.vertices.contains_key("DateTime"),
            "should have DateTime scalar"
        );
        assert!(
            schema.vertices.contains_key("CreatePostInput"),
            "should have CreatePostInput input"
        );
        assert!(
            schema.vertices.contains_key("Status"),
            "should have Status enum"
        );
        assert!(
            schema.vertices.contains_key("SearchResult"),
            "should have SearchResult union"
        );

        // Verify field vertices
        assert!(
            schema.vertices.contains_key("User.name"),
            "should have User.name field"
        );
        assert!(
            schema.vertices.contains_key("Post.title"),
            "should have Post.title field"
        );

        // Verify implements edges (forward reference: User -> Node)
        let user_implements: Vec<_> = schema
            .outgoing_edges("User")
            .iter()
            .filter(|e| e.kind == "implements")
            .collect();
        assert_eq!(
            user_implements.len(),
            1,
            "User should implement one interface"
        );
        assert_eq!(user_implements[0].tgt, "Node", "User should implement Node");

        assert_eq!(
            schema
                .outgoing_edges("Post")
                .iter()
                .filter(|e| e.kind == "implements")
                .count(),
            1,
            "Post should implement one interface"
        );

        // Verify type-of edges (forward reference: Post.author -> User)
        let author_type_of: Vec<_> = schema
            .outgoing_edges("Post.author")
            .iter()
            .filter(|e| e.kind == "type-of")
            .collect();
        assert_eq!(
            author_type_of.len(),
            1,
            "Post.author should have a type-of edge"
        );
        assert_eq!(
            author_type_of[0].tgt, "User",
            "Post.author should reference User"
        );

        // Verify type-of edge for Post.createdAt -> DateTime (scalar ref)
        let created_type_of: Vec<_> = schema
            .outgoing_edges("Post.createdAt")
            .iter()
            .filter(|e| e.kind == "type-of")
            .collect();
        assert_eq!(
            created_type_of.len(),
            1,
            "Post.createdAt should have a type-of edge"
        );
        assert_eq!(
            created_type_of[0].tgt, "DateTime",
            "Post.createdAt should reference DateTime"
        );

        // Verify union member-of edges
        let union_members: Vec<_> = schema
            .outgoing_edges("SearchResult")
            .iter()
            .filter(|e| e.kind == "member-of")
            .collect();
        assert_eq!(
            union_members.len(),
            2,
            "SearchResult should have 2 member-of edges"
        );
        let member_targets: std::collections::HashSet<&str> =
            union_members.iter().map(|e| e.tgt.as_str()).collect();
        assert!(
            member_targets.contains("User"),
            "SearchResult should include User"
        );
        assert!(
            member_targets.contains("Post"),
            "SearchResult should include Post"
        );

        // Verify enum values use "enum-value" kind
        let status_members: Vec<_> = schema
            .outgoing_edges("Status")
            .iter()
            .filter(|e| e.kind == "member-of")
            .collect();
        assert_eq!(status_members.len(), 3, "Status should have 3 enum values");
        for member in &status_members {
            let vertex = schema.vertices.get(&member.tgt).expect("vertex exists");
            assert_eq!(
                vertex.kind, "enum-value",
                "enum values should use enum-value kind"
            );
        }
    }

    #[test]
    fn emit_sdl_roundtrip() {
        let sdl = r"
type User {
  id: ID
  name: String
}

enum Status {
  ACTIVE
  INACTIVE
}
";
        let schema1 = parse_sdl(sdl).expect("first parse should succeed");
        let emitted = emit_sdl(&schema1).expect("emit should succeed");
        let schema2 = parse_sdl(&emitted).expect("re-parse should succeed");

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
    fn parse_extend_type() {
        let sdl = r"
type Query {
  users: [User]
}

extend type Query {
  posts: [Post]
}

type User {
  id: ID
}

type Post {
  id: ID
}
";
        let schema = parse_sdl(sdl).expect("should parse");
        assert!(schema.has_vertex("Query.users"));
        assert!(schema.has_vertex("Query.posts"));
    }

    #[test]
    fn parse_schema_definition() {
        let sdl = r"
schema {
  query: Query
  mutation: Mutation
}

type Query {
  hello: String
}

type Mutation {
  update: String
}
";
        let schema = parse_sdl(sdl).expect("should parse");
        assert!(schema.has_vertex("Query"));
        assert!(schema.has_vertex("Mutation"));
    }

    #[test]
    fn parse_directive_definition() {
        let sdl = r"
directive @deprecated(reason: String) on FIELD_DEFINITION | ENUM_VALUE

type Foo {
  bar: String
}
";
        let schema = parse_sdl(sdl).expect("should parse");
        assert!(schema.has_vertex("Foo"));
    }

    #[test]
    fn parse_subscription_type() {
        let sdl = r"
type Subscription {
  messageAdded: String
}
";
        let schema = parse_sdl(sdl).expect("should parse");
        assert!(schema.has_vertex("Subscription"));
        assert_eq!(
            schema.vertices.get("Subscription").unwrap().kind,
            "subscription"
        );
        assert!(schema.has_vertex("Subscription.messageAdded"));
    }
}
