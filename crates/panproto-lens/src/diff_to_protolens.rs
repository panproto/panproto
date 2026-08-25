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
use std::sync::Arc;

use panproto_gat::Name;
use panproto_inst::value::Value;
use panproto_schema::{Edge, Protocol, Schema};
use serde::{Deserialize, Serialize};

use crate::Lens;
use crate::error::LensError;
use crate::protolens::{
    Protolens, ProtolensChain, apply_add_edge_to_schema, apply_add_schema_vertex,
    apply_change_schema_vertex_kind, apply_drop_edge_from_schema, apply_drop_schema_vertex,
    elementary, rebuild_indices,
};

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
    /// Vertices whose kind changed between old and new. The target schema
    /// must register a value coercion for every old/new kind pair.
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
/// Returns [`LensError`] if a diff element disagrees with the supplied
/// schemas, a kind change has no registered coercion, the target has stale
/// adjacency indices, or the target changes metadata that [`DiffSpec`] cannot
/// represent exactly.
pub fn diff_to_protolens(
    diff: &DiffSpec,
    old_schema: &Schema,
    new_schema: &Schema,
) -> Result<ProtolensChain, LensError> {
    diff_to_protolens_with_defaults(diff, old_schema, new_schema, &HashMap::new())
}

fn diff_to_protolens_with_defaults(
    diff: &DiffSpec,
    old_schema: &Schema,
    new_schema: &Schema,
    defaults: &HashMap<Name, Value>,
) -> Result<ProtolensChain, LensError> {
    let mut steps: Vec<Protolens> = Vec::new();

    // Phase 1: Drops (edges first, then vertices). Diff elements identify
    // concrete schema objects, so use fiber-level operations rather than
    // theory kind operations that could affect unrelated objects.
    for edge in &diff.removed_edges {
        if !old_schema.edges.contains_key(edge) {
            return Err(LensError::ProtolensError(format!(
                "diff removes edge `{edge:?}`, but the old schema does not contain it"
            )));
        }
        steps.push(elementary::drop_schema_edge(edge));
    }
    for vertex_id in &diff.removed_vertices {
        old_schema.vertices.get(vertex_id.as_str()).ok_or_else(|| {
            LensError::ProtolensError(format!(
                "diff removes vertex `{vertex_id}`, but the old schema does not contain it"
            ))
        })?;
        steps.push(elementary::drop_schema_vertex(Name::from(
            vertex_id.as_str(),
        )));
    }

    // Phase 2: Kind changes target one vertex id, not every vertex sharing
    // the old theory sort.
    for change in &diff.kind_changes {
        let old_vertex = old_schema
            .vertices
            .get(change.vertex_id.as_str())
            .ok_or_else(|| {
                LensError::ProtolensError(format!(
                    "diff changes vertex `{}`, but the old schema does not contain it",
                    change.vertex_id
                ))
            })?;
        let new_vertex = new_schema
            .vertices
            .get(change.vertex_id.as_str())
            .ok_or_else(|| {
                LensError::ProtolensError(format!(
                    "diff changes vertex `{}`, but the new schema does not contain it",
                    change.vertex_id
                ))
            })?;
        if old_vertex.kind.as_ref() != change.old_kind
            || new_vertex.kind.as_ref() != change.new_kind
        {
            return Err(LensError::ProtolensError(format!(
                "kind change for vertex `{}` does not match the old/new schemas",
                change.vertex_id
            )));
        }
        if !new_schema.coercions.contains_key(&(
            Name::from(change.old_kind.as_str()),
            Name::from(change.new_kind.as_str()),
        )) {
            return Err(LensError::ProtolensError(format!(
                "kind change for vertex `{}` from `{}` to `{}` has no registered coercion",
                change.vertex_id, change.old_kind, change.new_kind,
            )));
        }
        steps.push(elementary::change_schema_vertex_kind(
            Name::from(change.vertex_id.as_str()),
            Name::from(change.old_kind.as_str()),
            Name::from(change.new_kind.as_str()),
        ));
    }

    // Phase 3: Adds (vertices first, then edges)
    for vertex_id in &diff.added_vertices {
        let vertex = new_schema.vertices.get(vertex_id.as_str()).ok_or_else(|| {
            LensError::ProtolensError(format!(
                "diff adds vertex `{vertex_id}`, but the new schema does not contain it"
            ))
        })?;
        let edge_defaults: Vec<_> = new_schema
            .incoming_edges(vertex_id)
            .iter()
            .filter_map(|edge| edge.name.as_ref().and_then(|name| defaults.get(name)))
            .collect();
        let default = defaults
            .get(vertex_id.as_str())
            .or_else(|| (edge_defaults.len() == 1).then(|| edge_defaults[0]))
            .or_else(|| defaults.get(&vertex.kind))
            .cloned();
        steps.push(elementary::add_schema_vertex(
            vertex.id.clone(),
            vertex.kind.clone(),
            vertex.nsid.as_ref(),
            new_schema.entries.contains(&vertex.id),
            default,
        ));
    }
    for edge in &diff.added_edges {
        if !new_schema.edges.contains_key(edge) {
            return Err(LensError::ProtolensError(format!(
                "diff adds edge `{edge:?}`, but the new schema does not contain it"
            )));
        }
        steps.push(elementary::add_schema_edge(edge));
    }

    validate_exact_target(diff, old_schema, new_schema)?;

    Ok(ProtolensChain::new(steps))
}

