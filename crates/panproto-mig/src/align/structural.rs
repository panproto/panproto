//! Structural alignment strategy: degree + kind signature last resort.
//!
//! Runs only at the Exploratory tier. Pairs source and target vertices
//! whose vertex kind and outgoing + incoming degree match and whose
//! edge-kind multisets on each side overlap, even when no name or
//! alias evidence is available. Emits anchors at low confidence so
//! higher-priority strategies always win when they fire.
//!
//! The strategy is name-independent: it cannot distinguish two
//! structurally-identical candidates and will return the best-scoring
//! one per source vertex. The CSP enforces naturality on every anchor
//! before accepting it, so false pairings are rejected downstream.

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_schema::Schema;

use super::{Anchor, StrategyTag, kinds_compatible};

/// Emit structural-match anchors at `Exploratory` tier.
///
/// `confidence_floor` is the minimum score below which anchors are
/// dropped. The strategy scales confidence by `(degree_similarity +
/// edge_kind_overlap) / 2` and keeps entries above `confidence_floor`.
///
/// Tie-break: when multiple candidate targets share the best score,
/// the lowest-alphabetical target wins (targets are iterated in sorted
/// order and the strict `>` comparison retains the first-seen maximum).
#[must_use]
pub fn structural_anchors(src: &Schema, tgt: &Schema, confidence_floor: f64) -> Vec<Anchor> {
    let floor = confidence_floor.clamp(0.0, 1.0);
    let src_profiles: HashMap<Name, VertexProfile> = src
        .vertices
        .keys()
        .map(|id| (id.clone(), profile_for(src, id)))
        .collect();
    let tgt_profiles: HashMap<Name, VertexProfile> = tgt
        .vertices
        .keys()
        .map(|id| (id.clone(), profile_for(tgt, id)))
        .collect();

    let mut out = Vec::new();
    let mut src_ids: Vec<&Name> = src.vertices.keys().collect();
    src_ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    let mut tgt_ids: Vec<&Name> = tgt.vertices.keys().collect();
    tgt_ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));

    for src_id in src_ids.iter().copied() {
        let Some(src_p) = src_profiles.get(src_id) else {
            continue;
        };
        if src_p.out_deg + src_p.in_deg == 0 {
            continue;
        }

        let mut best: Option<(Name, f64)> = None;
        for tgt_id in tgt_ids.iter().copied() {
            if !kinds_compatible(src, src_id, tgt, tgt_id) {
                continue;
            }
            let Some(tgt_p) = tgt_profiles.get(tgt_id) else {
                continue;
            };
            let score = similarity(src_p, tgt_p);
            if score < floor {
                continue;
            }
            if best.as_ref().is_none_or(|(_, bs)| score > *bs) {
                best = Some((tgt_id.clone(), score));
            }
        }

        if let Some((tgt_id, score)) = best {
            out.push(Anchor {
                src: src_id.clone(),
                tgt: tgt_id.clone(),
                confidence: score,
                strategy: StrategyTag::Structural,
                explanation: format!(
                    "structural match (degree+kind-signature similarity {:.2}): {} ↔ {}",
                    score,
                    src_id.as_str(),
                    tgt_id.as_str()
                ),
            });
        }
    }
    out
}

#[derive(Clone, Debug)]
struct VertexProfile {
    out_deg: usize,
    in_deg: usize,
    /// Multiset of outgoing edge kinds.
    out_kinds: HashMap<String, usize>,
    /// Multiset of incoming edge kinds.
    in_kinds: HashMap<String, usize>,
}

fn profile_for(schema: &Schema, vertex: &Name) -> VertexProfile {
    let out = schema.outgoing_edges(vertex);
    let incoming = schema.incoming_edges(vertex);
    let mut out_kinds: HashMap<String, usize> = HashMap::new();
    for edge in out {
        *out_kinds.entry(edge.kind.as_str().to_owned()).or_insert(0) += 1;
    }
    let mut in_kinds: HashMap<String, usize> = HashMap::new();
    for edge in incoming {
        *in_kinds.entry(edge.kind.as_str().to_owned()).or_insert(0) += 1;
    }
    VertexProfile {
        out_deg: out.len(),
        in_deg: incoming.len(),
        out_kinds,
        in_kinds,
    }
}

