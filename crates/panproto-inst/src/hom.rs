//! Internal hom schema construction.
//!
//! The internal hom `[S, T]` constructs a schema whose instances represent
//! lenses (structure-preserving maps) from schema `S` to schema `T`.
//!
//! This module provides three operations:
//!
//! 1. `hom_schema(S, T)`: build the schema `[S, T]`
//! 2. `curry_migration(m, S, [S,T])`: encode a compiled migration as an instance of `[S, T]`
//! 3. `eval_hom(h, x, S, T)`: given an instance `h` of `[S, T]` and an instance `x` of `S`,
//!    produce an instance of `T`

use std::collections::{HashMap, HashSet};

use panproto_gat::Name;
use panproto_schema::{Edge, Schema, Vertex};
use smallvec::SmallVec;

use crate::error::InstError;
use crate::metadata::Node;
use crate::value::Value;
use crate::wtype::{CompiledMigration, WInstance, wtype_restrict};

/// Construct the internal hom schema `[S, T]`.
///
/// For each source vertex `v` in `S`, the hom schema contains:
///
/// - A `choice_v` vertex (kind `"hom_choice"`) representing which target
///   vertex `v` maps to.
/// - A `maps_to` edge from `choice_v` to each compatible target vertex
///   (same kind).
/// - A `backward_v_e` vertex (kind `"hom_backward"`) for each outgoing
///   edge from `v`, representing the backward (edge-level) component of
///   the lens.
/// - A `backward_for` edge from `choice_v` to each of its backward
///   vertices.
#[must_use]
pub fn hom_schema(source: &Schema, target: &Schema) -> Schema {
    let mut vertices: HashMap<Name, Vertex> = HashMap::new();
    let mut edges: HashMap<Edge, Name> = HashMap::new();
    let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

    // Collect target vertices grouped by kind for compatibility matching.
    let mut target_by_kind: HashMap<Name, Vec<Name>> = HashMap::new();
    for (tid, tv) in &target.vertices {
        target_by_kind
            .entry(tv.kind.clone())
            .or_default()
            .push(tid.clone());
    }

    for (vid, sv) in &source.vertices {
        let choice_id: Name = format!("choice_{vid}").into();
        vertices.insert(
            choice_id.clone(),
            Vertex {
                id: choice_id.clone(),
                kind: Name::from("hom_choice"),
                nsid: None,
            },
        );

        add_maps_to_edges(
            &choice_id,
            &sv.kind,
            &target_by_kind,
            target,
            &mut vertices,
            &mut edges,
            &mut outgoing,
            &mut incoming,
            &mut between,
        );

        add_backward_vertices(
            vid,
            &choice_id,
            source,
            &mut vertices,
            &mut edges,
            &mut outgoing,
            &mut incoming,
            &mut between,
        );
    }

    Schema {
        protocol: "hom".into(),
        vertices,
        edges,
        hyper_edges: HashMap::new(),
        constraints: HashMap::new(),
        required: HashMap::new(),
        nsids: HashMap::new(),
        variants: HashMap::new(),
        orderings: HashMap::new(),
        recursion_points: HashMap::new(),
        spans: HashMap::new(),
        usage_modes: HashMap::new(),
        nominal: HashMap::new(),
        coercions: HashMap::new(),
        mergers: HashMap::new(),
        defaults: HashMap::new(),
        policies: HashMap::new(),
        outgoing,
        incoming,
        between,
    }
}

