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

/// Compose `hyper_resolver` tables from two migrations.
///
/// Keys are `(source_he_id, source_labels)`, values are
/// `(target_he_id, label_remap)`. For m1: keys are G1 source, values
/// are G2 target. We chase values through m2 to get G3 target.
#[allow(clippy::type_complexity)]
fn compose_hyper_resolvers(
    m1: &Migration,
    m2: &Migration,
) -> HashMap<(Name, Vec<Name>), (Name, HashMap<Name, Name>)> {
    let he_inverse: FxHashMap<&str, &Name> =
        m1.hyper_edge_map.iter().map(|(k, v)| (&**v, k)).collect();
    let vertex_inverse: FxHashMap<&str, &Name> =
        m1.vertex_map.iter().map(|(k, v)| (&**v, k)).collect();

    let mut hyper_resolver = HashMap::new();
    for ((he1, labels1), (he2_tgt, label_remap1)) in &m1.hyper_resolver {
        if let Some(he3_tgt) = m2.hyper_edge_map.get(he2_tgt) {
            let mut composed_remap = HashMap::new();
            for (l_src, l_g2) in label_remap1 {
                let l_g3 = m2.vertex_map.get(l_g2).unwrap_or(l_g2);
                composed_remap.insert(l_src.clone(), l_g3.clone());
            }
            hyper_resolver.insert(
                (he1.clone(), labels1.clone()),
                (he3_tgt.clone(), composed_remap),
            );
        }
    }
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
    hyper_resolver
}

/// Compose two migrations: `m1: G1 -> G2` and `m2: G2 -> G3`
/// into `m12: G1 -> G3`.
///
/// The composition composes vertex maps, edge maps, hyper-edge maps,
/// label maps, resolver tables, and expression resolvers. Precomputes
/// inverse maps for O(1) lookups instead of O(n) scans.
///
/// Partial-map semantics: if a vertex in the image of `m1` is not in
/// the domain of `m2`, it is silently dropped from the composed map.
/// This is intentional; the vertex was removed by `m2` and should not
/// appear in the composed migration. The same applies to edges and
/// hyper-edges.
///
/// # Errors
///
/// Returns `ComposeError` if composition fails.
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

    // Compose resolvers. Resolver keys are in the TARGET vertex space
    // (used by wtype_restrict after anchor remapping). So m1's resolver
    // keys are G2 vertices; remap to G3 via m2.vertex_map. Edges are
    // also in target space and need remapping via m2.edge_map.
    let mut resolver = HashMap::new();
    for ((src, tgt), edge) in &m1.resolver {
        // Remap G2 key vertices to G3 via m2.vertex_map.
        // If either vertex was dropped by m2, the resolver entry is invalid.
        let Some(src3) = m2.vertex_map.get(src) else {
            continue;
        };
        let Some(tgt3) = m2.vertex_map.get(tgt) else {
            continue;
        };
        // Remap G2 edge to G3 via m2.edge_map.
        if let Some(mapped_edge) = m2.edge_map.get(edge) {
            resolver.insert((src3.clone(), tgt3.clone()), mapped_edge.clone());
        }
        // If the edge was dropped by m2, skip this entry.
    }
    // m2's resolver entries are already in G3 space.
    for ((src, tgt), edge) in &m2.resolver {
        resolver
            .entry((src.clone(), tgt.clone()))
            .or_insert_with(|| edge.clone());
    }

    let hyper_resolver = compose_hyper_resolvers(m1, m2);

    // Compose expr_resolvers. Same key convention as the binary resolver
    // (TARGET vertex space): m1's keys are G2, remap to G3 via m2.vertex_map.
    let mut expr_resolvers = HashMap::new();
    for ((src, tgt), expr) in &m1.expr_resolvers {
        let Some(src3) = m2.vertex_map.get(src) else {
            continue;
        };
        let Some(tgt3) = m2.vertex_map.get(tgt) else {
            continue;
        };
        expr_resolvers.insert((src3.clone(), tgt3.clone()), expr.clone());
    }
    // m2's entries are already in G3 space.
    for ((src, tgt), expr) in &m2.expr_resolvers {
        expr_resolvers
            .entry((src.clone(), tgt.clone()))
            .or_insert_with(|| expr.clone());
    }

    Ok(Migration {
        vertex_map,
        edge_map,
        hyper_edge_map,
        label_map,
        resolver,
        hyper_resolver,
        expr_resolvers,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::redundant_clone)]
mod tests {
    use super::*;
    use panproto_schema::Edge;

