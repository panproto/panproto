//! Schema-level colimit (pushout) computation.
//!
//! Given two schemas and a description of their shared elements (the
//! [`SchemaOverlap`]), [`schema_pushout`] computes the categorical
//! pushout: a merged schema together with morphisms embedding each
//! input into the result.

use std::collections::HashMap;

use panproto_gat::Name;
use smallvec::SmallVec;

use crate::error::SchemaError;
use crate::induce::ordered_edges;
use crate::morphism::SchemaMorphism;
use crate::schema::{Edge, Schema, Vertex};

/// Specifies which elements of two schemas are identified (shared).
///
/// Each pair `(left_id, right_id)` declares that the left and right
/// elements represent the same concept and should be merged in the
/// pushout.
#[derive(Clone, Debug, Default)]
pub struct SchemaOverlap {
    /// Pairs of vertex IDs from `(left, right)` that represent the same vertex.
    pub vertex_pairs: Vec<(Name, Name)>,
    /// Pairs of edges from `(left, right)` that represent the same edge.
    pub edge_pairs: Vec<(Edge, Edge)>,
}

/// Remap an edge's `src` and `tgt` through a vertex rename map.
fn remap_edge(edge: &Edge, vmap: &HashMap<Name, Name>) -> Edge {
    Edge {
        src: vmap
            .get(&edge.src)
            .cloned()
            .unwrap_or_else(|| edge.src.clone()),
        tgt: vmap
            .get(&edge.tgt)
            .cloned()
            .unwrap_or_else(|| edge.tgt.clone()),
        kind: edge.kind.clone(),
        name: edge.name.clone(),
    }
}

/// Compute the pushout (colimit) of two schemas along their overlap.
///
/// Returns the pushout `Schema` plus `SchemaMorphism` values from each
/// input schema into the pushout.
///
/// # Errors
///
/// Returns [`SchemaError::VertexNotFound`] if an overlap pair references
/// a vertex ID that does not exist in the corresponding schema.
pub fn schema_pushout(
    left: &Schema,
    right: &Schema,
    overlap: &SchemaOverlap,
) -> Result<(Schema, SchemaMorphism, SchemaMorphism), SchemaError> {
    let right_vertex_rename = build_vertex_rename(left, right, overlap)?;

    let (merged_vertices, left_vertex_map, right_vertex_map) =
        build_merged_vertices(left, right, &right_vertex_rename);

    let (merged_edges, left_edge_map, right_edge_map) =
        build_merged_edges(left, right, overlap, &right_vertex_rename);

    let pushout = assemble_pushout(
        left,
        right,
        &right_vertex_rename,
        merged_vertices,
        merged_edges,
    );

    let left_morphism = SchemaMorphism {
        name: "left→pushout".into(),
        src_protocol: left.protocol.clone(),
        tgt_protocol: pushout.protocol.clone(),
        vertex_map: left_vertex_map,
        edge_map: left_edge_map,
        renames: vec![],
    };

    let right_morphism = SchemaMorphism {
        name: "right→pushout".into(),
        src_protocol: right.protocol.clone(),
        tgt_protocol: pushout.protocol.clone(),
        vertex_map: right_vertex_map,
        edge_map: right_edge_map,
        renames: vec![],
    };

    Ok((pushout, left_morphism, right_morphism))
}

/// Build the right→merged vertex ID rename map.
///
/// For identified vertices the right ID maps to the left ID.
/// For non-identified vertices the right ID is kept unless it
/// conflicts with a left ID, in which case it is prefixed with `"right."`.
fn build_vertex_rename(
    left: &Schema,
    right: &Schema,
    overlap: &SchemaOverlap,
) -> Result<HashMap<Name, Name>, SchemaError> {
    let mut rename: HashMap<Name, Name> = HashMap::new();

    for (left_id, right_id) in &overlap.vertex_pairs {
        if !left.vertices.contains_key(left_id) {
            return Err(SchemaError::VertexNotFound(left_id.to_string()));
        }
        if !right.vertices.contains_key(right_id) {
            return Err(SchemaError::VertexNotFound(right_id.to_string()));
        }
        rename.insert(right_id.clone(), left_id.clone());
    }

    for right_id in right.vertices.keys() {
        if rename.contains_key(right_id) {
            continue;
        }
        if left.vertices.contains_key(right_id) {
            let merged_id = Name::from(format!("right.{right_id}"));
            rename.insert(right_id.clone(), merged_id);
        } else {
            rename.insert(right_id.clone(), right_id.clone());
        }
    }

    Ok(rename)
}

