//! Neighborhood-propagation alignment strategy.
//!
//! Given a seed map `(s_parent → t_parent)` of already-aligned vertex
//! pairs, propagates the alignment to the children of each seeded pair
//! by scoring the unmatched outgoing-edge targets on each side by a
//! compound of:
//!
//! * edge-label similarity (via
//!   [`super::token_similarity::token_similarity`] on the two labels),
//! * edge-kind equality,
//! * child-vertex kind-and-constraints compatibility,
//! * structural fingerprint (outgoing + incoming degree overlap).
//!
//! Emits anchors above `threshold`. Runs only at `Lenient` and
//! `Exploratory` stringency; lower tiers reject cross-neighborhood
//! propagation because the signal is weaker than name or label
//! evidence.

use std::collections::{HashMap, HashSet};

use panproto_gat::Name;
use panproto_schema::{Edge, Schema};

use super::{
    Anchor, StrategyTag, kinds_and_constraints_compatible, token_similarity::token_similarity,
};

/// Emit neighborhood-propagation anchors.
///
/// For every seeded pair `(s_parent, t_parent)`, scores each unmatched
/// source child against each unmatched target child of the target
/// parent and emits an anchor when the compound score exceeds
/// `threshold`.
///
/// "Unmatched" means: not already a key (source side) or value (target
/// side) of the `seeds` map. A child may be proposed for anchoring
/// only once per run; the strategy keeps the best-scoring candidate
/// per source child when several targets compete.
#[must_use]
pub fn neighborhood_anchors(
    src: &Schema,
    tgt: &Schema,
    seeds: &HashMap<Name, Name>,
    threshold: f64,
) -> Vec<Anchor> {
    let seeded_sources: HashSet<&Name> = seeds.keys().collect();
    let seeded_targets: HashSet<&Name> = seeds.values().collect();

    let mut seed_pairs: Vec<(&Name, &Name)> = seeds.iter().collect();
    seed_pairs.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));

    let mut out = Vec::new();
    let mut proposed_src: HashSet<Name> = HashSet::new();
    for (s_parent, t_parent) in seed_pairs {
        let src_children = child_edges(src, s_parent);
        let tgt_children = child_edges(tgt, t_parent);

        for src_edge in &src_children {
            if seeded_sources.contains(&src_edge.tgt) || proposed_src.contains(&src_edge.tgt) {
                continue;
            }
            let mut best: Option<(Name, f64)> = None;
            for tgt_edge in &tgt_children {
                if seeded_targets.contains(&tgt_edge.tgt) {
                    continue;
                }
                if src_edge.kind != tgt_edge.kind {
                    continue;
                }
                if !kinds_and_constraints_compatible(src, &src_edge.tgt, tgt, &tgt_edge.tgt) {
                    continue;
                }
                let label_sim = match (src_edge.name.as_deref(), tgt_edge.name.as_deref()) {
                    (Some(a), Some(b)) => token_similarity(a, b),
                    (None, None) => 0.5,
                    _ => 0.25,
                };
                let deg = degree_overlap(src, &src_edge.tgt, tgt, &tgt_edge.tgt);
                // Average of the two signals. Kind compatibility is a
                // gate above, so it doesn't enter the score.
                let score = 0.5f64.mul_add(label_sim, 0.5 * deg);
                if best.as_ref().is_none_or(|(_, bs)| score > *bs) {
                    best = Some((tgt_edge.tgt.clone(), score));
                }
            }
            if let Some((t_child, score)) = best
                && score >= threshold
            {
                proposed_src.insert(src_edge.tgt.clone());
                out.push(Anchor {
                    src: src_edge.tgt.clone(),
                    tgt: t_child,
                    confidence: score,
                    strategy: StrategyTag::Neighborhood,
                    explanation: format!(
                        "neighborhood score {:.2}: {} child of seeded {} ↔ child of {}",
                        score,
                        src_edge.tgt.as_str(),
                        s_parent.as_str(),
                        t_parent.as_str(),
                    ),
                });
            }
        }
    }
    out
}

fn child_edges<'a>(schema: &'a Schema, parent: &Name) -> Vec<&'a Edge> {
    let mut out: Vec<&Edge> = schema.outgoing_edges(parent).iter().collect();
    // Stable order for reproducibility.
    out.sort();
    out
}

