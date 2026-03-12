//! Cambria-style lens combinators.
//!
//! Each combinator represents an atomic schema transformation that can be
//! compiled into a migration and composed with other combinators to build
//! complex bidirectional transformations.
//!
//! Supported combinators:
//! - [`Combinator::RenameField`]: rename an edge label (lossless)
//! - [`Combinator::AddField`]: add a new vertex with a default value
//! - [`Combinator::RemoveField`]: remove a vertex and its edges
//! - [`Combinator::WrapInObject`]: introduce an intermediate object vertex
//! - [`Combinator::HoistField`]: move a nested field up to a parent
//! - [`Combinator::CoerceType`]: change the kind of a vertex
//! - [`Combinator::Compose`]: sequential composition of two combinators

use std::collections::HashMap;

use panproto_inst::CompiledMigration;
use panproto_inst::value::Value;
use panproto_schema::{Edge, Protocol, Schema, Vertex};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::Lens;
use crate::error::LensError;

/// A lens combinator describing an atomic schema transformation.
///
/// Combinators can be composed via `Compose` or chained in a slice
/// passed to [`from_combinators`].
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Combinator {
    /// Rename a field (edge label). Lossless: complement is empty.
    RenameField {
        /// The old field name.
        old: String,
        /// The new field name.
        new: String,
    },
    /// Add a field with a default value.
    AddField {
        /// The field name (used as edge label and vertex ID suffix).
        name: String,
        /// The vertex kind for the new field (e.g., `"string"`).
        vertex_kind: String,
        /// Default value for the field.
        default: Value,
    },
    /// Remove a field from the schema.
    RemoveField {
        /// The field name to remove.
        name: String,
    },
    /// Wrap existing children in a new intermediate object vertex.
    WrapInObject {
        /// The name for the new wrapper object.
        field_name: String,
    },
    /// Hoist a nested field up to a parent vertex.
    HoistField {
        /// The host vertex that currently owns the field.
        host: String,
        /// The field to hoist from the host to its parent.
        field: String,
    },
    /// Change the kind (type) of a vertex.
    CoerceType {
        /// The source kind.
        from_kind: String,
        /// The target kind.
        to_kind: String,
    },
    /// Sequential composition of two combinators.
    Compose(Box<Self>, Box<Self>),
}

/// Build a [`Lens`] from a source schema and a chain of combinators.
///
/// Each combinator is applied in sequence, deriving the target schema and
/// migration at each step, then composing them all together.
///
/// # Errors
///
/// Returns `LensError` if any combinator references nonexistent schema
/// elements, or if composition fails.
pub fn from_combinators(
    src: &Schema,
    combinators: &[Combinator],
    _protocol: &Protocol,
) -> Result<Lens, LensError> {
    if combinators.is_empty() {
        // Identity lens
        return Ok(Lens {
            compiled: identity_compiled(src),
            src_schema: src.clone(),
            tgt_schema: src.clone(),
        });
    }

    // Apply each combinator step by step, building an intermediate compiled
    // migration for each step, then composing them all together. This
    // ensures multi-step chains work correctly (e.g., rename followed by
    // hoist that references the renamed name).
    let mut current_schema = src.clone();
    let mut composed_migration: Option<CompiledMigration> = None;

    for combinator in combinators {
        let next_schema = apply_combinator(&current_schema, combinator)?;
        let step_migration = build_compiled_migration(
            &current_schema,
            &next_schema,
            std::slice::from_ref(combinator),
        );

        composed_migration = Some(match composed_migration {
            Some(prev) => crate::compose::compose_compiled_migrations(&prev, &step_migration),
            None => step_migration,
        });

        current_schema = next_schema;
    }

    let compiled = composed_migration.unwrap_or_else(|| identity_compiled(src));

    Ok(Lens {
        compiled,
        src_schema: src.clone(),
        tgt_schema: current_schema,
    })
}

