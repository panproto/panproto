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
/// The overlap is read as a relation and closed into the equivalence it
/// generates, so identifying two left vertices with one right vertex identifies
/// them with each other, and identifying two edges identifies their endpoints.
/// Both injections are therefore graph homomorphisms into the quotient.
///
/// # Errors
///
/// Returns [`SchemaError::VertexNotFound`] if an overlap pair references
/// a vertex ID that does not exist in the corresponding schema, and
/// [`SchemaError::OverlapEdgeNotFound`] if it references an absent edge.
pub fn schema_pushout(
    left: &Schema,
    right: &Schema,
    overlap: &SchemaOverlap,
) -> Result<(Schema, SchemaMorphism, SchemaMorphism), SchemaError> {
    let quotient = build_vertex_quotient(left, right, overlap)?;

    let (merged_vertices, left_vertex_map, right_vertex_map) =
        build_merged_vertices(left, right, &quotient);

    let (merged_edges, left_edge_map, right_edge_map) =
        build_merged_edges(left, right, overlap, &quotient);

    let pushout = assemble_pushout(left, right, &quotient, merged_vertices, merged_edges);

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

/// An element of the span's apex: a vertex ID tagged with the schema it comes
/// from. `false` is the left schema, `true` the right, so the derived ordering
/// puts every left element before every right one.
type QuotientElem = (bool, Name);

/// Union-find over the vertex IDs of both schemas, used to close the overlap
/// relation into the equivalence it generates.
///
/// Each `union` keeps the smaller of the two roots, so a class's root is its
/// minimum element: the least left ID when the class has any left member, and
/// otherwise the least right ID.
struct VertexUnionFind {
    parent: HashMap<QuotientElem, QuotientElem>,
}

impl VertexUnionFind {
    fn new() -> Self {
        Self {
            parent: HashMap::new(),
        }
    }

    fn find(&mut self, elem: &QuotientElem) -> QuotientElem {
        let mut root = elem.clone();
        loop {
            let parent = self
                .parent
                .get(&root)
                .cloned()
                .unwrap_or_else(|| root.clone());
            if parent == root {
                break;
            }
            root = parent;
        }

        let mut cursor = elem.clone();
        while cursor != root {
            let next = self
                .parent
                .get(&cursor)
                .cloned()
                .unwrap_or_else(|| cursor.clone());
            self.parent.insert(cursor, root.clone());
            cursor = next;
        }

        root
    }

    fn union(&mut self, a: &QuotientElem, b: &QuotientElem) {
        let root_a = self.find(a);
        let root_b = self.find(b);
        if root_a == root_b {
            return;
        }
        let (keep, absorbed) = if root_a < root_b {
            (root_a, root_b)
        } else {
            (root_b, root_a)
        };
        self.parent.insert(absorbed, keep);
    }
}

/// The vertex quotient a pushout computes: for each schema, the map sending
/// one of its vertex IDs to the ID of its class in the merged schema.
struct VertexQuotient {
    /// Left vertex ID to merged vertex ID.
    left: HashMap<Name, Name>,
    /// Right vertex ID to merged vertex ID.
    right: HashMap<Name, Name>,
}

/// Build the vertex quotient the overlap generates.
///
/// The overlap's pairs are a relation, not a function: one right vertex may be
/// paired with several left vertices and vice versa, and identifying two edges
/// identifies their endpoints. The pushout is the quotient by the equivalence
/// that relation generates, so the pairs are closed under a union-find over the
/// disjoint union of both vertex sets before any name is chosen. Each class
/// takes the name of its least left vertex; a class with no left member keeps
/// its right vertex's name, prefixed with `"right."` when that name is already
/// a left vertex ID.
///
/// # Errors
///
/// Returns [`SchemaError::VertexNotFound`] if an overlap vertex pair names a
/// vertex absent from the schema it is drawn from, and
/// [`SchemaError::OverlapEdgeNotFound`] if an overlap edge pair names an edge
/// absent from the schema it is drawn from.
fn build_vertex_quotient(
    left: &Schema,
    right: &Schema,
    overlap: &SchemaOverlap,
) -> Result<VertexQuotient, SchemaError> {
    let mut uf = VertexUnionFind::new();

    for (left_id, right_id) in &overlap.vertex_pairs {
        if !left.vertices.contains_key(left_id) {
            return Err(SchemaError::VertexNotFound(left_id.to_string()));
        }
        if !right.vertices.contains_key(right_id) {
            return Err(SchemaError::VertexNotFound(right_id.to_string()));
        }
        uf.union(&(false, left_id.clone()), &(true, right_id.clone()));
    }

    for (left_edge, right_edge) in &overlap.edge_pairs {
        if !left.edges.contains_key(left_edge) {
            return Err(SchemaError::OverlapEdgeNotFound {
                side: "left",
                src: left_edge.src.to_string(),
                tgt: left_edge.tgt.to_string(),
                kind: left_edge.kind.to_string(),
            });
        }
        if !right.edges.contains_key(right_edge) {
            return Err(SchemaError::OverlapEdgeNotFound {
                side: "right",
                src: right_edge.src.to_string(),
                tgt: right_edge.tgt.to_string(),
                kind: right_edge.kind.to_string(),
            });
        }
        uf.union(
            &(false, left_edge.src.clone()),
            &(true, right_edge.src.clone()),
        );
        uf.union(
            &(false, left_edge.tgt.clone()),
            &(true, right_edge.tgt.clone()),
        );
    }

    let class_name = |uf: &mut VertexUnionFind, elem: QuotientElem| -> Name {
        let (from_right, id) = uf.find(&elem);
        if from_right && left.vertices.contains_key(&id) {
            Name::from(format!("right.{id}"))
        } else {
            id
        }
    };

    let mut left_map: HashMap<Name, Name> = HashMap::with_capacity(left.vertices.len());
    for id in sorted_names(left.vertices.keys()) {
        let name = class_name(&mut uf, (false, id.clone()));
        left_map.insert(id, name);
    }

    let mut right_map: HashMap<Name, Name> = HashMap::with_capacity(right.vertices.len());
    for id in sorted_names(right.vertices.keys()) {
        let name = class_name(&mut uf, (true, id.clone()));
        right_map.insert(id, name);
    }

    Ok(VertexQuotient {
        left: left_map,
        right: right_map,
    })
}

/// Clone an iterator of names into a vector sorted by name.
///
/// Merging walks both schemas in this order so that whichever side lands first
/// in a merged bucket does not depend on the hash seed.
fn sorted_names<'a>(names: impl Iterator<Item = &'a Name>) -> Vec<Name> {
    let mut out: Vec<Name> = names.cloned().collect();
    out.sort_unstable();
    out
}

