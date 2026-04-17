//! Type-signature alignment strategy.
//!
//! Proposes anchors between source and target vertices whose outgoing
//! edge-kind multisets and leaf-kind multisets match, even when their
//! names diverge entirely. This complements the alias and token-
//! similarity strategies by catching cases where the signature of a
//! record (what shape it has, not what it's called) is the dominant
//! evidence of correspondence.
//!
//! The strategy computes, for each vertex, its **kind signature**: a
//! sorted multiset of `(edge_kind, target_vertex_kind)` pairs over its
//! outgoing edges. Vertex pairs whose kind signatures are equal and
//! whose own kinds match receive a high-confidence anchor; pairs whose
//! signatures are non-equal but share a majority of entries receive a
//! lower-confidence anchor scaled by the overlap ratio.
//!
//! Because the strategy ignores names entirely, it runs well below
//! `exact`, `alias`, and `token_similarity` in priority. The CSP still
//! enforces naturality for every emitted anchor, so wrong matches are
//! rejected downstream.

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_schema::Schema;

use super::{Anchor, StrategyTag, kinds_compatible};

/// Emit anchors for kind-signature-compatible vertex pairs.
///
/// `threshold` is the minimum overlap ratio `|A ∩ B| / max(|A|, |B|)`
/// for an anchor to be emitted. Values below `0.5` are clipped up to
/// `0.5` to avoid pairing vertices that agree on less than half of
/// their signature.
///
/// Tie-break: when multiple candidate targets share the best overlap
/// score, the lowest-alphabetical target wins (source and target ids
/// are both iterated in sorted order, and the ">" comparison retains
/// the first-seen maximum).
#[must_use]
pub fn type_signature_anchors(src: &Schema, tgt: &Schema, threshold: f64) -> Vec<Anchor> {
    let effective_threshold = threshold.max(0.5);
    let src_sigs: HashMap<Name, Vec<SignatureEntry>> = src
        .vertices
        .keys()
        .map(|id| (id.clone(), signature(src, id)))
        .collect();
    let tgt_sigs: HashMap<Name, Vec<SignatureEntry>> = tgt
        .vertices
        .keys()
        .map(|id| (id.clone(), signature(tgt, id)))
        .collect();

    let mut out = Vec::new();

    let mut src_ids: Vec<&Name> = src.vertices.keys().collect();
    src_ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    let mut tgt_ids: Vec<&Name> = tgt.vertices.keys().collect();
    tgt_ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));

    for src_id in src_ids.iter().copied() {
        let Some(src_sig) = src_sigs.get(src_id) else {
            continue;
        };
        if src_sig.is_empty() {
            continue;
        }

        let mut best: Option<(Name, f64, usize)> = None;
        for tgt_id in tgt_ids.iter().copied() {
            if !kinds_compatible(src, src_id, tgt, tgt_id) {
                continue;
            }
            let Some(tgt_sig) = tgt_sigs.get(tgt_id) else {
                continue;
            };
            if tgt_sig.is_empty() {
                continue;
            }
            let (overlap, _matched) = multiset_overlap(src_sig, tgt_sig);
            if overlap < effective_threshold {
                continue;
            }
            let larger = src_sig.len().max(tgt_sig.len());
            if best.as_ref().is_none_or(|(_, bs, _)| overlap > *bs) {
                best = Some((tgt_id.clone(), overlap, larger));
            }
        }

        if let Some((tgt_id, overlap, size)) = best {
            // Exact-signature equality earns a confidence boost.
            let boost = if (overlap - 1.0).abs() < f64::EPSILON {
                0.1
            } else {
                0.0
            };
            // Scale confidence down slightly for very small signatures
            // (two fields with one edge is weak evidence regardless).
            #[allow(clippy::cast_precision_loss)]
            let size_penalty = if size < 2 { 0.1 } else { 0.0 };
            let confidence = (overlap + boost - size_penalty).clamp(0.4, 0.9);
            out.push(Anchor {
                src: src_id.clone(),
                tgt: tgt_id.clone(),
                confidence,
                strategy: StrategyTag::TypeSignature,
                explanation: format!(
                    "kind-signature overlap {:.2} on {size} field(s): {} ↔ {}",
                    overlap,
                    src_id.as_str(),
                    tgt_id.as_str()
                ),
            });
        }
    }

    out
}

/// One entry in a vertex's kind signature: `(edge_kind, leaf_kind)`.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
struct SignatureEntry {
    edge_kind: String,
    leaf_kind: String,
}