/// Apply a single combinator to a schema, producing the new target schema.
fn apply_combinator(schema: &Schema, combinator: &Combinator) -> Result<Schema, LensError> {
    match combinator {
        Combinator::RenameField { old, new } => apply_rename(schema, old, new),
        Combinator::AddField {
            name, vertex_kind, ..
        } => apply_add_field(schema, name, vertex_kind),
        Combinator::RemoveField { name } => apply_remove_field(schema, name),
        Combinator::WrapInObject { field_name } => apply_wrap_in_object(schema, field_name),
        Combinator::HoistField { host, field } => apply_hoist_field(schema, host, field),
        Combinator::CoerceType { from_kind, to_kind } => {
            apply_coerce_type(schema, from_kind, to_kind)
        }
        Combinator::Compose(first, second) => {
            let intermediate = apply_combinator(schema, first)?;
            apply_combinator(&intermediate, second)
        }
    }
}

/// Rename: change the `name` label on all edges matching `old` to `new`.
fn apply_rename(schema: &Schema, old: &str, new: &str) -> Result<Schema, LensError> {
    let has_match = schema.edges.keys().any(|e| e.name.as_deref() == Some(old));
    if !has_match {
        return Err(LensError::FieldNotFound(old.to_string()));
    }

    let mut result = schema.clone();
    let edges_to_update: Vec<Edge> = result
        .edges
        .keys()
        .filter(|e| e.name.as_deref() == Some(old))
        .cloned()
        .collect();

    for edge in edges_to_update {
        let kind = result.edges.remove(&edge).unwrap_or_default();
        let mut new_edge = edge.clone();
        new_edge.name = Some(new.to_string());
        result.edges.insert(new_edge, kind);
    }

    rebuild_indices(&mut result);
    Ok(result)
}

/// Add a new vertex (field) with an edge from the root.
fn apply_add_field(schema: &Schema, name: &str, vertex_kind: &str) -> Result<Schema, LensError> {
    let mut result = schema.clone();

    // Find the root vertex (first vertex, or the one matching schema convention)
    let root_id = find_root_vertex(schema)?;

    let new_vertex_id = format!("{root_id}.{name}");
    result.vertices.insert(
        new_vertex_id.clone(),
        Vertex {
            id: new_vertex_id.clone(),
            kind: vertex_kind.to_string(),
            nsid: None,
        },
    );

    let new_edge = Edge {
        src: root_id,
        tgt: new_vertex_id,
        kind: "prop".to_string(),
        name: Some(name.to_string()),
    };
    result.edges.insert(new_edge, "prop".to_string());
    rebuild_indices(&mut result);
    Ok(result)
}

/// Remove a field: drop the vertex and all incident edges.
fn apply_remove_field(schema: &Schema, name: &str) -> Result<Schema, LensError> {
    // Find edges with this name
    let matching_edges: Vec<Edge> = schema
        .edges
        .keys()
        .filter(|e| e.name.as_deref() == Some(name))
        .cloned()
        .collect();

    if matching_edges.is_empty() {
        return Err(LensError::FieldNotFound(name.to_string()));
    }

    let mut result = schema.clone();

    // Remove the target vertex for each matching edge and the edge itself
    for edge in &matching_edges {
        result.vertices.remove(&edge.tgt);
        result.edges.remove(edge);
        result.constraints.remove(&edge.tgt);
        result.required.remove(&edge.tgt);

        // Also remove edges incident on the removed vertex
        let removed_vertex = &edge.tgt;
        let to_remove: Vec<Edge> = result
            .edges
            .keys()
            .filter(|e| e.src == *removed_vertex || e.tgt == *removed_vertex)
            .cloned()
            .collect();
        for e in to_remove {
            result.edges.remove(&e);
        }
    }

    rebuild_indices(&mut result);
    Ok(result)
}

