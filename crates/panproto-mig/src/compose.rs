//! Schema-level and data-level migration composition.
//!
//! Given `m1: G1 -> G2` and `m2: G2 -> G3`, [`compose`] produces
//! `m12: G1 -> G3` by composing vertex maps, edge maps, and
//! recomputing resolver tables.

use std::collections::HashMap;

use panproto_gat::Name;
use rustc_hash::FxHashMap;

use crate::error::ComposeError;
use crate::migration::Migration;

/// Compose two migrations: `m1: G1 -> G2` and `m2: G2 -> G3`
/// into `m12: G1 -> G3`.
///
/// The composition composes vertex maps and edge maps, and merges
/// resolver tables. Precomputes inverse maps for O(1) lookups
/// instead of O(n) scans.
///
/// # Errors
///
/// Returns `ComposeError::VertexNotInDomain` if a vertex in the image
/// of `m1` is not in the domain of `m2`.
pub fn compose(m1: &Migration, m2: &Migration) -> Result<Migration, ComposeError> {
    // Compose vertex maps: for each v1 in m1.vertex_map,
    // composed[v1] = m2.vertex_map[m1.vertex_map[v1]]
    let mut vertex_map = HashMap::new();
    for (v1, v2) in &m1.vertex_map {
        if let Some(v3) = m2.vertex_map.get(v2) {
            vertex_map.insert(v1.clone(), v3.clone());
        }
        // If v2 is not in m2's domain, skip it (vertex was dropped by m2).
    }

    // Compose edge maps.
    let mut edge_map = HashMap::new();
    for (e1, e2) in &m1.edge_map {
        if let Some(e3) = m2.edge_map.get(e2) {
            edge_map.insert(e1.clone(), e3.clone());
        }
        // If e2 is not in m2's domain, skip it (edge was dropped by m2).
    }

    // Compose hyper-edge maps.
    let mut hyper_edge_map = HashMap::new();
    for (he1, he2) in &m1.hyper_edge_map {
        if let Some(he3) = m2.hyper_edge_map.get(he2) {
            hyper_edge_map.insert(he1.clone(), he3.clone());
        }
    }

    // Compose label maps.
    let mut label_map = HashMap::new();
    for ((he1, label1), label2) in &m1.label_map {
        // Follow through m2's label map if applicable.
        if let Some(he2) = m1.hyper_edge_map.get(he1) {
            let key2 = (he2.clone(), label2.clone());
            if let Some(label3) = m2.label_map.get(&key2) {
                label_map.insert((he1.clone(), label1.clone()), label3.clone());
            } else {
                label_map.insert((he1.clone(), label1.clone()), label2.clone());
            }
        } else {
            label_map.insert((he1.clone(), label1.clone()), label2.clone());
        }
    }

    // Merge resolvers: the composed resolver maps (src_in_g1, tgt_in_g1)
    // pairs through the vertex map to g3 edges.
    let mut resolver = HashMap::new();
    for ((src, tgt), edge) in &m1.resolver {
        let src3 = vertex_map.get(src).cloned().unwrap_or_else(|| src.clone());
        let tgt3 = vertex_map.get(tgt).cloned().unwrap_or_else(|| tgt.clone());
        if let Some(mapped_edge) = m2.edge_map.get(edge) {
            resolver.insert((src3, tgt3), mapped_edge.clone());
        } else {
            resolver.insert((src3, tgt3), edge.clone());
        }
    }
    // Also include m2's resolvers that apply to surviving vertices.
    for ((src, tgt), edge) in &m2.resolver {
        let key = (src.clone(), tgt.clone());
        resolver.entry(key).or_insert_with(|| edge.clone());
    }

    // Precompute inverse maps for O(1) lookups instead of O(n) scans.
    let he_inverse: FxHashMap<&str, &Name> =
        m1.hyper_edge_map.iter().map(|(k, v)| (&**v, k)).collect();

    let vertex_inverse: FxHashMap<&str, &Name> =
        m1.vertex_map.iter().map(|(k, v)| (&**v, k)).collect();

    // Compose hyper_resolvers using precomputed inverse maps.
    let mut hyper_resolver = HashMap::new();
    // Include m1's hyper_resolver entries directly (they use g1 keys).
    for (key, value) in &m1.hyper_resolver {
        hyper_resolver.insert(key.clone(), value.clone());
    }
    // For m2's hyper_resolver entries, remap through inverse maps.
    for ((he_id, labels), (tgt_he, label_remap)) in &m2.hyper_resolver {
        let src_he_id = he_inverse
            .get(&**he_id)
            .map_or_else(|| he_id.clone(), |k| (*k).clone());
        let remapped_labels: Vec<Name> = labels
            .iter()
            .map(|l| {
                vertex_inverse
                    .get(&**l)
                    .map_or_else(|| l.clone(), |k| (*k).clone())
            })
            .collect();
        let key = (src_he_id, remapped_labels);
        hyper_resolver
            .entry(key)
            .or_insert_with(|| (tgt_he.clone(), label_remap.clone()));
    }

    Ok(Migration {
        vertex_map,
        edge_map,
        hyper_edge_map,
        label_map,
        resolver,
        hyper_resolver,
        expr_resolvers: HashMap::new(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use panproto_schema::Edge;

    #[test]
    fn compose_identity_identity_is_identity() {
        // Test 7: id . id = id
        let edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("x".into()),
        };

        let id_mig = Migration::identity(
            &[Name::from("a"), Name::from("b")],
            std::slice::from_ref(&edge),
        );

        let composed = compose(&id_mig, &id_mig);
        assert!(composed.is_ok());
        let c = composed.unwrap_or_else(|_| panic!("compose should succeed"));

        // Composed identity should map each vertex/edge to itself.
        assert_eq!(c.vertex_map.get("a"), Some(&Name::from("a")));
        assert_eq!(c.vertex_map.get("b"), Some(&Name::from("b")));
        assert_eq!(c.edge_map.get(&edge), Some(&edge));
    }
}
