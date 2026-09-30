//! Composing the contraction keys of two compiled migrations.
//!
//! A [`CompiledMigration`] resolves contracted and expanded pairs through two
//! maps keyed in its *target* space: the resolver (a pair of target anchors to
//! the target edge a contracted pair becomes) and `expansion_path` (a pair of
//! target anchors to the intermediate target anchors an expanded arc passes
//! through). When two migrations run one after the other, the first one's
//! target space is the second one's source space, so the first one's entries
//! name vertices and edges the second may rename or drop. The composite is
//! keyed in the second one's target space; [`compose_contraction_keys`]
//! carries the first one's entries there.

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_schema::Edge;

use crate::wtype::CompiledMigration;

/// The contraction keys of a composite migration: its resolver, its
/// expansion paths, and the keys of the first migration that do not survive
/// the second.
pub type ComposedContractionKeys = (
    HashMap<(Name, Name), Edge>,
    HashMap<(Name, Name), Vec<Name>>,
    Vec<(Name, Name)>,
);

/// Carry a vertex of `m2`'s source space into its target space, or `None`
/// when `m2` drops it. `m2.surviving_verts` is keyed in `m2`'s target space
/// (the image of its vertex map), so the translated vertex is tested.
#[must_use]
pub fn translate_vertex(m2: &CompiledMigration, v: &Name) -> Option<Name> {
    let translated = m2.vertex_remap.get(v).unwrap_or(v);
    m2.surviving_verts
        .contains(translated)
        .then(|| translated.clone())
}

/// Carry an edge of `m2`'s source space into its target space: its entry
/// in `m2.edge_remap`, or the edge itself when it is in
/// `m2.surviving_edges` (keyed in `m2`'s target space). `None` otherwise.
#[must_use]
pub fn translate_edge(m2: &CompiledMigration, e: &Edge) -> Option<Edge> {
    if let Some(mapped) = m2.edge_remap.get(e) {
        return Some(mapped.clone());
    }
    m2.surviving_edges.contains(e).then(|| e.clone())
}