/// Wrap children in an intermediate object vertex.
fn apply_wrap_in_object(schema: &Schema, field_name: &str) -> Result<Schema, LensError> {
    let root_id = find_root_vertex(schema)?;

    let mut result = schema.clone();

    // Create new wrapper vertex
    let wrapper_id = format!("{root_id}.{field_name}");
    result.vertices.insert(
        wrapper_id.clone(),
        Vertex {
            id: wrapper_id.clone(),
            kind: "object".to_string(),
            nsid: None,
        },
    );

    // Add edge from root to wrapper
    let wrapper_edge = Edge {
        src: root_id.clone(),
        tgt: wrapper_id.clone(),
        kind: "prop".to_string(),
        name: Some(field_name.to_string()),
    };
    result.edges.insert(wrapper_edge, "prop".to_string());

    // Re-parent existing children of root under the wrapper
    let root_edges: Vec<Edge> = result
        .edges
        .keys()
        .filter(|e| e.src == root_id && e.name.as_deref() != Some(field_name))
        .cloned()
        .collect();

    for edge in root_edges {
        let kind = result.edges.remove(&edge).unwrap_or_default();
        let mut new_edge = edge;
        new_edge.src.clone_from(&wrapper_id);
        result.edges.insert(new_edge, kind);
    }

    rebuild_indices(&mut result);
    Ok(result)
}

/// Hoist a nested field from host to host's parent.
fn apply_hoist_field(schema: &Schema, host: &str, field: &str) -> Result<Schema, LensError> {
    // Find the edge from host that has this field name
    let field_edge = schema
        .edges
        .keys()
        .find(|e| e.src == host && e.name.as_deref() == Some(field))
        .cloned()
        .ok_or_else(|| LensError::FieldNotFound(format!("{host}.{field}")))?;

    // Find the parent of host
    let parent_edge = schema
        .edges
        .keys()
        .find(|e| e.tgt == host)
        .cloned()
        .ok_or_else(|| LensError::VertexNotFound(format!("parent of {host}")))?;

    let mut result = schema.clone();

    // Remove the old edge from host
    let kind = result.edges.remove(&field_edge).unwrap_or_default();

    // Add new edge from parent to the hoisted field's target
    let new_edge = Edge {
        src: parent_edge.src,
        tgt: field_edge.tgt,
        kind: kind.clone(),
        name: field_edge.name,
    };
    result.edges.insert(new_edge, kind);

    rebuild_indices(&mut result);
    Ok(result)
}

/// Change the kind of all vertices matching `from_kind` to `to_kind`.
fn apply_coerce_type(schema: &Schema, from_kind: &str, to_kind: &str) -> Result<Schema, LensError> {
    let has_match = schema.vertices.values().any(|v| v.kind == from_kind);
    if !has_match {
        return Err(LensError::IncompatibleCoercion {
            from: from_kind.to_string(),
            to: to_kind.to_string(),
        });
    }

    let mut result = schema.clone();
    for vertex in result.vertices.values_mut() {
        if vertex.kind == from_kind {
            vertex.kind = to_kind.to_string();
        }
    }
    Ok(result)
}

/// Find the root vertex of a schema (the lexicographically first vertex
/// without incoming edges, or the lexicographically first vertex if all
/// have incoming edges).
fn find_root_vertex(schema: &Schema) -> Result<String, LensError> {
    // Collect vertices with no incoming edges, sort, take first
    let mut roots: Vec<&String> = schema
        .vertices
        .keys()
        .filter(|id| !schema.edges.keys().any(|e| &e.tgt == *id))
        .collect();
    roots.sort();
    if let Some(root) = roots.first() {
        return Ok((*root).clone());
    }
    // Fallback: lexicographically first vertex
    let mut all_keys: Vec<&String> = schema.vertices.keys().collect();
    all_keys.sort();
    all_keys
        .first()
        .map(|k| (*k).clone())
        .ok_or_else(|| LensError::VertexNotFound("root".to_string()))
}

/// Rebuild the precomputed adjacency indices on a schema (public for test helpers).
#[cfg(test)]
pub(crate) fn rebuild_indices_pub(schema: &mut Schema) {
    rebuild_indices(schema);
}

/// Rebuild the precomputed adjacency indices on a schema.
fn rebuild_indices(schema: &mut Schema) {
    let mut outgoing: HashMap<String, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<String, SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(String, String), SmallVec<Edge, 2>> = HashMap::new();

    for edge in schema.edges.keys() {
        outgoing
            .entry(edge.src.clone())
            .or_default()
            .push(edge.clone());
        incoming
            .entry(edge.tgt.clone())
            .or_default()
            .push(edge.clone());
        between
            .entry((edge.src.clone(), edge.tgt.clone()))
            .or_default()
            .push(edge.clone());
    }

    schema.outgoing = outgoing;
    schema.incoming = incoming;
    schema.between = between;
}