/// Build merged vertices and the left/right vertex morphism maps.
fn build_merged_vertices(
    left: &Schema,
    right: &Schema,
    right_rename: &HashMap<Name, Name>,
) -> (
    HashMap<Name, Vertex>,
    HashMap<Name, Name>,
    HashMap<Name, Name>,
) {
    let mut merged: HashMap<Name, Vertex> = HashMap::new();

    for (id, v) in &left.vertices {
        merged.insert(id.clone(), v.clone());
    }

    for (right_id, v) in &right.vertices {
        let merged_id = right_rename
            .get(right_id)
            .cloned()
            .unwrap_or_else(|| right_id.clone());
        merged.entry(merged_id.clone()).or_insert_with(|| Vertex {
            id: merged_id,
            kind: v.kind.clone(),
            nsid: v.nsid.clone(),
        });
    }

    let left_map: HashMap<Name, Name> = left
        .vertices
        .keys()
        .map(|id| (id.clone(), id.clone()))
        .collect();

    let right_map: HashMap<Name, Name> = right_rename.clone();

    (merged, left_map, right_map)
}

/// Build merged edges and the left/right edge morphism maps.
fn build_merged_edges(
    left: &Schema,
    right: &Schema,
    overlap: &SchemaOverlap,
    right_rename: &HashMap<Name, Name>,
) -> (
    HashMap<Edge, Name>,
    HashMap<Edge, Edge>,
    HashMap<Edge, Edge>,
) {
    let right_edge_to_left: HashMap<Edge, Edge> = overlap
        .edge_pairs
        .iter()
        .map(|(l, r)| (r.clone(), l.clone()))
        .collect();

    let mut merged: HashMap<Edge, Name> = HashMap::new();
    let mut left_map: HashMap<Edge, Edge> = HashMap::new();
    let mut right_map: HashMap<Edge, Edge> = HashMap::new();

    for (edge, kind) in &left.edges {
        merged.insert(edge.clone(), kind.clone());
        left_map.insert(edge.clone(), edge.clone());
    }

    for (edge, kind) in &right.edges {
        if let Some(left_edge) = right_edge_to_left.get(edge) {
            right_map.insert(edge.clone(), left_edge.clone());
        } else {
            let remapped = remap_edge(edge, right_rename);
            if !merged.contains_key(&remapped) {
                merged.insert(remapped.clone(), kind.clone());
            }
            right_map.insert(edge.clone(), remapped);
        }
    }

    (merged, left_map, right_map)
}

/// The pushout's edges in the order the adjacency indices should record them:
/// every left edge in `left`'s own order, then each right edge that survives
/// renaming without colliding, in `right`'s own order.
///
/// This is the coproduct order on the merged edge set, and it is what keeps a
/// pushout's buckets reproducible: `merged_edges` is a [`HashMap`], so reading
/// the order off it instead would make the result depend on the hash seed.
/// Taking each side's order from [`ordered_edges`] rather than from its edge
/// map is what carries a parser's sibling order through the pushout.
fn ordered_merged_edges(
    left: &Schema,
    right: &Schema,
    merged_edges: &HashMap<Edge, Name>,
    right_rename: &HashMap<Name, Name>,
) -> Vec<Edge> {
    let mut out: Vec<Edge> = Vec::with_capacity(merged_edges.len());
    let mut seen: std::collections::HashSet<Edge> = std::collections::HashSet::new();

    let mut push = |edge: Edge| {
        if merged_edges.contains_key(&edge) && seen.insert(edge.clone()) {
            out.push(edge);
        }
    };

    for edge in ordered_edges(left) {
        push(edge);
    }
    for edge in ordered_edges(right) {
        push(remap_edge(&edge, right_rename));
    }

    // An edge the merge introduced that neither traversal reached still has to
    // appear, and in an order that does not depend on the hash seed.
    let mut rest: Vec<&Edge> = merged_edges
        .keys()
        .filter(|edge| !seen.contains(*edge))
        .collect();
    rest.sort_unstable();
    out.extend(rest.into_iter().cloned());

    out
}