/// Clone an iterator of edges into a vector sorted by edge.
fn sorted_edges<'a>(edges: impl Iterator<Item = &'a Edge>) -> Vec<Edge> {
    let mut out: Vec<Edge> = edges.cloned().collect();
    out.sort_unstable();
    out
}

/// Build merged vertices and the left/right vertex morphism maps.
///
/// Both sides are walked in name order, so when a class draws its kind and
/// NSID from whichever member lands first, that member is fixed rather than
/// hash-dependent. Left members are considered before right ones.
fn build_merged_vertices(
    left: &Schema,
    right: &Schema,
    quotient: &VertexQuotient,
) -> (
    HashMap<Name, Vertex>,
    HashMap<Name, Name>,
    HashMap<Name, Name>,
) {
    let mut merged: HashMap<Name, Vertex> = HashMap::new();

    let mut absorb = |source: &Schema, rename: &HashMap<Name, Name>| {
        for id in sorted_names(source.vertices.keys()) {
            let Some(v) = source.vertices.get(&id) else {
                continue;
            };
            let merged_id = resolve(rename, &id);
            merged.entry(merged_id.clone()).or_insert_with(|| Vertex {
                id: merged_id,
                kind: v.kind.clone(),
                nsid: v.nsid.clone(),
            });
        }
    };
    absorb(left, &quotient.left);
    absorb(right, &quotient.right);

    (merged, quotient.left.clone(), quotient.right.clone())
}

