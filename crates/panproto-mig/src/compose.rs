//! Schema-level and data-level migration composition.
//!
//! Given `m1: G1 -> G2` and `m2: G2 -> G3`, [`compose`] produces
//! `m12: G1 -> G3` by composing vertex maps, edge maps, and
//! recomputing resolver tables.

use std::collections::HashMap;
use std::hash::Hash;

use panproto_gat::Name;
use panproto_schema::Edge;
use rustc_hash::FxHashMap;

use crate::error::ComposeError;
use crate::migration::Migration;

/// What to do with a source key whose `m1`-image is absent from `m2` when
/// composing two relabeling maps `m1 : A → B` and `m2 : B → B`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnMissing {
    /// Drop the source key from the composite (partial-map semantics). The
    /// dropped keys are reported in [`RelabelComposition::dropped`].
    Drop,
    /// Keep the intermediate value as the composed image (identity fallback):
    /// `m2` is treated as the identity on values it does not remap.
    KeepIntermediate,
}

/// The result of composing two relabeling maps: the composed map together
/// with the source keys dropped under [`OnMissing::Drop`].
#[derive(Clone, Debug)]
pub struct RelabelComposition<A, B> {
    /// The composed map `A → B`.
    pub map: HashMap<A, B>,
    /// Source keys dropped because their intermediate value was absent from
    /// the second map. Always empty under [`OnMissing::KeepIntermediate`].
    pub dropped: Vec<A>,
}

/// Compose two relabeling maps `first : A → B` and `second : B → B` into
/// `A → B`.
///
/// For each `(a, b)` in `first`, look up `b` in `second`:
/// - present as `b ↦ c`: insert `a ↦ c`;
/// - absent: apply `on_missing` — either drop `a` (recording it in
///   [`RelabelComposition::dropped`]) or keep `b` as the composed image.
///
/// This is the shared kernel behind both schema-level migration composition
/// (vertex and edge maps, [`OnMissing::Drop`]) and compiled-lens composition
/// (vertex and edge remaps, [`OnMissing::KeepIntermediate`]). Iteration
/// follows `first`'s order, so callers that record dropped keys observe the
/// same order they did when the loop was inlined.
#[must_use]
pub fn compose_relabeling<A, B>(
    first: &HashMap<A, B>,
    second: &HashMap<B, B>,
    on_missing: OnMissing,
) -> RelabelComposition<A, B>
where
    A: Eq + Hash + Clone,
    B: Eq + Hash + Clone,
{
    let mut map = HashMap::with_capacity(first.len());
    let mut dropped = Vec::new();
    for (a, b) in first {
        match second.get(b) {
            Some(c) => {
                map.insert(a.clone(), c.clone());
            }
            None => match on_missing {
                OnMissing::Drop => dropped.push(a.clone()),
                OnMissing::KeepIntermediate => {
                    map.insert(a.clone(), b.clone());
                }
            },
        }
    }
    RelabelComposition { map, dropped }
}

/// Entries discarded while composing two migrations.
///
/// [`compose`] follows partial-map semantics: a vertex, edge, hyper-edge,
/// resolver entry, or label whose `m1`-image is absent from `m2`'s
/// corresponding map is dropped from the composite. Silent dropping hides
/// mis-paired compositions, so [`compose_with_report`] records every drop
/// here for the caller to inspect.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComposeReport {
    /// Source vertices dropped because `m2` did not map their `m1`-image.
    pub dropped_vertices: Vec<Name>,
    /// Source edges dropped because `m2` did not map their `m1`-image.
    pub dropped_edges: Vec<Edge>,
    /// Source hyper-edges dropped because `m2` did not map their `m1`-image.
    pub dropped_hyper_edges: Vec<Name>,
    /// Resolver keys dropped because an endpoint or edge did not survive `m2`.
    pub dropped_resolver_keys: Vec<(Name, Name)>,
    /// Label keys dropped because the governing hyper-edge did not survive `m2`.
    pub dropped_labels: Vec<(Name, Name)>,
}