/// Similarity in `[0.0, 1.0]` combining degree closeness and multiset
/// overlap on both outgoing and incoming edge kinds.
fn similarity(a: &VertexProfile, b: &VertexProfile) -> f64 {
    let deg_sim =
        degree_similarity(a.out_deg, b.out_deg).min(degree_similarity(a.in_deg, b.in_deg));
    let out_sim = multiset_jaccard(&a.out_kinds, &b.out_kinds);
    let in_sim = multiset_jaccard(&a.in_kinds, &b.in_kinds);
    let kind_sim = 0.5f64.mul_add(in_sim, 0.5 * out_sim);
    0.5f64.mul_add(kind_sim, 0.5 * deg_sim)
}

fn degree_similarity(a: usize, b: usize) -> f64 {
    if a == 0 && b == 0 {
        return 1.0;
    }
    let max = a.max(b);
    let min = a.min(b);
    #[allow(clippy::cast_precision_loss)]
    {
        min as f64 / max as f64
    }
}

fn multiset_jaccard(a: &HashMap<String, usize>, b: &HashMap<String, usize>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let mut intersection = 0usize;
    let mut union = 0usize;
    let keys: std::collections::HashSet<&String> = a.keys().chain(b.keys()).collect();
    for key in keys {
        let ca = a.get(key).copied().unwrap_or(0);
        let cb = b.get(key).copied().unwrap_or(0);
        intersection += ca.min(cb);
        union += ca.max(cb);
    }
    if union == 0 {
        1.0
    } else {
        #[allow(clippy::cast_precision_loss)]
        {
            intersection as f64 / union as f64
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;
    use panproto_schema::{Protocol, SchemaBuilder};

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

    fn build(verts: &[(&str, &str)], edges: &[(&str, &str, &str, &str)]) -> Schema {
        let proto = test_protocol();
        let mut b = SchemaBuilder::new(&proto);
        for (id, k) in verts {
            b = b.vertex(id, k, None::<&str>).unwrap();
        }
        for (s, t, k, n) in edges {
            b = b.edge(s, t, k, Some(*n)).unwrap();
        }
        b.build().unwrap()
    }

    #[test]
    fn degree_similarity_exact() {
        assert_eq!(degree_similarity(3, 3), 1.0);
        assert_eq!(degree_similarity(0, 0), 1.0);
        assert!((degree_similarity(2, 4) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn multiset_jaccard_identical() {
        let mut m = HashMap::new();
        m.insert("prop".to_owned(), 2);
        assert_eq!(multiset_jaccard(&m, &m), 1.0);
    }

    #[test]
    fn structural_anchors_pair_shaped_objects() {
        // Two object vertices with identical edge-kind signatures and
        // matching degrees. Names are totally different.
        let src = build(
            &[
                ("alpha", "object"),
                ("alpha.x", "string"),
                ("alpha.y", "string"),
            ],
            &[
                ("alpha", "alpha.x", "prop", "x"),
                ("alpha", "alpha.y", "prop", "y"),
            ],
        );
        let tgt = build(
            &[
                ("omega", "object"),
                ("omega.a", "string"),
                ("omega.b", "string"),
            ],
            &[
                ("omega", "omega.a", "prop", "a"),
                ("omega", "omega.b", "prop", "b"),
            ],
        );
        let anchors = structural_anchors(&src, &tgt, 0.5);
        assert!(
            anchors
                .iter()
                .any(|a| a.src.as_str() == "alpha" && a.tgt.as_str() == "omega"),
            "structural strategy should pair alpha↔omega on identical shape; got {anchors:?}"
        );
    }

    #[test]
    fn structural_anchors_respect_floor_for_root_mismatch() {
        let src = build(
            &[("alpha", "object"), ("alpha.x", "string")],
            &[("alpha", "alpha.x", "prop", "x")],
        );
        let tgt = build(
            &[
                ("omega", "object"),
                ("omega.a", "string"),
                ("omega.b", "string"),
                ("omega.c", "string"),
                ("omega.d", "string"),
            ],
            &[
                ("omega", "omega.a", "prop", "a"),
                ("omega", "omega.b", "prop", "b"),
                ("omega", "omega.c", "prop", "c"),
                ("omega", "omega.d", "prop", "d"),
            ],
        );
        // Roots have very different out-degrees (1 vs 4). With a high
        // floor, the root-to-root pairing must be suppressed. Leaf-level
        // pairings are still emitted because their degree signatures
        // match exactly (1 in, 0 out each side).
        let anchors = structural_anchors(&src, &tgt, 0.75);
        assert!(
            !anchors.iter().any(|a| a.src.as_str() == "alpha"
                && a.tgt.as_str().starts_with("omega")
                && !a.tgt.as_str().starts_with("omega.")),
            "high floor should suppress the mismatched root pairing: {anchors:?}"
        );
    }

    #[test]
    fn similarity_detects_degree_asymmetry() {
        // Vertex a: all incoming, no outgoing (sink).
        // Vertex b: all outgoing, no incoming (source).
        // Their degree profiles are mirror images; similarity should be
        // low because the min/max ratio on each side is 0/something.
        let sink = VertexProfile {
            out_deg: 0,
            in_deg: 3,
            out_kinds: HashMap::new(),
            in_kinds: {
                let mut m = HashMap::new();
                m.insert("prop".to_owned(), 3);
                m
            },
        };
        let source = VertexProfile {
            out_deg: 3,
            in_deg: 0,
            out_kinds: {
                let mut m = HashMap::new();
                m.insert("prop".to_owned(), 3);
                m
            },
            in_kinds: HashMap::new(),
        };
        let s = similarity(&sink, &source);
        // degree_similarity(0,3) = 0, degree_similarity(3,0) = 0.
        // kind_sim: out: one empty, one nonempty → jaccard 0; in: same.
        // Total: 0.5*0 + 0.5*0 = 0.
        assert!(s < 0.1, "sink vs source must score very low: {s}");
    }

    #[test]
    fn multiset_jaccard_empty_nonempty() {
        let empty: HashMap<String, usize> = HashMap::new();
        let mut m = HashMap::new();
        m.insert("prop".to_owned(), 1);
        // union=1 intersection=0 → 0.0.
        assert_eq!(multiset_jaccard(&empty, &m), 0.0);
        assert_eq!(multiset_jaccard(&empty, &empty), 1.0);
    }

    #[test]
    fn structural_anchors_leaf_only_schema_has_no_anchors() {
        // Single isolated vertex: out_deg + in_deg == 0, strategy skips.
        let src = build(&[("x", "string")], &[]);
        let tgt = build(&[("y", "string")], &[]);
        assert!(structural_anchors(&src, &tgt, 0.5).is_empty());
    }

    #[test]
    fn structural_anchors_deterministic() {
        let perms: [&[(&str, &str)]; 2] = [
            &[
                ("aa", "object"),
                ("bb", "object"),
                ("aa.x", "string"),
                ("bb.x", "string"),
            ],
            &[
                ("bb", "object"),
                ("aa", "object"),
                ("bb.x", "string"),
                ("aa.x", "string"),
            ],
        ];
        let tgt = build(
            &[("tt", "object"), ("tt.x", "string")],
            &[("tt", "tt.x", "prop", "x")],
        );
        let mut results = Vec::new();
        for verts in perms {
            let edges: Vec<(&str, &str, &str, &str)> = verts
                .iter()
                .filter(|(id, _)| !id.contains('.'))
                .map(|(id, _)| {
                    (
                        *id,
                        Box::leak(format!("{id}.x").into_boxed_str()) as &str,
                        "prop",
                        "x",
                    )
                })
                .collect();
            let src = build(verts, &edges);
            let anchors = structural_anchors(&src, &tgt, 0.5);
            let mut pairs: Vec<_> = anchors
                .iter()
                .map(|a| (a.src.as_str().to_owned(), a.tgt.as_str().to_owned()))
                .collect();
            pairs.sort();
            results.push(pairs);
        }
        assert_eq!(results[0], results[1]);
    }

    #[test]
    fn structural_anchors_emit_kind_compatible_only() {
        let src = build(
            &[("r", "object"), ("r.x", "string")],
            &[("r", "r.x", "prop", "x")],
        );
        let tgt = build(
            &[("r", "object"), ("r.x", "string")],
            &[("r", "r.x", "prop", "x")],
        );
        for anchor in structural_anchors(&src, &tgt, 0.5) {
            assert!(kinds_compatible(&src, &anchor.src, &tgt, &anchor.tgt));
        }
    }

    #[test]
    fn structural_single_isolated_vertex() {
        // Smallest legal schema: single vertex has zero in+out degree and
        // is skipped by the strategy.
        let s = build(&[("lone", "string")], &[]);
        let t = build(&[("other", "string")], &[]);
        assert!(structural_anchors(&s, &t, 0.5).is_empty());
    }

    #[test]
    fn structural_tie_break_picks_lowest_alpha_target() {
        // Source has one equally-good match against two targets with
        // identical structural profiles. Expect the lowest-alpha target.
        let src = build(
            &[("s", "object"), ("s.x", "string")],
            &[("s", "s.x", "prop", "x")],
        );
        let tgt = build(
            &[
                ("aaa", "object"),
                ("aaa.x", "string"),
                ("zzz", "object"),
                ("zzz.x", "string"),
            ],
            &[("aaa", "aaa.x", "prop", "x"), ("zzz", "zzz.x", "prop", "x")],
        );
        let anchors = structural_anchors(&src, &tgt, 0.5);
        let s_anchor = anchors.iter().find(|a| a.src.as_str() == "s").unwrap();
        assert_eq!(s_anchor.tgt.as_str(), "aaa");
    }

    #[test]
    fn structural_bit_identical_across_100_runs() {
        let src = build(
            &[
                ("alpha", "object"),
                ("alpha.x", "string"),
                ("alpha.y", "string"),
            ],
            &[
                ("alpha", "alpha.x", "prop", "x"),
                ("alpha", "alpha.y", "prop", "y"),
            ],
        );
        let tgt = build(
            &[
                ("omega", "object"),
                ("omega.a", "string"),
                ("omega.b", "string"),
            ],
            &[
                ("omega", "omega.a", "prop", "a"),
                ("omega", "omega.b", "prop", "b"),
            ],
        );
        let baseline: Vec<(String, String, u64)> = structural_anchors(&src, &tgt, 0.5)
            .iter()
            .map(|a| {
                (
                    a.src.as_str().into(),
                    a.tgt.as_str().into(),
                    a.confidence.to_bits(),
                )
            })
            .collect();
        for _ in 0..100 {
            let again: Vec<(String, String, u64)> = structural_anchors(&src, &tgt, 0.5)
                .iter()
                .map(|a| {
                    (
                        a.src.as_str().into(),
                        a.tgt.as_str().into(),
                        a.confidence.to_bits(),
                    )
                })
                .collect();
            assert_eq!(again, baseline);
        }
    }

    #[test]
    fn structural_anchors_skip_kind_mismatch() {
        let src = build(
            &[("a", "object"), ("a.x", "string")],
            &[("a", "a.x", "prop", "x")],
        );
        let tgt = build(
            &[("b", "integer"), ("b.y", "string")],
            &[("b", "b.y", "prop", "y")],
        );
        let anchors = structural_anchors(&src, &tgt, 0.5);
        assert!(
            anchors
                .iter()
                .all(|a| a.src.as_str() != "a" || a.tgt.as_str() != "b"),
            "kind mismatch must not produce anchor: {anchors:?}"
        );
    }
}
