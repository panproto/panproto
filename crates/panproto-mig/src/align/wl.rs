//! Weisfeiler-Leman color-refinement alignment strategy.
//!
//! Produces structural signatures for every vertex by iteratively
//! hashing each vertex's initial color together with the sorted
//! multiset of `(neighbor_color, edge_label, edge_kind)` triples
//! from its outgoing edges. After `iterations` rounds, vertices
//! grouped into a singleton color class on each side are anchored.
//!
//! Hashing uses [`blake3`] so signatures are stable across runs and
//! platform-independent. WL is structural-only (no type or name
//! information beyond kind and declared constraint sorts), so it is
//! placed at a low priority (32): it fires when a schema has enough
//! topology for color refinement to distinguish vertices uniquely,
//! and defers to name or label evidence when present.

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_schema::Schema;

use super::{Anchor, StrategyTag};

/// Confidence assigned to every WL anchor. Color refinement rarely
/// produces false positives on sensibly-authored schemas: two vertices
/// are anchored only when both their color classes are singletons,
/// which requires distinguishable neighborhoods on both sides.
const WL_CONFIDENCE: f64 = 0.9;

/// Emit WL-refinement anchors after running `iterations` rounds of
/// color refinement on each schema.
///
/// Algorithm:
///
/// 1. Initial color of each vertex = `hash((kind, sorted_constraint_sorts))`.
/// 2. For `iterations` rounds, new color of vertex `v` =
///    `hash((old_color, sorted_multiset_of (neighbor_color, edge_label, edge_kind)))`
///    where neighbor colors come from the previous iteration.
/// 3. For every color class that has exactly one vertex on each side,
///    emit an anchor. Ambiguous classes (multiple vertices on either
///    side) emit nothing.
#[must_use]
pub fn wl_anchors(src: &Schema, tgt: &Schema, iterations: usize) -> Vec<Anchor> {
    let src_colors = refine_colors(src, iterations);
    let tgt_colors = refine_colors(tgt, iterations);

    // Group by color on each side.
    let mut src_by_color: HashMap<[u8; 32], Vec<&Name>> = HashMap::new();
    for (id, color) in &src_colors {
        src_by_color.entry(*color).or_default().push(id);
    }
    let mut tgt_by_color: HashMap<[u8; 32], Vec<&Name>> = HashMap::new();
    for (id, color) in &tgt_colors {
        tgt_by_color.entry(*color).or_default().push(id);
    }

    // Deterministic iteration: collect colors that are singletons on
    // both sides, sort by color bytes, emit.
    let mut matched: Vec<([u8; 32], &Name, &Name)> = Vec::new();
    for (color, src_ids) in &src_by_color {
        if src_ids.len() != 1 {
            continue;
        }
        let Some(tgt_ids) = tgt_by_color.get(color) else {
            continue;
        };
        if tgt_ids.len() != 1 {
            continue;
        }
        matched.push((*color, src_ids[0], tgt_ids[0]));
    }
    matched.sort_by(|a, b| a.1.as_str().cmp(b.1.as_str()));

    matched
        .into_iter()
        .map(|(_color, s, t)| Anchor {
            src: s.clone(),
            tgt: t.clone(),
            confidence: WL_CONFIDENCE,
            strategy: StrategyTag::WlRefinement,
            explanation: format!(
                "WL refinement singleton color class: {} ↔ {}",
                s.as_str(),
                t.as_str()
            ),
        })
        .collect()
}

fn refine_colors(schema: &Schema, iterations: usize) -> HashMap<Name, [u8; 32]> {
    let mut colors: HashMap<Name, [u8; 32]> = schema
        .vertices
        .iter()
        .map(|(id, v)| (id.clone(), initial_color(schema, id, &v.kind)))
        .collect();

    for _ in 0..iterations {
        let next: HashMap<Name, [u8; 32]> = colors
            .keys()
            .map(|id| (id.clone(), refine_step(schema, id, &colors)))
            .collect();
        if next == colors {
            break;
        }
        colors = next;
    }
    colors
}