/// Look up a right vertex ID through the rename map, falling back to identity.
fn resolve(right_rename: &HashMap<Name, Name>, id: &Name) -> Name {
    right_rename.get(id).cloned().unwrap_or_else(|| id.clone())
}

/// Merge vertex-keyed maps (constraints, nsids, variants, etc.) from right into left.
fn merge_vertex_keyed(
    left: &Schema,
    right: &Schema,
    right_rename: &HashMap<Name, Name>,
) -> MergedVertexKeyed {
    // Constraints
    let mut constraints = left.constraints.clone();
    for (rid, rcs) in &right.constraints {
        let mid = resolve(right_rename, rid);
        let entry = constraints.entry(mid).or_default();
        for c in rcs {
            if !entry.contains(c) {
                entry.push(c.clone());
            }
        }
    }

    // Required edges
    let mut required = left.required.clone();
    for (rid, rreqs) in &right.required {
        let mid = resolve(right_rename, rid);
        let entry = required.entry(mid).or_default();
        for req in rreqs {
            let remapped = remap_edge(req, right_rename);
            if !entry.contains(&remapped) {
                entry.push(remapped);
            }
        }
    }

    // NSIDs
    let mut nsids = left.nsids.clone();
    for (rid, nsid) in &right.nsids {
        let mid = resolve(right_rename, rid);
        nsids.entry(mid).or_insert_with(|| nsid.clone());
    }

    // Variants
    let mut variants = left.variants.clone();
    for (rid, vs) in &right.variants {
        let mid = resolve(right_rename, rid);
        let entry = variants.entry(mid).or_default();
        for v in vs {
            let mut v2 = v.clone();
            v2.parent_vertex = resolve(right_rename, &v2.parent_vertex);
            if !entry.contains(&v2) {
                entry.push(v2);
            }
        }
    }

    // Nominal
    let mut nominal = left.nominal.clone();
    for (rid, &nom) in &right.nominal {
        let mid = resolve(right_rename, rid);
        nominal.entry(mid).or_insert(nom);
    }

    MergedVertexKeyed {
        constraints,
        required,
        nsids,
        variants,
        nominal,
    }
}

/// Intermediate result for merged vertex-keyed maps.
struct MergedVertexKeyed {
    constraints: HashMap<Name, Vec<crate::schema::Constraint>>,
    required: HashMap<Name, Vec<Edge>>,
    nsids: HashMap<Name, Name>,
    variants: HashMap<Name, Vec<crate::schema::Variant>>,
    nominal: HashMap<Name, bool>,
}