/// Add `maps_to` edges from a choice vertex to all compatible target vertices.
#[allow(clippy::too_many_arguments)]
fn add_maps_to_edges(
    choice_id: &Name,
    source_kind: &Name,
    target_by_kind: &HashMap<Name, Vec<Name>>,
    target: &Schema,
    vertices: &mut HashMap<Name, Vertex>,
    edges: &mut HashMap<Edge, Name>,
    outgoing: &mut HashMap<Name, SmallVec<Edge, 4>>,
    incoming: &mut HashMap<Name, SmallVec<Edge, 4>>,
    between: &mut HashMap<(Name, Name), SmallVec<Edge, 2>>,
) {
    let Some(compatible) = target_by_kind.get(source_kind) else {
        return;
    };
    for tid in compatible {
        let edge = Edge {
            src: choice_id.clone(),
            tgt: tid.clone(),
            kind: Name::from("maps_to"),
            name: Some(tid.clone()),
        };

        if !vertices.contains_key(tid) {
            let tv = &target.vertices[tid];
            vertices.insert(
                tid.clone(),
                Vertex {
                    id: tid.clone(),
                    kind: tv.kind.clone(),
                    nsid: tv.nsid.clone(),
                },
            );
        }

        outgoing
            .entry(choice_id.clone())
            .or_default()
            .push(edge.clone());
        incoming.entry(tid.clone()).or_default().push(edge.clone());
        between
            .entry((choice_id.clone(), tid.clone()))
            .or_default()
            .push(edge.clone());
        edges.insert(edge, Name::from("maps_to"));
    }
}

/// Add backward vertices for each outgoing edge from a source vertex.
#[allow(clippy::too_many_arguments)]
fn add_backward_vertices(
    vid: &Name,
    choice_id: &Name,
    source: &Schema,
    vertices: &mut HashMap<Name, Vertex>,
    edges: &mut HashMap<Edge, Name>,
    outgoing: &mut HashMap<Name, SmallVec<Edge, 4>>,
    incoming: &mut HashMap<Name, SmallVec<Edge, 4>>,
    between: &mut HashMap<(Name, Name), SmallVec<Edge, 2>>,
) {
    let Some(src_edges) = source.outgoing.get(vid) else {
        return;
    };
    for src_edge in src_edges {
        let backward_id: Name = format!("backward_{vid}_{}", edge_label(src_edge)).into();
        vertices.insert(
            backward_id.clone(),
            Vertex {
                id: backward_id.clone(),
                kind: Name::from("hom_backward"),
                nsid: None,
            },
        );

        let bf_edge = Edge {
            src: choice_id.clone(),
            tgt: backward_id.clone(),
            kind: Name::from("backward_for"),
            name: Some(backward_id.clone()),
        };
        outgoing
            .entry(choice_id.clone())
            .or_default()
            .push(bf_edge.clone());
        incoming
            .entry(backward_id.clone())
            .or_default()
            .push(bf_edge.clone());
        between
            .entry((choice_id.clone(), backward_id.clone()))
            .or_default()
            .push(bf_edge.clone());
        edges.insert(bf_edge, Name::from("backward_for"));
    }
}

/// Derive a stable label for an edge, used when constructing backward vertex
/// names. Prefers the edge's `name` field; falls back to the target vertex id.
fn edge_label(e: &Edge) -> &str {
    e.name.as_deref().unwrap_or(&e.tgt)
}