    fn edge(src: &str, tgt: &str, kind: &str, name: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: kind.into(),
            name: Some(name.into()),
        }
    }

    #[test]
    fn compose_identity_identity_is_identity() {
        let e = edge("a", "b", "prop", "x");

        let id_mig = Migration::identity(
            &[Name::from("a"), Name::from("b")],
            std::slice::from_ref(&e),
        );

        let composed = compose(&id_mig, &id_mig);
        assert!(composed.is_ok());
        let c = composed.unwrap_or_else(|_| panic!("compose should succeed"));

        assert_eq!(c.vertex_map.get("a"), Some(&Name::from("a")));
        assert_eq!(c.vertex_map.get("b"), Some(&Name::from("b")));
        assert_eq!(c.edge_map.get(&e), Some(&e));
    }

    #[test]
    fn compose_left_identity() {
        // id ; m = m
        let e_ab = edge("a", "b", "prop", "x");
        let e_cd = edge("c", "d", "prop", "y");

        let id_mig = Migration::identity(
            &[Name::from("a"), Name::from("b")],
            std::slice::from_ref(&e_ab),
        );

        let m = Migration {
            vertex_map: HashMap::from([("a".into(), "c".into()), ("b".into(), "d".into())]),
            edge_map: HashMap::from([(e_ab.clone(), e_cd.clone())]),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
        };

        let c = compose(&id_mig, &m).unwrap();
        assert_eq!(c.vertex_map.get("a"), Some(&Name::from("c")));
        assert_eq!(c.vertex_map.get("b"), Some(&Name::from("d")));
        assert_eq!(c.edge_map.get(&e_ab), Some(&e_cd));
    }

    #[test]
    fn compose_right_identity() {
        // m ; id = m
        let e_ab = edge("a", "b", "prop", "x");
        let e_cd = edge("c", "d", "prop", "y");

        let m = Migration {
            vertex_map: HashMap::from([("a".into(), "c".into()), ("b".into(), "d".into())]),
            edge_map: HashMap::from([(e_ab.clone(), e_cd.clone())]),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
        };

        let id_mig = Migration::identity(
            &[Name::from("c"), Name::from("d")],
            std::slice::from_ref(&e_cd),
        );

        let c = compose(&m, &id_mig).unwrap();
        assert_eq!(c.vertex_map.get("a"), Some(&Name::from("c")));
        assert_eq!(c.vertex_map.get("b"), Some(&Name::from("d")));
        assert_eq!(c.edge_map.get(&e_ab), Some(&e_cd));
    }

    #[test]
    fn compose_associativity() {
        // (m1 ; m2) ; m3 = m1 ; (m2 ; m3)
        let e_ab = edge("a", "b", "prop", "x");
        let e_cd = edge("c", "d", "prop", "x");
        let e_ef = edge("e", "f", "prop", "x");
        let e_gh = edge("g", "h", "prop", "x");

        let m1 = Migration {
            vertex_map: HashMap::from([("a".into(), "c".into()), ("b".into(), "d".into())]),
            edge_map: HashMap::from([(e_ab.clone(), e_cd.clone())]),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
        };

        let m2 = Migration {
            vertex_map: HashMap::from([("c".into(), "e".into()), ("d".into(), "f".into())]),
            edge_map: HashMap::from([(e_cd.clone(), e_ef.clone())]),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
        };

        let m3 = Migration {
            vertex_map: HashMap::from([("e".into(), "g".into()), ("f".into(), "h".into())]),
            edge_map: HashMap::from([(e_ef, e_gh)]),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
        };

        let left = compose(&compose(&m1, &m2).unwrap(), &m3).unwrap();
        let right = compose(&m1, &compose(&m2, &m3).unwrap()).unwrap();

        assert_eq!(left.vertex_map, right.vertex_map);
        assert_eq!(left.edge_map, right.edge_map);
    }

    #[test]
    fn compose_associativity_with_resolver() {
        // Verify associativity holds when resolvers are involved.
        let e_ab = edge("a", "b", "prop", "x");
        let e_cd = edge("c", "d", "prop", "x");
        let e_ef = edge("e", "f", "prop", "x");
        let e_gh = edge("g", "h", "prop", "x");

        // m1 has a resolver in G2 space (target of m1)
        let resolver_edge = edge("c", "d", "ref", "r");
        let resolver_edge_g3 = edge("e", "f", "ref", "r");
        let resolver_edge_g4 = edge("g", "h", "ref", "r");

        let m1 = Migration {
            vertex_map: HashMap::from([("a".into(), "c".into()), ("b".into(), "d".into())]),
            edge_map: HashMap::from([(e_ab.clone(), e_cd.clone())]),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::from([(("c".into(), "d".into()), resolver_edge.clone())]),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
        };

        let m2 = Migration {
            vertex_map: HashMap::from([("c".into(), "e".into()), ("d".into(), "f".into())]),
            edge_map: HashMap::from([
                (e_cd.clone(), e_ef.clone()),
                (resolver_edge.clone(), resolver_edge_g3.clone()),
            ]),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
        };

        let m3 = Migration {
            vertex_map: HashMap::from([("e".into(), "g".into()), ("f".into(), "h".into())]),
            edge_map: HashMap::from([
                (e_ef.clone(), e_gh.clone()),
                (resolver_edge_g3.clone(), resolver_edge_g4.clone()),
            ]),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
        };

        let left = compose(&compose(&m1, &m2).unwrap(), &m3).unwrap();
        let right = compose(&m1, &compose(&m2, &m3).unwrap()).unwrap();

        assert_eq!(left.vertex_map, right.vertex_map);
        assert_eq!(left.edge_map, right.edge_map);
        assert_eq!(left.resolver, right.resolver);
        // The composed resolver should map (g, h) -> resolver_edge_g4
        assert_eq!(
            left.resolver.get(&("g".into(), "h".into())),
            Some(&resolver_edge_g4),
        );
    }

    #[test]
    fn compose_drops_vertex() {
        // m2 drops a vertex that m1 maps to
        let e_ab = edge("a", "b", "prop", "x");
        let e_cd = edge("c", "d", "prop", "x");

        let m1 = Migration {
            vertex_map: HashMap::from([("a".into(), "c".into()), ("b".into(), "d".into())]),
            edge_map: HashMap::from([(e_ab.clone(), e_cd.clone())]),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
        };

        // m2 only maps "c", dropping "d"
        let m2 = Migration {
            vertex_map: HashMap::from([("c".into(), "e".into())]),
            edge_map: HashMap::new(),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
        };

        let c = compose(&m1, &m2).unwrap();
        assert_eq!(c.vertex_map.len(), 1);
        assert_eq!(c.vertex_map.get("a"), Some(&Name::from("e")));
        assert!(!c.vertex_map.contains_key("b"));
    }
}