/// Merge edge-keyed and structural maps from right into left, then
/// assemble the final `Schema` with rebuilt adjacency indices.
fn assemble_pushout(
    left: &Schema,
    right: &Schema,
    right_rename: &HashMap<Name, Name>,
    mut merged_vertices: HashMap<Name, Vertex>,
    merged_edges: HashMap<Edge, Name>,
) -> Schema {
    let vk = merge_vertex_keyed(left, right, right_rename);

    // A schema records each NSID twice: on the vertex and in `nsids`. The two
    // merges disagree about which side wins, because `build_merged_vertices`
    // merges whole vertices (left's `Vertex` wins outright) while `nsids` is
    // merged per key (an absent left key is filled from the right). Identifying
    // a left vertex that carries no NSID with a right vertex that carries one
    // therefore leaves the vertex saying `None` while the map says the NSID,
    // which every reader of one copy resolves differently from every reader of
    // the other. Deriving the vertex's copy from the merged map puts the two
    // back in step; the map is the more complete of the two by construction,
    // and every constructor writes a row for every vertex that has an NSID, so
    // this never erases one.
    for (id, vertex) in &mut merged_vertices {
        vertex.nsid = vk.nsids.get(id).cloned();
    }

    // Hyper-edges
    let mut hyper_edges = left.hyper_edges.clone();
    for (id, he) in &right.hyper_edges {
        let mid = if hyper_edges.contains_key(id) {
            Name::from(format!("right.{id}"))
        } else {
            id.clone()
        };
        let mut he2 = he.clone();
        he2.id = mid.clone();
        he2.signature = he2
            .signature
            .into_iter()
            .map(|(label, vid)| {
                let new_vid = right_rename.get(&vid).cloned().unwrap_or(vid);
                (label, new_vid)
            })
            .collect();
        hyper_edges.insert(mid, he2);
    }

    // Orderings
    let mut orderings = left.orderings.clone();
    for (edge, pos) in &right.orderings {
        let remapped = remap_edge(edge, right_rename);
        orderings.entry(remapped).or_insert(*pos);
    }

    // Recursion points
    let mut recursion_points = left.recursion_points.clone();
    for (id, rp) in &right.recursion_points {
        // The marker vertex is the key, so renaming it is renaming the entry.
        let mid = resolve(right_rename, id);
        recursion_points.entry(mid).or_insert_with(|| {
            let mut rp2 = rp.clone();
            rp2.target_vertex = resolve(right_rename, &rp2.target_vertex);
            rp2
        });
    }

    // Spans
    let mut spans = left.spans.clone();
    for (id, sp) in &right.spans {
        let mid = if spans.contains_key(id) {
            Name::from(format!("right.{id}"))
        } else {
            id.clone()
        };
        let mut sp2 = sp.clone();
        sp2.id = mid.clone();
        sp2.left = resolve(right_rename, &sp2.left);
        sp2.right = resolve(right_rename, &sp2.right);
        spans.insert(mid, sp2);
    }

    // Usage modes
    let mut usage_modes = left.usage_modes.clone();
    for (edge, mode) in &right.usage_modes {
        let remapped = remap_edge(edge, right_rename);
        usage_modes.entry(remapped).or_insert_with(|| mode.clone());
    }

    // Coercions: merge (Name, Name) → CoercionSpec, left wins on overlap.
    // The key is a pair of vertex *kinds*, not a pair of vertex ids, so it does
    // not pass through the vertex rename map: renaming a right vertex that
    // happens to spell a kind would rewrite the key into the vertex namespace
    // and put the coercion beyond every lookup.
    let mut coercions = left.coercions.clone();
    for (key, spec) in &right.coercions {
        coercions.entry(key.clone()).or_insert_with(|| spec.clone());
    }

    // Mergers: merge Name → Expr, left wins on overlap
    let mut mergers = left.mergers.clone();
    for (rid, expr) in &right.mergers {
        let mid = resolve(right_rename, rid);
        mergers.entry(mid).or_insert_with(|| expr.clone());
    }

    // Defaults: merge Name → Expr, left wins on overlap
    let mut defaults = left.defaults.clone();
    for (rid, expr) in &right.defaults {
        let mid = resolve(right_rename, rid);
        defaults.entry(mid).or_insert_with(|| expr.clone());
    }

    // Policies: merge Name → Expr, left wins on overlap.
    // The key is a constraint sort name, not a vertex id, so it does not pass
    // through the vertex rename map for the same reason the coercion key does
    // not.
    let mut policies = left.policies.clone();
    for (sort, expr) in &right.policies {
        policies.entry(sort.clone()).or_insert_with(|| expr.clone());
    }

    // Rebuild adjacency indices
    let idx = build_indices(left, right, &merged_edges, right_rename);

    let entries = merge_entries(left, right, right_rename);

    Schema {
        protocol: left.protocol.clone(),
        vertices: merged_vertices,
        edges: merged_edges,
        hyper_edges,
        constraints: vk.constraints,
        required: vk.required,
        nsids: vk.nsids,
        entries,
        variants: vk.variants,
        orderings,
        recursion_points,
        spans,
        usage_modes,
        nominal: vk.nominal,
        coercions,
        mergers,
        defaults,
        policies,
        outgoing: idx.outgoing,
        incoming: idx.incoming,
        between: idx.between,
    }
}

/// Coproduct of the pointed schemas: union of `left.entries` and the
/// renamed `right.entries`, preserving insertion order and
/// deduplicating. This is the canonical pointing on the pushout.
fn merge_entries(left: &Schema, right: &Schema, right_rename: &HashMap<Name, Name>) -> Vec<Name> {
    let mut entries: Vec<Name> = left.entries.clone();
    let mut seen: std::collections::HashSet<Name> = entries.iter().cloned().collect();
    for rid in &right.entries {
        let mid = resolve(right_rename, rid);
        if seen.insert(mid.clone()) {
            entries.push(mid);
        }
    }
    entries
}