impl ComposeReport {
    /// Returns `true` when no entry was dropped during composition.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.dropped_vertices.is_empty()
            && self.dropped_edges.is_empty()
            && self.dropped_hyper_edges.is_empty()
            && self.dropped_resolver_keys.is_empty()
            && self.dropped_labels.is_empty()
    }

    /// Render each dropped entry as a human-readable line.
    #[must_use]
    pub fn to_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for v in &self.dropped_vertices {
            lines.push(format!(
                "compose dropped vertex `{v}` (removed by second migration)"
            ));
        }
        for e in &self.dropped_edges {
            lines.push(format!(
                "compose dropped edge `{}->{}` (removed by second migration)",
                e.src, e.tgt
            ));
        }
        for he in &self.dropped_hyper_edges {
            lines.push(format!(
                "compose dropped hyper-edge `{he}` (removed by second migration)"
            ));
        }
        for (src, tgt) in &self.dropped_resolver_keys {
            lines.push(format!(
                "compose dropped resolver entry `({src}, {tgt})` (endpoint or edge removed by second migration)"
            ));
        }
        for (he, label) in &self.dropped_labels {
            lines.push(format!(
                "compose dropped label `({he}, {label})` (hyper-edge removed by second migration)"
            ));
        }
        lines
    }
}

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
/// Composability: when `m1.codomain` and `m2.domain` are both present and
/// unequal, the two migrations describe unrelated schema pairs, so
/// composition returns [`ComposeError::DomainMismatch`]. When either
/// identity is absent the check is skipped and composition proceeds
/// permissively. The composite carries `m1.domain` as its domain and
/// `m2.codomain` as its codomain.
///
/// Partial-map semantics: if a vertex in the image of `m1` is not in
/// the domain of `m2`, it is dropped from the composed map (the vertex
/// was removed by `m2`). The same applies to edges, hyper-edges,
/// resolver entries, and labels. Use [`compose_with_report`] to recover
/// which entries were dropped.
///
/// # Errors
///
/// Returns [`ComposeError::DomainMismatch`] when the two migrations are
/// not composable.
pub fn compose(m1: &Migration, m2: &Migration) -> Result<Migration, ComposeError> {
    compose_with_report(m1, m2).map(|(migration, _report)| migration)
}