/// Evaluate an instance of `[S, T]` (a lens encoding) against an instance
/// of `S` to produce an instance of `T`.
///
/// The evaluation proceeds in three steps:
///
/// 1. Extract the forward map from `hom_instance`: for each `choice_v`
///    node, read its `maps_to` target to determine the vertex remapping.
/// 2. Extract backward maps from `hom_instance`: for each `backward_v_e`
///    node, read the stored edge mapping.
/// 3. Build a `CompiledMigration` from the extracted maps and apply
///    `wtype_restrict` to the source instance.
///
/// # Errors
///
/// Returns `InstError::NodeNotFound` if a `maps_to` arc references a missing node,
/// or `InstError::EvalHom` if the underlying `wtype_restrict` fails.
pub fn eval_hom(
    hom_instance: &WInstance,
    source_instance: &WInstance,
    source_schema: &Schema,
    target_schema: &Schema,
) -> Result<WInstance, InstError> {
    // Step 1: extract vertex remap from choice nodes.
    let mut vertex_remap: HashMap<Name, Name> = HashMap::new();
    let mut surviving_verts: HashSet<Name> = HashSet::new();

    for (node_id, node) in &hom_instance.nodes {
        let anchor: &str = &node.anchor;
        let Some(source_vertex_str) = anchor.strip_prefix("choice_") else {
            continue;
        };
        let source_vertex: Name = source_vertex_str.into();

        // Find the maps_to arc from this choice node.
        for &(parent, child, ref edge) in &hom_instance.arcs {
            if parent == *node_id && &*edge.kind == "maps_to" {
                let target_node = hom_instance
                    .nodes
                    .get(&child)
                    .ok_or(InstError::NodeNotFound(child))?;
                surviving_verts.insert(target_node.anchor.clone());
                vertex_remap.insert(source_vertex.clone(), target_node.anchor.clone());
                break;
            }
        }
    }

    // Step 2: extract edge remap from backward nodes.
    let mut edge_remap: HashMap<Edge, Edge> = HashMap::new();
    let mut surviving_edges: HashSet<Edge> = HashSet::new();

    for node in hom_instance.nodes.values() {
        let anchor: &str = &node.anchor;
        if !anchor.starts_with("backward_") {
            continue;
        }
        if let (Some(Value::Str(src_src)), Some(Value::Str(src_tgt)), Some(Value::Str(src_kind))) = (
            node.extra_fields.get("src_src"),
            node.extra_fields.get("src_tgt"),
            node.extra_fields.get("src_kind"),
        ) {
            let src_edge = Edge {
                src: Name::from(src_src.as_str()),
                tgt: Name::from(src_tgt.as_str()),
                kind: Name::from(src_kind.as_str()),
                name: node.extra_fields.get("src_name").and_then(|v| match v {
                    Value::Str(s) => Some(Name::from(s.as_str())),
                    _ => None,
                }),
            };

            let tgt_edge = Edge {
                src: vertex_remap
                    .get(&src_edge.src)
                    .cloned()
                    .unwrap_or_else(|| src_edge.src.clone()),
                tgt: vertex_remap
                    .get(&src_edge.tgt)
                    .cloned()
                    .unwrap_or_else(|| src_edge.tgt.clone()),
                kind: node
                    .extra_fields
                    .get("tgt_kind")
                    .and_then(|v| match v {
                        Value::Str(s) => Some(Name::from(s.as_str())),
                        _ => None,
                    })
                    .unwrap_or_else(|| src_edge.kind.clone()),
                name: node
                    .extra_fields
                    .get("tgt_name")
                    .and_then(|v| match v {
                        Value::Str(s) => Some(Name::from(s.as_str())),
                        _ => None,
                    })
                    .or_else(|| src_edge.name.clone()),
            };

            surviving_edges.insert(src_edge.clone());
            edge_remap.insert(src_edge, tgt_edge);
        }
    }

    // Step 3: build compiled migration and apply wtype_restrict.
    let compiled = CompiledMigration {
        surviving_verts,
        surviving_edges,
        vertex_remap,
        edge_remap,
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
    };

    wtype_restrict(source_instance, source_schema, target_schema, &compiled)
        .map_err(|e| InstError::EvalHom(e.to_string()))
}

