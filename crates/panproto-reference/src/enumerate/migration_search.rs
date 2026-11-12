//! Complete finite scopes for the migration-search page.

use crate::pages::migration_search::{Edge, Schema};

/// The vertex ids of the scope. `a` and `b` differ in every byte, and `ab`
/// is one edit from each.
pub const VERTEX_IDS: &[&str] = &["a", "ab", "b"];

/// The vertex kinds of the scope.
pub const VERTEX_KINDS: &[&str] = &["object", "string"];

/// The one edge kind of the scope.
pub const EDGE_KIND: &str = "prop";

/// The edge names of the scope: an unnamed edge and the name `n`.
pub const EDGE_NAMES: &[Option<&str>] = &[None, Some("n")];

/// Every schema with at least one and at most `max_vertices` vertices drawn
/// from [`VERTEX_IDS`], each of a kind in [`VERTEX_KINDS`], and at most
/// `max_edges` edges, each of kind [`EDGE_KIND`] with a name in
/// [`EDGE_NAMES`] between an ordered pair of its vertices (loops included).
/// Labelled: no two listed schemas are equal.
#[must_use]
pub fn schemas(max_vertices: usize, max_edges: usize) -> Vec<Schema> {
    let mut out = Vec::new();
    for ids in super::subsets(VERTEX_IDS, max_vertices) {
        if ids.is_empty() {
            continue;
        }
        let mut typed: Vec<Vec<(&str, &str)>> = vec![Vec::new()];
        for id in &ids {
            typed = typed
                .into_iter()
                .flat_map(|prefix| {
                    VERTEX_KINDS.iter().map(move |kind| {
                        let mut grown = prefix.clone();
                        grown.push((*id, *kind));
                        grown
                    })
                })
                .collect();
        }
        let mut possible = Vec::new();
        for from in &ids {
            for to in &ids {
                for name in EDGE_NAMES {
                    possible.push(Edge::new(from, to, EDGE_KIND, *name));
                }
            }
        }
        for vertices in typed {
            for edges in super::subsets(&possible, max_edges) {
                out.push(Schema {
                    vertices: vertices
                        .iter()
                        .map(|(id, kind)| ((*id).to_owned(), (*kind).to_owned()))
                        .collect(),
                    edges: edges.into_iter().collect(),
                });
            }
        }
    }
    out
}

/// The count of [`schemas`]:
/// `sum_{1 <= k <= max_vertices} C(3, k) 2^k sum_{j <= max_edges} C(2 k^2, j)`,
/// three ids, two kinds and `k^2` ordered vertex pairs carrying either of
/// two names.
#[must_use]
pub fn schemas_count(max_vertices: u64, max_edges: u64) -> u64 {
    (1..=max_vertices.min(3))
        .map(|k| {
            let edge_sets: u64 = (0..=max_edges).map(|j| super::choose(2 * k * k, j)).sum();
            super::choose(3, k) * 2_u64.pow(u32::try_from(k).unwrap_or(0)) * edge_sets
        })
        .sum()
}