/// Compose two migrations and report every entry dropped by the
/// composition.
///
/// Behaves exactly like [`compose`] but additionally returns a
/// [`ComposeReport`] recording each vertex, edge, hyper-edge, resolver
/// key, and label removed because its `m1`-image was absent from `m2`'s
/// corresponding map.
///
/// # Errors
///
/// Returns [`ComposeError::DomainMismatch`] when `m1.codomain` and
/// `m2.domain` are both present and disagree.
pub fn compose_with_report(
    m1: &Migration,
    m2: &Migration,
) -> Result<(Migration, ComposeReport), ComposeError> {
    if let (Some(first_codomain), Some(second_domain)) = (&m1.codomain, &m2.domain) {
        if first_codomain != second_domain {
            return Err(ComposeError::DomainMismatch {
                first_codomain: first_codomain.to_string(),
                second_domain: second_domain.to_string(),
            });
        }
    }

    let mut report = ComposeReport::default();

    // Compose vertex maps through the shared relabeling kernel: for each v1
    // in m1.vertex_map, composed[v1] = m2.vertex_map[m1.vertex_map[v1]], with
    // partial-map (drop-on-miss) semantics.
    let vertex_composition = compose_relabeling(&m1.vertex_map, &m2.vertex_map, OnMissing::Drop);
    let vertex_map = vertex_composition.map;
    report.dropped_vertices = vertex_composition.dropped;

    // Compose edge maps through the same kernel.
    let edge_composition = compose_relabeling(&m1.edge_map, &m2.edge_map, OnMissing::Drop);
    let edge_map = edge_composition.map;
    report.dropped_edges = edge_composition.dropped;

    // Compose hyper-edge maps.
    let mut hyper_edge_map = HashMap::new();
    for (he1, he2) in &m1.hyper_edge_map {
        if let Some(he3) = m2.hyper_edge_map.get(he2) {
            hyper_edge_map.insert(he1.clone(), he3.clone());
        } else {
            report.dropped_hyper_edges.push(he1.clone());
        }
    }

    // Compose label maps. A label entry survives only when its governing
    // hyper-edge survives composition: `he1` must map through
    // `m1.hyper_edge_map` to an `he2` that `m2.hyper_edge_map` still
    // carries. Otherwise the label names a hyper-edge absent from G3 and
    // is dropped.
    let mut label_map = HashMap::new();
    for ((he1, label1), label2) in &m1.label_map {
        let survives = m1
            .hyper_edge_map
            .get(he1)
            .is_some_and(|he2| m2.hyper_edge_map.contains_key(he2));
        if survives {
            // `he2` exists and is retained by `m2`.
            let he2 = &m1.hyper_edge_map[he1];
            let key2 = (he2.clone(), label2.clone());
            let composed = m2.label_map.get(&key2).unwrap_or(label2);
            label_map.insert((he1.clone(), label1.clone()), composed.clone());
        } else {
            report.dropped_labels.push((he1.clone(), label1.clone()));
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
        let (Some(src3), Some(tgt3)) = (m2.vertex_map.get(src), m2.vertex_map.get(tgt)) else {
            report
                .dropped_resolver_keys
                .push((src.clone(), tgt.clone()));
            continue;
        };
        // Remap G2 edge to G3 via m2.edge_map.
        if let Some(mapped_edge) = m2.edge_map.get(edge) {
            resolver.insert((src3.clone(), tgt3.clone()), mapped_edge.clone());
        } else {
            report
                .dropped_resolver_keys
                .push((src.clone(), tgt.clone()));
        }
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

    let composed = Migration {
        vertex_map,
        edge_map,
        hyper_edge_map,
        label_map,
        resolver,
        hyper_resolver,
        expr_resolvers,
        domain: m1.domain.clone(),
        codomain: m2.codomain.clone(),
    };
    Ok((composed, report))
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
    fn compose_relabeling_drop_and_keep_policies() {
        // first: a→b, c→d ; second: b→B (d missing).
        let mut first: HashMap<Name, Name> = HashMap::new();
        first.insert("a".into(), "b".into());
        first.insert("c".into(), "d".into());
        let mut second: HashMap<Name, Name> = HashMap::new();
        second.insert("b".into(), "B".into());

        // Drop: a→B kept, c dropped (its image d is unmapped).
        let drop = compose_relabeling(&first, &second, OnMissing::Drop);
        assert_eq!(drop.map.get("a"), Some(&Name::from("B")));
        assert!(!drop.map.contains_key("c"));
        assert_eq!(drop.dropped, vec![Name::from("c")]);

        // KeepIntermediate: a→B, c→d (identity fallback), nothing dropped.
        let keep = compose_relabeling(&first, &second, OnMissing::KeepIntermediate);
        assert_eq!(keep.map.get("a"), Some(&Name::from("B")));
        assert_eq!(keep.map.get("c"), Some(&Name::from("d")));
        assert!(keep.dropped.is_empty());
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
            domain: None,
            codomain: None,
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
            domain: None,
            codomain: None,
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
            domain: None,
            codomain: None,
        };

        let m2 = Migration {
            vertex_map: HashMap::from([("c".into(), "e".into()), ("d".into(), "f".into())]),
            edge_map: HashMap::from([(e_cd.clone(), e_ef.clone())]),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
            domain: None,
            codomain: None,
        };

        let m3 = Migration {
            vertex_map: HashMap::from([("e".into(), "g".into()), ("f".into(), "h".into())]),
            edge_map: HashMap::from([(e_ef, e_gh)]),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
            domain: None,
            codomain: None,
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
            domain: None,
            codomain: None,
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
            domain: None,
            codomain: None,
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
            domain: None,
            codomain: None,
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
            domain: None,
            codomain: None,
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
            domain: None,
            codomain: None,
        };

        let c = compose(&m1, &m2).unwrap();
        assert_eq!(c.vertex_map.len(), 1);
        assert_eq!(c.vertex_map.get("a"), Some(&Name::from("e")));
        assert!(!c.vertex_map.contains_key("b"));
    }

    #[test]
    fn compose_rejects_domain_mismatch() {
        // m1: G1 -> G2, m2: G2b -> G3 with G2 != G2b.
        let mut m1 = Migration::empty();
        m1.domain = Some(Name::from("G1"));
        m1.codomain = Some(Name::from("G2"));
        let mut m2 = Migration::empty();
        m2.domain = Some(Name::from("G2b"));
        m2.codomain = Some(Name::from("G3"));

        let err = compose(&m1, &m2).unwrap_err();
        assert!(matches!(err, ComposeError::DomainMismatch { .. }));

        // Matching endpoints compose, carrying m1.domain and m2.codomain.
        m2.domain = Some(Name::from("G2"));
        let composed = compose(&m1, &m2).unwrap();
        assert_eq!(composed.domain, Some(Name::from("G1")));
        assert_eq!(composed.codomain, Some(Name::from("G3")));

        // Absent endpoints preserve the permissive behavior.
        assert!(compose(&Migration::empty(), &Migration::empty()).is_ok());
    }

    #[test]
    fn deserialize_migration_without_endpoints() {
        // A serialized migration lacking the `domain`/`codomain` fields
        // still deserializes, defaulting both to None (serde back-compat).
        let mut m = Migration::empty();
        m.vertex_map.insert("a".into(), "b".into());
        m.domain = Some(Name::from("G1"));
        m.codomain = Some(Name::from("G2"));

        let mut value: serde_json::Value = serde_json::to_value(&m).unwrap();
        let obj = value.as_object_mut().unwrap();
        obj.remove("domain");
        obj.remove("codomain");
        let text = serde_json::to_string(&value).unwrap();

        let restored: Migration = serde_json::from_str(&text).unwrap();
        assert_eq!(restored.domain, None);
        assert_eq!(restored.codomain, None);
        assert_eq!(restored.vertex_map.get("a"), Some(&Name::from("b")));
    }

    #[test]
    fn compose_reports_dropped_entries() {
        // m2 omits the vertex m1 maps "b" to, so the composed map drops it.
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
            domain: None,
            codomain: None,
        };
        // m2 maps only "c", dropping "d" and the edge c->d.
        let m2 = Migration {
            vertex_map: HashMap::from([("c".into(), "e".into())]),
            edge_map: HashMap::new(),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
            domain: None,
            codomain: None,
        };

        let (composed, report) = compose_with_report(&m1, &m2).unwrap();
        assert!(!composed.vertex_map.contains_key("b"));
        assert!(report.dropped_vertices.contains(&Name::from("b")));
        assert!(report.dropped_edges.contains(&e_ab));
        assert!(!report.is_empty());
    }

    #[test]
    fn compose_label_map_drops_removed_hyper_edge() {
        // m1 relabels (he, a) -> b on hyper-edge he; m2 drops he.
        let mut m1 = Migration::empty();
        m1.hyper_edge_map.insert("he".into(), "he".into());
        m1.label_map
            .insert(("he".into(), "a".into()), Name::from("b"));

        let m2 = Migration::empty(); // drops he: not in hyper_edge_map.

        let (composed, report) = compose_with_report(&m1, &m2).unwrap();
        assert!(
            !composed
                .label_map
                .keys()
                .any(|(he, _)| he == &Name::from("he")),
            "label entry keyed by dropped hyper-edge must not survive"
        );
        assert!(
            report
                .dropped_labels
                .contains(&(Name::from("he"), Name::from("a")))
        );
    }

    #[test]
    fn compose_label_map_chains_relabels() {
        // m1 relabels (he, a) -> b; m2 further relabels (he, b) -> c.
        let mut m1 = Migration::empty();
        m1.hyper_edge_map.insert("he".into(), "he".into());
        m1.label_map
            .insert(("he".into(), "a".into()), Name::from("b"));

        let mut m2 = Migration::empty();
        m2.hyper_edge_map.insert("he".into(), "he".into());
        m2.label_map
            .insert(("he".into(), "b".into()), Name::from("c"));

        let composed = compose(&m1, &m2).unwrap();
        assert_eq!(
            composed.label_map.get(&("he".into(), "a".into())),
            Some(&Name::from("c"))
        );
    }
}