/// Apply the supported structural operations with the same implementation used
/// by protolens instantiation, then compare every schema field. This prevents a
/// structural `DiffSpec` from silently dropping or inventing metadata that it
/// cannot represent.
fn validate_exact_target(
    diff: &DiffSpec,
    old_schema: &Schema,
    new_schema: &Schema,
) -> Result<(), LensError> {
    let mut predicted = old_schema.clone();

    for edge in &diff.removed_edges {
        let src = Arc::<str>::from(edge.src.as_str());
        let tgt = Arc::<str>::from(edge.tgt.as_str());
        let name = edge
            .name
            .as_ref()
            .map(|name| Arc::<str>::from(name.as_str()));
        let kind = Arc::<str>::from(edge.kind.as_str());
        predicted = apply_drop_edge_from_schema(&predicted, &src, &tgt, name.as_ref(), Some(&kind));
    }
    for vertex_id in &diff.removed_vertices {
        predicted = apply_drop_schema_vertex(&predicted, &Arc::<str>::from(vertex_id.as_str()));
    }
    for change in &diff.kind_changes {
        predicted = apply_change_schema_vertex_kind(
            &predicted,
            &Arc::<str>::from(change.vertex_id.as_str()),
            &Arc::<str>::from(change.old_kind.as_str()),
            &Arc::<str>::from(change.new_kind.as_str()),
        );
    }
    for vertex_id in &diff.added_vertices {
        let vertex = &new_schema.vertices[vertex_id.as_str()];
        let nsid = vertex
            .nsid
            .as_ref()
            .map(|nsid| Arc::<str>::from(nsid.as_str()));
        predicted = apply_add_schema_vertex(
            &predicted,
            &Arc::<str>::from(vertex.id.as_str()),
            &Arc::<str>::from(vertex.kind.as_str()),
            nsid.as_ref(),
            new_schema.entries.contains(&vertex.id),
        );
    }
    for edge in &diff.added_edges {
        let src = Arc::<str>::from(edge.src.as_str());
        let tgt = Arc::<str>::from(edge.tgt.as_str());
        let name = edge
            .name
            .as_ref()
            .map(|name| Arc::<str>::from(name.as_str()));
        let kind = Arc::<str>::from(edge.kind.as_str());
        predicted = apply_add_edge_to_schema(&predicted, &src, &tgt, name.as_ref(), &kind);
    }

    macro_rules! require_equal {
        ($field:ident) => {
            if predicted.$field != new_schema.$field {
                return Err(LensError::ProtolensError(format!(
                    "diff cannot reproduce target schema field `{}` exactly; DiffSpec does not describe that metadata change",
                    stringify!($field),
                )));
            }
        };
    }

    require_equal!(protocol);
    require_equal!(vertices);
    require_equal!(edges);
    require_equal!(hyper_edges);
    require_equal!(constraints);
    require_equal!(required);
    require_equal!(nsids);
    require_equal!(entries);
    require_equal!(variants);
    require_equal!(orderings);
    require_equal!(recursion_points);
    require_equal!(spans);
    require_equal!(usage_modes);
    require_equal!(nominal);
    require_equal!(coercions);
    require_equal!(mergers);
    require_equal!(defaults);
    require_equal!(policies);

    let mut canonical_target = new_schema.clone();
    rebuild_indices(&mut canonical_target);
    if !same_index4(&new_schema.outgoing, &canonical_target.outgoing) {
        return Err(LensError::ProtolensError(
            "target schema has stale `outgoing` adjacency metadata".into(),
        ));
    }
    if !same_index4(&new_schema.incoming, &canonical_target.incoming) {
        return Err(LensError::ProtolensError(
            "target schema has stale `incoming` adjacency metadata".into(),
        ));
    }
    if !same_index2(&new_schema.between, &canonical_target.between) {
        return Err(LensError::ProtolensError(
            "target schema has stale `between` adjacency metadata".into(),
        ));
    }

    Ok(())
}