/// Build merged edges and the left/right edge morphism maps.
///
/// Every edge is carried through its own side's vertex quotient, so an edge's
/// image runs between the images of its endpoints. An identified right edge
/// takes the image of the left edge it is paired with, which the endpoint
/// closure in [`build_vertex_quotient`] has already made compatible.
fn build_merged_edges(
    left: &Schema,
    right: &Schema,
    overlap: &SchemaOverlap,
    quotient: &VertexQuotient,
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

    for edge in sorted_edges(left.edges.keys()) {
        let Some(kind) = left.edges.get(&edge) else {
            continue;
        };
        let remapped = remap_edge(&edge, &quotient.left);
        merged
            .entry(remapped.clone())
            .or_insert_with(|| kind.clone());
        left_map.insert(edge, remapped);
    }

    for edge in sorted_edges(right.edges.keys()) {
        let Some(kind) = right.edges.get(&edge) else {
            continue;
        };
        let remapped = right_edge_to_left.get(&edge).map_or_else(
            || remap_edge(&edge, &quotient.right),
            |left_edge| remap_edge(left_edge, &quotient.left),
        );
        merged
            .entry(remapped.clone())
            .or_insert_with(|| kind.clone());
        right_map.insert(edge, remapped);
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
    quotient: &VertexQuotient,
) -> Vec<Edge> {
    let mut out: Vec<Edge> = Vec::with_capacity(merged_edges.len());
    let mut seen: std::collections::HashSet<Edge> = std::collections::HashSet::new();

    let mut push = |edge: Edge| {
        if merged_edges.contains_key(&edge) && seen.insert(edge.clone()) {
            out.push(edge);
        }
    };

    for edge in ordered_edges(left) {
        push(remap_edge(&edge, &quotient.left));
    }
    for edge in ordered_edges(right) {
        push(remap_edge(&edge, &quotient.right));
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

/// Look up a vertex ID through one side's rename map, falling back to identity.
fn resolve(rename: &HashMap<Name, Name>, id: &Name) -> Name {
    rename.get(id).cloned().unwrap_or_else(|| id.clone())
}

/// Merge vertex-keyed maps (constraints, nsids, variants, etc.) from both
/// schemas, rewriting every key and every embedded vertex reference through
/// that side's half of the vertex quotient.
///
/// Both sides are walked in key order, so a key that two vertices of one schema
/// now share resolves the same way in every process. Left entries are absorbed
/// first, so a value that only one side can supply comes from the left when
/// both offer one.
fn merge_vertex_keyed(
    left: &Schema,
    right: &Schema,
    quotient: &VertexQuotient,
) -> MergedVertexKeyed {
    let sides: [(&Schema, &HashMap<Name, Name>); 2] =
        [(left, &quotient.left), (right, &quotient.right)];

    // Constraints
    let mut constraints: HashMap<Name, Vec<crate::schema::Constraint>> = HashMap::new();
    for (schema, rename) in sides {
        for id in sorted_names(schema.constraints.keys()) {
            let Some(cs) = schema.constraints.get(&id) else {
                continue;
            };
            let entry = constraints.entry(resolve(rename, &id)).or_default();
            for c in cs {
                if !entry.contains(c) {
                    entry.push(c.clone());
                }
            }
        }
    }

    // Required edges
    let mut required: HashMap<Name, Vec<Edge>> = HashMap::new();
    for (schema, rename) in sides {
        for id in sorted_names(schema.required.keys()) {
            let Some(reqs) = schema.required.get(&id) else {
                continue;
            };
            let entry = required.entry(resolve(rename, &id)).or_default();
            for req in reqs {
                let remapped = remap_edge(req, rename);
                if !entry.contains(&remapped) {
                    entry.push(remapped);
                }
            }
        }
    }

    // NSIDs
    let mut nsids: HashMap<Name, Name> = HashMap::new();
    for (schema, rename) in sides {
        for id in sorted_names(schema.nsids.keys()) {
            let Some(nsid) = schema.nsids.get(&id) else {
                continue;
            };
            nsids
                .entry(resolve(rename, &id))
                .or_insert_with(|| nsid.clone());
        }
    }

    // Variants
    let mut variants: HashMap<Name, Vec<crate::schema::Variant>> = HashMap::new();
    for (schema, rename) in sides {
        for id in sorted_names(schema.variants.keys()) {
            let Some(vs) = schema.variants.get(&id) else {
                continue;
            };
            let entry = variants.entry(resolve(rename, &id)).or_default();
            for v in vs {
                // Both the variant's own ID and its parent name vertices, so
                // both pass through the quotient or the variant would point at
                // a vertex the merge renamed away.
                let mut renamed = v.clone();
                renamed.id = resolve(rename, &renamed.id);
                renamed.parent_vertex = resolve(rename, &renamed.parent_vertex);
                if !entry.contains(&renamed) {
                    entry.push(renamed);
                }
            }
        }
    }

    // Nominal
    let mut nominal: HashMap<Name, bool> = HashMap::new();
    for (schema, rename) in sides {
        for id in sorted_names(schema.nominal.keys()) {
            let Some(&nom) = schema.nominal.get(&id) else {
                continue;
            };
            nominal.entry(resolve(rename, &id)).or_insert(nom);
        }
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

/// The pushout's enrichment and structural maps: everything keyed by something
/// other than a plain vertex ID.
struct MergedEnrichments {
    hyper_edges: HashMap<Name, crate::schema::HyperEdge>,
    orderings: HashMap<Edge, u32>,
    recursion_points: HashMap<Name, crate::schema::RecursionPoint>,
    spans: HashMap<Name, crate::schema::Span>,
    usage_modes: HashMap<Edge, crate::schema::UsageMode>,
    coercions: HashMap<(Name, Name), crate::schema::CoercionSpec>,
    mergers: HashMap<Name, panproto_expr::Expr>,
    defaults: HashMap<Name, panproto_expr::Expr>,
    policies: HashMap<Name, panproto_expr::Expr>,
}

/// Merge an edge-keyed map from both schemas, rewriting each key through its
/// own side's half of the quotient.
///
/// Keys are visited in edge order and the left schema is absorbed first, so the
/// side that wins a key both supply is fixed rather than hash-dependent.
fn merge_edge_keyed_map<'a, V: Clone + 'a>(
    sides: [(&'a Schema, &'a HashMap<Name, Name>); 2],
    pick: impl Fn(&'a Schema) -> &'a HashMap<Edge, V>,
) -> HashMap<Edge, V> {
    let mut out: HashMap<Edge, V> = HashMap::new();
    for (schema, rename) in sides {
        let source = pick(schema);
        for edge in sorted_edges(source.keys()) {
            let Some(value) = source.get(&edge) else {
                continue;
            };
            out.entry(remap_edge(&edge, rename))
                .or_insert_with(|| value.clone());
        }
    }
    out
}

/// Merge a vertex-keyed map from both schemas, rewriting each key through its
/// own side's half of the quotient, under the same ordering discipline as
/// [`merge_edge_keyed_map`].
fn merge_vertex_keyed_map<'a, V: Clone + 'a>(
    sides: [(&'a Schema, &'a HashMap<Name, Name>); 2],
    pick: impl Fn(&'a Schema) -> &'a HashMap<Name, V>,
) -> HashMap<Name, V> {
    let mut out: HashMap<Name, V> = HashMap::new();
    for (schema, rename) in sides {
        let source = pick(schema);
        for id in sorted_names(source.keys()) {
            let Some(value) = source.get(&id) else {
                continue;
            };
            out.entry(resolve(rename, &id))
                .or_insert_with(|| value.clone());
        }
    }
    out
}

/// Merge both schemas' hyper-edges.
///
/// The ID names a hyper-edge, so only a right ID colliding with a left one is
/// renamed; the signature names vertices, so it passes through its own side's
/// half of the quotient.
fn merge_hyper_edges(
    sides: [(&Schema, &HashMap<Name, Name>); 2],
) -> HashMap<Name, crate::schema::HyperEdge> {
    let mut out: HashMap<Name, crate::schema::HyperEdge> = HashMap::new();
    for (schema, rename) in sides {
        for id in sorted_names(schema.hyper_edges.keys()) {
            let Some(he) = schema.hyper_edges.get(&id) else {
                continue;
            };
            let mid = if out.contains_key(&id) {
                Name::from(format!("right.{id}"))
            } else {
                id.clone()
            };
            let mut renamed = he.clone();
            renamed.id = mid.clone();
            renamed.signature = renamed
                .signature
                .into_iter()
                .map(|(label, vid)| {
                    let merged = resolve(rename, &vid);
                    (label, merged)
                })
                .collect();
            out.insert(mid, renamed);
        }
    }
    out
}

/// Merge both schemas' spans. The ID names a span, the legs name vertices.
fn merge_spans(sides: [(&Schema, &HashMap<Name, Name>); 2]) -> HashMap<Name, crate::schema::Span> {
    let mut out: HashMap<Name, crate::schema::Span> = HashMap::new();
    for (schema, rename) in sides {
        for id in sorted_names(schema.spans.keys()) {
            let Some(sp) = schema.spans.get(&id) else {
                continue;
            };
            let mid = if out.contains_key(&id) {
                Name::from(format!("right.{id}"))
            } else {
                id.clone()
            };
            let mut renamed = sp.clone();
            renamed.id = mid.clone();
            renamed.left = resolve(rename, &renamed.left);
            renamed.right = resolve(rename, &renamed.right);
            out.insert(mid, renamed);
        }
    }
    out
}

/// Merge both schemas' recursion points.
///
/// The marker vertex is the key, so renaming it renames the entry, and the
/// vertex it unfolds to is renamed with it.
fn merge_recursion_points(
    sides: [(&Schema, &HashMap<Name, Name>); 2],
) -> HashMap<Name, crate::schema::RecursionPoint> {
    let mut out: HashMap<Name, crate::schema::RecursionPoint> = HashMap::new();
    for (schema, rename) in sides {
        for id in sorted_names(schema.recursion_points.keys()) {
            let Some(rp) = schema.recursion_points.get(&id) else {
                continue;
            };
            out.entry(resolve(rename, &id)).or_insert_with(|| {
                let mut renamed = rp.clone();
                renamed.target_vertex = resolve(rename, &renamed.target_vertex);
                renamed
            });
        }
    }
    out
}

/// Merge the enrichment and structural maps of both schemas.
///
/// Every vertex reference inside a value passes through its own side's half of
/// the quotient; keys that name something other than a vertex — a hyper-edge, a
/// span, a vertex kind, a constraint sort — do not.
fn merge_enrichments(
    left: &Schema,
    right: &Schema,
    quotient: &VertexQuotient,
) -> MergedEnrichments {
    let sides: [(&Schema, &HashMap<Name, Name>); 2] =
        [(left, &quotient.left), (right, &quotient.right)];

    // Coercions. The key is a pair of vertex *kinds*, not of vertex IDs, so it
    // does not pass through the quotient: renaming a right vertex that happens
    // to spell a kind would rewrite the key into the vertex namespace and put
    // the coercion beyond every lookup.
    let mut coercions = left.coercions.clone();
    for (key, spec) in &right.coercions {
        coercions.entry(key.clone()).or_insert_with(|| spec.clone());
    }

    // Policies. The key is a constraint sort name, not a vertex ID, so it does
    // not pass through the quotient for the same reason the coercion key does
    // not.
    let mut policies = left.policies.clone();
    for (sort, expr) in &right.policies {
        policies.entry(sort.clone()).or_insert_with(|| expr.clone());
    }

    MergedEnrichments {
        hyper_edges: merge_hyper_edges(sides),
        orderings: merge_edge_keyed_map(sides, |s| &s.orderings),
        recursion_points: merge_recursion_points(sides),
        spans: merge_spans(sides),
        usage_modes: merge_edge_keyed_map(sides, |s| &s.usage_modes),
        coercions,
        mergers: merge_vertex_keyed_map(sides, |s| &s.mergers),
        defaults: merge_vertex_keyed_map(sides, |s| &s.defaults),
        policies,
    }
}

/// Merge every remaining map from both schemas, then assemble the final
/// `Schema` with rebuilt adjacency indices.
fn assemble_pushout(
    left: &Schema,
    right: &Schema,
    quotient: &VertexQuotient,
    mut merged_vertices: HashMap<Name, Vertex>,
    merged_edges: HashMap<Edge, Name>,
) -> Schema {
    let vk = merge_vertex_keyed(left, right, quotient);
    let enrichments = merge_enrichments(left, right, quotient);

    // A schema records each NSID twice: on the vertex and in `nsids`. The two
    // merges disagree about which side wins, because `build_merged_vertices`
    // merges whole vertices while `nsids` is merged per key (an absent left key
    // is filled from the right). Identifying a left vertex that carries no NSID
    // with a right vertex that carries one therefore leaves the vertex saying
    // `None` while the map says the NSID, which every reader of one copy
    // resolves differently from every reader of the other. Deriving the vertex's
    // copy from the merged map puts the two back in step; the map is the more
    // complete of the two by construction, and every constructor writes a row
    // for every vertex that has an NSID, so this never erases one.
    for (id, vertex) in &mut merged_vertices {
        vertex.nsid = vk.nsids.get(id).cloned();
    }

    let idx = build_indices(left, right, &merged_edges, quotient);
    let entries = merge_entries(left, right, quotient);

    Schema {
        protocol: left.protocol.clone(),
        vertices: merged_vertices,
        edges: merged_edges,
        hyper_edges: enrichments.hyper_edges,
        constraints: vk.constraints,
        required: vk.required,
        nsids: vk.nsids,
        entries,
        variants: vk.variants,
        orderings: enrichments.orderings,
        recursion_points: enrichments.recursion_points,
        spans: enrichments.spans,
        usage_modes: enrichments.usage_modes,
        nominal: vk.nominal,
        coercions: enrichments.coercions,
        mergers: enrichments.mergers,
        defaults: enrichments.defaults,
        policies: enrichments.policies,
        outgoing: idx.outgoing,
        incoming: idx.incoming,
        between: idx.between,
    }
}

/// Coproduct of the pointed schemas: union of `left.entries` and the
/// renamed `right.entries`, preserving insertion order and
/// deduplicating. This is the canonical pointing on the pushout.
fn merge_entries(left: &Schema, right: &Schema, quotient: &VertexQuotient) -> Vec<Name> {
    let mut entries: Vec<Name> = Vec::with_capacity(left.entries.len() + right.entries.len());
    let mut seen: std::collections::HashSet<Name> = std::collections::HashSet::new();
    for (schema, rename) in [(left, &quotient.left), (right, &quotient.right)] {
        for id in &schema.entries {
            let merged = resolve(rename, id);
            if seen.insert(merged.clone()) {
                entries.push(merged);
            }
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
    quotient: &VertexQuotient,
) -> AdjacencyIndices {
    let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

    for edge in &ordered_merged_edges(left, right, merged_edges, quotient) {
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

    /// Two left vertices identified with one right vertex are identified with
    /// each other: the overlap relation is closed, not overwritten, so the two
    /// injections commute over the shared vertex.
    #[test]
    fn overlap_identifying_two_left_vertices_quotients_them() {
        let left = build_schema(&[("a", "object"), ("b", "object")], &[]);
        let right = build_schema(&[("x", "object")], &[]);

        let overlap = SchemaOverlap {
            vertex_pairs: vec![
                (Name::from("a"), Name::from("x")),
                (Name::from("b"), Name::from("x")),
            ],
            edge_pairs: vec![],
        };

        let (pushout, left_m, right_m) = schema_pushout(&left, &right, &overlap).unwrap();

        let a_image = left_m.vertex_map.get("a").unwrap();
        let b_image = left_m.vertex_map.get("b").unwrap();
        let x_image = right_m.vertex_map.get("x").unwrap();
        assert_eq!(a_image, x_image, "the a-x identification must commute");
        assert_eq!(b_image, x_image, "the b-x identification must commute");
        assert_eq!(
            pushout.vertex_count(),
            1,
            "a ~ x ~ b is one class, so the pushout has one vertex",
        );
        assert!(pushout.has_vertex(a_image.as_str()));
    }

    /// Identifying two edges identifies their endpoints, so both injections
    /// stay graph homomorphisms: the image of an edge's source is the source
    /// of the edge's image.
    #[test]
    fn overlap_identifying_edges_identifies_their_endpoints() {
        let left = build_schema(
            &[("a", "object"), ("b", "string")],
            &[("a", "b", "prop", "p")],
        );
        let right = build_schema(
            &[("x", "object"), ("y", "string")],
            &[("x", "y", "prop", "p")],
        );

        let left_edge = Edge {
            src: Name::from("a"),
            tgt: Name::from("b"),
            kind: Name::from("prop"),
            name: Some(Name::from("p")),
        };
        let right_edge = Edge {
            src: Name::from("x"),
            tgt: Name::from("y"),
            kind: Name::from("prop"),
            name: Some(Name::from("p")),
        };

        let overlap = SchemaOverlap {
            vertex_pairs: vec![],
            edge_pairs: vec![(left_edge.clone(), right_edge.clone())],
        };

        let (pushout, left_m, right_m) = schema_pushout(&left, &right, &overlap).unwrap();

        let image = right_m.edge_map.get(&right_edge).unwrap();
        assert_eq!(
            &image.src,
            right_m.vertex_map.get("x").unwrap(),
            "the right injection must send the edge's source to the image edge's source",
        );
        assert_eq!(
            &image.tgt,
            right_m.vertex_map.get("y").unwrap(),
            "the right injection must send the edge's target to the image edge's target",
        );
        assert_eq!(
            left_m.edge_map.get(&left_edge).unwrap(),
            image,
            "the identified edges share one image",
        );
        assert_eq!(pushout.vertex_count(), 2);
        assert_eq!(pushout.edge_count(), 1);
    }

    /// An overlap naming an edge that is not in the schema it is drawn from is
    /// rejected rather than being used to identify vertices that do not exist.
    #[test]
    fn overlap_edge_absent_from_its_schema_is_rejected() {
        let left = build_schema(&[("a", "object"), ("b", "string")], &[]);
        let right = build_schema(&[("x", "object"), ("y", "string")], &[]);

        let phantom_left = Edge {
            src: Name::from("a"),
            tgt: Name::from("b"),
            kind: Name::from("prop"),
            name: Some(Name::from("p")),
        };
        let phantom_right = Edge {
            src: Name::from("x"),
            tgt: Name::from("y"),
            kind: Name::from("prop"),
            name: Some(Name::from("p")),
        };

        let overlap = SchemaOverlap {
            vertex_pairs: vec![],
            edge_pairs: vec![(phantom_left, phantom_right)],
        };

        let err = schema_pushout(&left, &right, &overlap).unwrap_err();
        assert!(
            matches!(err, SchemaError::OverlapEdgeNotFound { .. }),
            "expected OverlapEdgeNotFound, got {err:?}",
        );
    }
}