/// Precomputed adjacency indices for a schema.
struct AdjacencyIndices {
    /// Outgoing edges per vertex ID.
    outgoing: HashMap<Name, SmallVec<Edge, 4>>,
    /// Incoming edges per vertex ID.
    incoming: HashMap<Name, SmallVec<Edge, 4>>,
    /// Edges between a specific `(src, tgt)` pair.
    between: HashMap<(Name, Name), SmallVec<Edge, 2>>,
}

/// Rebuild the pushout's adjacency indices.
///
/// Bucket order comes from [`ordered_merged_edges`] rather than from
/// `merged_edges`: iterating the edge map would order the buckets by hash seed,
/// so a pushout would present its edges differently in every process.
fn build_indices(
    left: &Schema,
    right: &Schema,
    merged_edges: &HashMap<Edge, Name>,
    right_rename: &HashMap<Name, Name>,
) -> AdjacencyIndices {
    let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

    for edge in &ordered_merged_edges(left, right, merged_edges, right_rename) {
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

    AdjacencyIndices {
        outgoing,
        incoming,
        between,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{Protocol, SchemaBuilder};

    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "string".into(), "integer".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    fn build_schema(vertices: &[(&str, &str)], edges: &[(&str, &str, &str, &str)]) -> Schema {
        let proto = test_protocol();
        let mut builder = SchemaBuilder::new(&proto);
        for (id, kind) in vertices {
            builder = builder.vertex(id, kind, None::<&str>).unwrap();
        }
        for (src, tgt, kind, name) in edges {
            builder = builder.edge(src, tgt, kind, Some(*name)).unwrap();
        }
        builder.build().unwrap()
    }

    #[test]
    fn pushout_of_identical_schemas_is_itself() {
        let s = build_schema(
            &[("root", "object"), ("root.x", "string")],
            &[("root", "root.x", "prop", "x")],
        );
        let overlap = SchemaOverlap {
            vertex_pairs: vec![
                (Name::from("root"), Name::from("root")),
                (Name::from("root.x"), Name::from("root.x")),
            ],
            edge_pairs: vec![(
                Edge {
                    src: Name::from("root"),
                    tgt: Name::from("root.x"),
                    kind: Name::from("prop"),
                    name: Some(Name::from("x")),
                },
                Edge {
                    src: Name::from("root"),
                    tgt: Name::from("root.x"),
                    kind: Name::from("prop"),
                    name: Some(Name::from("x")),
                },
            )],
        };

        let (pushout, left_m, right_m) = schema_pushout(&s, &s, &overlap).unwrap();
        assert_eq!(pushout.vertex_count(), s.vertex_count());
        assert_eq!(pushout.edge_count(), s.edge_count());

        for (src, tgt) in &left_m.vertex_map {
            assert_eq!(src, tgt, "left morphism should be identity");
        }
        for (src, tgt) in &right_m.vertex_map {
            assert_eq!(src, tgt, "right morphism should be identity");
        }
    }

    #[test]
    fn pushout_of_disjoint_schemas_is_union() {
        let left = build_schema(
            &[("a", "object"), ("a.x", "string")],
            &[("a", "a.x", "prop", "x")],
        );
        let right = build_schema(
            &[("b", "object"), ("b.y", "integer")],
            &[("b", "b.y", "prop", "y")],
        );

        let overlap = SchemaOverlap::default();
        let (pushout, _left_m, _right_m) = schema_pushout(&left, &right, &overlap).unwrap();

        assert_eq!(pushout.vertex_count(), 4);
        assert_eq!(pushout.edge_count(), 2);

        assert!(pushout.has_vertex("a"));
        assert!(pushout.has_vertex("a.x"));
        assert!(pushout.has_vertex("b"));
        assert!(pushout.has_vertex("b.y"));
    }

    #[test]
    fn pushout_with_vertex_overlap_merges_vertices() {
        let left = build_schema(
            &[("root", "object"), ("root.x", "string")],
            &[("root", "root.x", "prop", "x")],
        );
        let right = build_schema(
            &[("base", "object"), ("base.y", "integer")],
            &[("base", "base.y", "prop", "y")],
        );

        let overlap = SchemaOverlap {
            vertex_pairs: vec![(Name::from("root"), Name::from("base"))],
            edge_pairs: vec![],
        };

        let (pushout, left_m, right_m) = schema_pushout(&left, &right, &overlap).unwrap();

        assert_eq!(pushout.vertex_count(), 3);
        assert!(pushout.has_vertex("root"));
        assert!(pushout.has_vertex("root.x"));
        assert!(pushout.has_vertex("base.y"));

        assert_eq!(
            left_m.vertex_map.get("root").map(Name::as_str),
            Some("root")
        );
        assert_eq!(
            right_m.vertex_map.get("base").map(Name::as_str),
            Some("root")
        );
        assert_eq!(
            right_m.vertex_map.get("base.y").map(Name::as_str),
            Some("base.y")
        );
    }

    #[test]
    fn pushout_with_edge_overlap_merges_edges() {
        let left = build_schema(
            &[("root", "object"), ("root.x", "string")],
            &[("root", "root.x", "prop", "x")],
        );
        let right = build_schema(
            &[("root", "object"), ("root.x", "string")],
            &[("root", "root.x", "prop", "x")],
        );

        let overlap = SchemaOverlap {
            vertex_pairs: vec![
                (Name::from("root"), Name::from("root")),
                (Name::from("root.x"), Name::from("root.x")),
            ],
            edge_pairs: vec![(
                Edge {
                    src: Name::from("root"),
                    tgt: Name::from("root.x"),
                    kind: Name::from("prop"),
                    name: Some(Name::from("x")),
                },
                Edge {
                    src: Name::from("root"),
                    tgt: Name::from("root.x"),
                    kind: Name::from("prop"),
                    name: Some(Name::from("x")),
                },
            )],
        };

        let (pushout, _left_m, right_m) = schema_pushout(&left, &right, &overlap).unwrap();

        assert_eq!(pushout.edge_count(), 1);

        let right_edge = Edge {
            src: Name::from("root"),
            tgt: Name::from("root.x"),
            kind: Name::from("prop"),
            name: Some(Name::from("x")),
        };
        assert!(
            right_m.edge_map.contains_key(&right_edge),
            "right morphism should map the overlapping edge"
        );
    }

    #[test]
    fn morphisms_into_pushout_are_valid() {
        let left = build_schema(
            &[("root", "object"), ("root.a", "string")],
            &[("root", "root.a", "prop", "a")],
        );
        let right = build_schema(
            &[("root", "object"), ("root.b", "integer")],
            &[("root", "root.b", "prop", "b")],
        );

        let overlap = SchemaOverlap {
            vertex_pairs: vec![(Name::from("root"), Name::from("root"))],
            edge_pairs: vec![],
        };

        let (pushout, left_m, right_m) = schema_pushout(&left, &right, &overlap).unwrap();

        for (src, tgt) in &left_m.vertex_map {
            assert!(
                pushout.has_vertex(tgt),
                "left morphism target `{tgt}` (from `{src}`) should exist in pushout"
            );
        }

        for (src, tgt) in &right_m.vertex_map {
            assert!(
                pushout.has_vertex(tgt),
                "right morphism target `{tgt}` (from `{src}`) should exist in pushout"
            );
        }

        for tgt_e in left_m.edge_map.values() {
            assert!(
                pushout.edges.contains_key(tgt_e),
                "left morphism edge target should exist in pushout"
            );
        }

        for tgt_e in right_m.edge_map.values() {
            assert!(
                pushout.edges.contains_key(tgt_e),
                "right morphism edge target should exist in pushout"
            );
        }
    }

    #[test]
    fn pushout_conflicting_vertex_ids_are_prefixed() {
        let left = build_schema(
            &[("v", "object"), ("v.x", "string")],
            &[("v", "v.x", "prop", "x")],
        );
        let right = build_schema(
            &[("v", "object"), ("v.y", "integer")],
            &[("v", "v.y", "prop", "y")],
        );

        let overlap = SchemaOverlap::default();
        let (pushout, _left_m, right_m) = schema_pushout(&left, &right, &overlap).unwrap();

        assert!(pushout.has_vertex("v"));
        assert!(pushout.has_vertex("right.v"));
        assert_eq!(
            right_m.vertex_map.get("v").map(Name::as_str),
            Some("right.v")
        );
    }

    #[test]
    fn overlap_with_missing_vertex_returns_error() {
        let s = build_schema(&[("a", "object")], &[]);
        let overlap = SchemaOverlap {
            vertex_pairs: vec![(Name::from("nonexistent"), Name::from("a"))],
            edge_pairs: vec![],
        };
        let result = schema_pushout(&s, &s, &overlap);
        assert!(result.is_err());
    }

    /// A coercion is keyed by a pair of vertex *kinds*, so a right vertex whose
    /// id spells a kind must not drag the key into the vertex namespace.
    #[test]
    fn a_coercion_keeps_its_kind_pair_key_through_a_rename() {
        let proto = test_protocol();
        let left = SchemaBuilder::new(&proto)
            .vertex("string", "object", None::<&str>)
            .unwrap()
            .build()
            .unwrap();
        let right = SchemaBuilder::new(&proto)
            .vertex("string", "string", None::<&str>)
            .unwrap()
            .coercion(
                "string",
                "object",
                crate::schema::CoercionSpec {
                    forward: panproto_expr::Expr::Lit(panproto_expr::Literal::Null),
                    inverse: None,
                    class: panproto_gat::CoercionClass::Opaque,
                },
            )
            .build()
            .unwrap();

        // `string` collides, so `build_vertex_rename` sends right's vertex to
        // `right.string`. The coercion key must not follow it.
        let (merged, _, _) = schema_pushout(&left, &right, &SchemaOverlap::default()).unwrap();
        assert!(
            merged.has_vertex("right.string"),
            "the rename this test depends on did happen"
        );
        assert!(
            merged
                .coercions
                .contains_key(&(Name::from("string"), Name::from("object"))),
            "a coercion is keyed by kinds, not vertex ids: got {:?}",
            merged.coercions.keys().collect::<Vec<_>>()
        );
    }

    /// A policy is keyed by a constraint sort name, with the same consequence.
    #[test]
    fn a_policy_keeps_its_sort_name_key_through_a_rename() {
        let proto = test_protocol();
        let left = SchemaBuilder::new(&proto)
            .vertex("format", "object", None::<&str>)
            .unwrap()
            .build()
            .unwrap();
        let right = SchemaBuilder::new(&proto)
            .vertex("format", "string", None::<&str>)
            .unwrap()
            .policy(
                "format",
                panproto_expr::Expr::Lit(panproto_expr::Literal::Int(2)),
            )
            .build()
            .unwrap();

        let (merged, _, _) = schema_pushout(&left, &right, &SchemaOverlap::default()).unwrap();
        assert!(merged.has_vertex("right.format"));
        assert!(
            merged.policies.contains_key("format"),
            "a policy is keyed by a sort name, not a vertex id: got {:?}",
            merged.policies.keys().collect::<Vec<_>>()
        );
    }

    /// A schema records each NSID on the vertex and in `nsids`; identifying a
    /// vertex that carries one with a vertex that does not must not leave the
    /// two copies disagreeing.
    #[test]
    fn an_identified_vertex_carries_one_nsid_on_both_copies() {
        let proto = test_protocol();
        let left = SchemaBuilder::new(&proto)
            .vertex("a", "object", None::<&str>)
            .unwrap()
            .build()
            .unwrap();
        let right = SchemaBuilder::new(&proto)
            .vertex("a", "object", Some("com.example.a"))
            .unwrap()
            .build()
            .unwrap();
        let overlap = SchemaOverlap {
            vertex_pairs: vec![(Name::from("a"), Name::from("a"))],
            edge_pairs: vec![],
        };

        let (merged, _, _) = schema_pushout(&left, &right, &overlap).unwrap();
        assert_eq!(
            merged.vertices["a"].nsid.as_ref(),
            merged.nsids.get("a"),
            "the vertex's copy of the NSID and the map's copy must agree"
        );

        // And the same pair the other way round, which already agreed.
        let (swapped, _, _) = schema_pushout(&right, &left, &overlap).unwrap();
        assert_eq!(swapped.vertices["a"].nsid.as_ref(), swapped.nsids.get("a"));
    }
}