fn same_index4(
    left: &HashMap<Name, smallvec::SmallVec<Edge, 4>>,
    right: &HashMap<Name, smallvec::SmallVec<Edge, 4>>,
) -> bool {
    left.len() == right.len()
        && left.iter().all(|(key, edges)| {
            let Some(other) = right.get(key) else {
                return false;
            };
            let mut edges = edges.to_vec();
            let mut other = other.to_vec();
            edges.sort();
            other.sort();
            edges == other
        })
}

fn same_index2(
    left: &HashMap<(Name, Name), smallvec::SmallVec<Edge, 2>>,
    right: &HashMap<(Name, Name), smallvec::SmallVec<Edge, 2>>,
) -> bool {
    left.len() == right.len()
        && left.iter().all(|(key, edges)| {
            let Some(other) = right.get(key) else {
                return false;
            };
            let mut edges = edges.to_vec();
            let mut other = other.to_vec();
            edges.sort();
            other.sort();
            edges == other
        })
}

/// Convert a [`DiffSpec`] directly into a concrete [`Lens`].
///
/// Builds the protolens chain via [`diff_to_protolens`] and then
/// instantiates it at `old_schema`. Each default key resolves against an
/// added target vertex id first, then a unique incoming edge label, and
/// finally the target vertex kind. An omitted default stays absent; an
/// explicitly supplied [`Value::Null`] is installed as a null value.
///
/// # Errors
///
/// Returns [`LensError`] if the protolens chain cannot be built, if
/// instantiation at `old_schema` fails, or if a supplied default does not
/// identify a uniquely placeable added field.
pub fn diff_to_lens(
    diff: &DiffSpec,
    old_schema: &Schema,
    new_schema: &Schema,
    protocol: &Protocol,
    defaults: &HashMap<Name, Value>,
) -> Result<Lens, LensError> {
    let chain = diff_to_protolens_with_defaults(diff, old_schema, new_schema, defaults)?;
    let mut lens = chain.instantiate(old_schema, protocol)?;
    crate::default_synthesis::attach_defaults(&mut lens, new_schema, defaults, true)?;
    Ok(lens)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use panproto_inst::metadata::Node;
    use panproto_inst::value::FieldPresence;
    use panproto_inst::{FieldTransform, WInstance};
    use panproto_schema::{Protocol, SchemaBuilder, UsageMode};

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

    fn assert_schema_eq(actual: &Schema, expected: &Schema) {
        fn without_derived_indices(schema: &Schema) -> serde_json::Value {
            let mut value = serde_json::to_value(schema).unwrap();
            let object = value.as_object_mut().unwrap();
            object.remove("outgoing");
            object.remove("incoming");
            object.remove("between");
            value
        }

        assert_eq!(
            without_derived_indices(actual),
            without_derived_indices(expected),
            "instantiated protolens target must reproduce the diff target schema"
        );
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
    fn diff_to_lens_applies_defaults_in_get_and_public_lift() {
        let protocol = test_protocol();
        let old = base_schema(&protocol);
        let new = extended_schema(&protocol);
        let d = compute_diff(&old, &new);
        let defaults = HashMap::from([(Name::from("boolean"), Value::Bool(true))]);
        let lens = diff_to_lens(&d, &old, &new, &protocol, &defaults).unwrap();
        let source =
            panproto_inst::parse_json(&old, "root", &serde_json::json!({"name": "Ada"})).unwrap();

        let (view, _) = crate::get(&lens, &source).unwrap();
        assert_eq!(
            panproto_inst::to_json(&new, &view),
            serde_json::json!({"active": true, "name": "Ada"}),
            "diff_to_lens must compile the default into executable get behavior"
        );

        let lifted = panproto_mig::lift_wtype(&lens.compiled, &old, &new, &source).unwrap();
        assert_eq!(
            panproto_inst::to_json(&new, &lifted),
            serde_json::json!({"active": true, "name": "Ada"}),
            "the public WInstance lift path must execute the same compiled default"
        );
    }

    #[test]
    fn absent_default_does_not_become_an_explicit_null() {
        let protocol = test_protocol();
        let old = base_schema(&protocol);
        let new = extended_schema(&protocol);
        let diff = compute_diff(&old, &new);
        let lens = diff_to_protolens(&diff, &old, &new)
            .unwrap()
            .instantiate(&old, &protocol)
            .unwrap();

        assert!(lens.compiled.field_transforms.values().all(|transforms| {
            transforms.iter().all(|transform| {
                !matches!(transform, FieldTransform::AddField { key, .. } if key == "active")
            })
        }));
        let source =
            panproto_inst::parse_json(&old, "root", &serde_json::json!({"name": "Ada"})).unwrap();
        let (view, _) = crate::get(&lens, &source).unwrap();
        assert_eq!(
            panproto_inst::to_json(&new, &view),
            serde_json::json!({"name": "Ada"})
        );
    }

    #[test]
    fn explicit_null_default_remains_executable() {
        let protocol = test_protocol();
        let old = base_schema(&protocol);
        let new = extended_schema(&protocol);
        let diff = compute_diff(&old, &new);
        let defaults = HashMap::from([(Name::from("root.active"), Value::Null)]);
        let lens = diff_to_lens(&diff, &old, &new, &protocol, &defaults).unwrap();
        let source =
            panproto_inst::parse_json(&old, "root", &serde_json::json!({"name": "Ada"})).unwrap();
        let (view, _) = crate::get(&lens, &source).unwrap();

        assert_eq!(
            panproto_inst::to_json(&new, &view),
            serde_json::json!({"active": null, "name": "Ada"})
        );
    }

    #[test]
    fn diff_to_lens_rejects_an_unplaceable_default() {
        let protocol = test_protocol();
        let old = base_schema(&protocol);
        let new = SchemaBuilder::new(&protocol)
            .vertex("root", "record", None::<&str>)
            .unwrap()
            .vertex("root.name", "string", None::<&str>)
            .unwrap()
            .vertex("root.active", "boolean", None::<&str>)
            .unwrap()
            .edge("root", "root.name", "prop", Some("name"))
            .unwrap()
            .build()
            .unwrap();
        let d = compute_diff(&old, &new);
        let defaults = HashMap::from([(Name::from("boolean"), Value::Bool(true))]);

        let error = diff_to_lens(&d, &old, &new, &protocol, &defaults).unwrap_err();
        assert!(
            error.to_string().contains("cannot be placed"),
            "unplaceable defaults must be rejected, not retained as inert metadata: {error}"
        );
    }

    #[test]
    fn diff_to_lens_rejects_a_default_with_the_wrong_value_kind() {
        let protocol = test_protocol();
        let old = base_schema(&protocol);
        let new = extended_schema(&protocol);
        let d = compute_diff(&old, &new);
        let defaults = HashMap::from([(Name::from("root.active"), Value::Str("true".to_owned()))]);

        let error = diff_to_lens(&d, &old, &new, &protocol, &defaults).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("value kind `str`"), "{message}");
        assert!(
            message.contains("target vertex kind is `boolean`"),
            "{message}"
        );
    }

    #[test]
    fn added_same_kind_vertices_and_edges_instantiate_exactly_and_get_defaults() {
        let protocol = test_protocol();
        let old = base_schema(&protocol);
        let new = SchemaBuilder::new(&protocol)
            .vertex("root", "record", None::<&str>)
            .unwrap()
            .vertex("root.name", "string", None::<&str>)
            .unwrap()
            .vertex("root.active", "boolean", None::<&str>)
            .unwrap()
            .vertex("root.archived", "boolean", None::<&str>)
            .unwrap()
            .edge("root", "root.name", "prop", Some("name"))
            .unwrap()
            .edge("root", "root.active", "prop", Some("active"))
            .unwrap()
            .edge("root", "root.archived", "prop", Some("archived"))
            .unwrap()
            .build()
            .unwrap();
        let diff = compute_diff(&old, &new);
        let defaults = HashMap::from([
            (Name::from("root.active"), Value::Bool(true)),
            (Name::from("root.archived"), Value::Bool(false)),
        ]);

        let lens = diff_to_lens(&diff, &old, &new, &protocol, &defaults).unwrap();
        assert_schema_eq(&lens.tgt_schema, &new);

        let source =
            panproto_inst::parse_json(&old, "root", &serde_json::json!({"name": "Ada"})).unwrap();
        let (view, _) = crate::get(&lens, &source).unwrap();
        assert_eq!(
            panproto_inst::to_json(&new, &view),
            serde_json::json!({"active": true, "archived": false, "name": "Ada"})
        );
    }

    #[test]
    fn edge_label_default_must_identify_one_added_vertex_globally() {
        let protocol = test_protocol();
        let old = SchemaBuilder::new(&protocol)
            .vertex("left", "record", None::<&str>)
            .unwrap()
            .vertex("right", "record", None::<&str>)
            .unwrap()
            .build()
            .unwrap();
        let new = SchemaBuilder::new(&protocol)
            .vertex("left", "record", None::<&str>)
            .unwrap()
            .vertex("right", "record", None::<&str>)
            .unwrap()
            .vertex("left.value", "string", None::<&str>)
            .unwrap()
            .vertex("right.value", "string", None::<&str>)
            .unwrap()
            .edge("left", "left.value", "prop", Some("value"))
            .unwrap()
            .edge("right", "right.value", "prop", Some("value"))
            .unwrap()
            .build()
            .unwrap();
        let diff = compute_diff(&old, &new);
        let defaults = HashMap::from([(Name::from("value"), Value::Str("x".to_owned()))]);

        let error = diff_to_lens(&diff, &old, &new, &protocol, &defaults).unwrap_err();
        assert!(error.to_string().contains("another added target vertex"));
    }

    #[test]
    fn removed_elements_cascade_supported_metadata_exactly() {
        let protocol = test_protocol();
        let mut old = extended_schema(&protocol);
        let active_edge = old
            .edges
            .keys()
            .find(|edge| edge.name.as_deref() == Some("active"))
            .unwrap()
            .clone();
        old.required
            .insert(Name::from("root"), vec![active_edge.clone()]);
        old.orderings.insert(active_edge.clone(), 0);
        old.usage_modes.insert(active_edge, UsageMode::Affine);
        old.nominal.insert(Name::from("root.active"), true);
        let new = base_schema(&protocol);
        let diff = compute_diff(&old, &new);

        let lens = diff_to_protolens(&diff, &old, &new)
            .unwrap()
            .instantiate(&old, &protocol)
            .unwrap();
        assert_schema_eq(&lens.tgt_schema, &new);
    }

    #[test]
    fn removing_one_parallel_edge_does_not_relabel_its_arc() {
        let protocol = test_protocol();
        let old = SchemaBuilder::new(&protocol)
            .vertex("root", "record", None::<&str>)
            .unwrap()
            .vertex("leaf", "string", None::<&str>)
            .unwrap()
            .edge("root", "leaf", "prop", Some("a"))
            .unwrap()
            .edge("root", "leaf", "prop", Some("b"))
            .unwrap()
            .build()
            .unwrap();
        let new = SchemaBuilder::new(&protocol)
            .vertex("root", "record", None::<&str>)
            .unwrap()
            .vertex("leaf", "string", None::<&str>)
            .unwrap()
            .edge("root", "leaf", "prop", Some("a"))
            .unwrap()
            .build()
            .unwrap();
        let edge_a = old
            .edges
            .keys()
            .find(|edge| edge.name.as_deref() == Some("a"))
            .unwrap()
            .clone();
        let edge_b = old
            .edges
            .keys()
            .find(|edge| edge.name.as_deref() == Some("b"))
            .unwrap()
            .clone();
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "root"));
        nodes.insert(
            1,
            Node::new(1, "leaf").with_value(FieldPresence::Present(Value::Str("A".to_owned()))),
        );
        nodes.insert(
            2,
            Node::new(2, "leaf").with_value(FieldPresence::Present(Value::Str("B".to_owned()))),
        );
        let source = WInstance::new(
            nodes,
            vec![(0, 1, edge_a.clone()), (0, 2, edge_b)],
            vec![],
            0,
            Name::from("root"),
        );
        let diff = compute_diff(&old, &new);
        let lens = diff_to_protolens(&diff, &old, &new)
            .unwrap()
            .instantiate(&old, &protocol)
            .unwrap();

        let (view, complement) = crate::get(&lens, &source).unwrap();
        assert!(view.nodes.contains_key(&1));
        assert!(!view.nodes.contains_key(&2));
        assert_eq!(view.arcs, vec![(0, 1, edge_a)]);
        let restored = crate::put(&lens, &view, &complement).unwrap();
        assert!(crate::laws::instances_equivalent(&restored, &source));
    }

    #[test]
    fn unsupported_added_edge_metadata_is_rejected_precisely() {
        let protocol = test_protocol();
        let old = base_schema(&protocol);
        let mut new = extended_schema(&protocol);
        let active_edge = new
            .edges
            .keys()
            .find(|edge| edge.name.as_deref() == Some("active"))
            .unwrap()
            .clone();
        new.usage_modes.insert(active_edge, UsageMode::Linear);
        let diff = compute_diff(&old, &new);

        let error = diff_to_protolens(&diff, &old, &new).unwrap_err();
        assert!(error.to_string().contains("`usage_modes`"));
    }

    #[test]
    fn target_entry_order_must_be_reproducible() {
        let protocol = test_protocol();
        let old = SchemaBuilder::new(&protocol)
            .vertex("root", "record", None::<&str>)
            .unwrap()
            .entry("root")
            .build()
            .unwrap();
        let new = SchemaBuilder::new(&protocol)
            .vertex("root", "record", None::<&str>)
            .unwrap()
            .vertex("other", "record", None::<&str>)
            .unwrap()
            .entry("other")
            .entry("root")
            .build()
            .unwrap();
        let diff = compute_diff(&old, &new);

        let error = diff_to_protolens(&diff, &old, &new).unwrap_err();
        assert!(error.to_string().contains("`entries`"));
    }

    #[test]
    fn stale_target_adjacency_is_rejected() {
        let protocol = test_protocol();
        let old = base_schema(&protocol);
        let mut new = extended_schema(&protocol);
        new.incoming.clear();
        let diff = compute_diff(&old, &new);

        let error = diff_to_protolens(&diff, &old, &new).unwrap_err();
        assert!(error.to_string().contains("stale `incoming`"));
    }

    #[test]
    fn removed_vertex_and_one_of_several_same_kind_edges_instantiate_exactly() {
        let protocol = test_protocol();
        let new = base_schema(&protocol);
        let old = SchemaBuilder::new(&protocol)
            .vertex("root", "record", None::<&str>)
            .unwrap()
            .vertex("root.name", "string", None::<&str>)
            .unwrap()
            .vertex("root.alias", "string", None::<&str>)
            .unwrap()
            .edge("root", "root.name", "prop", Some("name"))
            .unwrap()
            .edge("root", "root.alias", "prop", Some("alias"))
            .unwrap()
            .build()
            .unwrap();
        let diff = compute_diff(&old, &new);

        let lens = diff_to_protolens(&diff, &old, &new)
            .unwrap()
            .instantiate(&old, &protocol)
            .unwrap();
        assert_schema_eq(&lens.tgt_schema, &new);
    }

    #[test]
    fn vertex_kind_change_without_registered_coercion_is_rejected() {
        let protocol = test_protocol();
        let old = SchemaBuilder::new(&protocol)
            .vertex("root", "record", None::<&str>)
            .unwrap()
            .vertex("root.value", "string", None::<&str>)
            .unwrap()
            .vertex("root.untouched", "string", None::<&str>)
            .unwrap()
            .edge("root", "root.value", "prop", Some("value"))
            .unwrap()
            .edge("root", "root.untouched", "prop", Some("untouched"))
            .unwrap()
            .build()
            .unwrap();
        let new = SchemaBuilder::new(&protocol)
            .vertex("root", "record", None::<&str>)
            .unwrap()
            .vertex("root.value", "boolean", None::<&str>)
            .unwrap()
            .vertex("root.untouched", "string", None::<&str>)
            .unwrap()
            .edge("root", "root.value", "prop", Some("value"))
            .unwrap()
            .edge("root", "root.untouched", "prop", Some("untouched"))
            .unwrap()
            .build()
            .unwrap();
        let diff = compute_diff(&old, &new);

        let error = diff_to_protolens(&diff, &old, &new).unwrap_err();
        assert!(error.to_string().contains("has no registered coercion"));
    }

    #[test]
    fn vertex_kind_change_installs_registered_value_coercion() {
        let protocol = test_protocol();
        let mut old = SchemaBuilder::new(&protocol)
            .vertex("root", "record", None::<&str>)
            .unwrap()
            .vertex("root.value", "string", None::<&str>)
            .unwrap()
            .vertex("root.untouched", "string", None::<&str>)
            .unwrap()
            .edge("root", "root.value", "prop", Some("value"))
            .unwrap()
            .edge("root", "root.untouched", "prop", Some("untouched"))
            .unwrap()
            .build()
            .unwrap();
        let mut new = SchemaBuilder::new(&protocol)
            .vertex("root", "record", None::<&str>)
            .unwrap()
            .vertex("root.value", "boolean", None::<&str>)
            .unwrap()
            .vertex("root.untouched", "string", None::<&str>)
            .unwrap()
            .edge("root", "root.value", "prop", Some("value"))
            .unwrap()
            .edge("root", "root.untouched", "prop", Some("untouched"))
            .unwrap()
            .build()
            .unwrap();
        let coercion = panproto_schema::CoercionSpec {
            forward: panproto_expr::Expr::Lit(panproto_expr::Literal::Bool(true)),
            inverse: None,
            class: panproto_gat::CoercionClass::Opaque,
        };
        let coercion_key = (Name::from("string"), Name::from("boolean"));
        old.coercions.insert(coercion_key.clone(), coercion.clone());
        new.coercions.insert(coercion_key, coercion);
        let diff = compute_diff(&old, &new);
        let lens = diff_to_protolens(&diff, &old, &new)
            .unwrap()
            .instantiate(&old, &protocol)
            .unwrap();
        assert_schema_eq(&lens.tgt_schema, &new);
        assert_eq!(
            lens.tgt_schema.vertex("root.untouched").unwrap().kind,
            "string"
        );

        let source = panproto_inst::parse_json(
            &old,
            "root",
            &serde_json::json!({"untouched": "same", "value": "yes"}),
        )
        .unwrap();
        let (view, complement) = crate::get(&lens, &source).unwrap();
        assert_eq!(
            panproto_inst::to_json(&new, &view),
            serde_json::json!({"untouched": "same", "value": true})
        );
        let restored = crate::put(&lens, &view, &complement).unwrap();
        assert!(crate::laws::instances_equivalent(&restored, &source));
    }

    #[test]
    fn unnamed_edge_survives_protolens_serde_and_instantiation() {
        let protocol = test_protocol();
        let old = base_schema(&protocol);
        let new = SchemaBuilder::new(&protocol)
            .vertex("root", "record", None::<&str>)
            .unwrap()
            .vertex("root.name", "string", None::<&str>)
            .unwrap()
            .vertex("root.flag", "boolean", None::<&str>)
            .unwrap()
            .edge("root", "root.name", "prop", Some("name"))
            .unwrap()
            .edge("root", "root.flag", "prop", None::<&str>)
            .unwrap()
            .build()
            .unwrap();
        let diff = compute_diff(&old, &new);
        let chain = diff_to_protolens(&diff, &old, &new).unwrap();
        let encoded = serde_json::to_string(&chain).unwrap();
        let decoded: ProtolensChain = serde_json::from_str(&encoded).unwrap();

        let lens = decoded.instantiate(&old, &protocol).unwrap();
        assert_schema_eq(&lens.tgt_schema, &new);
        assert!(
            lens.tgt_schema.edges.keys().any(|edge| {
                edge.tgt == "root.flag" && edge.kind == "prop" && edge.name.is_none()
            })
        );
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