/// Build an identity compiled migration for a schema.
fn identity_compiled(schema: &Schema) -> CompiledMigration {
    let surviving_verts = schema.vertices.keys().cloned().collect();
    let surviving_edges = schema.edges.keys().cloned().collect();
    CompiledMigration {
        surviving_verts,
        surviving_edges,
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
    }
}

/// Build a compiled migration from a source schema, target schema, and
/// combinator chain.
fn build_compiled_migration(
    src: &Schema,
    tgt: &Schema,
    combinators: &[Combinator],
) -> CompiledMigration {
    let mut surviving_verts = std::collections::HashSet::new();
    let mut surviving_edges = std::collections::HashSet::new();
    let mut vertex_remap = HashMap::new();
    let mut edge_remap = HashMap::new();

    // Determine which source vertices survive in the target
    for src_id in src.vertices.keys() {
        if tgt.vertices.contains_key(src_id) {
            surviving_verts.insert(src_id.clone());
        }
    }

    // Determine which source edges survive in the target
    for src_edge in src.edges.keys() {
        if tgt.edges.contains_key(src_edge) {
            surviving_edges.insert(src_edge.clone());
        }
    }

    // Build vertex/edge remap based on combinators
    for combinator in combinators {
        match combinator {
            Combinator::RenameField { old, new } => {
                // Rename affects edge labels, not vertex IDs directly
                for src_edge in src.edges.keys() {
                    if src_edge.name.as_deref() == Some(old.as_str()) {
                        let mut new_edge = src_edge.clone();
                        new_edge.name = Some(new.clone());
                        edge_remap.insert(src_edge.clone(), new_edge);
                        surviving_edges.insert(src_edge.clone());
                    }
                }
            }
            Combinator::RemoveField { name } => {
                // Remove edges with this name and their target vertices
                for src_edge in src.edges.keys() {
                    if src_edge.name.as_deref() == Some(name.as_str()) {
                        surviving_verts.remove(&src_edge.tgt);
                        surviving_edges.remove(src_edge);
                    }
                }
            }
            Combinator::AddField { .. } | Combinator::CoerceType { .. } => {
                // AddField adds a new vertex+edge in the target that has no
                // source counterpart. All source vertices/edges survive as-is
                // (this is an embedding). No remap needed.
                // CoerceType changes vertex kind but keeps same IDs.
                // All vertices/edges survive (lossless transformation).
            }
            Combinator::WrapInObject { field_name } => {
                // Source children get re-parented under the wrapper in the target.
                // The wrapper vertex exists only in the target, not in source.
                // Source edges from root to children are removed (they go through
                // the wrapper now), so remove them from surviving_edges.
                if let Ok(root_id) = find_root_vertex(src) {
                    for src_edge in src.edges.keys() {
                        if src_edge.src == root_id
                            && src_edge.name.as_deref() != Some(field_name.as_str())
                        {
                            surviving_edges.remove(src_edge);
                            // The child vertex still survives, just under a new parent.
                            // Map src child vertex to itself (same ID, different parent).
                            vertex_remap.insert(src_edge.tgt.clone(), src_edge.tgt.clone());
                        }
                    }
                }
            }
            Combinator::HoistField { host, field } => {
                // The field vertex moves from being a child of `host` to being
                // a child of `host`'s parent. The field's vertex ID stays the
                // same; only the edge topology changes.
                // Old edge (host -> field) is removed, new edge (parent -> field)
                // is added.
                for src_edge in src.edges.keys() {
                    if src_edge.src == *host && src_edge.name.as_deref() == Some(field.as_str()) {
                        surviving_edges.remove(src_edge);
                        // Find the parent of host to build the new edge
                        if let Some(parent_edge) = src.edges.keys().find(|e| e.tgt == *host) {
                            let new_edge = Edge {
                                src: parent_edge.src.clone(),
                                tgt: src_edge.tgt.clone(),
                                kind: src_edge.kind.clone(),
                                name: src_edge.name.clone(),
                            };
                            edge_remap.insert(src_edge.clone(), new_edge);
                        }
                    }
                }
            }
            Combinator::Compose(first, second) => {
                // Recursively build compiled migrations for each part and compose.
                let intermediate = apply_combinator(src, first).unwrap_or_else(|_| src.clone());
                let m1 = build_compiled_migration(src, &intermediate, std::slice::from_ref(first));
                let m2 = build_compiled_migration(&intermediate, tgt, std::slice::from_ref(second));
                let composed = crate::compose::compose_compiled_migrations(&m1, &m2);
                // Merge composed results into the current state
                surviving_verts = composed.surviving_verts;
                surviving_edges = composed.surviving_edges;
                vertex_remap = composed.vertex_remap;
                edge_remap = composed.edge_remap;
            }
        }
    }

    // Build resolver for edges between surviving vertices in the target
    let mut resolver = HashMap::new();
    for edge in tgt.edges.keys() {
        if surviving_verts.contains(&edge.src) || vertex_remap.values().any(|v| v == &edge.src) {
            let src_key = vertex_remap.get(&edge.src).unwrap_or(&edge.src).clone();
            let tgt_key = vertex_remap.get(&edge.tgt).unwrap_or(&edge.tgt).clone();
            resolver.insert((src_key, tgt_key), edge.clone());
        }
    }

    CompiledMigration {
        surviving_verts,
        surviving_edges,
        vertex_remap,
        edge_remap,
        resolver,
        hyper_resolver: HashMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::three_node_schema;

    #[test]
    fn rename_field_updates_edge_label() {
        let schema = three_node_schema();
        let result = apply_rename(&schema, "text", "content");
        assert!(result.is_ok(), "rename should succeed");

        let new_schema = result.unwrap_or_else(|e| panic!("rename failed: {e}"));
        let has_old = new_schema
            .edges
            .keys()
            .any(|e| e.name.as_deref() == Some("text"));
        let has_new = new_schema
            .edges
            .keys()
            .any(|e| e.name.as_deref() == Some("content"));
        assert!(!has_old, "old name should be gone");
        assert!(has_new, "new name should be present");
    }

    #[test]
    fn add_field_creates_vertex_and_edge() {
        let schema = three_node_schema();
        let result = apply_add_field(&schema, "likes", "integer");
        assert!(result.is_ok(), "add_field should succeed");

        let new_schema = result.unwrap_or_else(|e| panic!("add_field failed: {e}"));
        assert!(
            new_schema.vertices.values().any(|v| v.id.contains("likes")),
            "new vertex should exist"
        );
        assert!(
            new_schema
                .edges
                .keys()
                .any(|e| e.name.as_deref() == Some("likes")),
            "new edge should exist"
        );
    }

    #[test]
    fn remove_field_drops_vertex_and_edge() {
        let schema = three_node_schema();
        let result = apply_remove_field(&schema, "text");
        assert!(result.is_ok(), "remove_field should succeed");

        let new_schema = result.unwrap_or_else(|e| panic!("remove_field failed: {e}"));
        assert_eq!(
            new_schema.vertex_count(),
            schema.vertex_count() - 1,
            "one vertex should be removed"
        );
    }

    #[test]
    fn rename_nonexistent_field_fails() {
        let schema = three_node_schema();
        let result = apply_rename(&schema, "nonexistent", "new_name");
        assert!(result.is_err(), "renaming nonexistent field should fail");
    }

    #[test]
    fn coerce_type_changes_vertex_kind() {
        let schema = three_node_schema();
        let result = apply_coerce_type(&schema, "string", "text");
        assert!(result.is_ok(), "coerce should succeed");

        let new_schema = result.unwrap_or_else(|e| panic!("coerce failed: {e}"));
        assert!(
            new_schema.vertices.values().all(|v| v.kind != "string"),
            "no vertices should have old kind"
        );
    }
}