fn initial_color(schema: &Schema, id: &Name, kind: &Name) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"v0|");
    hasher.update(kind.as_str().as_bytes());
    hasher.update(b"|");
    if let Some(cs) = schema.constraints.get(id) {
        // Include constraint (sort, value) pairs in the initial color
        // so that, e.g., two `string` vertices with different
        // `maxLength` values do not collide. Sort by (sort, value) for
        // a stable canonical order independent of insertion order.
        let mut pairs: Vec<(&str, &str)> = cs
            .iter()
            .map(|c| (c.sort.as_str(), c.value.as_str()))
            .collect();
        pairs.sort_unstable();
        pairs.dedup();
        for (s, v) in pairs {
            hasher.update(s.as_bytes());
            hasher.update(b"=");
            hasher.update(v.as_bytes());
            hasher.update(b",");
        }
    }
    *hasher.finalize().as_bytes()
}

/// A distinct sentinel for "edge points at a missing vertex": all
/// 0xFF differs from legitimate blake3 color outputs with negligible
/// probability, so misconfigured schemas do not silently map every
/// missing-endpoint edge onto the zero color reserved elsewhere.
const MISSING_SENTINEL: [u8; 32] = [0xFF; 32];

fn refine_step(schema: &Schema, id: &Name, colors: &HashMap<Name, [u8; 32]>) -> [u8; 32] {
    // Direction marker keeps outgoing and incoming triples in
    // disjoint parts of the multiset so a vertex reached via an edge
    // "in" is not conflated with a vertex reached via an edge "out".
    let mut triples: Vec<(u8, [u8; 32], &str, &str)> = Vec::new();
    for edge in schema.outgoing_edges(id) {
        debug_assert!(
            colors.contains_key(&edge.tgt),
            "edge tgt missing from colors map: {} -> {}",
            id.as_str(),
            edge.tgt.as_str(),
        );
        let neighbor_color = colors.get(&edge.tgt).copied().unwrap_or(MISSING_SENTINEL);
        let label = edge.name.as_deref().unwrap_or("");
        triples.push((b'>', neighbor_color, label, edge.kind.as_str()));
    }
    for edge in schema.incoming_edges(id) {
        debug_assert!(
            colors.contains_key(&edge.src),
            "edge src missing from colors map: {} <- {}",
            id.as_str(),
            edge.src.as_str(),
        );
        let neighbor_color = colors.get(&edge.src).copied().unwrap_or(MISSING_SENTINEL);
        let label = edge.name.as_deref().unwrap_or("");
        triples.push((b'<', neighbor_color, label, edge.kind.as_str()));
    }
    triples.sort_unstable();

    let mut hasher = blake3::Hasher::new();
    hasher.update(b"v1|");
    let own = colors.get(id).copied().unwrap_or([0u8; 32]);
    hasher.update(&own);
    hasher.update(b"|");
    for (dir, nc, label, kind) in triples {
        hasher.update(&[dir]);
        hasher.update(&nc);
        hasher.update(b":");
        hasher.update(label.as_bytes());
        hasher.update(b":");
        hasher.update(kind.as_bytes());
        hasher.update(b"|");
    }
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use panproto_schema::{EdgeRule, Protocol, SchemaBuilder};

    fn proto() -> Protocol {
        Protocol {
            name: "t".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![EdgeRule {
                edge_kind: "prop".into(),
                src_kinds: vec!["object".into(), "record".into()],
                tgt_kinds: vec!["string".into(), "object".into()],
            }],
            obj_kinds: vec!["record".into(), "object".into(), "string".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    #[test]
    fn distinctly_named_isomorphic_neighborhoods_anchor() {
        // Two schemas with disjoint vertex names but isomorphic
        // neighborhood structure: one record vertex with two string
        // children reached through differently-labeled edges.
        let p = proto();
        let src = SchemaBuilder::new(&p)
            .vertex("R", "record", None::<&str>)
            .unwrap()
            .vertex("a", "string", None::<&str>)
            .unwrap()
            .vertex("b", "string", None::<&str>)
            .unwrap()
            .edge("R", "a", "prop", Some("first"))
            .unwrap()
            .edge("R", "b", "prop", Some("second"))
            .unwrap()
            .build()
            .unwrap();
        let tgt = SchemaBuilder::new(&p)
            .vertex("Z", "record", None::<&str>)
            .unwrap()
            .vertex("x", "string", None::<&str>)
            .unwrap()
            .vertex("y", "string", None::<&str>)
            .unwrap()
            .edge("Z", "x", "prop", Some("first"))
            .unwrap()
            .edge("Z", "y", "prop", Some("second"))
            .unwrap()
            .build()
            .unwrap();
        let anchors = wl_anchors(&src, &tgt, 2);
        let pairs: Vec<(&str, &str)> = anchors
            .iter()
            .map(|a| (a.src.as_str(), a.tgt.as_str()))
            .collect();
        // Record vertices form singletons on each side.
        assert!(
            pairs.contains(&("R", "Z")),
            "record anchor missing in {pairs:?}"
        );
        // String children are distinguished by their incoming edge
        // labels, so each forms a singleton color class.
        assert!(
            pairs.contains(&("a", "x")),
            "first child missing: {pairs:?}"
        );
        assert!(
            pairs.contains(&("b", "y")),
            "second child missing: {pairs:?}"
        );
        for anchor in &anchors {
            assert_eq!(anchor.strategy, StrategyTag::WlRefinement);
            assert!((anchor.confidence - WL_CONFIDENCE).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn ambiguous_color_classes_emit_nothing() {
        // Two identically-structured children on each side: WL cannot
        // distinguish them, so no anchors are emitted.
        let p = proto();
        let src = SchemaBuilder::new(&p)
            .vertex("R", "record", None::<&str>)
            .unwrap()
            .vertex("a", "string", None::<&str>)
            .unwrap()
            .vertex("b", "string", None::<&str>)
            .unwrap()
            .edge("R", "a", "prop", Some("x"))
            .unwrap()
            .edge("R", "b", "prop", Some("x"))
            .unwrap()
            .build()
            .unwrap();
        let tgt = SchemaBuilder::new(&p)
            .vertex("Z", "record", None::<&str>)
            .unwrap()
            .vertex("p", "string", None::<&str>)
            .unwrap()
            .vertex("q", "string", None::<&str>)
            .unwrap()
            .edge("Z", "p", "prop", Some("x"))
            .unwrap()
            .edge("Z", "q", "prop", Some("x"))
            .unwrap()
            .build()
            .unwrap();
        let anchors = wl_anchors(&src, &tgt, 2);
        // Record vertices still anchor (singleton on each side).
        let pairs: Vec<(&str, &str)> = anchors
            .iter()
            .map(|a| (a.src.as_str(), a.tgt.as_str()))
            .collect();
        assert!(pairs.contains(&("R", "Z")));
        // Strings share identical color classes (two on each side) so
        // they are not emitted.
        assert!(!pairs.iter().any(|(s, _)| *s == "a"));
        assert!(!pairs.iter().any(|(s, _)| *s == "b"));
    }

    #[test]
    fn zero_iterations_degenerates_to_kind_plus_constraints() {
        let p = proto();
        let src = SchemaBuilder::new(&p)
            .vertex("r", "record", None::<&str>)
            .unwrap()
            .vertex("a", "string", None::<&str>)
            .unwrap()
            .edge("r", "a", "prop", Some("x"))
            .unwrap()
            .build()
            .unwrap();
        let tgt = SchemaBuilder::new(&p)
            .vertex("r2", "record", None::<&str>)
            .unwrap()
            .vertex("b", "string", None::<&str>)
            .unwrap()
            .edge("r2", "b", "prop", Some("x"))
            .unwrap()
            .build()
            .unwrap();
        // Zero iterations: only initial colors contribute. The record
        // vertices on each side share a color (both singletons) and
        // the string vertices share a color (both singletons). Both
        // pairs anchor.
        let anchors = wl_anchors(&src, &tgt, 0);
        assert_eq!(anchors.len(), 2);
    }

    #[test]
    fn deterministic_order() {
        let p = proto();
        let build = |labels: &[&str]| {
            let mut b = SchemaBuilder::new(&p)
                .vertex("R", "record", None::<&str>)
                .unwrap();
            for (i, l) in labels.iter().enumerate() {
                let id = format!("v{i}");
                b = b.vertex(&id, "string", None::<&str>).unwrap();
                b = b.edge("R", &id, "prop", Some(*l)).unwrap();
            }
            b.build().unwrap()
        };
        let s = build(&["a", "b", "c"]);
        let t = build(&["a", "b", "c"]);
        let r1: Vec<_> = wl_anchors(&s, &t, 2)
            .iter()
            .map(|a| (a.src.as_str().to_owned(), a.tgt.as_str().to_owned()))
            .collect();
        let r2: Vec<_> = wl_anchors(&s, &t, 2)
            .iter()
            .map(|a| (a.src.as_str().to_owned(), a.tgt.as_str().to_owned()))
            .collect();
        assert_eq!(r1, r2);
    }
}
