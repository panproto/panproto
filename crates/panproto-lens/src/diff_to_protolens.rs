//! Convert schema diffs into protolens chains.
//!
//! This module defines [`DiffSpec`], a lightweight representation of the
//! diff fields relevant to protolens construction, and a conversion
//! function that maps each diff element to one or more elementary
//! protolenses.
//!
//! Ordering: drops (edges then vertices) followed by renames followed by
//! adds (vertices then edges).
//!
//! The [`DiffSpec`] type mirrors the relevant fields of
//! `panproto_check::SchemaDiff` without introducing a cyclic dependency.
//! Callers in higher-level crates can construct a [`DiffSpec`] from their
//! own diff structures.

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_inst::value::Value;
use panproto_schema::{Edge, Protocol, Schema};
use serde::{Deserialize, Serialize};

use crate::Lens;
use crate::error::LensError;
use crate::protolens::{Protolens, ProtolensChain, elementary};

/// A kind change for a single vertex.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KindChange {
    /// The vertex ID.
    pub vertex_id: String,
    /// The kind in the old schema.
    pub old_kind: String,
    /// The kind in the new schema.
    pub new_kind: String,
}

/// Lightweight diff specification for protolens construction.
///
/// Contains only the structural diff fields that map to elementary
/// protolenses. This type mirrors the relevant fields of
/// `panproto_check::SchemaDiff` and can be constructed from it by
/// higher-level crates.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffSpec {
    /// Vertex IDs present in the new schema but absent from the old.
    pub added_vertices: Vec<String>,
    /// Vertex IDs present in the old schema but absent from the new.
    pub removed_vertices: Vec<String>,
    /// Vertices whose kind changed between old and new.
    pub kind_changes: Vec<KindChange>,
    /// Edges present in the new schema but absent from the old.
    pub added_edges: Vec<Edge>,
    /// Edges present in the old schema but absent from the new.
    pub removed_edges: Vec<Edge>,
}

/// Convert a [`DiffSpec`] into a [`ProtolensChain`].
///
/// Each element of the diff maps to one or more elementary protolenses.
/// Ordering: drops (edges then vertices) followed by renames followed by
/// adds (vertices then edges).
///
/// # Errors
///
/// Returns [`LensError`] if any protolens step cannot be constructed.
/// Currently infallible but returns `Result` for forward compatibility.
pub fn diff_to_protolens(
    diff: &DiffSpec,
    old_schema: &Schema,
    new_schema: &Schema,
) -> Result<ProtolensChain, LensError> {
    let mut steps: Vec<Protolens> = Vec::new();

    // Phase 1: Drops (edges first, then vertices)
    for edge in &diff.removed_edges {
        steps.push(elementary::drop_op(Name::from(&*edge.kind)));
    }
    for vertex_id in &diff.removed_vertices {
        // Get the vertex kind from the old schema
        if let Some(vertex) = old_schema.vertices.get(vertex_id.as_str()) {
            steps.push(elementary::drop_sort(Name::from(&*vertex.kind)));
        }
    }

    // Phase 2: Kind changes (rename sorts)
    for change in &diff.kind_changes {
        steps.push(elementary::rename_sort(
            Name::from(change.old_kind.as_str()),
            Name::from(change.new_kind.as_str()),
        ));
    }

    // Phase 3: Adds (vertices first, then edges)
    for vertex_id in &diff.added_vertices {
        if let Some(vertex) = new_schema.vertices.get(vertex_id.as_str()) {
            steps.push(elementary::add_sort(
                Name::from(&*vertex.kind),
                Name::from(&*vertex.kind),
                Value::Null,
            ));
        }
    }
    for edge in &diff.added_edges {
        steps.push(elementary::add_op(
            Name::from(&*edge.kind),
            Name::from(&*edge.src),
            Name::from(&*edge.tgt),
            Name::from(&*edge.kind),
        ));
    }

    Ok(ProtolensChain::new(steps))
}