/// Jaccard-style overlap on `(out_degree, in_degree)` signatures:
/// returns `min/max` on each axis averaged, in `[0, 1]`. Identical
/// degrees score `1.0`; wildly different ones score near zero.
fn degree_overlap(src: &Schema, s: &Name, tgt: &Schema, t: &Name) -> f64 {
    let so = src.outgoing_edges(s).len();
    let si = src.incoming_edges(s).len();
    let to = tgt.outgoing_edges(t).len();
    let ti = tgt.incoming_edges(t).len();
    let axis = |a: usize, b: usize| -> f64 {
        let hi = a.max(b);
        if hi == 0 {
            return 1.0;
        }
        // Convert via u32 (clamped) to f64: edge counts up to u32::MAX
        // are representable exactly by f64, and counts beyond that are
        // clipped rather than silently round-tripped through a raw
        // `as f64` cast.
        let lo_f = f64::from(u32::try_from(a.min(b)).unwrap_or(u32::MAX));
        let hi_f = f64::from(u32::try_from(hi).unwrap_or(u32::MAX));
        lo_f / hi_f
    };
    0.5f64.mul_add(axis(so, to), 0.5 * axis(si, ti))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
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
                src_kinds: vec!["object".into()],
                tgt_kinds: vec!["object".into(), "string".into()],
            }],
            obj_kinds: vec!["object".into(), "string".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    #[test]
    fn propagates_to_kind_compatible_child_pair() {
        let p = proto();
        // Source: p -> c (string, label "id")
        let src = SchemaBuilder::new(&p)
            .vertex("p", "object", None::<&str>)
            .unwrap()
            .vertex("c", "string", None::<&str>)
            .unwrap()
            .edge("p", "c", "prop", Some("id"))
            .unwrap()
            .build()
            .unwrap();
        // Target: q -> d (string, label "id"), q -> e (object, label "id2")
        let tgt = SchemaBuilder::new(&p)
            .vertex("q", "object", None::<&str>)
            .unwrap()
            .vertex("d", "string", None::<&str>)
            .unwrap()
            .vertex("e", "object", None::<&str>)
            .unwrap()
            .edge("q", "d", "prop", Some("id"))
            .unwrap()
            .edge("q", "e", "prop", Some("id2"))
            .unwrap()
            .build()
            .unwrap();
        let mut seeds = HashMap::new();
        seeds.insert(Name::from("p"), Name::from("q"));
        let anchors = neighborhood_anchors(&src, &tgt, &seeds, 0.4);
        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0].src.as_str(), "c");
        // Only `d` is a kind-compatible child of `q`.
        assert_eq!(anchors[0].tgt.as_str(), "d");
        assert_eq!(anchors[0].strategy, StrategyTag::Neighborhood);
    }

    #[test]
    fn skips_when_seed_parent_has_no_kind_compatible_child() {
        let p = proto();
        let src = SchemaBuilder::new(&p)
            .vertex("p", "object", None::<&str>)
            .unwrap()
            .vertex("c", "string", None::<&str>)
            .unwrap()
            .edge("p", "c", "prop", Some("id"))
            .unwrap()
            .build()
            .unwrap();
        let tgt = SchemaBuilder::new(&p)
            .vertex("q", "object", None::<&str>)
            .unwrap()
            .vertex("d", "object", None::<&str>)
            .unwrap()
            .edge("q", "d", "prop", Some("id"))
            .unwrap()
            .build()
            .unwrap();
        let mut seeds = HashMap::new();
        seeds.insert(Name::from("p"), Name::from("q"));
        assert!(neighborhood_anchors(&src, &tgt, &seeds, 0.4).is_empty());
    }

    #[test]
    fn does_not_repropose_seeded_vertices() {
        let p = proto();
        let src = SchemaBuilder::new(&p)
            .vertex("p", "object", None::<&str>)
            .unwrap()
            .vertex("c", "string", None::<&str>)
            .unwrap()
            .edge("p", "c", "prop", Some("id"))
            .unwrap()
            .build()
            .unwrap();
        let tgt = SchemaBuilder::new(&p)
            .vertex("q", "object", None::<&str>)
            .unwrap()
            .vertex("d", "string", None::<&str>)
            .unwrap()
            .edge("q", "d", "prop", Some("id"))
            .unwrap()
            .build()
            .unwrap();
        let mut seeds = HashMap::new();
        seeds.insert(Name::from("p"), Name::from("q"));
        // `c` already seeded: neighborhood must not re-anchor it.
        seeds.insert(Name::from("c"), Name::from("d"));
        assert!(neighborhood_anchors(&src, &tgt, &seeds, 0.1).is_empty());
    }

    #[test]
    fn no_seeds_yields_empty() {
        let p = proto();
        let src = SchemaBuilder::new(&p)
            .vertex("p", "object", None::<&str>)
            .unwrap()
            .build()
            .unwrap();
        let tgt = SchemaBuilder::new(&p)
            .vertex("q", "object", None::<&str>)
            .unwrap()
            .build()
            .unwrap();
        let seeds = HashMap::new();
        assert!(neighborhood_anchors(&src, &tgt, &seeds, 0.5).is_empty());
    }

    #[test]
    fn prefers_higher_scoring_label_similarity() {
        let p = proto();
        let src = SchemaBuilder::new(&p)
            .vertex("p", "object", None::<&str>)
            .unwrap()
            .vertex("c", "string", None::<&str>)
            .unwrap()
            .edge("p", "c", "prop", Some("userName"))
            .unwrap()
            .build()
            .unwrap();
        let tgt = SchemaBuilder::new(&p)
            .vertex("q", "object", None::<&str>)
            .unwrap()
            .vertex("d", "string", None::<&str>)
            .unwrap()
            .vertex("e", "string", None::<&str>)
            .unwrap()
            .edge("q", "d", "prop", Some("user_name"))
            .unwrap()
            .edge("q", "e", "prop", Some("zzz"))
            .unwrap()
            .build()
            .unwrap();
        let mut seeds = HashMap::new();
        seeds.insert(Name::from("p"), Name::from("q"));
        let anchors = neighborhood_anchors(&src, &tgt, &seeds, 0.4);
        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0].tgt.as_str(), "d");
    }
}