/// Curry a compiled migration into an instance of `[S, T]`.
///
/// Given `vertex_remap` and `edge_remap` from a `CompiledMigration`,
/// construct a `WInstance` of the hom schema encoding those mappings.
/// This is the inverse of `eval_hom`: currying a migration and then
/// evaluating should produce the same result as applying the migration
/// directly.
#[must_use]
pub fn curry_migration(
    compiled: &CompiledMigration,
    source_schema: &Schema,
    _hom_schema: &Schema,
) -> WInstance {
    let mut nodes: HashMap<u32, Node> = HashMap::new();
    let mut arcs: Vec<(u32, u32, Edge)> = Vec::new();
    let mut next_id: u32 = 0;

    // Create root node anchored to a synthetic hom root.
    let root_id = next_id;
    let root_anchor: Name = "hom_root".into();
    nodes.insert(root_id, Node::new(root_id, root_anchor.clone()));
    next_id += 1;

    for (src_v, tgt_v) in &compiled.vertex_remap {
        let choice_anchor: Name = format!("choice_{src_v}").into();
        let choice_node_id = next_id;
        next_id += 1;
        nodes.insert(
            choice_node_id,
            Node::new(choice_node_id, choice_anchor.clone()),
        );

        // Arc from root to choice node.
        arcs.push((
            root_id,
            choice_node_id,
            Edge {
                src: root_anchor.clone(),
                tgt: choice_anchor.clone(),
                kind: Name::from("has_choice"),
                name: Some(choice_anchor.clone()),
            },
        ));

        // Create target vertex node.
        let target_node_id = next_id;
        next_id += 1;
        nodes.insert(target_node_id, Node::new(target_node_id, tgt_v.clone()));

        // maps_to arc from choice to target.
        arcs.push((
            choice_node_id,
            target_node_id,
            Edge {
                src: choice_anchor.clone(),
                tgt: tgt_v.clone(),
                kind: Name::from("maps_to"),
                name: Some(tgt_v.clone()),
            },
        ));

        // Create backward nodes for each outgoing edge from this source vertex.
        if let Some(src_edges) = source_schema.outgoing.get(src_v) {
            for src_edge in src_edges {
                let backward_anchor: Name =
                    format!("backward_{src_v}_{}", edge_label(src_edge)).into();
                let backward_node_id = next_id;
                next_id += 1;

                let mut backward_node = Node::new(backward_node_id, backward_anchor.clone());
                backward_node
                    .extra_fields
                    .insert("src_src".into(), Value::Str(src_edge.src.to_string()));
                backward_node
                    .extra_fields
                    .insert("src_tgt".into(), Value::Str(src_edge.tgt.to_string()));
                backward_node
                    .extra_fields
                    .insert("src_kind".into(), Value::Str(src_edge.kind.to_string()));
                if let Some(ref n) = src_edge.name {
                    backward_node
                        .extra_fields
                        .insert("src_name".into(), Value::Str(n.to_string()));
                }

                if let Some(tgt_edge) = compiled.edge_remap.get(src_edge) {
                    backward_node
                        .extra_fields
                        .insert("tgt_kind".into(), Value::Str(tgt_edge.kind.to_string()));
                    if let Some(ref n) = tgt_edge.name {
                        backward_node
                            .extra_fields
                            .insert("tgt_name".into(), Value::Str(n.to_string()));
                    }
                }

                nodes.insert(backward_node_id, backward_node);

                arcs.push((
                    choice_node_id,
                    backward_node_id,
                    Edge {
                        src: choice_anchor.clone(),
                        tgt: backward_anchor.clone(),
                        kind: Name::from("backward_for"),
                        name: Some(backward_anchor.clone()),
                    },
                ));
            }
        }
    }

    WInstance::new(nodes, arcs, Vec::new(), root_id, root_anchor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::FieldPresence;

    /// Helper: build a minimal test schema with the given vertices (all kind
    /// "object") and edges, populating the precomputed indices.
    fn make_schema(verts: &[&str], edge_list: &[(&str, &str, &str)]) -> Schema {
        let mut vertices: HashMap<Name, Vertex> = HashMap::new();
        let mut edges: HashMap<Edge, Name> = HashMap::new();
        let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
        let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
        let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

        for &v in verts {
            vertices.insert(
                Name::from(v),
                Vertex {
                    id: Name::from(v),
                    kind: Name::from("object"),
                    nsid: None,
                },
            );
        }

        for &(src, tgt, label) in edge_list {
            let edge = Edge {
                src: Name::from(src),
                tgt: Name::from(tgt),
                kind: Name::from("prop"),
                name: Some(Name::from(label)),
            };
            outgoing
                .entry(Name::from(src))
                .or_default()
                .push(edge.clone());
            incoming
                .entry(Name::from(tgt))
                .or_default()
                .push(edge.clone());
            between
                .entry((Name::from(src), Name::from(tgt)))
                .or_default()
                .push(edge.clone());
            edges.insert(edge, Name::from("prop"));
        }

        Schema {
            protocol: "test".into(),
            vertices,
            edges,
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing,
            incoming,
            between,
        }
    }

    #[test]
    fn hom_schema_empty_schemas() {
        let s = make_schema(&[], &[]);
        let t = make_schema(&[], &[]);
        let h = hom_schema(&s, &t);

        assert!(
            h.vertices.is_empty(),
            "empty schemas yield empty hom schema"
        );
        assert!(h.edges.is_empty());
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn hom_schema_two_vertex() {
        let source = make_schema(&["a", "b"], &[("a", "b", "edge_ab")]);
        let target = make_schema(&["x", "y"], &[("x", "y", "edge_xy")]);
        let h = hom_schema(&source, &target);

        assert!(
            h.vertices.contains_key(&Name::from("choice_a")),
            "choice_a vertex must exist"
        );
        assert!(
            h.vertices.contains_key(&Name::from("choice_b")),
            "choice_b vertex must exist"
        );
        assert!(
            h.vertices.contains_key(&Name::from("backward_a_edge_ab")),
            "backward vertex for edge_ab must exist"
        );

        assert_eq!(h.vertices[&Name::from("choice_a")].kind, "hom_choice");
        assert_eq!(
            h.vertices[&Name::from("backward_a_edge_ab")].kind,
            "hom_backward"
        );

        let choice_a_out = h.outgoing.get(&Name::from("choice_a")).unwrap();
        let maps_to_count = choice_a_out
            .iter()
            .filter(|e| &*e.kind == "maps_to")
            .count();
        assert_eq!(maps_to_count, 2, "choice_a maps_to x and y");

        let backward_for_count = choice_a_out
            .iter()
            .filter(|e| &*e.kind == "backward_for")
            .count();
        assert_eq!(backward_for_count, 1, "choice_a has one backward_for edge");
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn curry_roundtrip() {
        let source = make_schema(&["a", "b"], &[("a", "b", "child")]);
        let target = make_schema(&["x", "y"], &[("x", "y", "child")]);

        let src_edge = Edge {
            src: Name::from("a"),
            tgt: Name::from("b"),
            kind: Name::from("prop"),
            name: Some(Name::from("child")),
        };
        let tgt_edge = Edge {
            src: Name::from("x"),
            tgt: Name::from("y"),
            kind: Name::from("prop"),
            name: Some(Name::from("child")),
        };

        let compiled = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("x"), Name::from("y")]),
            surviving_edges: HashSet::from([src_edge.clone()]),
            vertex_remap: HashMap::from([
                (Name::from("a"), Name::from("x")),
                (Name::from("b"), Name::from("y")),
            ]),
            edge_remap: HashMap::from([(src_edge.clone(), tgt_edge)]),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
        };

        let h = hom_schema(&source, &target);
        let curried = curry_migration(&compiled, &source, &h);

        let choice_count = curried
            .nodes
            .values()
            .filter(|n| n.anchor.starts_with("choice_"))
            .count();
        assert_eq!(choice_count, 2, "two choice nodes (one per source vertex)");

        // Build a source instance: root "a" with child "b".
        let src_nodes = HashMap::from([
            (0, Node::new(0, "a")),
            (
                1,
                Node::new(1, "b").with_value(FieldPresence::Present(Value::Str("hello".into()))),
            ),
        ]);
        let src_arcs = vec![(0, 1, src_edge)];
        let src_instance = WInstance::new(src_nodes, src_arcs, Vec::new(), 0, Name::from("a"));

        let result = eval_hom(&curried, &src_instance, &source, &target).unwrap();

        let root_node = result.nodes.get(&result.root).unwrap();
        assert_eq!(root_node.anchor, "x", "root remapped to x");

        let child_ids = result.children(result.root);
        assert_eq!(child_ids.len(), 1, "one child");
        let child_node = result.nodes.get(&child_ids[0]).unwrap();
        assert_eq!(child_node.anchor, "y", "child remapped to y");

        assert_eq!(
            child_node.value,
            Some(FieldPresence::Present(Value::Str("hello".into())))
        );

        // Compare with direct wtype_restrict.
        let direct = wtype_restrict(&src_instance, &source, &target, &compiled).unwrap();
        assert_eq!(result.node_count(), direct.node_count());
        assert_eq!(result.arc_count(), direct.arc_count());

        let direct_root = direct.nodes.get(&direct.root).unwrap();
        assert_eq!(direct_root.anchor, root_node.anchor);
    }
}