/// Convert a [`DiffSpec`] directly into a concrete [`Lens`].
///
/// Builds the protolens chain via [`diff_to_protolens`] and then
/// instantiates it at `old_schema`.
///
/// # Errors
///
/// Returns [`LensError`] if the protolens chain cannot be built or
/// if instantiation at `old_schema` fails.
pub fn diff_to_lens(
    diff: &DiffSpec,
    old_schema: &Schema,
    new_schema: &Schema,
    protocol: &Protocol,
    _defaults: &HashMap<Name, Value>,
) -> Result<Lens, LensError> {
    let chain = diff_to_protolens(diff, old_schema, new_schema)?;
    chain.instantiate(old_schema, protocol)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use panproto_schema::{Protocol, SchemaBuilder};

    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["record".into(), "string".into(), "boolean".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    fn base_schema(protocol: &Protocol) -> Schema {
        SchemaBuilder::new(protocol)
            .vertex("root", "record", None::<&str>)
            .unwrap()
            .vertex("root.name", "string", None::<&str>)
            .unwrap()
            .edge("root", "root.name", "prop", Some("name"))
            .unwrap()
            .build()
            .unwrap()
    }

    fn extended_schema(protocol: &Protocol) -> Schema {
        SchemaBuilder::new(protocol)
            .vertex("root", "record", None::<&str>)
            .unwrap()
            .vertex("root.name", "string", None::<&str>)
            .unwrap()
            .vertex("root.active", "boolean", None::<&str>)
            .unwrap()
            .edge("root", "root.name", "prop", Some("name"))
            .unwrap()
            .edge("root", "root.active", "prop", Some("active"))
            .unwrap()
            .build()
            .unwrap()
    }

    /// Build a `DiffSpec` by comparing two schemas manually.
    fn compute_diff(old: &Schema, new: &Schema) -> DiffSpec {
        let added_vertices: Vec<String> = new
            .vertices
            .keys()
            .filter(|k| !old.vertices.contains_key(*k))
            .map(ToString::to_string)
            .collect();
        let removed_vertices: Vec<String> = old
            .vertices
            .keys()
            .filter(|k| !new.vertices.contains_key(*k))
            .map(ToString::to_string)
            .collect();
        let kind_changes: Vec<KindChange> = old
            .vertices
            .iter()
            .filter_map(|(id, v)| {
                new.vertices.get(id).and_then(|nv| {
                    if v.kind == nv.kind {
                        None
                    } else {
                        Some(KindChange {
                            vertex_id: id.to_string(),
                            old_kind: v.kind.to_string(),
                            new_kind: nv.kind.to_string(),
                        })
                    }
                })
            })
            .collect();
        let added_edges: Vec<Edge> = new
            .edges
            .keys()
            .filter(|e| !old.edges.contains_key(*e))
            .cloned()
            .collect();
        let removed_edges: Vec<Edge> = old
            .edges
            .keys()
            .filter(|e| !new.edges.contains_key(*e))
            .cloned()
            .collect();

        DiffSpec {
            added_vertices,
            removed_vertices,
            kind_changes,
            added_edges,
            removed_edges,
        }
    }

    #[test]
    fn empty_diff_empty_chain() {
        let protocol = test_protocol();
        let s = base_schema(&protocol);
        let d = compute_diff(&s, &s);
        let chain = diff_to_protolens(&d, &s, &s).unwrap();
        assert!(chain.is_empty());
    }

    #[test]
    fn added_vertex_produces_add_sort() {
        let protocol = test_protocol();
        let old = base_schema(&protocol);
        let new = extended_schema(&protocol);
        let d = compute_diff(&old, &new);
        let chain = diff_to_protolens(&d, &old, &new).unwrap();
        assert!(!chain.is_empty());
        // Should have at least one add step
        let has_add = chain.steps.iter().any(|s| s.name.contains("add"));
        assert!(has_add, "should have an add step");
    }

    #[test]
    fn removed_vertex_produces_drop_sort() {
        let protocol = test_protocol();
        let old = extended_schema(&protocol);
        let new = base_schema(&protocol);
        let d = compute_diff(&old, &new);
        let chain = diff_to_protolens(&d, &old, &new).unwrap();
        assert!(!chain.is_empty());
        let has_drop = chain.steps.iter().any(|s| s.name.contains("drop"));
        assert!(has_drop, "should have a drop step");
    }
}