/// Build the kind signature for `vertex`: the sorted multiset of
/// (edge.kind, edge.tgt.kind) over its outgoing edges. Edges whose
/// target vertex is unknown (should not happen in a well-formed
/// schema) are skipped.
fn signature(schema: &Schema, vertex: &Name) -> Vec<SignatureEntry> {
    let mut out: Vec<SignatureEntry> = schema
        .outgoing_edges(vertex)
        .iter()
        .filter_map(|edge| {
            schema.vertex(&edge.tgt).map(|v| SignatureEntry {
                edge_kind: edge.kind.as_str().to_owned(),
                leaf_kind: v.kind.as_str().to_owned(),
            })
        })
        .collect();
    out.sort();
    out
}

/// Compute `(overlap_ratio, matched_count)` between two multisets.
/// Overlap is defined as `|intersection| / max(|A|, |B|)` so identical
/// multisets score `1.0`, disjoint score `0.0`.
fn multiset_overlap(a: &[SignatureEntry], b: &[SignatureEntry]) -> (f64, usize) {
    if a.is_empty() && b.is_empty() {
        return (1.0, 0);
    }
    let mut counts_a: HashMap<&SignatureEntry, usize> = HashMap::new();
    for e in a {
        *counts_a.entry(e).or_insert(0) += 1;
    }
    let mut counts_b: HashMap<&SignatureEntry, usize> = HashMap::new();
    for e in b {
        *counts_b.entry(e).or_insert(0) += 1;
    }
    let mut intersection = 0usize;
    for (entry, &ca) in &counts_a {
        if let Some(&cb) = counts_b.get(entry) {
            intersection += ca.min(cb);
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let ratio = intersection as f64 / a.len().max(b.len()) as f64;
    (ratio, intersection)
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

    fn build_schema(vertices: &[(&str, &str)], edges: &[(&str, &str, &str, &str)]) -> Schema {
        let proto = test_protocol();
        let mut b = SchemaBuilder::new(&proto);
        for (id, kind) in vertices {
            b = b.vertex(id, kind, None::<&str>).unwrap();
        }
        for (src, tgt, kind, name) in edges {
            b = b.edge(src, tgt, kind, Some(*name)).unwrap();
        }
        b.build().unwrap()
    }

    #[test]
    fn signature_equal_multisets_full_overlap() {
        let a = vec![
            SignatureEntry {
                edge_kind: "prop".into(),
                leaf_kind: "string".into(),
            },
            SignatureEntry {
                edge_kind: "prop".into(),
                leaf_kind: "string".into(),
            },
        ];
        let (ratio, count) = multiset_overlap(&a, &a);
        assert_eq!(ratio, 1.0);
        assert_eq!(count, 2);
    }

    #[test]
    fn disjoint_multisets_zero_overlap() {
        let a = vec![SignatureEntry {
            edge_kind: "prop".into(),
            leaf_kind: "string".into(),
        }];
        let b = vec![SignatureEntry {
            edge_kind: "prop".into(),
            leaf_kind: "integer".into(),
        }];
        let (ratio, _) = multiset_overlap(&a, &b);
        assert_eq!(ratio, 0.0);
    }

    #[test]
    fn aligns_records_with_same_type_signature_and_different_names() {
        // Two records with two string children each — renames across
        // both sides. Type signature still pairs them.
        let src = build_schema(
            &[
                ("post", "object"),
                ("post.text", "string"),
                ("post.createdAt", "string"),
            ],
            &[
                ("post", "post.text", "prop", "text"),
                ("post", "post.createdAt", "prop", "createdAt"),
            ],
        );
        let tgt = build_schema(
            &[
                ("message", "object"),
                ("message.body", "string"),
                ("message.sentAt", "string"),
            ],
            &[
                ("message", "message.body", "prop", "body"),
                ("message", "message.sentAt", "prop", "sentAt"),
            ],
        );

        let anchors = type_signature_anchors(&src, &tgt, 0.5);
        assert!(
            anchors
                .iter()
                .any(|a| a.src.as_str() == "post" && a.tgt.as_str() == "message"),
            "expected post ↔ message anchor from matching type signature; got {anchors:?}"
        );
    }

    #[test]
    fn rejects_mismatched_leaf_kinds() {
        let src = build_schema(
            &[("r", "object"), ("r.x", "string")],
            &[("r", "r.x", "prop", "x")],
        );
        let tgt = build_schema(
            &[("r", "object"), ("r.y", "integer")],
            &[("r", "r.y", "prop", "y")],
        );
        let anchors = type_signature_anchors(&src, &tgt, 0.5);
        assert!(
            anchors.is_empty(),
            "string vs integer leaves must not align via type signature: {anchors:?}"
        );
    }

    #[test]
    fn signature_is_order_insensitive() {
        let a = build_schema(
            &[("r", "object"), ("r.x", "string"), ("r.y", "integer")],
            &[("r", "r.x", "prop", "x"), ("r", "r.y", "prop", "y")],
        );
        let b = build_schema(
            &[("r", "object"), ("r.y", "integer"), ("r.x", "string")],
            &[("r", "r.y", "prop", "y"), ("r", "r.x", "prop", "x")],
        );
        let sig_a = signature(&a, &Name::from("r"));
        let sig_b = signature(&b, &Name::from("r"));
        assert_eq!(
            sig_a, sig_b,
            "multiset signatures must be equal regardless of edge insertion order"
        );
    }

    #[test]
    fn multiset_overlap_empty_nonempty() {
        let a: Vec<SignatureEntry> = vec![];
        let b = vec![SignatureEntry {
            edge_kind: "prop".into(),
            leaf_kind: "string".into(),
        }];
        let (ratio, count) = multiset_overlap(&a, &b);
        assert_eq!(count, 0);
        // Per code: |A|=0, |B|=1, intersection=0 → 0/1 = 0.0. Only
        // empty-empty returns 1.0.
        assert_eq!(ratio, 0.0);
    }

    #[test]
    fn type_signature_leaf_only_schema() {
        // Leaves have empty signatures; strategy must skip them without panic.
        let src = build_schema(&[("x", "string")], &[]);
        let tgt = build_schema(&[("y", "string")], &[]);
        assert!(type_signature_anchors(&src, &tgt, 0.5).is_empty());
    }

    #[test]
    fn type_signature_deterministic_emission() {
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
        let tgt = build_schema(
            &[
                ("tt", "object"),
                ("uu", "object"),
                ("tt.x", "string"),
                ("uu.x", "string"),
            ],
            &[("tt", "tt.x", "prop", "x"), ("uu", "uu.x", "prop", "x")],
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
            let src = build_schema(verts, &edges);
            let anchors = type_signature_anchors(&src, &tgt, 0.5);
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
    fn type_signature_emits_kind_compatible_only() {
        let src = build_schema(
            &[("r", "object"), ("r.x", "string")],
            &[("r", "r.x", "prop", "x")],
        );
        let tgt = build_schema(
            &[("r", "object"), ("r.x", "string")],
            &[("r", "r.x", "prop", "x")],
        );
        for anchor in type_signature_anchors(&src, &tgt, 0.5) {
            assert!(kinds_compatible(&src, &anchor.src, &tgt, &anchor.tgt));
        }
    }

    #[test]
    fn type_signature_single_isolated_vertex() {
        // Smallest legal schema: one vertex with no edges → empty signature
        // → no anchors emitted.
        let s = build_schema(&[("lone", "string")], &[]);
        let t = build_schema(&[("other", "string")], &[]);
        assert!(type_signature_anchors(&s, &t, 0.5).is_empty());
    }

    #[test]
    fn type_signature_tie_break_picks_lowest_alpha_target() {
        // Source matches two targets with identical signatures. Sorted
        // iteration + strict > means the first-seen (alphabetically
        // lowest) target wins.
        let src = build_schema(
            &[("r", "object"), ("r.x", "string")],
            &[("r", "r.x", "prop", "x")],
        );
        let tgt = build_schema(
            &[
                ("aa", "object"),
                ("aa.x", "string"),
                ("zz", "object"),
                ("zz.x", "string"),
            ],
            &[("aa", "aa.x", "prop", "x"), ("zz", "zz.x", "prop", "x")],
        );
        let anchors = type_signature_anchors(&src, &tgt, 0.5);
        let r_anchor = anchors.iter().find(|a| a.src.as_str() == "r").unwrap();
        assert_eq!(
            r_anchor.tgt.as_str(),
            "aa",
            "tie-break favors lowest-alpha target"
        );
    }

    #[test]
    fn type_signature_tie_break_is_stable_across_three_identical_targets() {
        // When three candidate targets all tie at ratio = 1.0, the
        // strict `>` check retains the first-seen maximum; combined
        // with sorted target iteration this must pick the
        // alphabetically lowest target regardless of insertion order
        // in the backing `HashMap`.
        let src = build_schema(
            &[("r", "object"), ("r.x", "string")],
            &[("r", "r.x", "prop", "x")],
        );
        let tgt = build_schema(
            &[
                ("bbb", "object"),
                ("bbb.x", "string"),
                ("aaa", "object"),
                ("aaa.x", "string"),
                ("ccc", "object"),
                ("ccc.x", "string"),
            ],
            &[
                ("bbb", "bbb.x", "prop", "x"),
                ("aaa", "aaa.x", "prop", "x"),
                ("ccc", "ccc.x", "prop", "x"),
            ],
        );
        let anchors = type_signature_anchors(&src, &tgt, 0.5);
        let r_anchor = anchors.iter().find(|a| a.src.as_str() == "r").unwrap();
        assert_eq!(
            r_anchor.tgt.as_str(),
            "aaa",
            "three-way tie must still resolve to lowest-alpha target"
        );
    }

    #[test]
    fn type_signature_bit_identical_across_100_runs() {
        let src = build_schema(
            &[
                ("post", "object"),
                ("post.text", "string"),
                ("post.createdAt", "string"),
            ],
            &[
                ("post", "post.text", "prop", "text"),
                ("post", "post.createdAt", "prop", "createdAt"),
            ],
        );
        let tgt = build_schema(
            &[
                ("message", "object"),
                ("message.body", "string"),
                ("message.sentAt", "string"),
            ],
            &[
                ("message", "message.body", "prop", "body"),
                ("message", "message.sentAt", "prop", "sentAt"),
            ],
        );
        let baseline: Vec<(String, String, u64)> = type_signature_anchors(&src, &tgt, 0.5)
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
            let again: Vec<(String, String, u64)> = type_signature_anchors(&src, &tgt, 0.5)
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
    fn multiset_overlap_asymmetric_counts() {
        // Pins the counts-based intersection semantics. Multiset A =
        // {prop-string: 3, prop-integer: 1}, B = {prop-string: 2,
        // prop-integer: 2}. Intersection = min(3,2) + min(1,2) = 3.
        // |A| = 4, |B| = 4, denom = max = 4, ratio = 3/4 = 0.75.
        let a = vec![
            SignatureEntry {
                edge_kind: "prop".into(),
                leaf_kind: "string".into(),
            },
            SignatureEntry {
                edge_kind: "prop".into(),
                leaf_kind: "string".into(),
            },
            SignatureEntry {
                edge_kind: "prop".into(),
                leaf_kind: "string".into(),
            },
            SignatureEntry {
                edge_kind: "prop".into(),
                leaf_kind: "integer".into(),
            },
        ];
        let b = vec![
            SignatureEntry {
                edge_kind: "prop".into(),
                leaf_kind: "string".into(),
            },
            SignatureEntry {
                edge_kind: "prop".into(),
                leaf_kind: "string".into(),
            },
            SignatureEntry {
                edge_kind: "prop".into(),
                leaf_kind: "integer".into(),
            },
            SignatureEntry {
                edge_kind: "prop".into(),
                leaf_kind: "integer".into(),
            },
        ];
        let (ratio, count) = multiset_overlap(&a, &b);
        assert_eq!(count, 3);
        assert!(
            (ratio - 0.75).abs() < 1e-9,
            "asymmetric multiset ratio should be 0.75, got {ratio}"
        );
        // Symmetric in argument order.
        let (ratio_swapped, count_swapped) = multiset_overlap(&b, &a);
        assert_eq!(count_swapped, 3);
        assert!((ratio_swapped - 0.75).abs() < 1e-9);
    }

    #[test]
    fn threshold_gates_emission() {
        // Source record with 4 children; target with 1 matching child + 3 different.
        let src = build_schema(
            &[
                ("r", "object"),
                ("r.a", "string"),
                ("r.b", "string"),
                ("r.c", "string"),
                ("r.d", "string"),
            ],
            &[
                ("r", "r.a", "prop", "a"),
                ("r", "r.b", "prop", "b"),
                ("r", "r.c", "prop", "c"),
                ("r", "r.d", "prop", "d"),
            ],
        );
        let tgt = build_schema(
            &[
                ("r", "object"),
                ("r.a", "string"),
                ("r.b", "integer"),
                ("r.c", "integer"),
                ("r.d", "integer"),
            ],
            &[
                ("r", "r.a", "prop", "a"),
                ("r", "r.b", "prop", "b"),
                ("r", "r.c", "prop", "c"),
                ("r", "r.d", "prop", "d"),
            ],
        );
        // Overlap is 1/4 which is below the 0.5 floor.
        let anchors = type_signature_anchors(&src, &tgt, 0.5);
        assert!(
            !anchors.iter().any(|a| a.src.as_str() == "r"),
            "low overlap should not emit an anchor: {anchors:?}"
        );
    }
}
