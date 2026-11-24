//! The complete finite scope of the schema-merge page.

use crate::pages::schema_merge::Schema;

/// The vertex ids of the scope.
pub const VERTEX_IDS: &[&str] = &["a", "b"];

/// The vertex kinds of the scope.
pub const VERTEX_KINDS: &[&str] = &["object", "string"];

/// Every schema of the scope.
///
/// Each id of [`VERTEX_IDS`] is absent or present
/// with a kind of [`VERTEX_KINDS`], and any set of edges between present
/// vertices, loops included. Labelled: no two listed schemas are equal.
#[must_use]
pub fn schemas() -> Vec<Schema> {
    let mut vertex_sets = vec![Vec::<(&str, &str)>::new()];
    for id in VERTEX_IDS {
        vertex_sets = vertex_sets
            .into_iter()
            .flat_map(|prefix| {
                let mut grown = vec![prefix.clone()];
                for kind in VERTEX_KINDS {
                    let mut with = prefix.clone();
                    with.push((id, kind));
                    grown.push(with);
                }
                grown
            })
            .collect();
    }
    let mut out = Vec::new();
    for vertices in vertex_sets {
        let possible: Vec<(String, String)> = vertices
            .iter()
            .flat_map(|(src, _)| {
                vertices
                    .iter()
                    .map(move |(tgt, _)| ((*src).to_owned(), (*tgt).to_owned()))
            })
            .collect();
        for edges in super::subsets(&possible, possible.len()) {
            out.push(Schema {
                vertices: vertices
                    .iter()
                    .map(|(id, kind)| ((*id).to_owned(), (*kind).to_owned()))
                    .collect(),
                edges: edges.into_iter().collect(),
            });
        }
    }
    out
}

/// The number of schemas [`schemas`] lists.
///
/// Counted by vertex set: the empty
/// schema, two ids each of two kinds alone with any of the two subsets of
/// its loop, and four kind pairs each with any of the sixteen subsets of
/// the four edges between two vertices.
pub const SCHEMAS_COUNT: usize = 1 + 2 * 2 * 2 + 4 * 16;

/// The bytes repository format markers are drawn from: three digits, a line
/// feed, a space, and a byte that is not UTF-8.
pub const MARKER_BYTES: &[u8] = &[b'0', b'1', b'2', b'\n', b' ', 0xff];

/// Every format marker of the scope: an absent marker, and every byte string
/// of at most `max_len` bytes of [`MARKER_BYTES`].
#[must_use]
pub fn format_markers(max_len: usize) -> Vec<Option<Vec<u8>>> {
    let mut out = vec![None];
    let mut layer = vec![Vec::new()];
    for _ in 0..=max_len {
        let mut next = Vec::new();
        for marker in layer {
            for byte in MARKER_BYTES {
                let mut longer = marker.clone();
                longer.push(*byte);
                next.push(longer);
            }
            out.push(Some(marker));
        }
        layer = next;
    }
    out
}