/// The resolver and expansion paths of the composite of `m1` then `m2`,
/// keyed in `m2`'s target space, with the keys of `m1` that `m2` drops.
///
/// Each entry of `m1` names vertices of `m1`'s target space, which is
/// `m2`'s source space. Its key vertices and each intermediate anchor are
/// translated by `v -> m2.vertex_remap.get(v).unwrap_or(v)`, and the entry
/// is kept only when every translated vertex is in `m2.surviving_verts`
/// (like `m2.surviving_edges`, keyed in `m2`'s target space). A resolver
/// edge is mapped through `m2.edge_remap`, or kept unchanged when it is in
/// `m2.surviving_edges`.
/// An entry that fails either test is left out, and its `m1` key is
/// reported in the third component (ascending, one per key even when both
/// maps hold it). `m2`'s own entries are then added unchanged, and on a key
/// both define `m2`'s wins.
///
/// ```
/// use std::collections::{HashMap, HashSet};
///
/// use panproto_gat::Name;
/// use panproto_inst::CompiledMigration;
/// use panproto_inst::contraction_keys::compose_contraction_keys;
/// use panproto_schema::Edge;
///
/// let edge = |src: &str, tgt: &str| Edge {
///     src: src.into(), tgt: tgt.into(), kind: "prop".into(), name: Some("x".into()),
/// };
/// let names = |vs: &[&str]| vs.iter().map(|v| Name::from(*v)).collect::<HashSet<_>>();
/// let m1 = CompiledMigration {
///     surviving_verts: names(&["r", "leaf"]),
///     resolver: HashMap::from([(("r".into(), "leaf".into()), edge("r", "leaf"))]),
///     ..CompiledMigration::default()
/// };
/// // m2 renames `leaf` to `item` and carries the edge across.
/// let m2 = CompiledMigration {
///     surviving_verts: names(&["r", "item"]),
///     surviving_edges: HashSet::from([edge("r", "item")]),
///     vertex_remap: HashMap::from([("leaf".into(), "item".into())]),
///     edge_remap: HashMap::from([(edge("r", "leaf"), edge("r", "item"))]),
///     ..CompiledMigration::default()
/// };
/// let (resolver, paths, dropped) = compose_contraction_keys(&m1, &m2);
/// assert_eq!(resolver[&("r".into(), "item".into())], edge("r", "item"));
/// assert!(paths.is_empty() && dropped.is_empty());
/// ```
#[must_use]
pub fn compose_contraction_keys(
    m1: &CompiledMigration,
    m2: &CompiledMigration,
) -> ComposedContractionKeys {
    let translate_key = |(a, b): &(Name, Name)| -> Option<(Name, Name)> {
        Some((translate_vertex(m2, a)?, translate_vertex(m2, b)?))
    };

    let mut dropped: Vec<(Name, Name)> = Vec::new();

    let mut resolver: HashMap<(Name, Name), Edge> = HashMap::new();
    for (key, edge) in &m1.resolver {
        match translate_key(key).zip(translate_edge(m2, edge)) {
            Some((translated, mapped)) => {
                resolver.insert(translated, mapped);
            }
            None => dropped.push(key.clone()),
        }
    }

    let mut expansion_path: HashMap<(Name, Name), Vec<Name>> = HashMap::new();
    for (key, intermediates) in &m1.expansion_path {
        let translated_mids: Option<Vec<Name>> = intermediates
            .iter()
            .map(|v| translate_vertex(m2, v))
            .collect();
        match translate_key(key).zip(translated_mids) {
            Some((translated, mids)) => {
                expansion_path.insert(translated, mids);
            }
            None => dropped.push(key.clone()),
        }
    }

    for (key, edge) in &m2.resolver {
        resolver.insert(key.clone(), edge.clone());
    }
    for (key, mids) in &m2.expansion_path {
        expansion_path.insert(key.clone(), mids.clone());
    }

    dropped.sort();
    dropped.dedup();
    (resolver, expansion_path, dropped)
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use panproto_gat::Name;
    use panproto_schema::Edge;

    use super::compose_contraction_keys;
    use crate::wtype::CompiledMigration;

    fn edge(src: &str, tgt: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "prop".into(),
            name: Some("x".into()),
        }
    }

    fn names(vs: &[&str]) -> HashSet<Name> {
        vs.iter().map(|v| Name::from(*v)).collect()
    }

    fn key(a: &str, b: &str) -> (Name, Name) {
        (a.into(), b.into())
    }

    fn m1() -> CompiledMigration {
        CompiledMigration {
            surviving_verts: names(&["r", "leaf"]),
            resolver: HashMap::from([(key("r", "leaf"), edge("r", "leaf"))]),
            expansion_path: HashMap::from([(key("r", "c"), vec!["mid".into()])]),
            ..CompiledMigration::default()
        }
    }

    #[test]
    fn composing_with_a_non_renaming_migration_keeps_every_contraction_key() {
        let m2 = CompiledMigration {
            surviving_verts: names(&["r", "leaf", "c", "mid"]),
            surviving_edges: HashSet::from([edge("r", "leaf")]),
            ..CompiledMigration::default()
        };
        let (resolver, paths, dropped) = compose_contraction_keys(&m1(), &m2);
        assert_eq!(resolver, m1().resolver);
        assert_eq!(paths, m1().expansion_path);
        assert!(dropped.is_empty());
    }

    #[test]
    fn compose_contraction_keys_reports_dropped_keys() {
        // m2 drops `leaf` (so the resolver key goes) and `mid` (so the
        // expansion path goes).
        let m2 = CompiledMigration {
            surviving_verts: names(&["r", "c"]),
            surviving_edges: HashSet::from([edge("r", "leaf")]),
            ..CompiledMigration::default()
        };
        let (resolver, paths, dropped) = compose_contraction_keys(&m1(), &m2);
        assert!(resolver.is_empty());
        assert!(paths.is_empty());
        assert_eq!(dropped, vec![key("r", "c"), key("r", "leaf")]);

        // Endpoints survive but the resolved edge does not: dropped too.
        let m2 = CompiledMigration {
            surviving_verts: names(&["r", "leaf", "c", "mid"]),
            ..CompiledMigration::default()
        };
        let (resolver, _, dropped) = compose_contraction_keys(&m1(), &m2);
        assert!(resolver.is_empty());
        assert_eq!(dropped, vec![key("r", "leaf")]);
    }

    #[test]
    fn compose_contraction_keys_maps_the_edge_through_the_edge_remap() {
        let m2 = CompiledMigration {
            surviving_verts: names(&["r", "leaf", "c", "m2"]),
            vertex_remap: HashMap::from([("mid".into(), "m2".into())]),
            edge_remap: HashMap::from([(edge("r", "leaf"), edge("r", "item"))]),
            ..CompiledMigration::default()
        };
        let (resolver, paths, dropped) = compose_contraction_keys(&m1(), &m2);
        assert_eq!(resolver[&key("r", "leaf")], edge("r", "item"));
        assert_eq!(paths[&key("r", "c")], vec![Name::from("m2")]);
        assert!(dropped.is_empty());
    }

    #[test]
    fn compose_contraction_keys_prefers_the_second_migration() {
        let m2 = CompiledMigration {
            surviving_verts: names(&["r", "leaf", "c", "mid"]),
            surviving_edges: HashSet::from([edge("r", "leaf")]),
            resolver: HashMap::from([(key("r", "leaf"), edge("r", "other"))]),
            expansion_path: HashMap::from([
                (key("r", "c"), vec!["other".into()]),
                (key("a", "b"), vec!["i".into()]),
            ]),
            ..CompiledMigration::default()
        };
        let (resolver, paths, _) = compose_contraction_keys(&m1(), &m2);
        assert_eq!(resolver[&key("r", "leaf")], edge("r", "other"));
        assert_eq!(paths[&key("r", "c")], vec![Name::from("other")]);
        assert_eq!(paths[&key("a", "b")], vec![Name::from("i")]);
    }
}
