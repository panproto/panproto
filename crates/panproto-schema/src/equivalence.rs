//! Kind-multiset witnesses for schema equivalence under round-tripping.
//!
//! The parse / decorate / emit lens preserves the **kind multiset** of
//! a schema, not its precise vertex IDs (the parser invents fresh ids)
//! nor its precise byte spans (those are layout, not content). This
//! module hosts the canonical witnesses used by the law-checker
//! harness in `panproto-lens`.

use std::collections::BTreeMap;

use panproto_gat::Name;

use crate::Schema;

/// Vertex-kind multiset.
///
/// Maps each vertex kind to its number of occurrences. Together with
/// [`edge_multiset`], this is a complete witness for the abstract-
/// content equivalence used by the `Section` and `AbstractRoundTrip`
/// lens laws.
#[must_use]
pub fn kind_multiset(schema: &Schema) -> BTreeMap<String, usize> {
    let mut map = BTreeMap::new();
    for v in schema.vertices.values() {
        *map.entry(v.kind.to_string()).or_insert(0) += 1;
    }
    map
}

/// Edge-shape multiset: counts of `(src_kind, edge_kind, tgt_kind)` triples.
///
/// Projects each edge to its *kind signature* — what kinds of vertices
/// it connects via which edge kind. This is the appropriate level of
/// granularity for round-trip laws: the parser invents fresh vertex
/// IDs but must preserve the shape of every edge.
#[must_use]
pub fn edge_multiset(schema: &Schema) -> BTreeMap<(String, String, String), usize> {
    let mut map: BTreeMap<(String, String, String), usize> = BTreeMap::new();
    for edge in schema.edges.keys() {
        let src_kind = vertex_kind(schema, &edge.src);
        let tgt_kind = vertex_kind(schema, &edge.tgt);
        *map.entry((src_kind, edge.kind.to_string(), tgt_kind))
            .or_insert(0) += 1;
    }
    map
}

fn vertex_kind(schema: &Schema, id: &Name) -> String {
    schema
        .vertices
        .get(id)
        .map(|v| v.kind.to_string())
        .unwrap_or_default()
}
